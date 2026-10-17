// Musical time as the UI shows it. The project counts in ticks (960 to a
// quarter note); people count in bars and beats, from 1.

export interface BarBeat {
  /** From 1. */
  bar: number;
  /** From 1 to the beats in a bar. */
  beat: number;
}

/** Which bar and beat `ticks` falls in, counting from 1. */
export function barBeat(ticks: number, ticksPerQuarter: number, beatsPerBar: number): BarBeat {
  const beats = Math.floor(Math.max(0, ticks) / ticksPerQuarter);
  return { bar: Math.floor(beats / beatsPerBar) + 1, beat: (beats % beatsPerBar) + 1 };
}

/** "3.2" for bar 3, beat 2. */
export function formatBarBeat({ bar, beat }: BarBeat): string {
  return `${bar}.${beat}`;
}

/** `ticks` as seconds at `bpm` quarter notes a minute. */
export function secondsOf(ticks: number, ticksPerQuarter: number, bpm: number): number {
  return (ticks / ticksPerQuarter) * (60 / bpm);
}

/** One bar in seconds at `bpm`, its beats quarter notes (4/4 for now). */
export function barSecondsAt(beatsPerBar: number, bpm: number): number {
  return beatsPerBar * (60 / bpm);
}
