// How a slider's positions map to values, shared by the slider and its callers.

/** How a slider's whole positions, from 0 to `steps`, map to values. */
export interface SliderScale {
  /** The value at each end, for assistive tech. */
  min: number;
  max: number;
  /** The number of steps from one end to the other. */
  steps: number;
  /** The nearest whole position to `value`, clamped to the slider. */
  toPosition: (value: number) => number;
  /** The value at whole position `position`. The ends give exactly `min` and `max`. */
  fromPosition: (position: number) => number;
}

/** A scale from `min` to `max` in steps of `step`. */
export function linearScale(min: number, max: number, step: number): SliderScale {
  const steps = Math.round((max - min) / step);
  // Rounded to the step's decimals, so 0.01 steps give -0.5, not -0.49999999999999994.
  const decimals = (String(step).split(".")[1] ?? "").length;
  return {
    min,
    max,
    steps,
    toPosition: (value) => Math.min(steps, Math.max(0, Math.round((value - min) / step))),
    fromPosition: (position) => {
      if (position <= 0) return min;
      if (position >= steps) return max;
      return Number((min + position * step).toFixed(decimals));
    },
  };
}

/** How far ⌥-drag moves the slider for the same mouse movement, against a plain drag. */
export const FINE_DRAG = 0.1;
