import {
  type KeyboardEvent,
  type PointerEvent,
  type Ref,
  useEffect,
  useImperativeHandle,
  useRef,
  useState,
} from "react";
import type { ClipView, NoteView, ProjectView } from "../backend";
import { Follow } from "../follow";
import { useColourScheme } from "../useColourScheme";
import { nextGesture } from "../useGesture";
import { createCanvas2DRenderer } from "./canvasRenderer";
import { duplicateNotes, pasteNotes } from "./clipboard";
import { type Drag, dragNote, dragVelocities, moveNotes, sameNote } from "./editing";
import type { FrameLoop } from "../frameLoop";
import type { PlayheadClock } from "./playhead";
import type { RendererFactory } from "./renderer";
import { PianoRollScene } from "./scene";
import { DEFAULT_SNAP, SNAPS, type Snap, snapDown, snapStep } from "./snap";
import {
  type Rect,
  VELOCITY_LANE_HEIGHT,
  noteArea,
  xToTick,
  yToPitch,
  zoomPitch,
  zoomTime,
} from "./viewport";

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
  /**
   * Trims the notes of the same pitch that notes `ids` cover, as part of
   * `gesture`: once a drag of them ends, or a paste lands.
   */
  trim(ids: string[], gesture: number): void;
  /** Puts back everything `gesture` changed. */
  cancel(gesture: number): void;
  /** Plays a note briefly, without changing the project. */
  audition(pitch: number, velocity: number): void;
}

/** What the Edit menu's Copy, Paste and Duplicate do to the piano roll. */
export interface PianoRollHandle {
  /** Copies the selected notes. */
  copy(): void;
  /** Pastes the copied notes at the playhead, snapped to the grid, and selects them. */
  paste(): void;
  /** Puts a copy of the selected notes right after them, and selects it. */
  duplicate(): void;
}

interface Props {
  ref?: Ref<PianoRollHandle>;
  project: ProjectView;
  /** The clip it shows and edits. */
  clip: ClipView;
  editor: NoteEditor;
  /** Where the playhead is, between the engine's reports. */
  clock: PlayheadClock;
  /** Draws it once a screen frame, with the timeline. */
  frames: FrameLoop;
  /** Draws the layers. Canvas 2D unless a test or WebGL says otherwise. */
  createRenderer?: RendererFactory;
}

/** Each zoom button press zooms by this much. */
const ZOOM_STEP = 1.25;
/** How much a wheel's `deltaY` zooms with ⌘, Ctrl (a trackpad pinch) or ⌥ held. */
const WHEEL_ZOOM_RATE = 0.01;
/** How far the pointer moves before a press becomes a drag, so a click never nudges a note. */
const DRAG_THRESHOLD_PIXELS = 3;
/** How hard a drawn note plays. The velocity lane changes it afterwards. */
const NEW_NOTE_VELOCITY = 100;

/** A drag in progress: of notes, velocities or a selection box. */
interface DragState {
  /** Where the pointer went down, in CSS pixels in the piano roll. */
  x: number;
  y: number;
  /** Whether the pointer has moved far enough to count as dragging. */
  moving: boolean;
  /** Stops listening to the pointer and the keyboard. */
  stop: () => void;
}

/** What a drag does as the pointer moves, when it's released, and on Esc. */
interface DragHandlers {
  /** The pointer at `x`, `y`, once it has moved far enough to be a drag. */
  move(x: number, y: number, event: globalThis.PointerEvent): void;
  /** `moved` says whether it became a drag or stayed a click. */
  end?(moved: boolean): void;
  cancel?(): void;
}

/**
 * The piano roll: the clip's notes over a keyboard and a bar ruler, with the
 * playhead, and a velocity lane underneath. It's drawn on three stacked
 * canvases, each redrawn only when it needs to be (RFC-002, "The shared
 * model", point 8).
 */
