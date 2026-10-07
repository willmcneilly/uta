import type { KeyboardEvent, PointerEvent } from "react";
import "./Divider.css";

/** How far each arrow key press moves the divider, in CSS pixels. */
const KEY_STEP = 16;

interface Props {
  /** The height of the panel below, in CSS pixels. */
  height: number;
  min: number;
  max: number;
  onChange: (height: number) => void;
}

/**
 * The boundary between the timeline and the editor below it. Drag it, or
 * focus it and use the arrow keys, to give one more room than the other.
 */
export function Divider({ height, min, max, onChange }: Props) {
  const clamp = (value: number) => Math.round(Math.min(max, Math.max(min, value)));

  const onPointerDown = (event: PointerEvent<HTMLDivElement>) => {
    if (event.button !== 0) return;
    event.preventDefault();
    event.currentTarget.focus();
    const startY = event.clientY;
    const startHeight = height;
    const move = (move: globalThis.PointerEvent) =>
      onChange(clamp(startHeight - (move.clientY - startY)));
    const end = () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", end);
      window.removeEventListener("pointercancel", end);
      window.removeEventListener("blur", end);
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", end);
    // A drag the window loses, to ⌘-Tab say, ends where it is.
    window.addEventListener("pointercancel", end);
    window.addEventListener("blur", end);
  };

  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    const step = { ArrowUp: KEY_STEP, ArrowDown: -KEY_STEP }[event.key];
    if (step === undefined) return;
    event.preventDefault();
    onChange(clamp(height + step));
  };

  return (
    <div
      className="divider"
      role="separator"
      aria-orientation="horizontal"
      aria-label="Editor height"
      aria-valuenow={height}
      aria-valuemin={min}
      aria-valuemax={max}
      tabIndex={0}
      onPointerDown={onPointerDown}
      onKeyDown={onKeyDown}
    />
  );
}
