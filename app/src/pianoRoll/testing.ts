// Test helpers: a project view and a renderer that records what it's asked
// to draw instead of drawing.

import { act } from "@testing-library/react";
import { emit } from "@tauri-apps/api/event";
import type {
  ClipNotes,
  ClipView,
  ChoiceSettingView,
  DrumSettingView,
  KickModel,
  KitRowView,
  Limits,
  MixerView,
  NoteView,
  Outline,
  ProjectView,
  SynthParam,
  SynthSettingView,
  TrackView,
  Unit,
  Update,
  Waveform,
} from "../backend";
import type { PlacedNote } from "./notes";
import type { GridScene, NotesScene, PianoRollRenderer, RendererFactory, TopMarks } from "./renderer";
import type { Rect, Viewport } from "./viewport";

/**
 * Emits `project-changed` as Rust does after Undo or Redo from the menu bar,
 * with no payload, and waits for the UI to fetch the project and draw it.
 * Set the mocked back end's project first.
 */
export async function announceChange(): Promise<void> {
  await act(async () => {
    await emit("project-changed");
    await new Promise((resolve) => setTimeout(resolve, 0));
  });
}

/**
 * Turns the mocked back end's project into updates, as Rust's app layer
 * does: the outline, and the notes of each clip it hasn't sent. A clip's
 * notes count as changed when they're a different array, as Rust compares
 * pointers, so a mock that leaves a clip alone keeps its array.
 */
export class Updates {
  private sequence = 0;
  private lastRevision = 0;
  private sent = new Map<string, { notes: NoteView[]; revision: number }>();

  /** The update for `project`, recording its notes as sent. */
  send(project: ProjectView): Update {
    this.sequence += 1;
    const notes: ClipNotes[] = [];
    const seen = new Set<string>();
    const outline: Outline = {
      ...project,
      tracks: project.tracks.map((track) => ({
        ...track,
        clips: track.clips.map((clip) => {
          seen.add(clip.id);
          const [revision, isNew] = this.check(clip);
          if (isNew) notes.push({ clip: clip.id, revision, notes: clip.notes });
          return { id: clip.id, start: clip.start, length: clip.length, notesRevision: revision };
        }),
      })),
    };
    for (const id of this.sent.keys()) if (!seen.has(id)) this.sent.delete(id);
    return { sequence: this.sequence, outline, notes };
  }

  /** One clip's notes at its current revision, as `get_notes` answers. */
  notes(project: ProjectView, id: string): ClipNotes {
    const clip = project.tracks.flatMap((track) => track.clips).find((c) => c.id === id);
    if (!clip) throw new Error(`unknown clip ${id}`);
    const [revision] = this.check(clip);
    return { clip: id, revision, notes: clip.notes };
  }

  private check(clip: ClipView): [number, boolean] {
    const sent = this.sent.get(clip.id);
    if (sent?.notes === clip.notes) return [sent.revision, false];
    this.lastRevision += 1;
    this.sent.set(clip.id, { notes: clip.notes, revision: this.lastRevision });
    return [this.lastRevision, true];
  }
}

/** A new track's mixer strip, as Rust sends it. */
export const MIXER_DEFAULTS: MixerView = { volumeDb: 0, pan: 0, mute: false, solo: false };

/** A synth's values, by setting, as tests write them. */
export interface SynthValues {
  waveform: Waveform;
  cutoffHz: number;
  resonance: number;
  attackSeconds: number;
  decaySeconds: number;
  sustain: number;
  releaseSeconds: number;
}

/** Each synth setting's field in {@link SynthValues}. */
export const SYNTH_FIELDS: Record<SynthParam["name"], keyof SynthValues> = {
  waveform: "waveform",
  cutoff_hz: "cutoffHz",
  resonance: "resonance",
  attack_seconds: "attackSeconds",
  decay_seconds: "decaySeconds",
  sustain: "sustain",
  release_seconds: "releaseSeconds",
};

/** A new track's synth settings, as Rust's defaults have them. */
export const SYNTH_DEFAULTS: SynthValues = {
  waveform: "saw",
  cutoffHz: 20_000,
  resonance: 0,
  attackSeconds: 0.005,
  decaySeconds: 0.2,
  sustain: 0.7,
  releaseSeconds: 0.2,
};

