import {
  type KeyboardEvent,
  type PointerEvent,
  type Ref,
  type RefObject,
  useEffect,
  useImperativeHandle,
  useRef,
  useState,
} from "react";
import type { ClipPosition, PastedClip, ProjectView } from "../backend";
import { Follow } from "../follow";
import type { FrameLoop } from "../frameLoop";
import type { PlayheadClock } from "../pianoRoll/playhead";
import { snapDown } from "../pianoRoll/snap";
import { useColourScheme } from "../useColourScheme";
import { nextGesture } from "../useGesture";
import { createTimelineRenderer } from "./canvasRenderer";
import {
  type ClipOnTrack,
  type CopiedClip,
  copyClips,
  duplicateClips,
  pasteClips,
} from "./clipboard";
import {
  type ClipHit,
  type LoopBars,
  type Span,
  drawnClip,
  moveClips,
  oneBarClip,
  resizeClip,
  rulerClick,
  rulerLoop,
  sameSpan,
} from "./editing";
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
import {
  RULER_HEIGHT,
  type TimelineViewport,
  inTracks,
  xToTick,
  yToTrack,
  zoomTime,
} from "./viewport";
import "./Timeline.css";

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
  /** Adds copies of clips, a paste or a duplicate, and selects them once they're in. */
  paste(clips: PastedClip[]): void;
  /** Puts back everything `gesture` changed. */
  cancel(gesture: number): void;
}

/** What the Edit menu does on the timeline. */
export interface TimelineHandle {
  /** Copies the selected clips. */
  copy(): void;
  /**
   * Pastes the copied clips on the selected track at the playhead, snapped
   * to the grid, and selects them.
   */
  paste(): void;
  /** Puts a copy of the selected clips straight after them, and selects it. */
  duplicate(): void;
}

/** What the ruler does: set the loop region, and move the play start or jump. */
export interface RulerActions {
  /** Sets the loop region in whole bars. One drag's changes share a `gesture`. */
  loop(startBar: number, bars: number, gesture: number): void;
  /** Moves the play start while stopped, or jumps there while playing. */
  locate(ticks: number): void;
}

