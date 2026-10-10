import {
  type KeyboardEvent,
  type PointerEvent,
  useEffect,
  useId,
  useRef,
  useState,
} from "react";
import { nextGesture } from "../useGesture";
import { FINE_DRAG, type SliderScale } from "./sliderScale";
import "./Slider.css";

// Every slider in the app. During a drag it draws its own value, where the
// mouse is, rather than the project's, so it stays under the mouse however
// slowly Rust replies (RFC-004, part 3). That value is gesture state, like
// the text in a field you're typing in: it's gone when you let go, and the
// slider shows the project's value again. Each step goes to `onChange` with
// the drag's gesture number, so the caller can send it through `DragSteps`
// and Rust undoes the whole drag as one step.

/** A press this close to the thumb, in CSS pixels, picks it up where it is instead of jumping. */
const THUMB_GRAB = 6;

interface Props {
  /** The visible label. */
  label: string;
  /** What assistive tech calls it, when that's more than the label ("Synth 1 volume"). */
  ariaLabel?: string;
  /** The project's value, as Rust last sent it. */
  value: number;
  /** What double-click resets it to, from the outline. */
  defaultValue: number;
  scale: SliderScale;
  format: (value: number) => string;
  /** `gesture` is the same for every change in one drag, and undefined otherwise. */
  onChange: (value: number, gesture?: number) => void;
  /** How many positions an arrow key moves it. */
  keyStep?: number;
  /** How many positions Shift+arrow moves it. Ten arrow steps unless given. */
  bigStep?: number;
  /** The row's class, for its layout in the area around it. */
  className?: string;
}

/**
 * A label, the slider and its value. Drag it, or click it and use the arrow
 * keys (Shift for bigger steps). ⌥-drag moves it in fine steps, and
 * double-click resets it to its default.
 */
export function Slider({
  label,
  ariaLabel,
  value,
  defaultValue,
  scale,
  format,
  onChange,
  keyStep = 1,
  bigStep = keyStep * 10,
  className = "",
}: Props) {
  const labelId = useId();
  const rail = useRef<HTMLSpanElement>(null);
  // The position under the mouse, from press to release.
  const [dragging, setDragging] = useState<number | null>(null);
  const stopDrag = useRef<(() => void) | null>(null);
  // Focused by the pointer, so no focus ring until a key is pressed.
  const [pointerFocused, setPointerFocused] = useState(false);
  // A drag sends through the latest `onChange`: the caller's can change mid-drag.
  const latestOnChange = useRef(onChange);
  useEffect(() => {
    latestOnChange.current = onChange;
  });

  // A drag ends if the slider goes away mid-drag.
  useEffect(() => () => stopDrag.current?.(), []);

  const { steps } = scale;
  const clampPosition = (position: number) => Math.min(steps, Math.max(0, position));
  const position = dragging ?? scale.toPosition(value);
  const shown =
    dragging === null
      ? Math.min(scale.max, Math.max(scale.min, value))
      : scale.fromPosition(dragging);
  const percent = (at: number) => `${(at / steps) * 100}%`;

  const onPointerDown = (event: PointerEvent<HTMLDivElement>) => {
    if (event.button !== 0) return;
    event.preventDefault();
    event.currentTarget.focus();
    setPointerFocused(true);
    const box = rail.current?.getBoundingClientRect();
    if (!box || box.width <= 0) return;
    stopDrag.current?.();

    const gesture = nextGesture();
    const stepsPerPixel = steps / box.width;
    const start = position;
    const thumbX = box.left + (start / steps) * box.width;
    // A press on the thumb, or with ⌥ held, picks it up where it is; a press
    // anywhere else on the slider jumps it there.
    const grabbed = event.altKey || Math.abs(event.clientX - thumbX) <= THUMB_GRAB;
    // Where the drag measures from. It moves when ⌥ is pressed or released,
    // so the thumb carries on from where it is at the new rate.
    let anchor = {
      x: event.clientX,
      at: grabbed ? start : (event.clientX - box.left) * stepsPerPixel,
      fine: event.altKey,
    };
    // Not clamped, so a plain drag stays under the mouse after leaving the ends.
    let exact = anchor.at;
    let lastX = event.clientX;
    let sent = start;

    const moveTo = (x: number, fine: boolean) => {
      if (fine !== anchor.fine) anchor = { x: lastX, at: clampPosition(exact), fine };
      exact = anchor.at + (x - anchor.x) * stepsPerPixel * (fine ? FINE_DRAG : 1);
      lastX = x;
      const next = clampPosition(Math.round(exact));
      setDragging(next);
      if (next !== sent) {
        sent = next;
        latestOnChange.current(scale.fromPosition(next), gesture);
      }
    };
    const move = (move: globalThis.PointerEvent) => moveTo(move.clientX, move.altKey);
    const end = () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", end);
      window.removeEventListener("pointercancel", end);
      window.removeEventListener("blur", end);
      stopDrag.current = null;
      setDragging(null);
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", end);
    // A drag the window loses, to ⌘-Tab say, ends where it is.
    window.addEventListener("pointercancel", end);
    window.addEventListener("blur", end);
    stopDrag.current = end;
    moveTo(event.clientX, event.altKey);
  };

  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    setPointerFocused(false);
    if (dragging !== null) return;
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

  const onDoubleClick = () => {
    if (value !== defaultValue) onChange(defaultValue);
  };

  return (
    <div className={`slider-row ${className}`.trim()}>
      <span id={labelId}>{label}</span>
      <div
        className="slider"
        role="slider"
        tabIndex={0}
        aria-label={ariaLabel}
        aria-labelledby={ariaLabel ? undefined : labelId}
        aria-orientation="horizontal"
        aria-valuemin={scale.min}
        aria-valuemax={scale.max}
        aria-valuenow={shown}
        aria-valuetext={format(shown)}
        data-dragging={dragging !== null || undefined}
        data-pointer-focused={pointerFocused || undefined}
        onPointerDown={onPointerDown}
        onBlur={() => setPointerFocused(false)}
        onKeyDown={onKeyDown}
        onDoubleClick={onDoubleClick}
      >
        <span className="slider-rail" ref={rail}>
          <span
            className="slider-default"
            style={{ left: percent(scale.toPosition(defaultValue)) }}
          />
          <span className="slider-thumb" style={{ left: percent(position) }} />
        </span>
      </div>
      <output>{format(shown)}</output>
    </div>
  );
}
