import { useRef } from "react";

// Shared by every control, so two drags never get the same number, even on
// different controls.
let gestures = 0;

/** A new drag's number, never used before. */
export function nextGesture(): number {
  gestures += 1;
  return gestures;
}

/**
 * Numbers the drags of one control. Call `start` on pointer down; `current`
 * is then the drag's number until the pointer is released, and `undefined`
 * otherwise (a keyboard change, say). Rust undoes changes that share a
 * number as one step.
 */
export function useGesture(): { start: () => void; current: () => number | undefined } {
  const gesture = useRef<number | undefined>(undefined);
  return {
    start: () => {
      gesture.current = nextGesture();
      // The pointer can be released outside the control.
      window.addEventListener(
        "pointerup",
        () => {
          gesture.current = undefined;
        },
        { once: true },
      );
    },
    current: () => gesture.current,
  };
}