/** A stand-in for the sentence Rust sends on how a setting sounds. */
const SOUNDS = "How it sounds.";

/** Frequencies and times move on a log scale; levels and amounts on a linear one, as Rust has them. */
const scaleOf = (unit: Unit) => (unit === "hz" || unit === "seconds" ? "log" : "linear");

/**
 * The synth's settings with `values`, described as Rust describes them, in
 * its order, each resetting to its value in `defaults`.
 */
export function synthSettings(
  values: SynthValues = SYNTH_DEFAULTS,
  defaults: SynthValues = SYNTH_DEFAULTS,
): SynthSettingView[] {
  const number = (
    name: Exclude<SynthParam["name"], "waveform">,
    label: string,
    unit: Unit,
    limits: Limits,
  ): SynthSettingView => {
    const field = SYNTH_FIELDS[name] as Exclude<keyof SynthValues, "waveform">;
    const scale = scaleOf(unit);
    return { kind: "number", name, label, sounds: SOUNDS, value: values[field], unit, scale, limits, default: defaults[field] };
  };
  const waveform: ChoiceSettingView<"waveform", Waveform> = {
    kind: "choice",
    name: "waveform",
    label: "Waveform",
    sounds: SOUNDS,
    value: values.waveform,
    options: [
      { value: "sine", label: "Sine" },
      { value: "triangle", label: "Triangle" },
      { value: "saw", label: "Saw" },
      { value: "square", label: "Square" },
    ],
    default: defaults.waveform,
  };
  return [
    waveform,
    number("cutoff_hz", "Cutoff", "hz", [20, 20_000]),
    number("resonance", "Resonance", "amount", [0, 1]),
    number("attack_seconds", "Attack", "seconds", [0.001, 10]),
    number("decay_seconds", "Decay", "seconds", [0.001, 10]),
    number("sustain", "Sustain", "fraction", [0, 1]),
    number("release_seconds", "Release", "seconds", [0.001, 10]),
  ];
}

/** A track with the default sound and mixer, and `clips`. */
export function trackView(id: string, name: string, clips: ClipView[] = []): TrackView {
  return {
    id,
    name,
    mixer: { ...MIXER_DEFAULTS },
    source: { kind: "synth", settings: synthSettings() },
    clips,
  };
}

/** A synth track's values, read back from the settings Rust sent. */
export function synthOf(track: TrackView): SynthValues {
  if (track.source.kind !== "synth") throw new Error(`${track.name} isn't a synth track`);
  const values = { ...SYNTH_DEFAULTS };
  for (const setting of track.source.settings) {
    if (setting.kind === "choice") values.waveform = setting.value;
    else values[SYNTH_FIELDS[setting.name] as Exclude<keyof SynthValues, "waveform">] = setting.value;
  }
  return values;
}

/** `track`, a synth track, with its values set to `values`, keeping each setting's default. */
export function withSynth(track: TrackView, values: Partial<SynthValues>): TrackView {
  if (track.source.kind !== "synth") throw new Error(`${track.name} isn't a synth track`);
  const defaults = { ...SYNTH_DEFAULTS };
  for (const setting of track.source.settings) {
    if (setting.kind === "choice") defaults.waveform = setting.default;
    else defaults[SYNTH_FIELDS[setting.name] as Exclude<keyof SynthValues, "waveform">] = setting.default;
  }
  return {
    ...track,
    source: { kind: "synth", settings: synthSettings({ ...synthOf(track), ...values }, defaults) },
  };
}

/** What a setting of a drum sound is called, as Rust has it. */
const LABELS: Record<DrumSettingView["name"], string> = {
  tune_hz: "Tune",
  tone: "Tone",
  decay_seconds: "Decay",
  snappy: "Snappy",
  level_db: "Level",
  sweep: "Sweep",
  attack: "Attack",
};

/** A setting of a drum sound at its default, described as Rust describes it. */
export function setting(
  name: DrumSettingView["name"],
  value: number,
  limits: Limits,
  unit: Unit,
): DrumSettingView {
  const scale = scaleOf(unit);
  return { kind: "number", name, label: LABELS[name], sounds: SOUNDS, value, unit, scale, limits, default: value };
}

