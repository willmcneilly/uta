import {
  type KeyboardEvent,
  type PointerEvent,
  type RefObject,
  useEffect,
  useRef,
  useState,
} from "react";
import type { ClipPosition, ProjectView } from "../backend";
import type { FrameLoop } from "../frameLoop";
import type { PlayheadClock } from "../pianoRoll/playhead";
import { useColourScheme } from "../useColourScheme";
import { nextGesture } from "../useGesture";
import { createTimelineRenderer } from "./canvasRenderer";
import { type Span, drawnClip, moveClip, oneBarClip, resizeClip, sameSpan } from "./editing";
import type { TimelineRendererFactory } from "./renderer";
import { TimelineScene } from "./scene";
import {
  CLIP_SNAPS,
  CLIP_SNAP_NAMES,
  type ClipSnap,
  DEFAULT_CLIP_SNAP,
  clipSnapStep,
  minClipLength,
} from "./snap";
import { type TimelineViewport, inTracks, xToTick, yToTrack, zoomTime } from "./viewport";

/**
 * What the timeline's edits do. The app sends each to Rust; the timeline
 * only draws what comes back.
 */
export interface ClipEditor {
  /** Adds an empty clip to `track`. Its ID is already chosen. */
  add(track: string, id: string, start: number, length: number): void;
  /** Sets clips' track, start and length. One drag's changes share a `gesture`. */
  set(clips: ClipPosition[], gesture: number): void;
  remove(ids: string[]): void;
  /** Puts back everything `gesture` changed. */
  cancel(gesture: number): void;
}

interface Props {
  project: ProjectView;
  /** The selected track's and clip's IDs. The clip is the one in the Notes tab. */
  selectedTrack: string | null;
  selectedClip: string | null;
  editor: ClipEditor;
  /** Where the playhead is, between the engine's reports. */
  clock: PlayheadClock;
  /** Draws it once a screen frame, with the piano roll. */
  frames: FrameLoop;
  /**
   * The track headers' scrolling element. It scrolls with the timeline, and
   * the wheel over it scrolls the timeline, so each header stays level with
   * its track.
   */
  headers: RefObject<HTMLElement | null>;
  onSelectTrack: (track: string) => void;
  /** Selects a clip, and its track. */
  onSelectClip: (track: string, clip: string) => void;
  /** Selects a clip and opens it in the Notes tab: a double-click. */
  onOpenClip: (track: string, clip: string) => void;
  /** Draws the layers. Canvas 2D unless a test says otherwise. */
  createRenderer?: TimelineRendererFactory;
}

/** Each zoom button press zooms by this much. */
const ZOOM_STEP = 1.25;
/** How much a wheel's `deltaY` zooms with ⌘ or Ctrl (a trackpad pinch) held. */
const WHEEL_ZOOM_RATE = 0.01;
/** How far the pointer moves before a press becomes a drag, so a click never nudges a clip. */
const DRAG_THRESHOLD_PIXELS = 3;

