// How the synth panel's controls map to settings, and how settings read.
// A control runs over whole positions from 0 to SETTING_STEPS. Frequencies and
// times move on a log scale, so each step is the same ratio, the way you
// hear them; levels move on a linear scale.

import type { Limits } from "./backend";

/** The number of steps from one end of a control to the other. */
export const SETTING_STEPS = 1000;

export type Scale = "log" | "linear";

/** The setting at position `position`. The ends give exactly the limits. */
export function fromPosition(position: number, [min, max]: Limits, scale: Scale): number {
  if (position <= 0) return min;
  if (position >= SETTING_STEPS) return max;
  const fraction = position / SETTING_STEPS;
  return scale === "log" ? min * (max / min) ** fraction : min + (max - min) * fraction;
}

/** The nearest position to `value`, clamped to the control. */
export function toPosition(value: number, [min, max]: Limits, scale: Scale): number {
  const fraction =
    scale === "log" ? Math.log(value / min) / Math.log(max / min) : (value - min) / (max - min);
  const position = Math.round(fraction * SETTING_STEPS);
  return Math.min(SETTING_STEPS, Math.max(0, Number.isFinite(position) ? position : 0));
}

export function formatHz(hz: number): string {
  if (hz < 1000) return `${Math.round(hz)} Hz`;
  return `${(hz / 1000).toFixed(hz < 10_000 ? 2 : 1)} kHz`;
}

export function formatSeconds(seconds: number): string {
  if (seconds < 0.01) return `${(seconds * 1000).toFixed(1)} ms`;
  if (seconds < 1) return `${Math.round(seconds * 1000)} ms`;
  return `${seconds.toFixed(2)} s`;
}

export function formatAmount(amount: number): string {
  return amount.toFixed(2);
}

export function formatLevel(level: number): string {
  return `${Math.round(level * 100)}%`;
}
