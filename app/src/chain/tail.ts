// What the Reverb's drawing draws, worked out from its settings alone, the
// way the Delay's echoes are (echoes.ts). It's a picture of the settings,
// not a measurement of the sound: each band falls 60 dB in Decay times its
// Mult, the cuts lower where it starts, the early burst sits after Predelay
// for Early size, and the tail builds up after it, slowly with a long Early
// size and much Early send. From the design sandbox's round 08
// (`live/src/reverb/tail.ts`) and round 08c's time scale. provisional: D-40

/** The settings the drawing needs, in Rust's units. */
export interface TailSettings {
  predelaySeconds: number;
  decaySeconds: number;
  /** From 0 (only the early burst) to 1 (only the tail). */
  depth: number;
  highCutHz: number;
  /** Null when the Low cut is off. */
  lowCutHz: number | null;
  earlySizeSeconds: number;
  diffusion: number;
  earlySend: number;
  lateSize: number;
  bassMult: number;
  bassXoverHz: number;
  highMult: number;
  highXoverHz: number;
}

/** The level everything is drawn down to: RT60's −60 dB. */
const FLOOR_DB = -60;

/** A frequency in the middle of each band the drawing works out. */
export const LOW_HZ = 80;
export const MID_HZ = 1000;
export const HIGH_HZ = 8000;

const db = (gain: number) => 20 * Math.log10(Math.max(gain, 1e-6));

/** The High cut's −12 dB per octave low-pass at `hz`, in dB. */
function highCutDb(s: TailSettings, hz: number): number {
  return -10 * Math.log10(1 + (hz / s.highCutHz) ** 4);
}

/** The Low cut's low shelf at `hz`, in dB: nothing while it's off. */
function lowCutDb(s: TailSettings, hz: number): number {
  if (s.lowCutHz === null) return 0;
  return -24 / (1 + (hz / s.lowCutHz) ** 2);
}

/** How long `hz` lasts, as a multiple of Decay: Bass mult below its xover, High mult above its. */
function multAt(s: TailSettings, hz: number): number {
  const low = 1 / (1 + (hz / s.bassXoverHz) ** 4);
  const high = 1 / (1 + (s.highXoverHz / hz) ** 4);
  return s.bassMult ** low * s.highMult ** high;
}

/** Where the tail starts at `hz`, in dB: Depth's share, less what the cuts take. */
function lateStartDb(s: TailSettings, hz: number): number {
  return db(Math.sqrt(s.depth)) + highCutDb(s, hz) + lowCutDb(s, hz);
}

/** The early burst's level, in dB. */
function earlyDb(s: TailSettings): number {
  return db(Math.sqrt(1 - s.depth));
}

/** How long the tail takes to build to its loudest after Predelay, in seconds. */
export function buildSeconds(s: TailSettings): number {
  return 0.01 + 0.04 * s.lateSize + s.earlySend * s.earlySizeSeconds * 0.9;
}

/** The tail's level at `hz`, `t` seconds after the note, in dB, or null before it starts. */
function lateDbAt(s: TailSettings, hz: number, t: number): number | null {
  const start = s.predelaySeconds;
  if (t < start) return null;
  const top = lateStartDb(s, hz);
  const peak = start + buildSeconds(s);
  if (t < peak) return FLOOR_DB + (top - FLOOR_DB) * ((t - start) / (peak - start)) ** 0.7;
  return top - (60 * (t - peak)) / (s.decaySeconds * multAt(s, hz));
}

/** A level in dB as 0 (the floor, −60 dB) to 1 (full). */
function amount(level: number | null): number {
  return level === null ? 0 : Math.max(0, Math.min(1, (level - FLOOR_DB) / -FLOOR_DB));
}

/** The tail's level at `hz`, `t` seconds after the note, from 0 to 1. */
export function lateAt(s: TailSettings, hz: number, t: number): number {
  return amount(lateDbAt(s, hz, t));
}