/** The kick's choice of model, the 808 chosen, as Rust sends it. */
export const KICK_MODEL: ChoiceSettingView<"model", KickModel> = {
  kind: "choice",
  name: "model",
  label: "Model",
  sounds: SOUNDS,
  value: "808",
  options: [
    { value: "808", label: "808" },
    { value: "909", label: "909" },
  ],
  default: "808",
};

/** The 909 kick's settings at their defaults, as Rust sends them with the 909 chosen. */
export const KICK_909_SETTINGS: DrumSettingView[] = [
  setting("tune_hz", 55, [45, 70], "hz"),
  setting("sweep", 0.4, [0, 1], "fraction"),
  setting("attack", 0.5, [0, 1], "fraction"),
  setting("decay_seconds", 0.5, [0.1, 1.5], "seconds"),
  setting("level_db", 0, [-60, 6], "db"),
];

/** The kit's rows as Rust sends them, bottom to top. */
export const KIT_ROWS: KitRowView[] = [
  {
    sound: "kick",
    name: "Kick",
    pitch: 36,
    settings: [
      setting("tune_hz", 49, [40, 80], "hz"),
      setting("tone", 0.2, [0, 1], "fraction"),
      setting("decay_seconds", 0.3, [0.05, 0.8], "seconds"),
      setting("level_db", 0, [-60, 6], "db"),
    ],
    shares: null,
    model: KICK_MODEL,
  },
  {
    sound: "snare",
    name: "Snare",
    pitch: 38,
    settings: [
      setting("tune_hz", 180, [140, 260], "hz"),
      setting("tone", 0.16, [0.04, 0.4], "seconds"),
      setting("snappy", 0.5, [0, 1], "fraction"),
      setting("level_db", 0, [-60, 6], "db"),
    ],
    shares: null,
    model: null,
  },
  {
    sound: "clap",
    name: "Clap",
    pitch: 39,
    settings: [
      setting("tone", 1000, [700, 2000], "hz"),
      setting("decay_seconds", 0.2, [0.05, 0.4], "seconds"),
      setting("level_db", 0, [-60, 6], "db"),
    ],
    shares: null,
    model: null,
  },
  { sound: "low_tom", name: "Low tom", pitch: 45, settings: [], shares: null, model: null },
  { sound: "high_tom", name: "High tom", pitch: 50, settings: [], shares: null, model: null },
  {
    sound: "closed_hat",
    name: "Closed hat",
    pitch: 42,
    settings: [
      setting("tune_hz", 205.3, [102.65, 410.6], "hz"),
      setting("tone", 7100, [4000, 12_000], "hz"),
      setting("decay_seconds", 0.05, [0.02, 0.15], "seconds"),
      setting("level_db", 0, [-60, 6], "db"),
    ],
    shares: null,
    model: null,
  },
  {
    sound: "open_hat",
    name: "Open hat",
    pitch: 46,
    settings: [
      setting("decay_seconds", 0.35, [0.09, 0.6], "seconds"),
      setting("level_db", 0, [-60, 6], "db"),
    ],
    shares: { sound: "closed_hat", labels: ["Tune", "Tone"] },
    model: null,
  },
  {
    sound: "cymbal",
    name: "Cymbal",
    pitch: 49,
    settings: [
      setting("tone", 0.5, [0, 1], "fraction"),
      setting("decay_seconds", 0.8, [0.35, 1.2], "seconds"),
      setting("level_db", 0, [-60, 6], "db"),
    ],
    shares: { sound: "closed_hat", labels: ["Tune"] },
    model: null,
  },
];

/** A drum track with the default kit and mixer, and `clips`. */
export function drumTrackView(id: string, name: string, clips: ClipView[] = []): TrackView {
  return {
    id,
    name,
    mixer: { ...MIXER_DEFAULTS },
    source: { kind: "drums", kit: { rows: KIT_ROWS } },
    clips,
  };
}

