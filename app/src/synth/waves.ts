// The oscillators' and the LFO's waves, for the synth panel to draw,
// following the engine's: each oscillator's shape before its corners are
// smoothed (`naive` in crates/uta-engine/src/synth/oscillator.rs) and the
// LFO's (`shape_value` in crates/uta-engine/src/synth/lfo.rs). Like the
// filter's curve, it's presentation: worked out here so a drawing follows a
// drag with no round trip.

import type { LfoShape, Waveform } from "../backend";
import type { Point } from "./filterCurve";

/** The fractional part of `phase`: where in its cycle a wave is, from 0 to 1. */
function within(phase: number): number {
  return phase - Math.floor(phase);
}

/**
 * An oscillator's level at `phase` cycles, from -1 to 1. The pulse is high
 * for the first `width` of each cycle.
 */
export function oscillatorAt(wave: Waveform, phase: number, width: number): number {
  const t = within(phase);
  switch (wave) {
    case "sine":
      return Math.sin(2 * Math.PI * t);
    case "triangle":
      return 1 - 4 * Math.abs(t - 0.5);
    case "saw":
      return 2 * t - 1;
    case "pulse":
      return t < width ? 1 : -1;
  }
}

/**
 * A level from -1 to 1 for sample-and-hold's cycle `cycle`: the same each
 * time it's drawn, so the drawing stands still. The engine's are random, so
 * these only show that it jumps once a cycle.
 */
function heldAt(cycle: number): number {
  const noise = Math.sin(cycle * 12.9898 + 78.233) * 43_758.5453;
  return within(noise) * 2 - 1;
}

/** The LFO's level at `phase` cycles from the start of a bar, from -1 to 1. */
export function lfoAt(shape: LfoShape, phase: number): number {
  const t = within(phase);
  switch (shape) {
    case "sine":
      return Math.sin(2 * Math.PI * t);
    case "triangle":
      return t < 0.25 ? 4 * t : t < 0.75 ? 2 - 4 * t : 4 * t - 4;
    case "saw":
      return 2 * t - 1;
    case "square":
      return t < 0.5 ? 1 : -1;
    case "sample_and_hold":
      return heldAt(Math.floor(phase));
  }
}

/** How each oscillator is tuned against the note, and whether oscillator 2 is synced. */
export interface Tuning {
  osc1Octave: number;
  osc2Octave: number;
  osc2PitchSemitones: number;
  sync: boolean;
}

/** Oscillator 2's frequency as a multiple of oscillator 1's. */
export function osc2Ratio({ osc1Octave, osc2Octave, osc2PitchSemitones }: Tuning): number {
  return 2 ** (osc2Octave - osc1Octave + osc2PitchSemitones / 12);
}

/** Oscillator 2's pulse width: a square, as the engine's `SQUARE`. */
const SQUARE = 0.5;
/** How many of oscillator 1's cycles an oscillator's drawing spans. */
export const OSCILLATOR_CYCLES = 2;
/** Points across each pixel, so a pulse's edges and a sync's tear stay sharp. */
const POINTS_PER_PIXEL = 3;

/**
 * Oscillator `which`'s wave across a `width` by `height` drawing, over two
 * of oscillator 1's cycles: oscillator 1 at its pulse width (only it has
 * one; oscillator 2's pulse is a square), oscillator 2 at its
 * pitch against oscillator 1, restarting with oscillator 1's cycle when
 * synced, so a sync lead draws its tear. Full level is at the top.
 */
export function oscillatorPoints(
  which: 1 | 2,
  wave: Waveform,
  pulseWidth: number,
  tuning: Tuning,
  width: number,
  height: number,
): Point[] {
  const ratio = osc2Ratio(tuning);
  const count = Math.round(width * POINTS_PER_PIXEL);
  return Array.from({ length: count + 1 }, (_, i) => {
    const cycles = (i / count) * OSCILLATOR_CYCLES;
    const phase = which === 1 ? cycles : tuning.sync ? within(cycles) * ratio : cycles * ratio;
    const level = oscillatorAt(wave, phase, which === 1 ? pulseWidth : SQUARE);
    return { x: (i / count) * width, y: ((1 - level) / 2) * height };
  });
}

/**
 * The LFO's wave across one bar, `width` by `height`, at `cyclesPerBar`.
 * Full level is at the top.
 */
export function lfoPoints(
  shape: LfoShape,
  cyclesPerBar: number,
  width: number,
  height: number,
): Point[] {
  const count = Math.round(width * POINTS_PER_PIXEL);
  return Array.from({ length: count + 1 }, (_, i) => {
    const level = lfoAt(shape, (i / count) * cyclesPerBar);
    return { x: (i / count) * width, y: ((1 - level) / 2) * height };
  });
}
