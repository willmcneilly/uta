import {
  type KeyboardEvent,
  type PointerEvent,
  type Ref,
  useEffect,
  useImperativeHandle,
  useMemo,
  useRef,
  useState,
} from "react";
import type { ClipView, NoteView, ProjectView } from "../backend";
import { Menu } from "../design/Menu";
import { spacing } from "../design/tokens";
import { useTokenVersion } from "../design/tokenChanges";
import { ToolChip, ZoomKeys } from "../design/ToolChip";
import { Follow } from "../follow";
import { nextGesture } from "../useGesture";
import { createCanvas2DRenderer } from "./canvasRenderer";
import { duplicateNotes, pasteNotes } from "./clipboard";
import {
  type Drag,
  dragNote,
  dragVelocities,
  moveNotes,
  resizeNotes,
  sameNote,
  shiftNotes,
  transposeNotes,
} from "./editing";
import type { FrameLoop } from "../frameLoop";
import type { PlayheadClock } from "./playhead";
import type { RendererFactory } from "./renderer";
import { PianoRollScene } from "./scene";
import { DEFAULT_SNAP, SNAPS, type Snap, snapDown, snapStep } from "./snap";
import {
  KEYBOARD,
  type Lane,
  RULER_HEIGHT,
  type Rect,
  type Rows,
  VELOCITY_LANE_HEIGHT,
  drumLanes,
  noteArea,
  xToTick,
  yToPitch,
  zoomPitch,
  zoomTime,
} from "./viewport";
import "./PianoRoll.css";

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
  /**
   * Its track's drum lanes, bottom to top, from the outline, shown in place
   * of the keyboard. Left out for a synth track.
   */
  lanes?: readonly Lane[];
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
/** The cursor over each part of a note, and while dragging it. */
const NOTE_CURSORS = { body: "grab", start: "ew-resize", end: "ew-resize" } as const;
const DRAG_CURSORS = { body: "grabbing", start: "ew-resize", end: "ew-resize" } as const;
/** How far ↑ and ↓ move the selected notes, and with Shift held. */
const SEMITONE = 1;
const OCTAVE = 12;
/** The cursor while ⌥-dragging copies of notes, as the system shows for copying. */
const COPY_CURSOR = "copy";
/** How long a notice, such as notes a paste left out, stays up. */
const NOTICE_MS = 5000;

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

/** The keys held that change what a drag does: ⌥ copies, ⌘ turns snapping off. */
interface Modifiers {
  altKey: boolean;
  metaKey: boolean;
}

