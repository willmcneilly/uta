// How the synth panel's controls map to settings, and how settings read.
// A control runs over whole positions from 0 to SETTING_STEPS. Frequencies and
// times move on a log scale, so each step is the same ratio, the way you
// hear them; levels move on a linear scale; an offset either side of zero,
// such as oscillator 2's Pitch, on a cubic one, fine near zero.

import type { Limits, NumberSettingView, Scale } from "./backend";
import { type NumberScale, linearScale } from "./design/numberScale";

/** The number of steps from one end of a control to the other. */
export const SETTING_STEPS = 1000;

export type { Scale };

/** The setting at position `position`. The ends give exactly the limits. */
export function fromPosition(position: number, [min, max]: Limits, scale: Scale): number {
  if (position <= 0) return min;
  if (position >= SETTING_STEPS) return max;
  const fraction = position / SETTING_STEPS;
  switch (scale) {
    case "log":
      return min * (max / min) ** fraction;
    case "linear":
      return min + (max - min) * fraction;
    case "cubic": {
      // From -1 to 1 across the control, cubed: the centre is exactly the middle.
      const centred = 2 * fraction - 1;
      return (min + max) / 2 + ((max - min) / 2) * centred ** 3;
    }
  }
}

/** How far through the limits `value` is on `scale`, from 0 to 1. */
function fractionOf(value: number, [min, max]: Limits, scale: Scale): number {
  switch (scale) {
    case "log":
      return Math.log(value / min) / Math.log(max / min);
    case "linear":
      return (value - min) / (max - min);
    case "cubic":
      return (Math.cbrt((value - (min + max) / 2) / ((max - min) / 2)) + 1) / 2;
  }
}

/** The nearest position to `value`, clamped to the control. */
export function toPosition(value: number, limits: Limits, scale: Scale): number {
  const fraction = fractionOf(value, limits, scale);
  const position = Math.round(fraction * SETTING_STEPS);
  return Math.min(SETTING_STEPS, Math.max(0, Number.isFinite(position) ? position : 0));
}

/**
 * A setting's positions, on the scale and within the limits Rust describes
 * it with, or one position a step for a setting that takes only some values
 * (an oscillator's whole octaves).
 */
export function settingScale({ limits, scale, step }: NumberSettingView): NumberScale {
  if (step !== undefined) return linearScale(limits[0], limits[1], step);
  return {
    min: limits[0],
    max: limits[1],
    steps: SETTING_STEPS,
    toPosition: (value) => toPosition(value, limits, scale),
    fromPosition: (position) => fromPosition(position, limits, scale),
  };
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

/** An offset to `digits` places, signed unless it rounds to zero: "+7", "−12" (a true minus), "0". */
function signed(value: number, digits: number): string {
  const text = Math.abs(value).toFixed(digits);
  if (Number(text) === 0) return text;
  return `${value > 0 ? "+" : "\u2212"}${text}`;
}

/** Semitones to two places, so a few cents of detune shows: "+0.07 st". */
export function formatSemitones(semitones: number): string {
  return `${signed(semitones, 2)} st`;
}

/** Whole octaves: "+1 oct", "−2 oct". */
export function formatOctaves(octaves: number): string {
  return `${signed(octaves, 0)} oct`;
}

/** A share either side of zero, signed: "+60%", "−25%". */
export function formatSignedLevel(level: number): string {
  return `${signed(level * 100, 0)}%`;
}

/** Octaves that needn't be whole, to a tenth: "+1.5 oct". */
export function formatFineOctaves(octaves: number): string {
  return `${signed(octaves, 1)} oct`;
}

/**
 * Whether a setting is an offset either side of zero, such as oscillator
 * 2's Pitch or a mod slot's Amount: its control fills from the middle, and
 * its value reads signed.
 */
export function isCentred({ limits: [min, max] }: NumberSettingView): boolean {
  return min < 0 && max === -min;
}

/** How a setting's value reads, from its unit, its step and its range. */
export function formatOf(setting: NumberSettingView): (value: number) => string {
  switch (setting.unit) {
    case "hz":
      return formatHz;
    case "seconds":
      return formatSeconds;
    case "db":
      return (db) => `${db.toFixed(1)} dB`;
    case "fraction":
      return isCentred(setting) ? formatSignedLevel : formatLevel;
    case "amount":
      return formatAmount;
    case "semitones":
      return formatSemitones;
    case "octaves":
      return setting.step === undefined ? formatFineOctaves : formatOctaves;
  }
}
