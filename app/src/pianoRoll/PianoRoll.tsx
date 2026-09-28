import { type KeyboardEvent, type PointerEvent, useEffect, useRef, useState } from "react";
import type { NoteView, ProjectView } from "../backend";
import { nextGesture } from "../useGesture";
import { createCanvas2DRenderer } from "./canvasRenderer";
import { type Drag, dragNote, sameNote } from "./editing";
import type { FrameStats } from "./frameStats";
import type { PlayheadClock } from "./playhead";
import type { RendererFactory } from "./renderer";
import { PianoRollScene } from "./scene";
import { DEFAULT_SNAP, SNAPS, type Snap, snapDown, snapStep } from "./snap";
import { noteArea, xToTick, yToPitch, zoomPitch, zoomTime } from "./viewport";

/**
 * What the piano roll's edits do. The app sends each to Rust; the piano roll
 * only draws what comes back.
 */
export interface NoteEditor {
  /** Adds notes to the clip. Their IDs are already chosen. */
  add(notes: NoteView[], gesture: number): void;
  /** Sets every value of existing notes. One drag's changes share a `gesture`. */
  set(notes: NoteView[], gesture: number): void;
  remove(ids: string[]): void;
  /** Puts back everything `gesture` changed. */
  cancel(gesture: number): void;
  /** Plays a note briefly, without changing the project. */
  audition(pitch: number, velocity: number): void;
}

interface Props {
  project: ProjectView;
  editor: NoteEditor;
  /** Where the playhead is, between the engine's reports. */
  clock: PlayheadClock;
  /** Where each frame's timing goes. */
  stats: FrameStats;
  /** Draws the layers. Canvas 2D unless a test or WebGL says otherwise. */
  createRenderer?: RendererFactory;
}

/** Each zoom button press zooms by this much. */
const ZOOM_STEP = 1.25;
/** How much a wheel's `deltaY` zooms with ⌘, Ctrl (a trackpad pinch) or ⌥ held. */
const WHEEL_ZOOM_RATE = 0.01;
/** How far the pointer moves before a press becomes a drag, so a click never nudges a note. */
const DRAG_THRESHOLD_PIXELS = 3;
/** How hard a drawn note plays, until there's a velocity lane (UTA-14). */
const NEW_NOTE_VELOCITY = 100;

/** A drag in progress. */
interface DragState {
  drag: Drag;
  gesture: number;
  /** The note as last sent to Rust. */
  last: NoteView;
  /** Whether anything has been sent, so Esc has something to put back. */
  sent: boolean;
  /** Whether the pointer has moved far enough to count as dragging. */
  moving: boolean;
  /** Where the pointer went down, in CSS pixels in the piano roll. */
  x: number;
  y: number;
  /** Stops listening to the pointer and the keyboard. */
  stop: () => void;
}

/**
 * The piano roll: the clip's notes over a keyboard and a bar ruler, with the
 * playhead. It's drawn on three stacked canvases, each redrawn only when it
 * needs to be (RFC-002, "The shared model", point 8).
 */