/** What a drag does as the pointer moves, when it's released, and on Esc. */
interface DragHandlers {
  /**
   * The pointer at `x`, `y`, once it has moved far enough to be a drag, with
   * the keys held. Pressing or letting go of ⌥ or ⌘ mid-drag calls it again,
   * where the pointer is.
   */
  move(x: number, y: number, keys: Modifiers): void;
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
  lanes,
  editor,
  clock,
  frames,
  createRenderer = createCanvas2DRenderer,
}: Props) {
  // The scene keeps its rows while new ones have the same lanes, so a new
  // outline doesn't make it rebuild anything.
  const rows: Rows = useMemo(() => (lanes ? drumLanes(lanes) : KEYBOARD), [lanes]);
  // Something worth saying about the last edit in a clip, such as notes a
  // paste left out. It's only shown while that clip is.
  const [notice, setNotice] = useState<{ clip: string; text: string } | null>(null);
  const containerRef = useRef<HTMLDivElement>(null);
  const gridRef = useRef<HTMLCanvasElement>(null);
  const notesRef = useRef<HTMLCanvasElement>(null);
  const topRef = useRef<HTMLCanvasElement>(null);
  const [scene] = useState(() => new PianoRollScene());
  // Scrolling or editing stops it following the playhead until the next Play.
  const [follow] = useState(() => new Follow(clock));
  const [snap, setSnap] = useState<Snap>(DEFAULT_SNAP);
  const tokens = useTokenVersion();
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
  const latest = useRef({ project, clip, rows, editor: editing, snap });
  // Which notes are selected, and what was copied, are the piano roll's own:
  // neither is part of the project.
  const selected = useRef<ReadonlySet<string>>(new Set());
  const clipboard = useRef<NoteView[]>([]);
  const dragging = useRef<DragState | null>(null);
  // The last arrow-key move. Its notes overlap what they pass over until
  // they're deselected or something else is done, then they trim what they
  // cover, as part of that move's undo step.
  const untrimmed = useRef<{ ids: string[]; gesture: number; editor: NoteEditor } | null>(
    null,
  );

  useEffect(() => {
    latest.current = { project, clip, rows, editor: editing, snap };
  });

  useEffect(() => scene.setProject(project, clip, rows), [scene, project, clip, rows]);

  // A notice goes after a while.
  useEffect(() => {
    if (notice === null) return;
    const timer = setTimeout(() => setNotice(null), NOTICE_MS);
    return () => clearTimeout(timer);
  }, [notice]);

  // Another clip's notes aren't selected. What was copied stays, to paste
  // into this one.
  useEffect(() => {
    trimMoved();
    selected.current = new Set();
    scene.setSelection(selected.current);
  }, [scene, clip.id]);

  // A drag ends if the piano roll goes away mid-drag, and notes moved with
  // the arrow keys trim what they cover.
  useEffect(
    () => () => {
      dragging.current?.stop();
      trimMoved();
    },
    [],
  );

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
  // scheme, or a change from the tuning panel, makes a new renderer, which
  // reads the new tokens.
  useEffect(() => {
    const grid = gridRef.current;
    const notes = notesRef.current;
    const top = topRef.current;
    if (!grid || !notes || !top) return;
    scene.setRenderer(createRenderer({ grid, notes, top }));
    const stop = frames.add((now) => {
      const playhead = clock.at(now);
      if (follow.following()) scene.follow(playhead);
      scene.draw(playhead, clock.isPlaying());
    });
    return () => {
      stop();
      scene.setRenderer(null);
    };
  }, [scene, clock, follow, frames, createRenderer, tokens]);

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
    trimMoved();
    selected.current = new Set(ids);
    scene.setSelection(selected.current);
  };

  /**
   * Trims what the last arrow-key move's notes cover, as part of its gesture.
   * Rust ignores it if anything else has changed the project since, an undo
   * included, so it never trims notes that have moved back.
   */
  function trimMoved() {
    const moved = untrimmed.current;
    if (!moved) return;
    untrimmed.current = null;
    moved.editor.trim(moved.ids, moved.gesture);
  }

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

  const setCursor = (cursor: string) => {
    const container = containerRef.current;
    if (container) container.style.cursor = cursor;
  };

  /** Follows the pointer until it's released, or Esc is pressed. */
  const startDrag = (x: number, y: number, handlers: DragHandlers) => {
    let point = { x, y };
    const onMove = (move: globalThis.PointerEvent) => {
      const state = dragging.current;
      if (!state) return;
      point = pointOf(move);
      if (!state.moving) {
        if (Math.hypot(point.x - state.x, point.y - state.y) < DRAG_THRESHOLD_PIXELS) return;
        state.moving = true;
      }
      handlers.move(point.x, point.y, move);
    };
    const onKey = (key: globalThis.KeyboardEvent) => {
      const state = dragging.current;
      if (!state) return;
      if (key.key === "Alt" || key.key === "Meta") {
        if (state.moving) handlers.move(point.x, point.y, key);
        return;
      }
      if (key.key !== "Escape" || key.type !== "keydown") return;
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
    window.addEventListener("keyup", onKey);
    dragging.current = {
      x,
      y,
      moving: false,
      stop: () => {
        window.removeEventListener("pointermove", onMove);
        window.removeEventListener("pointerup", onUp);
        window.removeEventListener("pointercancel", onUp);
        window.removeEventListener("keydown", onKey);
        window.removeEventListener("keyup", onKey);
        dragging.current = null;
        setCursor("");
        // The pointer may have been released anywhere; the next move over
        // the piano roll marks what's under it again.
        scene.setHovered(null);
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
    trimMoved();
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

    const group = selectedNotes();

    // An end resizes every selected note together.
    if (hit.part !== "body") {
      const drag: Drag = { kind: hit.part, from, tick, pitch };
      const changes = noteChanges(gesture, group, false);
      startDrag(x, y, {
        move: (x, _y, keys) => {
          const view = scene.getView();
          if (!view) return;
          const next = resizeNotes(
            group,
            drag,
            xToTick(view, x),
            stepFor(keys),
            latest.current.clip.start,
          );
          changes.send(next);
        },
        end: changes.end,
        cancel: changes.cancel,
      });
      return;
    }

    // The body moves every selected note together, or with ⌥ held, copies
    // of them, leaving the notes where they were. Pressing or letting go of
    // ⌥ mid-drag swaps one for the other: it puts back what the drag did so
    // far, and starts again as a new gesture, so what's left when the drag
    // ends is one undo step.
    setCursor(DRAG_CURSORS.body);
    const drag: Drag = { kind: "move", from, tick, pitch };
    // Chosen here, before Rust applies the command (RFC-002, "The shared model", point 3).
    const copies = group.map((note) => ({ ...note, id: crypto.randomUUID() }));
    const dragged = group.findIndex((note) => note.id === from.id);
    let copying: boolean | null = null;
    let changes = noteChanges(gesture, group, false);
    let heard = from.pitch;
    startDrag(x, y, {
      move: (x, y, keys) => {
        const view = scene.getView();
        if (!view) return;
        const { clip, editor } = latest.current;
        const notes = keys.altKey ? copies : group;
        const next = moveNotes(
          notes,
          drag,
          xToTick(view, x),
          yToPitch(view, y),
          stepFor(keys),
          clip.start,
          latest.current.rows,
        );
        if (keys.altKey !== copying) {
          if (copying !== null) changes.cancel();
          const swapped = copying === null ? gesture : nextGesture();
          copying = keys.altKey;
          setCursor(copying ? COPY_CURSOR : DRAG_CURSORS.body);
          select(notes.map((note) => note.id));
          if (copying) {
            // One AddNotes, which the drag's later steps fold into.
            editor.add(next, swapped);
            changes = noteChanges(swapped, next, true);
          } else {
            changes = noteChanges(swapped, group, false);
          }
        }
        const moved = next[dragged];
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
      cancel: () => {
        changes.cancel();
        select(group.map((note) => note.id));
      },
    });
  };

  /** Where `drag` (of one note) puts it with the pointer at `x`, `y`. */
  const dragAt = (drag: Drag, x: number, y: number, keys: Modifiers): NoteView => {
    const view = scene.getView();
    if (!view) return drag.from;
    return dragNote(
      drag,
      xToTick(view, x),
      yToPitch(view, y),
      stepFor(keys),
      latest.current.clip.start,
      latest.current.rows,
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

  // Shows what pressing would do: the note under the pointer, or whose
  // velocity stem it's over, is drawn heavier, and the cursor says whether
  // it would move, resize at the note's ends, or change the velocity.
  const onHover = (event: PointerEvent<HTMLDivElement>) => {
    if (dragging.current) return;
    const { x, y } = pointOf(event);
    if (scene.inVelocityLane(x, y)) {
      const bar = scene.hitVelocity(x);
      scene.setHovered(bar);
      setCursor(bar ? "ns-resize" : "");
      return;
    }
    const hit = scene.hitTest(x, y);
    scene.setHovered(hit?.note ?? null);
    setCursor(hit ? NOTE_CURSORS[hit.part] : "");
  };

  const onLeave = () => {
    if (!dragging.current) scene.setHovered(null);
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
    if (event.key.startsWith("Arrow")) {
      nudge(event);
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

  /**
   * The arrow keys move the selected notes: ↑ and ↓ a semitone (a row on a
   * drum track), or an octave (12 rows) with Shift, playing the first of
   * them where it lands; ← and → a grid
   * step (a sixteenth with snapping off). Each press is one undo step. The
   * notes trim what they cover only once they're deselected or something
   * else is done, so moving through a chord leaves it whole. With ⌘, Ctrl or
   * ⌥ held the keys are left to the app's own shortcuts.
   */
  const nudge = (event: KeyboardEvent<HTMLDivElement>) => {
    if (event.metaKey || event.ctrlKey || event.altKey) return;
    const notes = selectedNotes();
    const { project, rows, editor, snap } = latest.current;
    let next: NoteView[];
    switch (event.key) {
      case "ArrowUp":
      case "ArrowDown": {
        const step = event.shiftKey ? OCTAVE : SEMITONE;
        next = transposeNotes(notes, event.key === "ArrowUp" ? step : -step, rows);
        break;
      }
      case "ArrowLeft":
      case "ArrowRight": {
        const step = newNoteLength(snap, project.ticksPerQuarter);
        next = shiftNotes(notes, event.key === "ArrowRight" ? step : -step);
        break;
      }
      default:
        return;
    }
    if (notes.length === 0) return;
    event.preventDefault();
    if (next.every((note, i) => sameNote(note, notes[i]))) return;
    const gesture = nextGesture();
    editor.set(next, gesture);
    untrimmed.current = { ids: next.map((note) => note.id), gesture, editor };
    const first = earliest(next);
    if (first.pitch !== earliest(notes).pitch) editor.audition(first.pitch, first.velocity);
  };

  /** Adds pasted or duplicated notes, trims what they land on, and selects them. */
  const land = (notes: NoteView[]) => {
    trimMoved();
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
      const { project, clip, rows, snap } = latest.current;
      const step = snapStep(snap, project.ticksPerQuarter);
      const at = snapDown(clock.at(performance.now()), step);
      const notes = pasteNotes(clipboard.current, at, clip.start, () =>
        crypto.randomUUID(),
      );
      // On a drum track, only the notes on its lanes come across: Rust
      // refuses a note on any other pitch (RFC-006, "In the project").
      const kept = notes.filter((note) => rows.row(note.pitch) >= 0);
      const left = notes.length - kept.length;
      setNotice(left > 0 ? { clip: clip.id, text: leftOut(left, kept.length) } : null);
      if (kept.length > 0) land(kept);
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
        onPointerLeave={onLeave}
        onDoubleClick={onDoubleClick}
        onKeyDown={onKeyDown}
      >
        <canvas ref={gridRef} aria-hidden="true" />
        <canvas ref={notesRef} aria-hidden="true" />
        <canvas ref={topRef} aria-hidden="true" />
      </div>
      {/* Above the velocity lane, so it never hides a bar. */}
      <div className="piano-roll-tools" style={{ bottom: VELOCITY_LANE_HEIGHT + 8 }}>
        <ToolChip label="Snap">
          <Menu aria-label="Snap" value={snap} onChange={(event) => setSnap(event.target.value as Snap)}>
            {SNAPS.map((option) => (
              <option key={option} value={option}>
                {option === "off" ? "Off" : option}
              </option>
            ))}
          </Menu>
        </ToolChip>
        <ToolChip role="group" aria-label="Zoom" className="zoom">
          <ZoomKeys
            label="Time"
            what="time"
            onOut={() => zoomTimeBy(1 / ZOOM_STEP)}
            onIn={() => zoomTimeBy(ZOOM_STEP)}
          />
          {/* Drum lanes always fit the height, so they don't zoom. */}
          {!rows.lanes && (
            <ZoomKeys
              label="Pitch"
              what="pitch"
              onOut={() => zoomPitchBy(1 / ZOOM_STEP)}
              onIn={() => zoomPitchBy(ZOOM_STEP)}
            />
          )}
        </ToolChip>
      </div>
      {/* Over the keyboard's place, lined up with the lanes, which share out
          the height between the ruler and the velocity lane (provisional:
          D-23). Top to bottom, so the kick is at the bottom. */}
      {rows.lanes && (
        <div
          className="drum-lanes"
          role="group"
          aria-label="Drum sounds"
          style={{ top: RULER_HEIGHT, bottom: VELOCITY_LANE_HEIGHT, width: rows.width }}
        >
          {[...rows.lanes].reverse().map((lane) => (
            <button
              key={lane.pitch}
              type="button"
              className="drum-lane"
              title={`Play the ${lane.name.toLowerCase()}`}
              onClick={() => editor.audition(lane.pitch, NEW_NOTE_VELOCITY)}
            >
              {lane.name}
            </button>
          ))}
        </div>
      )}
      {notice?.clip === clip.id && (
        <p
          className="piano-roll-notice"
          role="status"
          style={{ left: rows.width + spacing.space4, bottom: VELOCITY_LANE_HEIGHT + spacing.space4 }}
        >
          {notice.text}
        </p>
      )}
    </section>
  );
}

/** What a paste onto a drum track says about the notes it left out. */
function leftOut(left: number, kept: number): string {
  const notes = left === 1 ? "1 note" : `${left} notes`;
  const where = left === 1 ? "isn't on one of the drum kit's rows" : "aren't on the drum kit's rows";
  return kept > 0 ? `Left out ${notes} that ${where}.` : `Nothing pasted: the ${notes} ${where}.`;
}

/** The note that starts first, and the lowest of those that start together. */
function earliest(notes: readonly NoteView[]): NoteView {
  return notes.reduce((first, note) =>
    note.start < first.start || (note.start === first.start && note.pitch < first.pitch)
      ? note
      : first,
  );
}

/** A drawn note is one grid step long, or a sixteenth with snapping off. */
function newNoteLength(snap: Snap, ticksPerQuarter: number): number {
  return snap === "off" ? ticksPerQuarter / 4 : snapStep(snap, ticksPerQuarter);
}
