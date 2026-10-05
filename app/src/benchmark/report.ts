// What Develop → Run Benchmark measured, and how it compares with RFC-004's
// targets ("How we'll verify it", manual check 1). The text goes in PRs, so
// a change's before and after are measured the same way.

import type { TestSong } from "../backend";
import { percentile } from "../pianoRoll/frameStats";

/**
 * The longest a step can measure and still have been drawn on the next
 * frame. Frames are 16.7 ms apart at 60 frames a second. A step is sent
 * just after a frame and timed to the frame that draws it, and the web
 * view's clock counts whole milliseconds, so even a step that takes no time
 * measures one frame: 17 ms. The rest allows for a frame that starts a
 * little late. (Re-scoped from "under one frame" after UTA-30's run,
 * which that measurement can never show.)
 */
export const NEXT_FRAME_MS = 20;

/** What was measured at one test song. Times are in milliseconds. */
export interface LoadResult {
  song: TestSong;
  tracks: number;
  clips: number;
  notes: number;
  /** From sending the song's build to the screen showing it. */
  buildMs: number;
  /** Each slider step (a track's volume), from sending to the screen updating. */
  sliderSteps: number[];
  /** Each note delete in the piano roll, likewise. */
  noteDeletes: number[];
  /**
   * Each step of the drag, from making it to the screen showing it: its own
   * reply, or the reply to a newer step that replaced it while it waited.
   */
  dragSteps: number[];
  /** How many of the drag's steps were sent; the rest were replaced by newer ones. */
  dragSent: number;
  /** The time between screen frames, from the drag's first step until it's all drawn. */
  dragFrames: number[];
  /** From the drag's first step until it's all drawn. */
  dragMs: number;
  /** How many changes were sent, and how many `project-changed` events came back. */
  changes: number;
  events: number;
}

export interface Report {
  startedAt: Date;
  loads: LoadResult[];
}

export interface Summary {
  n: number;
  p50: number;
  p99: number;
  max: number;
}

/** The p50, p99 and max of `values`, by the nearest-rank method. */
export function summarise(values: readonly number[]): Summary {
  const sorted = [...values].sort((a, b) => a - b);
  return {
    n: sorted.length,
    p50: percentile(sorted, 0.5),
    p99: percentile(sorted, 0.99),
    max: sorted.length ? sorted[sorted.length - 1] : NaN,
  };
}

/** The figures a target is judged on. */
export type Measure = "sliderSteps" | "noteDeletes" | "dragSteps" | "dragFrames";

/** One of RFC-004's targets: `measure`'s `statistic` must stay under (or at) `limit`. */
export interface Target {
  measure: Measure;
  statistic: "p99" | "max";
  limit: number;
  /** Whether the limit itself is allowed: "no frame over 50 ms" allows 50. */
  inclusive: boolean;
}

/**
 * RFC-004's targets at each song: at heavy and wide, each step drawn on the
 * next frame (see NEXT_FRAME_MS) and under 20 ms frames at p99 during the
 * drag; at check 7, no frame of the
 * drag over 50 ms (it mustn't freeze, even if it isn't smooth).
 */
export function targetsFor(song: TestSong): Target[] {
  if (song === "check-7") {
    return [{ measure: "dragFrames", statistic: "max", limit: 50, inclusive: true }];
  }
  return [
    { measure: "sliderSteps", statistic: "p99", limit: NEXT_FRAME_MS, inclusive: true },
    { measure: "noteDeletes", statistic: "p99", limit: NEXT_FRAME_MS, inclusive: true },
    { measure: "dragFrames", statistic: "p99", limit: 20, inclusive: false },
  ];
}

/** Whether `summary` meets `target`. Nothing measured doesn't meet it. */
export function meets(summary: Summary, target: Target): boolean {
  const value = summary[target.statistic];
  if (Number.isNaN(value)) return false;
  return target.inclusive ? value <= target.limit : value < target.limit;
}

/** Whether every load met every one of its targets. */
export function allMet(report: Report): boolean {
  return report.loads.every((load) =>
    targetsFor(load.song).every((target) => meets(summarise(load[target.measure]), target)),
  );
}

const ROWS: [Measure, string][] = [
  ["sliderSteps", "slider step"],
  ["noteDeletes", "note delete"],
  ["dragSteps", "drag step"],
  ["dragFrames", "drag frames"],
];

const SONG_NAMES: Record<TestSong, string> = {
  heavy: "heavy",
  wide: "wide",
  "check-7": "check 7",
};

const count = (value: number) => value.toLocaleString("en-GB");
const ms = (value: number) => (Number.isNaN(value) ? "-" : value.toFixed(1));

function describe(target: Target): string {
  const sign = target.inclusive ? "≤" : "<";
  return `${target.statistic} ${sign} ${target.limit}`;
}

/** The report as plain text, laid out to paste into a PR as a code block. */
export function formatReport(report: Report): string {
  const lines = [
    `Uta benchmark, ${report.startedAt.toISOString().slice(0, 16).replace("T", " ")} UTC`,
    "Times in ms, from sending a change to the screen showing it. Targets from RFC-004.",
  ];
  for (const load of report.loads) {
    const targets = targetsFor(load.song);
    lines.push(
      "",
      `${SONG_NAMES[load.song]}: ${load.tracks} tracks, ${count(load.clips)} clips, ` +
        `${count(load.notes)} notes (built and drawn in ${count(Math.round(load.buildMs))} ms)`,
      `  ${"".padEnd(12)}${"n".padStart(5)}${"p50".padStart(9)}${"p99".padStart(9)}${"max".padStart(9)}   target`,
    );
    for (const [measure, label] of ROWS) {
      const summary = summarise(load[measure]);
      const target = targets.find((t) => t.measure === measure);
      const judged = target
        ? `   ${describe(target)}  ${meets(summary, target) ? "✓ met" : "✗ missed"}`
        : "";
      lines.push(
        `  ${label.padEnd(12)}${String(summary.n).padStart(5)}` +
          `${ms(summary.p50).padStart(9)}${ms(summary.p99).padStart(9)}${ms(summary.max).padStart(9)}` +
          judged,
      );
    }
    lines.push(
      `  the drag took ${count(Math.round(load.dragMs))} ms from its first step until all drawn`,
      `  the drag sent ${count(load.dragSent)} of its ${count(load.dragSteps.length)} steps; newer ones replaced the rest`,
      `  project-changed events: ${count(load.events)} for ${count(load.changes)} changes`,
    );
  }
  lines.push("", allMet(report) ? "Every target met." : "Some targets missed.");
  return lines.join("\n");
}