export function PianoRoll({
  project,
  editor,
  clock,
  stats,
  createRenderer = createCanvas2DRenderer,
}: Props) {
  const containerRef = useRef<HTMLDivElement>(null);
  const gridRef = useRef<HTMLCanvasElement>(null);
  const notesRef = useRef<HTMLCanvasElement>(null);
  const topRef = useRef<HTMLCanvasElement>(null);
  const [scene] = useState(() => new PianoRollScene());
  const [snap, setSnap] = useState<Snap>(DEFAULT_SNAP);
  const scheme = useColourScheme();
  // The pointer and keyboard handlers read the latest of these, including
  // those added to the window for the length of a drag.
  const latest = useRef({ project, editor, snap });
  const selected = useRef<string | null>(null);
  const dragging = useRef<DragState | null>(null);

  useEffect(() => {
    latest.current = { project, editor, snap };
  }, [project, editor, snap]);

  useEffect(() => scene.setProject(project), [scene, project]);

  // A drag ends if the piano roll goes away mid-drag.
  useEffect(() => () => dragging.current?.stop(), []);

  // The size, as the window changes.
  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;
    const resize = (width: number, height: number) =>
      scene.resize(width, height, window.devicePixelRatio || 1);
    if (typeof ResizeObserver === "undefined") {
      const box = container.getBoundingClientRect();
      resize(box.width, box.height);
      return;
    }
    const observer = new ResizeObserver(([entry]) =>
      resize(entry.contentRect.width, entry.contentRect.height),
    );
    observer.observe(container);
    return () => observer.disconnect();
  }, [scene]);

  // The renderer, and the drawing loop, once a screen frame. A new colour
  // scheme makes a new renderer, which reads the new colours.
  useEffect(() => {
    const grid = gridRef.current;
    const notes = notesRef.current;
    const top = topRef.current;
    if (!grid || !notes || !top) return;
    scene.setRenderer(createRenderer({ grid, notes, top }));

    let request = 0;
    const frame = (now: number) => {
      const started = performance.now();
      scene.draw(clock.at(now));
      stats.record(now, performance.now() - started);
      request = requestAnimationFrame(frame);
    };
    request = requestAnimationFrame(frame);
    // Frames stop while the window is hidden; that gap isn't a slow frame.
    const onVisibility = () => stats.pause();
    document.addEventListener("visibilitychange", onVisibility);
    return () => {
      cancelAnimationFrame(request);
      document.removeEventListener("visibilitychange", onVisibility);
      scene.setRenderer(null);
    };
  }, [scene, clock, stats, createRenderer, scheme]);

  // Scroll with the wheel or trackpad; zoom time with ⌘ or a pinch, and
  // pitch with ⌥. Not a React handler: it has to be able to preventDefault.
  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;
    const onWheel = (event: WheelEvent) => {
      event.preventDefault();
      const box = container.getBoundingClientRect();
      const x = event.clientX - box.left;
      const y = event.clientY - box.top;
      const zoom = Math.exp(-event.deltaY * WHEEL_ZOOM_RATE);
      scene.changeView((view) => {
        if (event.ctrlKey || event.metaKey) return zoomTime(view, zoom, x);
        if (event.altKey) return zoomPitch(view, zoom, y);
        // Shift turns a mouse's vertical wheel into horizontal scrolling.
        const [dx, dy] = event.shiftKey
          ? [event.deltaY, event.deltaX]
          : [event.deltaX, event.deltaY];
        return {
          ...view,
          scrollTicks: view.scrollTicks + dx / view.pixelsPerTick,
          scrollY: view.scrollY + dy,
        };
      });
    };
    container.addEventListener("wheel", onWheel, { passive: false });
    return () => container.removeEventListener("wheel", onWheel);
  }, [scene]);

  const select = (id: string | null) => {
    selected.current = id;
    scene.setSelection(id);
  };

  /** Where a pointer event is, in CSS pixels from the piano roll's top left. */
  const pointOf = (event: { clientX: number; clientY: number }) => {
    const box = containerRef.current?.getBoundingClientRect();
    return { x: event.clientX - (box?.left ?? 0), y: event.clientY - (box?.top ?? 0) };
  };

  // Press on a note to select it and drag it (its body moves it, its ends
  // resize it); press on empty space to draw a note and drag out its length.
  const onPointerDown = (event: PointerEvent<HTMLDivElement>) => {
    const view = scene.getView();
    if (event.button !== 0 || dragging.current || !view) return;
    const { x, y } = pointOf(event);
    if (!scene.inNoteArea(x, y)) return;
    event.preventDefault();
    containerRef.current?.focus({ preventScroll: true });
    // So the drag still ends if the pointer is released outside the window.
    try {
      event.currentTarget.setPointerCapture(event.pointerId);
    } catch {
      // No such pointer (a synthetic event): the window's events still arrive.
    }

    const { project, editor, snap } = latest.current;
    const clip = project.track.clip;
    const tick = xToTick(view, x);
    const pitch = yToPitch(view, y);
    const gesture = nextGesture();
    const hit = scene.hitTest(x, y);
    let drag: Drag;
    if (hit) {
      const from = clip.notes.find((note) => note.id === hit.note.id);
      if (!from) return;
      drag = { kind: hit.part === "body" ? "move" : hit.part, from, tick, pitch };
    } else {
      if (tick < clip.start) return;
      const step = event.metaKey ? 1 : snapStep(snap, project.ticksPerQuarter);
      const note: NoteView = {
        // Chosen here, before Rust applies the command (RFC-002, "The shared model", point 3).
        id: crypto.randomUUID(),
        pitch,
        velocity: NEW_NOTE_VELOCITY,
        start: snapDown(tick, step) - clip.start,
        length: newNoteLength(snap, project.ticksPerQuarter),
      };
      editor.add([note], gesture);
      editor.audition(note.pitch, note.velocity);
      drag = { kind: "draw", from: note, tick, pitch };
    }
    select(drag.from.id);

    const onMove = (move: globalThis.PointerEvent) => {
      const state = dragging.current;
      const view = scene.getView();
      if (!state || !view) return;
      const point = pointOf(move);
      if (!state.moving) {
        if (Math.hypot(point.x - state.x, point.y - state.y) < DRAG_THRESHOLD_PIXELS) return;
        state.moving = true;
      }
      const { project, editor, snap } = latest.current;
      // ⌘ turns snapping off while it's held.
      const step = move.metaKey ? 1 : snapStep(snap, project.ticksPerQuarter);
      const next = dragNote(
        state.drag,
        xToTick(view, point.x),
        yToPitch(view, point.y),
        step,
        project.track.clip.start,
      );
      if (sameNote(next, state.last)) return;
      if (next.pitch !== state.last.pitch) editor.audition(next.pitch, next.velocity);
      editor.set([next], state.gesture);
      state.last = next;
      state.sent = true;
    };
    const onKey = (key: globalThis.KeyboardEvent) => {
      const state = dragging.current;
      if (key.key !== "Escape" || !state) return;
      key.preventDefault();
      if (state.sent) latest.current.editor.cancel(state.gesture);
      // A cancelled drawing leaves no note to select.
      if (state.drag.kind === "draw") select(null);
      state.stop();
    };
    const onUp = () => dragging.current?.stop();
    window.addEventListener("pointermove", onMove);
    window.addEventListener("pointerup", onUp);
    window.addEventListener("pointercancel", onUp);
    window.addEventListener("keydown", onKey);
    dragging.current = {
      drag,
      gesture,
      last: drag.from,
      sent: drag.kind === "draw",
      moving: false,
      x,
      y,
      stop: () => {
        window.removeEventListener("pointermove", onMove);
        window.removeEventListener("pointerup", onUp);
        window.removeEventListener("pointercancel", onUp);
        window.removeEventListener("keydown", onKey);
        dragging.current = null;
      },
    };
  };

  // Shows what pressing would do: resize at a note's ends.
  const onHover = (event: PointerEvent<HTMLDivElement>) => {
    const container = containerRef.current;
    if (!container || dragging.current) return;
    const { x, y } = pointOf(event);
    const part = scene.hitTest(x, y)?.part;
    container.style.cursor = part === "start" || part === "end" ? "ew-resize" : "";
  };

  const onDoubleClick = (event: { clientX: number; clientY: number }) => {
    const { x, y } = pointOf(event);
    const hit = scene.hitTest(x, y);
    if (!hit) return;
    latest.current.editor.remove([hit.note.id]);
    select(null);
  };

  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (event.key !== "Backspace" && event.key !== "Delete") return;
    const id = selected.current;
    const { project, editor } = latest.current;
    if (dragging.current || !id || !project.track.clip.notes.some((note) => note.id === id)) {
      return;
    }
    event.preventDefault();
    editor.remove([id]);
    select(null);
  };

  const zoomTimeBy = (factor: number) =>
    scene.changeView((view) => {
      const area = noteArea(view);
      return zoomTime(view, factor, area.x + area.width / 2);
    });
  const zoomPitchBy = (factor: number) =>
    scene.changeView((view) => {
      const area = noteArea(view);
      return zoomPitch(view, factor, area.y + area.height / 2);
    });

  return (
    <section className="piano-roll" aria-label="Piano roll">
      <div
        className="piano-roll-canvas"
        ref={containerRef}
        role="application"
        aria-label="Notes"
        tabIndex={0}
        onPointerDown={onPointerDown}
        onPointerMove={onHover}
        onDoubleClick={onDoubleClick}
        onKeyDown={onKeyDown}
      >
        <canvas ref={gridRef} aria-hidden="true" />
        <canvas ref={notesRef} aria-hidden="true" />
        <canvas ref={topRef} aria-hidden="true" />
      </div>
      <div className="piano-roll-tools">
        <label className="snap">
          <span>Snap</span>
          <select value={snap} onChange={(event) => setSnap(event.target.value as Snap)}>
            {SNAPS.map((option) => (
              <option key={option} value={option}>
                {option === "off" ? "Off" : option}
              </option>
            ))}
          </select>
        </label>
        <div className="zoom" role="group" aria-label="Zoom">
          <button
            type="button"
            aria-label="Zoom out time"
            onClick={() => zoomTimeBy(1 / ZOOM_STEP)}
          >
            −
          </button>
          <span>Time</span>
          <button type="button" aria-label="Zoom in time" onClick={() => zoomTimeBy(ZOOM_STEP)}>
            +
          </button>
          <button
            type="button"
            aria-label="Zoom out pitch"
            onClick={() => zoomPitchBy(1 / ZOOM_STEP)}
          >
            −
          </button>
          <span>Pitch</span>
          <button
            type="button"
            aria-label="Zoom in pitch"
            onClick={() => zoomPitchBy(ZOOM_STEP)}
          >
            +
          </button>
        </div>
      </div>
    </section>
  );
}

/** A drawn note is one grid step long, or a sixteenth with snapping off. */
function newNoteLength(snap: Snap, ticksPerQuarter: number): number {
  return snap === "off" ? ticksPerQuarter / 4 : snapStep(snap, ticksPerQuarter);
}

/** "light" or "dark", following the system, so the canvases redraw in the new colours. */
function useColourScheme(): "light" | "dark" {
  const [query] = useState(() =>
    typeof window.matchMedia === "function"
      ? window.matchMedia("(prefers-color-scheme: dark)")
      : null,
  );
  const [dark, setDark] = useState(() => query?.matches ?? false);
  useEffect(() => {
    if (!query) return;
    const onChange = (event: MediaQueryListEvent) => setDark(event.matches);
    query.addEventListener("change", onChange);
    return () => query.removeEventListener("change", onChange);
  }, [query]);
  return dark ? "dark" : "light";
}
