// Develop → Run Benchmark: builds each test song, works the controls by
// itself through the same commands the UI sends, and times each change from
// sending it to the screen showing it (RFC-004, "The benchmark stays"). It
// needs the real web view, so it can't run in CI.

import {
  type ProjectView,
  type TestSong,
  type Update,
  buildTestSong,
  removeNotes,
  setTrackMixer,
} from "../backend";
import { DragSteps } from "../dragSteps";
import { nextGesture } from "../useGesture";
import type { LoadResult, Report } from "./report";

/** What the benchmark needs from the app. */
export interface BenchmarkHost {
  /** Shows an update Rust sent back, as a reply to one of the app's own commands is shown. */
  apply(update: Update): void;
  /** The project as the app now draws it. */
  project(): ProjectView | null;
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
  /**
   * A real drag makes a step every frame, and sends them as the app does:
   * one at a time, latest wins.
   */
  dragSteps: number;
  /**
   * How long to wait for a `project-changed` event once a reply is drawn.
   * Changes the UI asks for send none (RFC-004, part 1), so the wait isn't
   * counted and the report shows 0 events. If one ever comes back, the step
   * isn't done until it's drawn too, as before UTA-27.
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
  const step = async (send: () => Promise<Update>): Promise<number> => {
    const before = host.events();
    const start = clock.now();
    const update = await send();
    changes += 1;
    host.apply(update);
    project = host.project();
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
  let clipId = built.tracks[0].clips[0].id;
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
    // A clip that runs out of notes (the wide song's hold 8) hands over to
    // the next one on the track, shown in the piano roll in its turn.
    const clip = project?.tracks
      .find((t) => t.id === trackId)
      ?.clips.find((c) => c.notes.length > 0);
    if (!clip) break;
    if (clip.id !== clipId) {
      clipId = clip.id;
      host.showClip(clipId);
      // Opening it is drawn before the next delete is timed.
      await clock.frame();
    }
    const [clipNow, note] = [clipId, clip.notes[0]];
    noteDeletes.push(await step(() => removeNotes(clipNow, [note.id])));
  }

  say("a drag");
  const drag = await runDrag(trackId, mixer(), host, options, clock, eventDrawn);
  changes += drag.dragSent;

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
 * frame, all in one gesture, sent through the app's `DragSteps`, so one is
 * in flight at a time and newer ones replace those waiting. Each reply is
 * shown as it arrives. A step is timed from when it's made to the frame
 * that draws it, or a newer step that replaced it.
 */
async function runDrag(
  trackId: string,
  mixer: ProjectView["tracks"][number]["mixer"],
  host: BenchmarkHost,
  options: BenchmarkOptions,
  clock: Clock,
  eventDrawn: (before: number, expected: number) => Promise<number | null>,
): Promise<Pick<LoadResult, "dragSteps" | "dragSent" | "dragFrames" | "dragMs">> {
  const gesture = nextGesture();
  const drags = new DragSteps();
  const before = host.events();
  // When each step was made, and how long until it was drawn.
  const made: number[] = [];
  const dragSteps: number[] = [];
  let dragSent = 0;
  // Each frame's time and the time since the one before it.
  const frames: { at: number; gap: number }[] = [];
  let recording = true;
  let lastFrame: number | null = null;
  const record = async () => {
    while (recording) {
      const time = await clock.frame();
      if (lastFrame !== null && recording) frames.push({ at: time, gap: time - lastFrame });
      lastFrame = time;
    }
  };
  const recorder = record();

  const start = clock.now();
  let finished: () => void = () => {};
  let failed: (reason: unknown) => void = () => {};
  const allDrawn = new Promise<void>((resolve, reject) => {
    finished = resolve;
    failed = reject;
  });
  for (let i = 0; i < options.dragSteps; i++) {
    await clock.frame();
    made.push(clock.now());
    const volumeDb = -10 - i * 0.5;
    drags.step(gesture, async () => {
      dragSent += 1;
      try {
        const update = await setTrackMixer(trackId, { ...mixer, volumeDb }, gesture);
        host.apply(update);
        await clock.frame();
      } catch (reason) {
        failed(reason);
        return;
      }
      // This reply draws step `i`, and every step before it still waiting.
      const drawn = clock.now();
      while (dragSteps.length <= i) dragSteps.push(drawn - made[dragSteps.length]);
      if (dragSteps.length === options.dragSteps) finished();
    });
  }
  await allDrawn;
  // If no events come, the drag ended when its last reply was drawn: the
  // wait for them, and the idle frames during it, aren't counted.
  const drawn = clock.now();
  const end = (await eventDrawn(before, dragSent)) ?? drawn;
  recording = false;
  await recorder;
  const dragFrames = frames.filter((frame) => frame.at <= end).map((frame) => frame.gap);
  return { dragSteps, dragSent, dragFrames, dragMs: end - start };
}
