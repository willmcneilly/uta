import {
  type KeyboardEvent,
  type MouseEvent,
  type PointerEvent,
  useEffect,
  useRef,
  useState,
} from "react";
import { nextGesture } from "../useGesture";
import type { NumberScale } from "./numberScale";

// What it means to set a number by dragging, from the keyboard and by
// reset, for every control that sets one (RFC-005, part 7), apart from how
// the control is drawn. The fader draws it today.
//
// During a drag the control draws its own value, where the mouse has taken
// it, rather than the project's, so it keeps up however slowly Rust replies
// (RFC-004, part 3). That value is gesture state, like the text in a field
// you're typing in: it's gone when you let go, and the control shows the
// project's value again. Each step goes to `onChange` with the drag's
// gesture number, so the caller can send it through `DragSteps` and Rust
// undoes the whole drag as one step.

/** How far a Shift-drag moves a control for the same mouse movement, against a plain drag. */
export const FINE_DRAG = 0.1;

export interface NumberControlOptions {
  /** The project's value, as Rust last sent it. */
  value: number;
  /** What a reset sets it to, from the outline. */
  defaultValue: number;
  scale: NumberScale;
  /** The value with its unit, as shown and read out ("-6.0 dB"). */
  format: (value: number) => string;
  /** `gesture` is the same for every change in one drag, and undefined otherwise. */
  onChange: (value: number, gesture?: number) => void;
  /**
   * The value it shows during a drag, on each change, then null when the
   * drag ends, for anything that draws what the control shows.
   */
  onDrag?: (value: number | null) => void;
  /** How many positions an arrow key moves it. */
  keyStep?: number;
  /** How many positions Shift+arrow moves it. Ten arrow steps unless given. */
  bigStep?: number;
}

export interface NumberControl {
  /** The whole position it's drawn at: the drag's during a drag, the project's otherwise. */
  position: number;
  /** The value it shows, with its unit. */
  text: string;
  /**
   * Spread on the element that takes the pointer and focus. It's a slider to
   * assistive tech; the caller names it and gives its orientation.
   */
  props: {
    role: "slider";
    tabIndex: 0;
    "aria-valuemin": number;
    "aria-valuemax": number;
    "aria-valuenow": number;
    "aria-valuetext": string;
    "data-dragging": true | undefined;
    "data-pointer-focused": true | undefined;
    onPointerDown: (event: PointerEvent<HTMLElement>) => void;
    onKeyDown: (event: KeyboardEvent<HTMLElement>) => void;
    onDoubleClick: (event: MouseEvent<HTMLElement>) => void;
    onBlur: () => void;
  };
}

/**
 * The behaviour of a control that sets a number. Press anywhere on it and
 * drag: it moves from where it is, never jumping to the press, and `travel`
 * pixels of drag take it from one end to the other. Shift-drag moves it in
 * fine steps, and Shift can be pressed or let go part-way. The arrow keys
 * step it (Shift for bigger steps). Double-click, ⌥-click, and Delete or
 * Backspace reset it to its default. The scroll wheel leaves it alone.
 *
 * `travel` is measured as each drag starts, so the control can change size.
 * A drag follows the mouse left and right.
 */