export function PianoRoll({
  ref,
  project,
  clip,
  editor,
  clock,
  frames,
  createRenderer = createCanvas2DRenderer,
}: Props) {
  const containerRef = useRef<HTMLDivElement>(null);
  const gridRef = useRef<HTMLCanvasElement>(null);
  const notesRef = useRef<HTMLCanvasElement>(null);
  const topRef = useRef<HTMLCanvasElement>(null);
  const [scene] = useState(() => new PianoRollScene());
  // Scrolling or editing stops it following the playhead until the next Play.
  const [follow] = useState(() => new Follow(clock));
  const [snap, setSnap] = useState<Snap>(DEFAULT_SNAP);
  const scheme = useColourScheme();
  // Every edit stops it following the playhead until the next Play.
  // Auditioning a note isn't an edit.
  const editing: NoteEditor = {
    ...editor,
    add: (...args) => {
      follow.pause();
      editor.add(...args);
    },
    set: (...args) => {
      follow.pause();
      editor.set(...args);
    },
    remove: (...args) => {
      follow.pause();
      editor.remove(...args);
    },
    trim: (...args) => {
      follow.pause();
      editor.trim(...args);
    },
  };
  // The pointer, keyboard and menu handlers read the latest of these,
  // including those added to the window for the length of a drag.
  const latest = useRef({ project, clip, editor: editing, snap });
  // Which notes are selected, and what was copied, are the piano roll's own:
  // neither is part of the project.
  const selected = useRef<ReadonlySet<string>>(new Set());
  const clipboard = useRef<NoteView[]>([]);
  const dragging = useRef<DragState | null>(null);

  useEffect(() => {
    latest.current = { project, clip, editor: editing, snap };
  });

  useEffect(() => scene.setProject(project, clip), [scene, project, clip]);

  // Another clip's notes aren't selected. What was copied stays, to paste
  // into this one.
  useEffect(() => {
    selected.current = new Set();
    scene.setSelection(selected.current);
  }, [scene, clip.id]);

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
    const stop = frames.add((now) => {
      const playhead = clock.at(now);
      if (follow.following()) scene.follow(playhead);
      scene.draw(playhead);
    });
    return () => {
      stop();
      scene.setRenderer(null);
    };
  }, [scene, clock, follow, frames, createRenderer, scheme]);

  // Scroll with the wheel or trackpad; zoom time with ⌘ or a pinch, and
  // pitch with ⌥. Not a React handler: it has to be able to preventDefault.
  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;
    const onWheel = (event: WheelEvent) => {
      event.preventDefault();
      follow.pause();
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
  }, [scene, follow]);

  const select = (ids: Iterable<string>) => {
    selected.current = new Set(ids);
    scene.setSelection(selected.current);
  };

  /** Adds `id` to the selection, or takes it out if it's there. */
  const toggle = (id: string) => {
    const ids = new Set(selected.current);
    if (!ids.delete(id)) ids.add(id);
    select(ids);
  };

  /** The selected notes as they are now. Any that have gone (undone, say) are left out. */
  const selectedNotes = (): NoteView[] =>
    latest.current.clip.notes.filter((note) => selected.current.has(note.id));

  /** Where a pointer event is, in CSS pixels from the piano roll's top left. */
  const pointOf = (event: { clientX: number; clientY: number }) => {
    const box = containerRef.current?.getBoundingClientRect();
    return { x: event.clientX - (box?.left ?? 0), y: event.clientY - (box?.top ?? 0) };
  };

  /** Follows the pointer until it's released, or Esc is pressed. */
  const startDrag = (x: number, y: number, handlers: DragHandlers) => {
    const onMove = (move: globalThis.PointerEvent) => {
      const state = dragging.current;
      if (!state) return;
      const point = pointOf(move);
      if (!state.moving) {
        if (Math.hypot(point.x - state.x, point.y - state.y) < DRAG_THRESHOLD_PIXELS) return;
        state.moving = true;
      }
      handlers.move(point.x, point.y, move);
    };
    const onKey = (key: globalThis.KeyboardEvent) => {
      const state = dragging.current;
      if (key.key !== "Escape" || !state) return;
      key.preventDefault();
      state.stop();
      handlers.cancel?.();
    };
    const onUp = () => {
      const state = dragging.current;
      if (!state) return;
      state.stop();
      handlers.end?.(state.moving);
    };
    window.addEventListener("pointermove", onMove);
    window.addEventListener("pointerup", onUp);
    window.addEventListener("pointercancel", onUp);
    window.addEventListener("keydown", onKey);
    dragging.current = {
      x,
      y,
      moving: false,
      stop: () => {
        window.removeEventListener("pointermove", onMove);
        window.removeEventListener("pointerup", onUp);
        window.removeEventListener("pointercancel", onUp);
        window.removeEventListener("keydown", onKey);
        dragging.current = null;
      },
    };
  };

  /**
   * Sends one drag's changes to notes, as `SetNotes` under one gesture, so
   * it's one undo step. `sent` says whether the gesture has already changed
   * something (drawing a note has), so Esc has something to put back.
   */
  const noteChanges = (gesture: number, from: NoteView[], sent: boolean) => {
    let last = from;
    return {
      send(next: NoteView[]) {
        if (next.length === last.length && next.every((note, i) => sameNote(note, last[i]))) {
          return;
        }
        latest.current.editor.set(next, gesture);
        last = next;
        sent = true;
      },
      /**
       * Once the drag ends, trims the notes it now covers, as part of the
       * same undo step. Never mid-drag, so dragging across notes and back
       * leaves them as they were.
       */
      end() {
        if (sent) latest.current.editor.trim(last.map((note) => note.id), gesture);
      },
      cancel() {
        if (sent) latest.current.editor.cancel(gesture);
      },
    };
  };

  /** The grid step for an edit: 1 tick with snapping off, or while ⌘ is held. */
  const stepFor = (event: { metaKey: boolean }) => {
    const { project, snap } = latest.current;
    return event.metaKey ? 1 : snapStep(snap, project.ticksPerQuarter);
  };

  // In the notes: press on a note to select it and drag it (its body moves
  // the selection, its ends resize it); press on empty space to draw a note
  // and drag out its length. Shift-click a note to add it to the selection
  // or take it out, or Shift-drag on empty space to box-select more. In the
  // velocity lane: drag a note's bar to change how hard it plays, with the
  // rest of the selection.
  const onPointerDown = (event: PointerEvent<HTMLDivElement>) => {
    const view = scene.getView();
    if (event.button !== 0 || dragging.current || !view) return;
    const { x, y } = pointOf(event);
    const inNotes = scene.inNoteArea(x, y);
    const bar = !inNotes && scene.inVelocityLane(x, y) ? scene.hitVelocity(x) : null;
    if (!inNotes && !bar) return;
    event.preventDefault();
    containerRef.current?.focus({ preventScroll: true });
    // So the drag still ends if the pointer is released outside the window.
    try {
      event.currentTarget.setPointerCapture(event.pointerId);
    } catch {
      // No such pointer (a synthetic event): the window's events still arrive.
    }
    if (bar) pressVelocity(bar.id, x, y, event.shiftKey);
    else pressNotes(x, y, event);
  };

  const pressNotes = (x: number, y: number, event: PointerEvent<HTMLDivElement>) => {
    const view = scene.getView();
    if (!view) return;
    const { project, clip, editor, snap } = latest.current;
    const tick = xToTick(view, x);
    const pitch = yToPitch(view, y);
    const hit = scene.hitTest(x, y);

    if (event.shiftKey) {
      if (hit) toggle(hit.note.id);
      else boxSelect(x, y);
      return;
    }

    const gesture = nextGesture();
    if (!hit) {
      if (tick < clip.start) return;
      const note: NoteView = {
        // Chosen here, before Rust applies the command (RFC-002, "The shared model", point 3).
        id: crypto.randomUUID(),
        pitch,
        velocity: NEW_NOTE_VELOCITY,
        start: snapDown(tick, stepFor(event)) - clip.start,
        length: newNoteLength(snap, project.ticksPerQuarter),
      };
      editor.add([note], gesture);
      editor.audition(note.pitch, note.velocity);
      select([note.id]);
      const drag: Drag = { kind: "draw", from: note, tick, pitch };
      const changes = noteChanges(gesture, [note], true);
      startDrag(x, y, {
        move: (x, y, move) => changes.send([dragAt(drag, x, y, move)]),
        end: changes.end,
        cancel: () => {
          changes.cancel();
          // A cancelled drawing leaves no note to select.
          select([]);
        },
      });
      return;
    }

    const from = clip.notes.find((note) => note.id === hit.note.id);
    if (!from) return;
    const wasSelected = selected.current.has(from.id);
    if (!wasSelected) select([from.id]);

    if (hit.part !== "body") {
      const drag: Drag = { kind: hit.part, from, tick, pitch };
      const changes = noteChanges(gesture, [from], false);
      startDrag(x, y, {
        move: (x, y, move) => changes.send([dragAt(drag, x, y, move)]),
        end: changes.end,
        cancel: changes.cancel,
      });
      return;
    }

    // The body moves every selected note together.
    const drag: Drag = { kind: "move", from, tick, pitch };
    const group = selectedNotes();
    const changes = noteChanges(gesture, group, false);
    let heard = from.pitch;
    startDrag(x, y, {
      move: (x, y, move) => {
        const view = scene.getView();
        if (!view) return;
        const { clip, editor } = latest.current;
        const next = moveNotes(
          group,
          drag,
          xToTick(view, x),
          yToPitch(view, y),
          stepFor(move),
          clip.start,
        );
        const moved = next[group.findIndex((note) => note.id === from.id)];
        if (moved.pitch !== heard) {
          editor.audition(moved.pitch, moved.velocity);
          heard = moved.pitch;
        }
        changes.send(next);
      },
      // A click on a note of a larger selection selects just that note.
      end: (moved) => {
        changes.end();
        if (!moved && wasSelected) select([from.id]);
      },
      cancel: changes.cancel,
    });
  };

  /** Where `drag` (of one note) puts it with the pointer at `x`, `y`. */
  const dragAt = (drag: Drag, x: number, y: number, event: { metaKey: boolean }): NoteView => {
    const view = scene.getView();
    if (!view) return drag.from;
    return dragNote(
      drag,
      xToTick(view, x),
      yToPitch(view, y),
      stepFor(event),
      latest.current.clip.start,
    );
  };

  /** Drags out a box from `x`, `y`, adding the notes it touches to the selection. */
  const boxSelect = (x: number, y: number) => {
    const before = selected.current;
    startDrag(x, y, {
      move: (toX, toY) => {
        const box: Rect = {
          x: Math.min(x, toX),
          y: Math.min(y, toY),
          width: Math.abs(toX - x),
          height: Math.abs(toY - y),
        };
        scene.setBox(box);
        select([...before, ...scene.notesIn(box)]);
      },
      end: () => scene.setBox(null),
      cancel: () => {
        scene.setBox(null);
        select(before);
      },
    });
  };

  /** Press on note `id`'s velocity bar: drag it, with the rest of the selection. */
  const pressVelocity = (id: string, x: number, y: number, shift: boolean) => {
    if (shift) {
      toggle(id);
      return;
    }
    const wasSelected = selected.current.has(id);
    if (!wasSelected) select([id]);
    const group = selectedNotes();
    const changes = noteChanges(nextGesture(), group, false);
    startDrag(x, y, {
      move: (_x, toY) => {
        const view = scene.getView();
        if (view) changes.send(dragVelocities(view, group, y - toY));
      },
      end: (moved) => {
        if (!moved && wasSelected) select([id]);
      },
      cancel: changes.cancel,
    });
  };

  // Shows what pressing would do: resize at a note's ends, or change a velocity.
  const onHover = (event: PointerEvent<HTMLDivElement>) => {
    const container = containerRef.current;
    if (!container || dragging.current) return;
    const { x, y } = pointOf(event);
    if (scene.inVelocityLane(x, y)) {
      container.style.cursor = scene.hitVelocity(x) ? "ns-resize" : "";
      return;
    }
    const part = scene.hitTest(x, y)?.part;
    container.style.cursor = part === "start" || part === "end" ? "ew-resize" : "";
  };

  const onDoubleClick = (event: { clientX: number; clientY: number }) => {
    const { x, y } = pointOf(event);
    const hit = scene.hitTest(x, y);
    if (!hit) return;
    latest.current.editor.remove([hit.note.id]);
    select([...selected.current].filter((id) => id !== hit.note.id));
  };

  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (dragging.current) return;
    if (event.key === "Escape") {
      select([]);
      return;
    }
    if (event.key !== "Backspace" && event.key !== "Delete") return;
    const notes = selectedNotes();
    if (notes.length === 0) return;
    event.preventDefault();
    // One command, so one undo step.
    latest.current.editor.remove(notes.map((note) => note.id));
    select([]);
  };

  /** Adds pasted or duplicated notes, trims what they land on, and selects them. */
  const land = (notes: NoteView[]) => {
    const { editor } = latest.current;
    const gesture = nextGesture();
    const ids = notes.map((note) => note.id);
    editor.add(notes, gesture);
    editor.trim(ids, gesture);
    select(ids);
  };

  useImperativeHandle(ref, () => ({
    copy() {
      const notes = selectedNotes();
      if (notes.length > 0) clipboard.current = notes;
    },
    paste() {
      const view = scene.getView();
      if (dragging.current || !view || clipboard.current.length === 0) return;
      const { project, clip, snap } = latest.current;
      const step = snapStep(snap, project.ticksPerQuarter);
      const at = snapDown(clock.at(performance.now()), step);
      const notes = pasteNotes(clipboard.current, at, clip.start, () =>
        crypto.randomUUID(),
      );
      land(notes);
    },
    duplicate() {
      if (dragging.current) return;
      const { project } = latest.current;
      const notes = duplicateNotes(selectedNotes(), project.ticksPerQuarter, () =>
        crypto.randomUUID(),
      );
      if (notes.length > 0) land(notes);
    },
  }));

  const zoomTimeBy = (factor: number) => {
    follow.pause();
    scene.changeView((view) => {
      const area = noteArea(view);
      return zoomTime(view, factor, area.x + area.width / 2);
    });
  };
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
      {/* Above the velocity lane, so it never hides a bar. */}
      <div className="piano-roll-tools" style={{ bottom: VELOCITY_LANE_HEIGHT + 8 }}>
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
