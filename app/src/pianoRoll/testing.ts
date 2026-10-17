// Test helpers: a project view and a renderer that records what it's asked
// to draw instead of drawing.

import { act } from "@testing-library/react";
import { emit } from "@tauri-apps/api/event";
import type {
  ClipNotes,
  ClipView,
  ChoiceSettingView,
  FilterMode,
  DrumSettingView,
  KickModel,
  KitRowView,
  Limits,
  MixerView,
  NoteView,
  NumberSettingView,
  Outline,
  ProjectView,
  SynthNumber,
  SynthParam,
  SynthSettingView,
  SynthToggle,
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
  osc1Wave: Waveform;
  osc1Octave: number;
  osc1Width: number;
  osc2Wave: Waveform;
  osc2Octave: number;
  osc2PitchSemitones: number;
  osc2Sync: boolean;
  subOctave: number;
  osc1Level: number;
  osc2Level: number;
  subLevel: number;
  noiseLevel: number;
  filterMode: FilterMode;
  cutoffHz: number;
  resonance: number;
  filterEnvOctaves: number;
  filterKey: number;
  filterAttackSeconds: number;
  filterDecaySeconds: number;
  filterSustain: number;
  filterReleaseSeconds: number;
  attackSeconds: number;
  decaySeconds: number;
  sustain: number;
  releaseSeconds: number;
  oscRetrigger: boolean;
}

/** Each synth setting's field in {@link SynthValues}. */
export const SYNTH_FIELDS: Record<SynthParam["name"], keyof SynthValues> = {
  osc1_wave: "osc1Wave",
  osc1_octave: "osc1Octave",
  osc1_width: "osc1Width",
  osc2_wave: "osc2Wave",
  osc2_octave: "osc2Octave",
  osc2_pitch_semitones: "osc2PitchSemitones",
  osc2_sync: "osc2Sync",
  sub_octave: "subOctave",
  osc1_level: "osc1Level",
  osc2_level: "osc2Level",
  sub_level: "subLevel",
  noise_level: "noiseLevel",
  filter_mode: "filterMode",
  cutoff_hz: "cutoffHz",
  resonance: "resonance",
  filter_env_octaves: "filterEnvOctaves",
  filter_key: "filterKey",
  filter_attack_seconds: "filterAttackSeconds",
  filter_decay_seconds: "filterDecaySeconds",
  filter_sustain: "filterSustain",
  filter_release_seconds: "filterReleaseSeconds",
  attack_seconds: "attackSeconds",
  decay_seconds: "decaySeconds",
  sustain: "sustain",
  release_seconds: "releaseSeconds",
  osc_retrigger: "oscRetrigger",
};

/** A new track's synth settings, as Rust's defaults have them. */
export const SYNTH_DEFAULTS: SynthValues = {
  osc1Wave: "saw",
  osc1Octave: 0,
  osc1Width: 0.5,
  osc2Wave: "saw",
  osc2Octave: 0,
  osc2PitchSemitones: 0,
  osc2Sync: false,
  subOctave: -1,
  osc1Level: 1,
  osc2Level: 0,
  subLevel: 0,
  noiseLevel: 0,
  filterMode: "low_pass",
  cutoffHz: 20_000,
  resonance: 0,
  filterEnvOctaves: 0,
  filterKey: 0,
  filterAttackSeconds: 0.001,
  filterDecaySeconds: 0.3,
  filterSustain: 0,
  filterReleaseSeconds: 0.2,
  attackSeconds: 0.005,
  decaySeconds: 0.2,
  sustain: 0.7,
  releaseSeconds: 0.2,
  oscRetrigger: true,
};

/** A stand-in for the sentence Rust sends on how a setting sounds. */
const SOUNDS = "How it sounds.";

/** Every wave, as Rust offers them. */
const WAVES: ChoiceSettingView<string, Waveform>["options"] = [
  { value: "sine", label: "Sine" },
  { value: "triangle", label: "Triangle" },
  { value: "saw", label: "Saw" },
  { value: "pulse", label: "Pulse" },
];

/** Every filter mode, as Rust offers them. */
const FILTER_MODES: ChoiceSettingView<string, FilterMode>["options"] = [
  { value: "low_pass", label: "Low-pass" },
  { value: "band_pass", label: "Band-pass" },
  { value: "high_pass", label: "High-pass" },
];

/**
 * How a control moves through each unit, as Rust has it: frequencies and
 * times on a log scale, a pitch offset fine near zero, and levels, amounts
 * and octaves on a linear one.
 */
const scaleOf = (unit: Unit) =>
  unit === "hz" || unit === "seconds" ? "log" : unit === "semitones" ? "cubic" : "linear";

/** The values in `values` that are numbers, by field. */
type NumberField = {
  [K in keyof SynthValues]: SynthValues[K] extends number ? K : never;
}[keyof SynthValues];

/**
 * The synth's settings with `values`, described as Rust describes them, in
 * its order, each resetting to its value in `defaults`.
 */
