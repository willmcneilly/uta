// The filter's response curve, for the synth panel to draw.
//
// The UI works it out here, from the exact formula for the engine's filter
// (crates/uta-engine/src/synth/filter.rs), rather than asking Rust or
// measuring the filter. It's presentation, like turning a velocity into a
// colour: the curve follows a knob during a drag with no round trip, and
// working it out is cheap. Asking Rust would be a round trip on every knob
// step, and measuring the real filter (with test tones or an FFT) is far too
// slow for a drawing that follows a drag.
//
// The risk is that this copy of the maths drifts from the sound, which is
// what happened to HISE's filter display when it drew from a generic
// approximation. So a Rust test measures the real filter with sine waves and
// keeps the gains in filter-response.json, beside this file, and
// filterCurve.test.ts checks this formula against them within 0.5 dB.
// Research: https://app.notion.com/p/3f23af969b6f81f9a4d9f08a1bc97317
//
// The engine's filter is Andrew Simper's state-variable low-pass, built by
// trapezoidal integration. That's the bilinear transform of the analogue
// two-pole low-pass 1 / (s² + k·s + 1) with its cutoff prewarped, so its
// exact gain at a frequency is the analogue one at the warped frequency
// tan(π·f/rate) / tan(π·cutoff/rate). It depends on the sample rate near
// the top of the range.

/** The damping at full resonance, as the engine's `MIN_DAMPING`. */
const MIN_DAMPING = 0.1;
/** The highest cutoff the engine uses, as a share of the sample rate (`MAX_CUTOFF_RATIO`). */
const MAX_CUTOFF_RATIO = 0.49;

/** The engine's rate before a device is open, as its `DEFAULT_SAMPLE_RATE`. */
export const DEFAULT_SAMPLE_RATE = 48_000;

/**
 * How much the filter lets through at `frequencyHz`, in dB: 0 is unchanged,
 * negative is quieter. -Infinity at or above half the sample rate.
 */
export function filterGainDb(
  frequencyHz: number,
  cutoffHz: number,
  resonance: number,
  sampleRate: number,
): number {
  if (frequencyHz >= sampleRate / 2) return -Infinity;
  const cutoff = Math.min(cutoffHz, sampleRate * MAX_CUTOFF_RATIO);
  const g = Math.tan((Math.PI * cutoff) / sampleRate);
  const k = Math.SQRT2 + (MIN_DAMPING - Math.SQRT2) * resonance;
  const w = Math.tan((Math.PI * frequencyHz) / sampleRate) / g;
  const power = 1 / ((1 - w * w) ** 2 + (k * w) ** 2);
  return 10 * Math.log10(power);
}

/** The frequencies the drawing spans, in Hz: the cutoff knob's range. */
export const FREQUENCIES: readonly [number, number] = [20, 20_000];
/**
 * The gains it spans, in dB, from the bottom edge to the top. The top leaves
 * room over full resonance's 20 dB peak; below the bottom it's silent enough.
 */
export const GAINS: readonly [number, number] = [-36, 24];

export interface Point {
  x: number;
  y: number;
}

/** Where `hz` sits across a drawing `width` wide, on a log scale. */
export function xOfFrequency(hz: number, width: number): number {
  const [low, high] = FREQUENCIES;
  return (Math.log(hz / low) / Math.log(high / low)) * width;
}

export function frequencyOfX(x: number, width: number): number {
  const [low, high] = FREQUENCIES;
  return low * (high / low) ** (x / width);
}

/** Where `db` sits down a drawing `height` tall. Anything quieter sits on the bottom edge. */
export function yOfGain(db: number, height: number): number {
  const [bottom, top] = GAINS;
  const clamped = Math.min(top, Math.max(bottom, db));
  return ((top - clamped) / (top - bottom)) * height;
}

export function gainOfY(y: number, height: number): number {
  const [bottom, top] = GAINS;
  return top - (y / height) * (top - bottom);
}

/**
 * The response curve as points across a `width` by `height` drawing: one
 * per pixel, and one at the cutoff so a resonant peak isn't missed between
 * two pixels.
 */
export function filterCurve(
  cutoffHz: number,
  resonance: number,
  sampleRate: number,
  width: number,
  height: number,
): Point[] {
  const xs = Array.from({ length: Math.ceil(width) + 1 }, (_, i) => Math.min(i, width));
  const cutoffX = xOfFrequency(cutoffHz, width);
  if (cutoffX > 0 && cutoffX < width) xs.push(cutoffX);
  xs.sort((a, b) => a - b);
  return xs.map((x) => ({
    x,
    y: yOfGain(filterGainDb(frequencyOfX(x, width), cutoffHz, resonance, sampleRate), height),
  }));
}
