// The envelope's shape, for the synth panel to draw, following the engine's
// envelope (crates/uta-engine/src/synth/envelope.rs): the attack rises in a
// straight line, and decay and release fall on its exponential curve, each
// landing on its end level exactly when its time is up.
//
// Attack, decay and release share the width in proportion to their times,
// so a long release looks long next to a short attack. The sustain lasts as
// long as the note is held, which no setting says, so it gets a fixed share.

import type { Point } from "./filterCurve";

/** The share of the width the sustain is drawn across. */
export const SUSTAIN_SHARE = 0.2;
/** How far past its end level a curve aims, as the engine's `OVERSHOOT`. */
const OVERSHOOT = 0.001;
/** Points per curved stage. */
const CURVE_POINTS = 32;

export interface EnvelopeTimes {
  attackSeconds: number;
  decaySeconds: number;
  sustain: number;
  releaseSeconds: number;
}

export type StageName = "attack" | "decay" | "sustain" | "release";

/** One stage, from `start` to `end` across the drawing, and its levels from 0 to 1. */
export interface Stage {
  name: StageName;
  start: number;
  end: number;
  from: number;
  to: number;
}

export interface EnvelopeShape {
  stages: Stage[];
  /** The line to draw, `width` by `height`, with full level at the top. */
  points: Point[];
}

/** How far a decay or release has fallen, from 1 to 0, a share `t` of the way through it. */
export function curveAt(t: number): number {
  return (1 + OVERSHOOT) * (OVERSHOOT / (1 + OVERSHOOT)) ** t - OVERSHOOT;
}

export function envelopeShape(times: EnvelopeTimes, width: number, height: number): EnvelopeShape {
  const { attackSeconds, decaySeconds, sustain, releaseSeconds } = times;
  const perSecond = (width * (1 - SUSTAIN_SHARE)) / (attackSeconds + decaySeconds + releaseSeconds);
  const attackEnd = attackSeconds * perSecond;
  const decayEnd = attackEnd + decaySeconds * perSecond;
  const sustainEnd = decayEnd + width * SUSTAIN_SHARE;
  const stages: Stage[] = [
    { name: "attack", start: 0, end: attackEnd, from: 0, to: 1 },
    { name: "decay", start: attackEnd, end: decayEnd, from: 1, to: sustain },
    { name: "sustain", start: decayEnd, end: sustainEnd, from: sustain, to: sustain },
    { name: "release", start: sustainEnd, end: width, from: sustain, to: 0 },
  ];

  const y = (level: number) => (1 - level) * height;
  const curve = ({ start, end, from, to }: Stage) =>
    Array.from({ length: CURVE_POINTS }, (_, i) => {
      const t = (i + 1) / CURVE_POINTS;
      return { x: start + (end - start) * t, y: y(to + (from - to) * curveAt(t)) };
    });
  const points = [
    { x: 0, y: y(0) },
    { x: attackEnd, y: y(1) },
    ...curve(stages[1]),
    { x: sustainEnd, y: y(sustain) },
    ...curve(stages[3]),
  ];
  return { stages, points };
}
