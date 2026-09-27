// The meter's maths, kept apart from the component so it can be tested
// without a canvas.

/** The bottom of the meter's scale. */
export const METER_FLOOR_DB = -60;
/** How fast the meter falls after a peak, like a hardware peak meter. */
export const METER_FALL_DB_PER_SECOND = 24;

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

export function toDb(level: number): number {
  return level > 0 ? 20 * Math.log10(level) : -Infinity;
}

/** The level to show next: the new peak, or the last one falling away. */
export function nextShownDb(shownDb: number, peak: number, seconds: number): number {
  const fallen = shownDb - METER_FALL_DB_PER_SECOND * seconds;
  return Math.max(fallen, toDb(peak), METER_FLOOR_DB);
}

/** Where `db` sits on the meter, from 0 to 1. */
export function meterFraction(db: number): number {
  return Math.min(1, Math.max(0, (db - METER_FLOOR_DB) / -METER_FLOOR_DB));
}