export function useNumberControl(
  {
    value,
    defaultValue,
    scale,
    format,
    onChange,
    onDrag,
    keyStep = 1,
    bigStep = keyStep * 10,
  }: NumberControlOptions,
  travel: () => number | null,
): NumberControl {
  // The position the drag has taken it to, from press to release.
  const [dragging, setDragging] = useState<number | null>(null);
  const stopDrag = useRef<(() => void) | null>(null);
  // Focused by the pointer, so no focus ring until a key is pressed.
  const [pointerFocused, setPointerFocused] = useState(false);
  // The project's value a reset was sent from, until Rust replies, so a
  // second reset before then (⌥-click, then the double-click it makes)
  // doesn't send it again.
  const resetFrom = useRef<number | null>(null);
  // A drag sends through the latest `onChange`: the caller's can change mid-drag.
  const latestOnChange = useRef(onChange);
  const latestOnDrag = useRef(onDrag);
  useEffect(() => {
    latestOnChange.current = onChange;
    latestOnDrag.current = onDrag;
  });
  useEffect(() => {
    resetFrom.current = null;
  }, [value]);

  // A drag ends if the control goes away mid-drag.
  useEffect(() => () => stopDrag.current?.(), []);

  const { steps } = scale;
  const clampPosition = (position: number) => Math.min(steps, Math.max(0, position));
  const position = dragging ?? scale.toPosition(value);
  const shown =
    dragging === null
      ? Math.min(scale.max, Math.max(scale.min, value))
      : scale.fromPosition(dragging);

  /** Sets the default as a step of its own, unless it's there already. */
  const reset = () => {
    if (value === defaultValue || resetFrom.current === value) return;
    resetFrom.current = value;
    onChange(defaultValue);
  };

  const onPointerDown = (event: PointerEvent<HTMLElement>) => {
    if (event.button !== 0) return;
    event.preventDefault();
    event.currentTarget.focus();
    setPointerFocused(true);
    if (event.altKey) {
      reset();
      return;
    }
    const length = travel();
    if (!length || length <= 0) return;
    stopDrag.current?.();

    const gesture = nextGesture();
    const stepsPerPixel = steps / length;
    const start = position;
    // Where the drag measures from. It moves when Shift is pressed or let
    // go, so the control carries on from where it is at the new rate.
    let anchor = { x: event.clientX, at: start, fine: event.shiftKey };
    // Not clamped, so a plain drag stays under the mouse after leaving the ends.
    let exact = start;
    let lastX = event.clientX;
    let sent = start;
    let reported: number | null = null;

    const moveTo = (x: number, fine: boolean) => {
      if (fine !== anchor.fine) anchor = { x: lastX, at: clampPosition(exact), fine };
      exact = anchor.at + (x - anchor.x) * stepsPerPixel * (fine ? FINE_DRAG : 1);
      lastX = x;
      const next = clampPosition(Math.round(exact));
      setDragging(next);
      if (next !== reported) {
        reported = next;
        latestOnDrag.current?.(scale.fromPosition(next));
      }
      if (next !== sent) {
        sent = next;
        latestOnChange.current(scale.fromPosition(next), gesture);
      }
    };
    const move = (move: globalThis.PointerEvent) => moveTo(move.clientX, move.shiftKey);
    const end = () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", end);
      window.removeEventListener("pointercancel", end);
      window.removeEventListener("blur", end);
      stopDrag.current = null;
      setDragging(null);
      latestOnDrag.current?.(null);
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", end);
    // A drag the window loses, to ⌘-Tab say, ends where it is.
    window.addEventListener("pointercancel", end);
    window.addEventListener("blur", end);
    stopDrag.current = end;
    moveTo(event.clientX, event.shiftKey);
  };

  const onKeyDown = (event: KeyboardEvent<HTMLElement>) => {
    setPointerFocused(false);
    if (dragging !== null) return;
    if (event.key === "Delete" || event.key === "Backspace") {
      event.preventDefault();
      reset();
      return;
    }
    const step = event.shiftKey ? bigStep : keyStep;
    const keys: Record<string, number> = {
      ArrowRight: position + step,
      ArrowUp: position + step,
      ArrowLeft: position - step,
      ArrowDown: position - step,
      PageUp: position + bigStep,
      PageDown: position - bigStep,
      Home: 0,
      End: steps,
    };
    const target = keys[event.key];
    if (target === undefined) return;
    event.preventDefault();
    const next = clampPosition(target);
    if (next !== position) onChange(scale.fromPosition(next));
  };

  const text = format(shown);
  return {
    position,
    text,
    props: {
      role: "slider",
      tabIndex: 0,
      "aria-valuemin": scale.min,
      "aria-valuemax": scale.max,
      "aria-valuenow": shown,
      "aria-valuetext": text,
      "data-dragging": dragging !== null || undefined,
      "data-pointer-focused": pointerFocused || undefined,
      onPointerDown,
      onKeyDown,
      // A double-click's presses don't move it, so the reset is the only step.
      onDoubleClick: reset,
      onBlur: () => setPointerFocused(false),
    },
  };
}