interface Props {
  ref?: Ref<TimelineHandle>;
  project: ProjectView;
  /**
   * The selected track's ID, and the clips picked on the timeline, in the
   * order they were picked. They're highlighted, and Backspace, Copy and
   * Duplicate act on them.
   */
  selectedTrack: string | null;
  selectedClips: readonly string[];
  editor: ClipEditor;
  ruler: RulerActions;
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
  /** Selects these clips, and the last one's track. */
  onSelectClips: (clips: string[]) => void;
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
  ref,
  project,
  selectedTrack,
  selectedClips,
  editor,
  ruler,
  clock,
  frames,
  headers,
  onSelectTrack,
  onSelectClips,
  onOpenClip,
  createRenderer = createTimelineRenderer,
}: Props) {
  const containerRef = useRef<HTMLDivElement>(null);
  const gridRef = useRef<HTMLCanvasElement>(null);
  const clipsRef = useRef<HTMLCanvasElement>(null);
  const topRef = useRef<HTMLCanvasElement>(null);
  const [scene] = useState(() => new TimelineScene());
  // Scrolling or editing stops it following the playhead until the next Play.
  const [follow] = useState(() => new Follow(clock));
  const [snap, setSnap] = useState<ClipSnap>(DEFAULT_CLIP_SNAP);
  const scheme = useColourScheme();
  // Every edit stops it following the playhead until the next Play.
  const editing: ClipEditor = {
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
    paste: (...args) => {
      follow.pause();
      editor.paste(...args);
    },
    cancel: (gesture) => editor.cancel(gesture),
  };
  // The pointer and keyboard handlers read the latest of these, including
  // those added to the window for the length of a drag.
  const latest = useRef({ project, selectedTrack, selectedClips, editor: editing, ruler, snap });
  // What was copied is the timeline's own, like the piano roll's notes: it
  // isn't part of the project.
  const clipboard = useRef<CopiedClip[]>([]);
  const dragging = useRef<DragState | null>(null);

  useEffect(() => {
    latest.current = { project, selectedTrack, selectedClips, editor: editing, ruler, snap };
  });

  useEffect(() => scene.setProject(project), [scene, project]);
  useEffect(
    () => scene.setSelection(selectedTrack, new Set(selectedClips)),
    [scene, selectedTrack, selectedClips],
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
      const playhead = clock.at(now);
      if (follow.following()) scene.follow(playhead);
      scene.draw(playhead);
      const view = scene.getView();
      const list = headers.current;
      if (view && list && list.scrollTop !== view.scrollY) list.scrollTop = view.scrollY;
    });
    return () => {
      stop();
      scene.setRenderer(null);
    };
  }, [scene, clock, follow, frames, headers, createRenderer, scheme]);

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
      follow.pause();
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
  }, [scene, follow, headers]);

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

  /**
   * The selected clips as they are now, with their tracks' indices, in the
   * order they were picked. Any that have gone are left out.
   */
  const selection = (): ClipOnTrack[] => {
    const { project, selectedClips } = latest.current;
    return selectedClips.flatMap((id) => {
      for (const [track, view] of project.tracks.entries()) {
        const clip = view.clips.find((c) => c.id === id);
        if (clip) return [{ track, clip }];
      }
      return [];
    });
  };

  /** Adds clip `id` to the selection, or takes it out if it's there. */
  const toggle = (id: string) => {
    const { selectedClips } = latest.current;
    onSelectClips(
      selectedClips.includes(id)
        ? selectedClips.filter((other) => other !== id)
        : [...selectedClips, id],
    );
  };

  /** Where the pointer is: a tick, and a track's index in the order. */
  const at = (view: TimelineViewport, x: number, y: number) => ({
    tick: xToTick(view, x),
    track: yToTrack(view, y),
  });

  // Press on a clip to select it and drag it: its body moves the selection,
  // along its tracks or onto others; its right edge resizes it. Press on
  // empty space on a track to select the track, and drag to draw a clip that
  // long. Shift-click a clip to add it to the selection or take it out, or
  // Shift-drag on empty space to box-select more. On the ruler, click to
  // move the play start (or jump, while playing), and drag to set the loop
  // region.
  const onPointerDown = (event: PointerEvent<HTMLDivElement>) => {
    const view = scene.getView();
    if (event.button !== 0 || dragging.current || !view) return;
    const { x, y } = pointOf(event);
    const onRuler = y >= 0 && y < RULER_HEIGHT;
    if (!onRuler && !inTracks(view, y)) return;
    const { project } = latest.current;
    const press = at(view, x, y);
    const hit = onRuler ? null : scene.hitTest(x, y);
    const track = project.tracks[press.track];
    if (!onRuler && !hit && !track) return;
    event.preventDefault();
    containerRef.current?.focus({ preventScroll: true });
    // So the drag still ends if the pointer is released outside the window.
    try {
      event.currentTarget.setPointerCapture(event.pointerId);
    } catch {
      // No such pointer (a synthetic event): the window's events still arrive.
    }
    if (onRuler) {
      pressRuler(press.tick, x, y, event);
      return;
    }
    if (hit && event.shiftKey) toggle(hit.clip.id);
    else if (hit) pressClip(hit, press, x, y);
    else if (event.shiftKey) boxSelect(x, y);
    else if (track) drawClip(track.id, press, x, y);
  };

  const pressClip = (
    hit: ClipHit,
    press: { tick: number; track: number },
    x: number,
    y: number,
  ) => {
    const { id } = hit.clip;
    const wasSelected = latest.current.selectedClips.includes(id);
    if (!wasSelected) onSelectClips([id]);
    const from: Span = { track: hit.track, start: hit.clip.start, length: hit.clip.length };
    // The body moves every selected clip together; the right edge resizes
    // just this one.
    const group =
      hit.part === "body" && wasSelected
        ? selection().map(({ track, clip }) => ({
            id: clip.id,
            track,
            start: clip.start,
            length: clip.length,
          }))
        : [{ id, ...from }];
    const gesture = nextGesture();
    let last = group;
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
        const minLength = minClipLength(step, project.ticksPerQuarter);
        const next =
          hit.part === "end"
            ? [{ id, ...resizeClip(from, press.tick, now.tick, step, minLength) }]
            : moveClips(
                group,
                from,
                press.tick,
                press.track,
                now.tick,
                now.track,
                step,
                project.tracks.length,
              );
        if (next.every((span, i) => sameSpan(span, last[i]))) return;
        if (next.some((span) => !project.tracks[span.track])) return;
        const clips = next.map(({ id, track, start, length }) => ({
          id,
          track: project.tracks[track].id,
          start,
          length,
        }));
        editor.set(clips, gesture);
        last = next;
        sent = true;
      },
      // A click on a clip of a larger selection selects just that clip.
      end: (moved) => {
        if (!moved && wasSelected && hit.part === "body") onSelectClips([id]);
      },
      cancel: () => {
        if (sent) latest.current.editor.cancel(gesture);
      },
    });
  };

  /** Drags out a box from `x`, `y`, adding the clips it touches to the selection. */
  const boxSelect = (x: number, y: number) => {
    const before = latest.current.selectedClips;
    let last = before;
    startDrag(x, y, {
      move: (toX, toY) => {
        const box = {
          x: Math.min(x, toX),
          y: Math.min(y, toY),
          width: Math.abs(toX - x),
          height: Math.abs(toY - y),
        };
        scene.setBox(box);
        const added = scene.clipsIn(box).filter((id) => !before.includes(id));
        const next = [...before, ...added];
        if (next.length === last.length && next.every((id, i) => id === last[i])) return;
        onSelectClips(next);
        last = next;
      },
      end: () => scene.setBox(null),
      cancel: () => {
        scene.setBox(null);
        onSelectClips([...before]);
      },
    });
  };

  /**
   * A press on the ruler at `pressTick`. Dragging sets the loop region, from
   * bar line to bar line, as one undo step, and Esc puts it back. A click
   * without dragging moves the play start to the nearest grid line, or
   * jumps there while playing.
   */
  const pressRuler = (pressTick: number, x: number, y: number, press: { metaKey: boolean }) => {
    const gesture = nextGesture();
    let last: LoopBars | null = null;
    startDrag(x, y, {
      move: (x) => {
        const view = scene.getView();
        if (!view) return;
        const { project, ruler } = latest.current;
        const bar = project.ticksPerQuarter * project.beatsPerBar;
        const next = rulerLoop(pressTick, xToTick(view, x), bar);
        if (last && last.startBar === next.startBar && last.bars === next.bars) return;
        ruler.loop(next.startBar, next.bars, gesture);
        last = next;
      },
      end: (moved) => {
        if (!moved) latest.current.ruler.locate(rulerClick(pressTick, stepFor(press)));
      },
      cancel: () => {
        if (last) latest.current.editor.cancel(gesture);
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
    if (event.key === "Escape") {
      if (latest.current.selectedClips.length > 0) onSelectClips([]);
      return;
    }
    if (event.key !== "Backspace" && event.key !== "Delete") return;
    const ids = selection().map(({ clip }) => clip.id);
    if (ids.length === 0) return;
    event.preventDefault();
    // One command, so one undo step.
    latest.current.editor.remove(ids);
  };

  /** Adds pasted or duplicated clips. The app selects them once they're in. */
  const land = (copies: ClipOnTrack[]) => {
    const { project, editor } = latest.current;
    editor.paste(
      copies.map(({ track, clip }) => ({
        id: clip.id,
        track: project.tracks[track].id,
        start: clip.start,
        length: clip.length,
        notes: clip.notes,
      })),
    );
  };

  useImperativeHandle(ref, () => ({
    copy() {
      const clips = selection();
      if (clips.length > 0) clipboard.current = copyClips(clips);
    },
    paste() {
      const { project, selectedTrack } = latest.current;
      const track = project.tracks.findIndex((t) => t.id === selectedTrack);
      if (dragging.current || track < 0 || clipboard.current.length === 0) return;
      const at = Math.max(0, snapDown(clock.at(performance.now()), stepFor({ metaKey: false })));
      land(
        pasteClips(clipboard.current, at, track, project.tracks.length, () => crypto.randomUUID()),
      );
    },
    duplicate() {
      if (dragging.current) return;
      const copies = duplicateClips(selection(), () => crypto.randomUUID());
      if (copies.length > 0) land(copies);
    },
  }));

  const zoomBy = (factor: number) => {
    follow.pause();
    scene.changeView((view) => zoomTime(view, factor, view.width / 2));
  };

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
