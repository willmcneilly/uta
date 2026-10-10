// Shared by every control, so two drags never get the same number, even on
// different controls.
let gestures = 0;

/** A new drag's number, never used before. */
export function nextGesture(): number {
  gestures += 1;
  return gestures;
}