/** A drag in progress: of a clip, or drawing one. */
interface DragState {
  /** Where the pointer went down, in CSS pixels in the timeline. */
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
 * The timeline: every track's clips, each with a preview of its notes, under
 * a bar ruler, with the playhead. It's drawn on three stacked canvases like
 * the piano roll, each redrawn only when it needs to be (RFC-003, "Clips on
 * the timeline").
 */
export function Timeline({
  project,
  selectedTrack,
  selectedClip,
  editor,
  clock,
  frames,
  headers,
  onSelectTrack,
  onSelectClip,
  onOpenClip,
  createRenderer = createTimelineRenderer,
}: Props) {
  const containerRef = useRef<HTMLDivElement>(null);
  const gridRef = useRef<HTMLCanvasElement>(null);
  const clipsRef = useRef<HTMLCanvasElement>(null);
  const topRef = useRef<HTMLCanvasElement>(null);
  const [scene] = useState(() => new TimelineScene());
  const [snap, setSnap] = useState<ClipSnap>(DEFAULT_CLIP_SNAP);
  const scheme = useColourScheme();
  // The pointer and keyboard handlers read the latest of these, including
  // those added to the window for the length of a drag.
  const latest = useRef({ project, selectedClip, editor, snap });
  const dragging = useRef<DragState | null>(null);

  useEffect(() => {
    latest.current = { project, selectedClip, editor, snap };
  }, [project, selectedClip, editor, snap]);

  useEffect(() => scene.setProject(project), [scene, project]);
  useEffect(
    () => scene.setSelection(selectedTrack, selectedClip),
    [scene, selectedTrack, selectedClip],
  );

  // A drag ends if the timeline goes away mid-drag.
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

  // The renderer, and drawing once a screen frame. The headers follow the
  // timeline's scrolling as it draws. A new colour scheme makes a new
  // renderer, which reads the new colours.
  useEffect(() => {
    const grid = gridRef.current;
    const clips = clipsRef.current;
    const top = topRef.current;
    if (!grid || !clips || !top) return;
    scene.setRenderer(createRenderer({ grid, clips, top }));
    const stop = frames.add((now) => {
      scene.draw(clock.at(now));
      const view = scene.getView();
      const list = headers.current;
      if (view && list && list.scrollTop !== view.scrollY) list.scrollTop = view.scrollY;
    });
    return () => {
      stop();
      scene.setRenderer(null);
    };
  }, [scene, clock, frames, headers, createRenderer, scheme]);

  // Scroll with the wheel or trackpad, over the timeline or the headers;
  // zoom time with ⌘ or a pinch. Not React handlers: they have to be able to
  // preventDefault. When the headers scroll themselves, to show a control
  // that gets focus say, the timeline follows.
  useEffect(() => {
    const container = containerRef.current;
    const list = headers.current;
    if (!container) return;
    const onWheel = (event: WheelEvent) => {
      event.preventDefault();
      const box = container.getBoundingClientRect();
      const x = Math.max(0, event.clientX - box.left);
      const zoom = Math.exp(-event.deltaY * WHEEL_ZOOM_RATE);
      scene.changeView((view) => {
        if (event.ctrlKey || event.metaKey) return zoomTime(view, zoom, x);
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
    const onHeadersScroll = () => {
      if (!list) return;
      const scrollY = list.scrollTop;
      scene.changeView((view) => (view.scrollY === scrollY ? view : { ...view, scrollY }));
    };
    container.addEventListener("wheel", onWheel, { passive: false });
    list?.addEventListener("wheel", onWheel, { passive: false });
    list?.addEventListener("scroll", onHeadersScroll);
    return () => {
      container.removeEventListener("wheel", onWheel);
      list?.removeEventListener("wheel", onWheel);
      list?.removeEventListener("scroll", onHeadersScroll);
    };
  }, [scene, headers]);

  /** Where a pointer event is, in CSS pixels from the timeline's top left. */
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
   * The grid step for an edit: the lines on screen by default, 1 tick with
   * snapping off, or while ⌘ is held.
   */
  const stepFor = (event: { metaKey: boolean }) => {
    const { project, snap } = latest.current;
    const view = scene.getView();
    if (event.metaKey || !view) return 1;
    return clipSnapStep(snap, project.ticksPerQuarter, project.beatsPerBar, view.pixelsPerTick);
  };

  /** Where the pointer is: a tick, and a track's index in the order. */
  const at = (view: TimelineViewport, x: number, y: number) => ({
    tick: xToTick(view, x),
    track: yToTrack(view, y),
  });

  // Press on a clip to select it and drag it: its body moves it, along its
  // track or onto another; its right edge resizes it. Press on empty space
  // on a track to select the track, and drag to draw a clip that long.
  const onPointerDown = (event: PointerEvent<HTMLDivElement>) => {
    const view = scene.getView();
    if (event.button !== 0 || dragging.current || !view) return;
    const { x, y } = pointOf(event);
    if (!inTracks(view, y)) return;
    const { project } = latest.current;
    const press = at(view, x, y);
    const hit = scene.hitTest(x, y);
    const track = project.tracks[press.track];
    if (!hit && !track) return;
    event.preventDefault();
    containerRef.current?.focus({ preventScroll: true });
    // So the drag still ends if the pointer is released outside the window.
    try {
      event.currentTarget.setPointerCapture(event.pointerId);
    } catch {
      // No such pointer (a synthetic event): the window's events still arrive.
    }
    if (hit) {
      const from = { track: hit.track, start: hit.clip.start, length: hit.clip.length };
      pressClip(hit.clip.id, hit.part, from, press, x, y);
    } else if (track) drawClip(track.id, press, x, y);
  };

  const pressClip = (
    id: string,
    part: "body" | "end",
    from: Span,
    press: { tick: number; track: number },
    x: number,
    y: number,
  ) => {
    const { project } = latest.current;
    onSelectClip(project.tracks[from.track].id, id);
    const gesture = nextGesture();
    let last = from;
    let sent = false;
    // One drag's changes go as `SetClips` under one gesture, so it's one
    // undo step. It never trims the clips it lands on: clips may overlap.
    startDrag(x, y, {
      move: (x, y, move) => {
        const view = scene.getView();
        if (!view) return;
        const { project, editor } = latest.current;
        const now = at(view, x, y);
        const step = stepFor(move);
        const next =
          part === "end"
            ? resizeClip(
                from,
                press.tick,
                now.tick,
                step,
                minClipLength(step, project.ticksPerQuarter),
              )
            : moveClip(
                from,
                press.tick,
                press.track,
                now.tick,
                now.track,
                step,
                project.tracks.length,
              );
        const track = project.tracks[next.track];
        if (sameSpan(next, last) || !track) return;
        editor.set([{ id, track: track.id, start: next.start, length: next.length }], gesture);
        last = next;
        sent = true;
      },
      cancel: () => {
        if (sent) latest.current.editor.cancel(gesture);
      },
    });
  };

  /** Drags out a new clip on track `trackId`. It's only added once the drag ends. */
  const drawClip = (
    trackId: string,
    press: { tick: number; track: number },
    x: number,
    y: number,
  ) => {
    onSelectTrack(trackId);
    let drawing: Span | null = null;
    startDrag(x, y, {
      move: (x, _y, move) => {
        const view = scene.getView();
        if (!view) return;
        const step = stepFor(move);
        const minLength = minClipLength(step, latest.current.project.ticksPerQuarter);
        drawing = drawnClip(press.track, press.tick, xToTick(view, x), step, minLength);
        scene.setDrawing(drawing);
      },
      end: () => {
        scene.setDrawing(null);
        if (drawing) {
          latest.current.editor.add(trackId, crypto.randomUUID(), drawing.start, drawing.length);
        }
      },
      cancel: () => scene.setDrawing(null),
    });
  };

  // Shows what pressing would do: resize at a clip's right edge.
  const onHover = (event: PointerEvent<HTMLDivElement>) => {
    const container = containerRef.current;
    if (!container || dragging.current) return;
    const { x, y } = pointOf(event);
    container.style.cursor = scene.hitTest(x, y)?.part === "end" ? "ew-resize" : "";
  };

  // Double-click a clip to open it in the Notes tab, or empty space on a
  // track for a one-bar clip.
  const onDoubleClick = (event: { clientX: number; clientY: number; metaKey: boolean }) => {
    const view = scene.getView();
    if (!view) return;
    const { x, y } = pointOf(event);
    if (!inTracks(view, y)) return;
    const { project, editor } = latest.current;
    const hit = scene.hitTest(x, y);
    if (hit) {
      onOpenClip(project.tracks[hit.track].id, hit.clip.id);
      return;
    }
    const press = at(view, x, y);
    const track = project.tracks[press.track];
    if (!track) return;
    const bar = project.ticksPerQuarter * project.beatsPerBar;
    const clip = oneBarClip(press.track, press.tick, stepFor(event), bar);
    editor.add(track.id, crypto.randomUUID(), clip.start, clip.length);
  };

  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (dragging.current) return;
    if (event.key !== "Backspace" && event.key !== "Delete") return;
    const { project, selectedClip, editor } = latest.current;
    const exists = project.tracks.some((track) => track.clips.some((c) => c.id === selectedClip));
    if (!selectedClip || !exists) return;
    event.preventDefault();
    editor.remove([selectedClip]);
  };

  const zoomBy = (factor: number) =>
    scene.changeView((view) => zoomTime(view, factor, view.width / 2));

  return (
    <section className="timeline" aria-label="Timeline">
      <div
        className="timeline-canvas"
        ref={containerRef}
        role="application"
        aria-label="Clips"
        tabIndex={0}
        onPointerDown={onPointerDown}
        onPointerMove={onHover}
        onDoubleClick={onDoubleClick}
        onKeyDown={onKeyDown}
      >
        <canvas ref={gridRef} aria-hidden="true" />
        <canvas ref={clipsRef} aria-hidden="true" />
        <canvas ref={topRef} aria-hidden="true" />
      </div>
      <div className="timeline-tools">
        <div className="snap">
          <span aria-hidden="true">Snap</span>
          <select
            aria-label="Clip snap"
            value={snap}
            onChange={(event) => setSnap(event.target.value as ClipSnap)}
          >
            {CLIP_SNAPS.map((option) => (
              <option key={option} value={option}>
                {CLIP_SNAP_NAMES[option]}
              </option>
            ))}
          </select>
        </div>
        <div className="zoom" role="group" aria-label="Timeline zoom">
          <button
            type="button"
            aria-label="Zoom out timeline"
            onClick={() => zoomBy(1 / ZOOM_STEP)}
          >
            −
          </button>
          <span>Time</span>
          <button type="button" aria-label="Zoom in timeline" onClick={() => zoomBy(ZOOM_STEP)}>
            +
          </button>
        </div>
      </div>
    </section>
  );
}