export function synthSettings(
  values: SynthValues = SYNTH_DEFAULTS,
  defaults: SynthValues = SYNTH_DEFAULTS,
): SynthSettingView[] {
  const number = (
    name: SynthNumber["name"],
    label: string,
    unit: Unit,
    limits: Limits,
    step?: number,
  ): SynthSettingView => {
    const field = SYNTH_FIELDS[name] as NumberField;
    const setting: NumberSettingView<SynthNumber["name"]> = {
      kind: "number",
      name,
      label,
      sounds: SOUNDS,
      value: values[field],
      unit,
      scale: scaleOf(unit),
      limits,
      default: defaults[field],
    };
    return step === undefined ? setting : { ...setting, step };
  };
  const toggle = (name: SynthToggle, label: string): SynthSettingView => {
    const field = name === "osc2_sync" ? "osc2Sync" : "oscRetrigger";
    return {
      kind: "toggle",
      name,
      label,
      sounds: SOUNDS,
      value: values[field],
      default: defaults[field],
    };
  };
  const wave = <N extends "osc1_wave" | "osc2_wave">(
    name: N,
    label: string,
  ): ChoiceSettingView<N, Waveform> => {
    const field = name === "osc1_wave" ? "osc1Wave" : "osc2Wave";
    return {
      kind: "choice",
      name,
      label,
      sounds: SOUNDS,
      value: values[field],
      options: WAVES,
      default: defaults[field],
    };
  };
  return [
    wave("osc1_wave", "Osc 1 wave"),
    number("osc1_octave", "Osc 1 octave", "octaves", [-2, 2], 1),
    number("osc1_width", "Osc 1 width", "fraction", [0.05, 0.95]),
    wave("osc2_wave", "Osc 2 wave"),
    number("osc2_octave", "Osc 2 octave", "octaves", [-2, 2], 1),
    number("osc2_pitch_semitones", "Osc 2 pitch", "semitones", [-12, 12]),
    toggle("osc2_sync", "Osc 2 sync"),
    number("sub_octave", "Sub octave", "octaves", [-2, -1], 1),
    number("osc1_level", "Osc 1 level", "fraction", [0, 1]),
    number("osc2_level", "Osc 2 level", "fraction", [0, 1]),
    number("sub_level", "Sub level", "fraction", [0, 1]),
    number("noise_level", "Noise level", "fraction", [0, 1]),
    {
      kind: "choice",
      name: "filter_mode",
      label: "Filter mode",
      sounds: SOUNDS,
      value: values.filterMode,
      options: FILTER_MODES,
      default: defaults.filterMode,
    },
    number("cutoff_hz", "Cutoff", "hz", [20, 20_000]),
    number("resonance", "Resonance", "amount", [0, 1]),
    number("filter_env_octaves", "Env amount", "octaves", [-10, 10]),
    number("filter_key", "Key", "fraction", [0, 1]),
    number("filter_attack_seconds", "Filter attack", "seconds", [0.001, 10]),
    number("filter_decay_seconds", "Filter decay", "seconds", [0.001, 10]),
    number("filter_sustain", "Filter sustain", "fraction", [0, 1]),
    number("filter_release_seconds", "Filter release", "seconds", [0.001, 10]),
    number("attack_seconds", "Attack", "seconds", [0.001, 10]),
    number("decay_seconds", "Decay", "seconds", [0.001, 10]),
    number("sustain", "Sustain", "fraction", [0, 1]),
    number("release_seconds", "Release", "seconds", [0.001, 10]),
    toggle("osc_retrigger", "Osc retrigger"),
  ];
}

/** A track with the default sound and mixer, and `clips`. */
export function trackView(id: string, name: string, clips: ClipView[] = []): TrackView {
  return {
    id,
    name,
    mixer: { ...MIXER_DEFAULTS },
    source: { kind: "synth", settings: synthSettings() },
    effects: [],
    clips,
  };
}

/** The synth's values in `settings`: each one's value, or each one's default. */
function valuesOf(settings: SynthSettingView[], which: "value" | "default"): SynthValues {
  const values = { ...SYNTH_DEFAULTS };
  for (const setting of settings) Object.assign(values, { [SYNTH_FIELDS[setting.name]]: setting[which] });
  return values;
}

/** A synth track's values, read back from the settings Rust sent. */
export function synthOf(track: TrackView): SynthValues {
  if (track.source.kind !== "synth") throw new Error(`${track.name} isn't a synth track`);
  return valuesOf(track.source.settings, "value");
}

/** `track`, a synth track, with its values set to `values`, keeping each setting's default. */
export function withSynth(track: TrackView, values: Partial<SynthValues>): TrackView {
  if (track.source.kind !== "synth") throw new Error(`${track.name} isn't a synth track`);
  const defaults = valuesOf(track.source.settings, "default");
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
    effects: [],
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
    masterEffects: [],
    effectKinds: [{ kind: "delay", name: "Delay" }],
    maxEffects: 8,
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