/** A project with one track, "Synth 1", whose one 4-bar clip holds `notes`. */
export function projectView(overrides: Partial<ProjectView> = {}, notes: NoteView[] = []): ProjectView {
  return {
    volumeDb: -12,
    minVolumeDb: -60,
    maxVolumeDb: 0,
    defaultVolumeDb: -12,
    canUndo: false,
    canRedo: false,
    bpm: 120,
    minBpm: 20,
    maxBpm: 300,
    defaultBpm: 120,
    loopStart: 0,
    loopLength: 4 * 3840,
    loopEnabled: true,
    songEnd: 5 * 3840,
    ticksPerQuarter: 960,
    beatsPerBar: 4,
    mixerLimits: { volumeDb: [-60, 6], pan: [-1, 1] },
    mixerDefaults: MIXER_DEFAULTS,
    maxTracks: 32,
    tracks: [trackView("track-1", "Synth 1", [{ id: "clip-1", start: 0, length: 4 * 3840, notes }])],
    ...overrides,
  };
}

/** The first track's first clip: the one the piano roll shows in a new project. */
export function firstClip(project: ProjectView): ClipView {
  return project.tracks[0].clips[0];
}

/** `project` with its first track's first clip's notes replaced. */
export function withFirstClipNotes(project: ProjectView, notes: NoteView[]): ProjectView {
  const [first, ...rest] = project.tracks;
  const [clip, ...clips] = first.clips;
  return { ...project, tracks: [{ ...first, clips: [{ ...clip, notes }, ...clips] }, ...rest] };
}

export class RecordingRenderer implements PianoRollRenderer {
  sizes: [number, number][] = [];
  grids: GridScene[] = [];
  notes: NotesScene[] = [];
  tops: number[] = [];
  boxes: (Rect | null)[] = [];
  marks: TopMarks[] = [];

  resize(width: number, height: number): void {
    this.sizes.push([width, height]);
  }
  drawGrid(scene: GridScene): void {
    this.grids.push(scene);
  }
  drawNotes(scene: NotesScene): void {
    this.notes.push(scene);
  }
  drawTop(_view: Viewport, playhead: number, box: Rect | null, marks: TopMarks): void {
    this.tops.push(playhead);
    this.boxes.push(box);
    this.marks.push(marks);
  }

  /** The view the notes were last drawn in. */
  lastView(): Viewport {
    const view = this.notes.at(-1)?.view;
    if (!view) throw new Error("no notes drawn yet");
    return view;
  }

  /** The IDs of the notes drawn as selected most recently, sorted. */
  lastSelected(): string[] {
    return [...(this.notes.at(-1)?.selected ?? [])].sort();
  }

  /** The notes drawn most recently. */
  lastNotes(): readonly PlacedNote[] {
    return this.notes.at(-1)?.notes ?? [];
  }
}

/** A factory that hands out one recording renderer, for tests to look at. */
export function recordingFactory(): { factory: RendererFactory; renderer: RecordingRenderer } {
  const renderer = new RecordingRenderer();
  return { factory: () => renderer, renderer };
}

type Invoke = (cmd: string, args?: unknown, options?: unknown) => Promise<unknown>;

/**
 * Holds back the mocked back end's replies to `commands`, as a slow Rust
 * would: each command still reaches the mock when it's sent, but its reply
 * waits until `reply` lets it go. Call it after `mockIPC`.
 */
export class HeldReplies {
  private waiting: (() => void)[] = [];

  constructor(commands: readonly string[]) {
    const internals = (window as unknown as { __TAURI_INTERNALS__: { invoke: Invoke } })
      .__TAURI_INTERNALS__;
    const invoke = internals.invoke;
    internals.invoke = (cmd, args, options) => {
      const reply = invoke(cmd, args, options);
      if (!commands.includes(cmd)) return reply;
      // A failure is the UI's to handle once the reply goes, not before.
      reply.catch(() => {});
      return new Promise((resolve) => this.waiting.push(() => resolve(reply)));
    };
  }

  /** How many commands are waiting for their reply. */
  get inFlight(): number {
    return this.waiting.length;
  }

  /** Lets the oldest waiting reply go, and waits for the UI to take it. */
  async reply(): Promise<void> {
    await act(async () => {
      this.waiting.shift()?.();
      await new Promise((resolve) => setTimeout(resolve, 0));
    });
  }

  /** Lets replies go, one at a time, until none is waiting. */
  async replyToAll(): Promise<void> {
    while (this.inFlight > 0) await this.reply();
  }
}
