// The meter's maths, kept apart from the component so it can be tested
// without a canvas.

/** The bottom of the meter's scale. */
export const METER_FLOOR_DB = -60;
/** How long the meter holds a peak before it falls. */
export const METER_HOLD_SECONDS = 1;
/**
 * How fast the meter falls once the hold is over: 20 dB in 1.7 s, the
 * digital peak meter's rate in IEC 60268-18.
 */
export const METER_FALL_DB_PER_SECOND = 20 / 1.7;

/**
 * Collects the peaks that arrive between two drawn frames, so none is
 * missed when the stream and the screen fall out of step.
 */
export class MeterLevel {
  private peak = 0;

  push(peak: number): void {
    if (peak > this.peak) this.peak = peak;
  }

  /** The loudest peak since the last call. */
  take(): number {
    const peak = this.peak;
    this.peak = 0;
    return peak;
  }
}

/**
 * A level for each track, by track ID, made the first time it's asked for.
 * The frame stream pushes into them, and each track's meter takes from its
 * own.
 */
export class TrackLevels {
  private levels = new Map<string, MeterLevel>();

  level(track: string): MeterLevel {
    let level = this.levels.get(track);
    if (!level) {
      level = new MeterLevel();
      this.levels.set(track, level);
    }
    return level;
  }

  /** Pushes each track's peak into its level. */
  push(peaks: Record<string, number>): void {
    for (const [track, peak] of Object.entries(peaks)) this.level(track).push(peak);
  }
}

export function toDb(level: number): number {
  return level > 0 ? 20 * Math.log10(level) : -Infinity;
}

/** What the meter shows: a level, and how long it has held it since its last peak. */
export interface Shown {
  db: number;
  heldSeconds: number;
}

export const SILENT: Shown = { db: METER_FLOOR_DB, heldSeconds: 0 };

/**
 * What to show `seconds` later, given the loudest `peak` since: the new peak
 * if it's as loud as what's shown, or the last one, held for
 * `METER_HOLD_SECONDS` and then falling away.
 */
export function nextShown(shown: Shown, peak: number, seconds: number): Shown {
  const held = shown.heldSeconds + seconds;
  // Only the time past the hold counts towards the fall.
  const falling =
    Math.max(0, held - METER_HOLD_SECONDS) - Math.max(0, shown.heldSeconds - METER_HOLD_SECONDS);
  const fallen = Math.max(shown.db - METER_FALL_DB_PER_SECOND * falling, METER_FLOOR_DB);
  const peakDb = toDb(peak);
  if (peakDb >= fallen) return { db: Math.max(peakDb, METER_FLOOR_DB), heldSeconds: 0 };
  return { db: fallen, heldSeconds: held };
}

/** Where `db` sits on the meter, from 0 to 1. */
export function meterFraction(db: number): number {
  return Math.min(1, Math.max(0, (db - METER_FLOOR_DB) / -METER_FLOOR_DB));
}

/**
 * Whether the clip light is lit: the master has clipped since the light was
 * last clicked, when the count was `seen`.
 */
export function clipLit(clips: number, seen: number): boolean {
  return clips > seen;
}
