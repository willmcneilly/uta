// What the Delay's drawing shows: where each echo lands and how loud it is,
// from the settings Rust sent. It's a picture of the settings, not the
// engine's sound: the levels follow Feedback and Drive the way the tape loop
// does, without its filters or the wobble's timing. provisional: D-31

/** One echo: when it sounds after the note that made it, how loud, and on which side. */
export interface Echo {
  seconds: number;
  /** From 0 to 1, before Mix. */
  level: number;
  /** Stereo echoes on both sides; ping-pong bounces left, right, left. */
  side: "both" | "left" | "right";
}

/** What the drawing needs of the Delay's settings. */
export interface EchoSettings {
  /** How far apart the echoes are, in seconds, at the song's tempo (from Rust). */
  spacing: number;
  /** From 0 to 1.1. */
  feedback: number;
  /** From 0 to 1. */
  drive: number;
  pingPong: boolean;
}

/** The most bars the drawing shows: past 100% feedback the echoes never die away. */
export const MAX_BARS = 4;
/** An echo quieter than this is too short a tick to draw, and ends the drawing. */
export const QUIETEST = 0.02;
/** The most echoes drawn, for a time of a few milliseconds. */
const MOST_ECHOES = 64;
/** How long an echo stays lit after it sounds, in seconds. */
export const LIT_SECONDS = 0.3;

/**
 * Each echo within `span` seconds. Feedback scales each one by itself, and
 * the tape's saturation holds the loud ones, sooner the harder Drive hits
 * it, so past 100% they settle rather than grow off the drawing.
 */
export function echoesOf(settings: EchoSettings, span: number): Echo[] {
  const { spacing, feedback, drive, pingPong } = settings;
  if (!(spacing > 0)) return [];
  const hardness = 0.6 + 2.4 * drive;
  const echoes: Echo[] = [];
  let raw = 1;
  for (let n = 1; n * spacing <= span + 1e-9 && n <= MOST_ECHOES; n++) {
    const level = Math.min(1, Math.tanh(raw * hardness) / Math.tanh(hardness));
    if (level < QUIETEST) break;
    const side = !pingPong ? "both" : n % 2 === 1 ? "left" : "right";
    echoes.push({ seconds: n * spacing, level, side });
    raw *= feedback;
  }
  return echoes;
}

/** How many whole bars of `barSeconds` the echoes last: at least one, at most `MAX_BARS`. */
export function barsFor(settings: EchoSettings, barSeconds: number): number {
  const echoes = echoesOf(settings, MAX_BARS * barSeconds);
  const last = echoes.at(-1)?.seconds ?? 0;
  return Math.min(MAX_BARS, Math.max(1, Math.ceil(last / barSeconds - 1e-9)));
}

/** How lit an echo that sounds at `at` is at `t`: 1 as it sounds, fading to 0 over `LIT_SECONDS`. */
export function litAt(t: number, at: number): number {
  const age = t - at;
  return age >= 0 && age < LIT_SECONDS ? 1 - age / LIT_SECONDS : 0;
}

/**
 * The sweep at `t`: as tall as the echo it last passed (the note going in
 * counts as one, at full height), easing to the next one's height, and to
 * nothing after the last. It's on the side of the echo it last passed.
 */
export function sweepAt(t: number, echoes: Echo[], spacing: number): Pick<Echo, "level" | "side"> {
  const events: Echo[] = [{ seconds: 0, level: 1, side: "both" }, ...echoes];
  let i = 0;
  while (i + 1 < events.length && events[i + 1].seconds <= t) i++;
  const last = events[i];
  const next = events[i + 1];
  const through = (t - last.seconds) / (next ? next.seconds - last.seconds : spacing);
  const level = next
    ? last.level + (next.level - last.level) * through
    : last.level * (1 - Math.min(1, through));
  return { level: Math.max(0, level), side: last.side };
}

/**
 * How far Wobble sways the `n`th echo off the grid, in pixels: a fixed
 * pattern, so the drawing doesn't shimmer, as wide as Wobble is high.
 */
export function swayOf(n: number, wobble: number): number {
  return wobble * 3 * Math.sin(n * 1.7 + 0.4);
}
