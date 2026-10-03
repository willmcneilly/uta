// Develop → Run Benchmark: builds each test song, works the controls by
// itself through the same commands the UI sends, and times each change from
// sending it to the screen showing it (RFC-004, "The benchmark stays"). It
// needs the real web view, so it can't run in CI.

import {
  type ProjectView,
  type TestSong,
  buildTestSong,
  removeNotes,
  setTrackMixer,
} from "../backend";
import { nextGesture } from "../useGesture";
import type { LoadResult, Report } from "./report";

/** What the benchmark needs from the app. */
export interface BenchmarkHost {
  /** Shows a project Rust sent back, as a reply to one of the app's own commands is shown. */
  apply(view: ProjectView): void;
  /** How many `project-changed` events have arrived so far. */
  events(): number;
  /** Opens a clip in the piano roll. */
  showClip(id: string): void;
}

/** Time, as the benchmark reads it. Tests pass their own. */
export interface Clock {
  /** Milliseconds, as `performance.now()`. */
  now(): number;
  /** Resolves at the next screen frame, with its time, as `requestAnimationFrame`. */
  frame(): Promise<number>;
  sleep(ms: number): Promise<void>;
}

export const browserClock: Clock = {
  now: () => performance.now(),
  frame: () => new Promise((resolve) => requestAnimationFrame(resolve)),
  sleep: (ms) => new Promise((resolve) => setTimeout(resolve, ms)),
};

export interface BenchmarkOptions {
  songs: TestSong[];
  sliderSteps: number;
  noteDeletes: number;
  /** A real drag sends a step every frame, without waiting for replies. */
  dragSteps: number;
  /**
   * How long to wait for a `project-changed` event once a reply is drawn.
   * Rust sends one after every change today, and drawing it is most of the
   * cost (RFC-004, "What we measured"), so a step isn't done until it's
   * drawn too. Once changes stop sending one, the wait isn't counted.
   */
  eventWaitMs: number;
  /** A pause after building each song, so the build doesn't spill into the steps. */
  settleMs: number;
}

export const DEFAULT_OPTIONS: BenchmarkOptions = {
  songs: ["heavy", "wide", "check-7"],
  sliderSteps: 10,
  noteDeletes: 10,
  dragSteps: 30,
  eventWaitMs: 250,
  settleMs: 1000,
};

/** What the benchmark is doing, to show while it runs. */
export interface Progress {
  song: TestSong;
  doing: string;
}

/** Runs the benchmark over every song in `options`, in turn. */
export async function runBenchmark(
  host: BenchmarkHost,
  onProgress: (progress: Progress) => void,
  options: BenchmarkOptions = DEFAULT_OPTIONS,
  clock: Clock = browserClock,
): Promise<Report> {
  const report: Report = { startedAt: new Date(), loads: [] };
  for (const song of options.songs) {
    report.loads.push(await runLoad(song, host, (doing) => onProgress({ song, doing }), options, clock));
  }
  return report;
}

async function runLoad(
  song: TestSong,
  host: BenchmarkHost,
  say: (doing: string) => void,
  options: BenchmarkOptions,
  clock: Clock,
): Promise<LoadResult> {
  let project = null as ProjectView | null;
  let changes = 0;
  const startEvents = host.events();

  /**
   * If fewer than `expected` `project-changed` events have arrived since
   * `before`, waits for them and returns when they're drawn. Returns `null`
   * if they'd all arrived already, or none came in time.
   */
  const eventDrawn = async (before: number, expected: number): Promise<number | null> => {
    if (host.events() - before >= expected) return null;
    const waitFrom = clock.now();
    while (host.events() - before < expected) {
      if (clock.now() - waitFrom > options.eventWaitMs) return null;
      await clock.frame();
    }
    await clock.frame();
    return clock.now();
  };

  /** Sends one change and times it until the screen shows it. */
  const step = async (send: () => Promise<ProjectView>): Promise<number> => {
    const before = host.events();
    const start = clock.now();
    project = await send();
    changes += 1;
    host.apply(project);
    await clock.frame();
    const drawn = clock.now();
    return ((await eventDrawn(before, 1)) ?? drawn) - start;
  };

  say("building the song");
  const buildMs = await step(() => buildTestSong(song));
  const built = project;
  if (!built || built.tracks.length === 0 || built.tracks[0].clips.length === 0) {
    throw new Error(`the ${song} song came back with no clips`);
  }
  const clips = built.tracks.flatMap((track) => track.clips);
  const shape = {
    tracks: built.tracks.length,
    clips: clips.length,
    notes: clips.reduce((sum, clip) => sum + clip.notes.length, 0),
  };
  const trackId = built.tracks[0].id;
  const clipId = built.tracks[0].clips[0].id;
  host.showClip(clipId);
  await clock.sleep(options.settleMs);

  const mixer = () => {
    const track = project?.tracks.find((t) => t.id === trackId);
    if (!track) throw new Error("the benchmark's track went");
    return track.mixer;
  };

  say("slider steps");
  const sliderSteps: number[] = [];
  for (let i = 0; i < options.sliderSteps; i++) {
    const volumeDb = -1 - (i % 12);
    sliderSteps.push(await step(() => setTrackMixer(trackId, { ...mixer(), volumeDb })));
  }

  say("note deletes");
  const noteDeletes: number[] = [];
  for (let i = 0; i < options.noteDeletes; i++) {
    const clip = project?.tracks.flatMap((t) => t.clips).find((c) => c.id === clipId);
    const note = clip?.notes[0];
    if (!note) break;
    noteDeletes.push(await step(() => removeNotes(clipId, [note.id])));
  }

  say("a drag");
  const drag = await runDrag(trackId, mixer(), host, options, clock, eventDrawn);
  changes += options.dragSteps;

  return {
    song,
    ...shape,
    buildMs,
    sliderSteps,
    noteDeletes,
    ...drag,
    changes,
    events: host.events() - startEvents,
  };
}

/**
 * A drag of the track's volume, as the slider makes one: a step every
 * frame, all in one gesture, each sent without waiting for the last reply.
 * Each reply is shown as it arrives.
 */
async function runDrag(
  trackId: string,
  mixer: ProjectView["tracks"][number]["mixer"],
  host: BenchmarkHost,
  options: BenchmarkOptions,
  clock: Clock,
  eventDrawn: (before: number, expected: number) => Promise<number | null>,
): Promise<Pick<LoadResult, "dragSteps" | "dragFrames" | "dragMs">> {
  const gesture = nextGesture();
  const before = host.events();
  const dragSteps: number[] = [];
  const dragFrames: number[] = [];
  let recording = true;
  let lastFrame: number | null = null;
  const record = async () => {
    while (recording) {
      const time = await clock.frame();
      if (lastFrame !== null && recording) dragFrames.push(time - lastFrame);
      lastFrame = time;
    }
  };
  const recorder = record();

  const start = clock.now();
  const replies: Promise<void>[] = [];
  for (let i = 0; i < options.dragSteps; i++) {
    await clock.frame();
    const sent = clock.now();
    const volumeDb = -10 - i * 0.5;
    replies.push(
      setTrackMixer(trackId, { ...mixer, volumeDb }, gesture).then(async (view) => {
        host.apply(view);
        await clock.frame();
        dragSteps.push(clock.now() - sent);
      }),
    );
  }
  await Promise.all(replies);
  const end = (await eventDrawn(before, options.dragSteps)) ?? clock.now();
  recording = false;
  await recorder;
  return { dragSteps, dragFrames, dragMs: end - start };
}