/** The tail's three bands at `t`, and the loudest of them, each from 0 to 1. */
export function tailAt(s: TailSettings, t: number): { low: number; high: number; all: number } {
  const low = lateAt(s, LOW_HZ, t);
  const mid = lateAt(s, MID_HZ, t);
  const high = lateAt(s, HIGH_HZ, t);
  return { low, high, all: Math.max(low, mid, high) };
}

/** An early reflection: when it arrives, in seconds after the note, and how loud, from 0 to 1. */
export interface Reflection {
  seconds: number;
  level: number;
}

/** A repeatable random sequence, so the drawing is the same every time. */
export function random(seed: number): () => number {
  let state = seed;
  return () => {
    state = (state * 16807) % 2147483647;
    return state / 2147483647;
  };
}

/** The early burst's reflections, from Diffusion (how many) and Early size (how long). */
export function reflectionsOf(s: TailSettings): Reflection[] {
  const count = Math.round(4 + s.diffusion * 14);
  const top = earlyDb(s);
  if (top < FLOOR_DB + 3) return [];
  const next = random(7);
  const reflections: Reflection[] = [];
  for (let i = 0; i < count; i += 1) {
    const at = (i + 0.2 + next() * 0.6) / count;
    // A short burst thins out as it goes; a long one (gated) holds, then stops.
    const fall = s.earlySizeSeconds > 0.15 ? 4 : 14;
    reflections.push({
      seconds: s.predelaySeconds + at * s.earlySizeSeconds,
      level: amount(top - fall * at - next() * (10 - 7 * s.diffusion)),
    });
  }
  return reflections;
}

/** When the tail at `hz` is gone (down 60 dB), in seconds after the note. */
function endOf(s: TailSettings, hz: number): number {
  const top = lateStartDb(s, hz);
  if (top <= FLOOR_DB) return s.predelaySeconds;
  const peak = s.predelaySeconds + buildSeconds(s);
  return peak + ((top - FLOOR_DB) * s.decaySeconds * multAt(s, hz)) / 60;
}

/** When it's silent at `hz`: the later of the tail's end and the early burst's. */
export function silentAt(s: TailSettings, hz: number): number {
  const early = earlyDb(s) > FLOOR_DB ? s.predelaySeconds + s.earlySizeSeconds : 0;
  return Math.max(endOf(s, hz), early);
}

/** When the whole reverb is silent: its longest band's end. */
export function endSeconds(s: TailSettings): number {
  return Math.max(silentAt(s, LOW_HZ), silentAt(s, MID_HZ), silentAt(s, HIGH_HZ));
}

// The time scale, the same for every sound, so a long tail reaches further
// than a short one instead of every tail filling the width: nearly straight
// up to 50 ms and logarithmic past it, to 30 s.

/** The longest time the drawing shows, in seconds. */
export const LONGEST_SECONDS = 30;
const KNEE = 0.05;
const WHOLE = Math.log1p(LONGEST_SECONDS / KNEE);

/** Where `seconds` falls across `width`. */
export function xOf(seconds: number, width: number): number {
  return (width * Math.log1p(Math.min(LONGEST_SECONDS, Math.max(0, seconds)) / KNEE)) / WHOLE;
}

/** The time at `x` across `width`. */
export function secondsAt(x: number, width: number): number {
  return KNEE * Math.expm1((Math.max(0, x) / width) * WHOLE);
}

/**
 * The Decay that makes the reverb end at `seconds`: dragging the end scales
 * Decay, which scales every band's length together. Kept just past the
 * tail's build, and within `limits`, Rust's.
 */
export function decayFor(s: TailSettings, seconds: number, limits: [number, number]): number {
  const peak = s.predelaySeconds + buildSeconds(s);
  const target = Math.max(seconds, peak + 0.05);
  const decay = (s.decaySeconds * (target - peak)) / Math.max(0.01, endSeconds(s) - peak);
  return Math.max(limits[0], Math.min(limits[1], decay));
}

/** A time, read as the drawing labels it: "640 ms", "2.4 s". */
export function formatTime(seconds: number): string {
  if (seconds < 1) return `${Math.round(seconds * 1000)} ms`;
  return `${Number(seconds.toFixed(1))} s`;
}
