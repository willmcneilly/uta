// The Rust side, as the UI sees it. Every change goes to Rust as a Tauri
// command; the UI only renders what comes back. The shapes mirror
// app/src-tauri/src/uta.rs.

import { Channel, invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

/** What the UI shows of the project. Sent after every change to it. */
export interface ProjectView {
  volumeDb: number;
  minVolumeDb: number;
  maxVolumeDb: number;
  canUndo: boolean;
  canRedo: boolean;
  /** Quarter notes per minute. */
  bpm: number;
  minBpm: number;
  maxBpm: number;
  /** Where the loop starts, in ticks. */
  loopStart: number;
  /** How long the loop is, in ticks. */
  loopLength: number;
  /** Whether the loop is switched on. */
  loopEnabled: boolean;
  /** Where the song ends, in ticks: one bar after the last clip ends. */
  songEnd: number;
  ticksPerQuarter: number;
  /** Always 4 for now (4/4). */
  beatsPerBar: number;
  /** The limits of the synth's settings, the same for every track. */
  synthLimits: SynthLimits;
  /** The limits of a track's volume and pan. */
  mixerLimits: MixerLimits;
  /** The most tracks a project can have. */
  maxTracks: number;
  /** Every track, in order from the top. */
  tracks: TrackView[];
}

/** A track: its name, mixer strip, synth and clips. */
export interface TrackView {
  id: string;
  /** Such as "Synth 2". It stays the same when tracks move or go. */
  name: string;
  mixer: MixerView;
  synth: SynthView;
  /** In order of start, then ID. They may overlap. */
  clips: ClipView[];
}

/** A track's volume, pan, mute and solo. */
export interface MixerView {
  volumeDb: number;
  /** From -1 (left) through 0 (centre) to 1 (right). */
  pan: number;
  mute: boolean;
  solo: boolean;
}

/** The limits of a track's volume and pan. */
export interface MixerLimits {
  volumeDb: Limits;
  pan: Limits;
}

export type Waveform = "sine" | "triangle" | "saw" | "square";

/** The synth's settings. */
export interface SynthView {
  waveform: Waveform;
  cutoffHz: number;
  resonance: number;
  attackSeconds: number;
  decaySeconds: number;
  /** A linear level, from 0 to 1. */
  sustain: number;
  releaseSeconds: number;
}

/** An inclusive range: `[min, max]`. */
export type Limits = [number, number];

/** The limits of each synth setting that has a range. */
export interface SynthLimits {
  cutoffHz: Limits;
  resonance: Limits;
  /** For attack, decay and release. */
  envelopeSeconds: Limits;
  sustain: Limits;
}

/** One synth setting and its new value, as `set_synth_param` takes it. */
export type SynthParam =
  | { name: "waveform"; value: Waveform }
  | {
      name:
        | "cutoff_hz"
        | "resonance"
        | "attack_seconds"
        | "decay_seconds"
        | "sustain"
        | "release_seconds";
      value: number;
    };

/** A clip and its notes. Positions and lengths are in ticks. */
export interface ClipView {
  id: string;
  /** Where the clip starts, from the start of the song. */
  start: number;
  length: number;
  notes: NoteView[];
}

export interface NoteView {
  id: string;
  /** The MIDI note number, from 0 to 127. Middle C (C4) is 60. */
  pitch: number;
  /** From 1 to 127. */
  velocity: number;
  /** In ticks from the start of its clip. */
  start: number;
  length: number;
}

export type OutputState = "running" | "waiting" | "failed";

/** The output device. */
export interface OutputView {
  state: OutputState;
  /** The device playing, or the last one that did. */
  device: string | null;
  sampleRate: number | null;
  /** The buffer size the stream is actually using. */
  bufferSize: number;
  /** The buffer size picked in the UI. */
  requestedBufferSize: number;
  /** The buffer sizes the device supports, of 32, 64 and 128. */
  bufferSizes: number[];
}

/** Everything fast-changing, sent once per screen frame. */
export interface Frame {
  playing: boolean;
  /**
   * The playhead, in ticks from the start of the song. While stopped, the
   * play start, or where it paused.
   */
  playhead: number;
  /** The master's loudest sample since the last frame, as a linear level. */
  peak: number;
  /** Each track's loudest sample since the last frame, by track ID. */
  trackPeaks: Record<string, number>;
  /** Samples the master has clipped since the app started. */
  clips: number;
  dropouts: number;
  /**
   * The audio thread's slowest block since the last frame, as a share of its
   * deadline. 1 or more is late.
   */
  slowestBlock: number;
  output: OutputView;
}

/**
 * Sent, with no payload, after a change the UI didn't ask for: Undo and Redo
 * from the menu bar. The UI fetches the project with `getProject`. A change
 * the UI asked for comes back only as its command's reply.
 */
export const PROJECT_CHANGED = "project-changed";

/** Sent with the item's ID when Copy, Paste or Duplicate is chosen from the Edit menu. */
export const EDIT_MENU = "edit-menu";

export type EditMenuItem = "copy" | "paste" | "duplicate";

/** Sent with the item's ID when an item is chosen from the Track menu. */
export const TRACK_MENU = "track-menu";

export type TrackMenuItem = "add-track" | "delete-track" | "duplicate-track";

/** Sent with the item's ID when an item is chosen from the Develop menu. */
export const DEVELOP_MENU = "develop-menu";

export type DevelopMenuItem = "add-stress-notes" | "run-benchmark";

export function getProject(): Promise<ProjectView> {
  return invoke<ProjectView>("get_project");
}

/** Changes with the same `gesture` (one drag) undo as one step. */
export function setVolume(volumeDb: number, gesture?: number): Promise<ProjectView> {
  return invoke<ProjectView>("set_volume", { volumeDb, gesture: gesture ?? null });
}

/** Changes with the same `gesture` (one drag) undo as one step. */
export function setTempo(bpm: number, gesture?: number): Promise<ProjectView> {
  return invoke<ProjectView>("set_tempo", { bpm, gesture: gesture ?? null });
}

/**
 * Sets the loop region in whole bars: the bar it starts at, counting from 0,
 * and how many bars long it is. Changes with the same `gesture` (one drag)
 * undo as one step.
 */
export function setLoop(startBar: number, bars: number, gesture?: number): Promise<ProjectView> {
  return invoke<ProjectView>("set_loop", { startBar, bars, gesture: gesture ?? null });
}

/** Switches the loop on or off. One undo step. */
export function setLoopEnabled(enabled: boolean): Promise<ProjectView> {
  return invoke<ProjectView>("set_loop_enabled", { enabled });
}

/**
 * Sets one of a track's synth settings. Changes to the same setting with
 * the same `gesture` (one drag) undo as one step.
 */
export function setSynthParam(
  track: string,
  param: SynthParam,
  gesture?: number,
): Promise<ProjectView> {
  return invoke<ProjectView>("set_synth_param", { track, param, gesture: gesture ?? null });
}

/**
 * Sets a track's volume, pan, mute and solo. Changes to the same track with
 * the same `gesture` (one drag) undo as one step.
 */
export function setTrackMixer(
  track: string,
  mixer: MixerView,
  gesture?: number,
): Promise<ProjectView> {
  return invoke<ProjectView>("set_track_mixer", { track, mixer, gesture: gesture ?? null });
}

/** Solos a track on its own, or unsolos it if it already is. One undo step. */
export function soloTrackAlone(track: string): Promise<ProjectView> {
  return invoke<ProjectView>("solo_track_alone", { track });
}

/** Adds a synth track below the others, with the ID picked here. */
export function addTrack(id: string): Promise<ProjectView> {
  return invoke<ProjectView>("add_track", { id });
}

/** Adds a copy of `track` below it, with the ID `id`. Rust picks its clips' and notes' IDs. */
export function duplicateTrack(track: string, id: string): Promise<ProjectView> {
  return invoke<ProjectView>("duplicate_track", { track, id });
}

export function removeTrack(track: string): Promise<ProjectView> {
  return invoke<ProjectView>("remove_track", { track });
}

/** Moves a track to `index` in the order, counting from 0 at the top. */
export function moveTrack(track: string, index: number): Promise<ProjectView> {
  return invoke<ProjectView>("move_track", { track, index });
}

/** Where a clip is, as `set_clips` takes it. Positions and lengths are in ticks. */
export interface ClipPosition {
  id: string;
  track: string;
  /** From the start of the song. */
  start: number;
  length: number;
}

/** Adds an empty clip to `track`, with the ID `id` picked here. One undo step. */
export function addClip(
  track: string,
  id: string,
  start: number,
  length: number,
): Promise<ProjectView> {
  return invoke<ProjectView>("add_clip", { track, id, start, length });
}

/**
 * Sets clips' track, start and length: a move or a resize. Changes with the
 * same `gesture` (one drag) undo as one step.
 */
export function setClips(clips: ClipPosition[], gesture?: number): Promise<ProjectView> {
  return invoke<ProjectView>("set_clips", { clips, gesture: gesture ?? null });
}

/**
 * A copy of a clip to add, for `pasteClips`: the clip as it was copied, with
 * where it goes and the ID picked for it. Its notes keep the IDs they were
 * copied with; Rust gives them new ones.
 */
export interface PastedClip {
  id: string;
  track: string;
  /** From the start of the song. */
  start: number;
  length: number;
  notes: NoteView[];
}

/**
 * Adds copies of clips, as one undo step: a paste or a duplicate. Each
 * copy's ID is picked here; Rust picks its notes'.
 */
export function pasteClips(clips: PastedClip[]): Promise<ProjectView> {
  return invoke<ProjectView>("paste_clips", { clips });
}

/** Deletes clips, with their notes, as one undo step. */
export function removeClips(clips: string[]): Promise<ProjectView> {
  return invoke<ProjectView>("remove_clips", { clips });
}

/** Fills a clip with a few thousand notes, as one undo step. */
export function addStressNotes(clip: string): Promise<ProjectView> {
  return invoke<ProjectView>("add_stress_notes", { clip });
}

/**
 * The benchmark's test songs (RFC-004, "What we measured"). Rust knows each
 * one's recipe: heavy is 12 tracks × 64 one-bar clips × 64 notes, wide is
 * 32 × 200 × 8, and check-7 is 7 × 28 × 3,000 stress notes.
 */
export type TestSong = "heavy" | "wide" | "check-7";

/** Replaces every track with a test song's, as one undo step. */
export function buildTestSong(song: TestSong): Promise<ProjectView> {
  return invoke<ProjectView>("build_test_song", { song });
}

/** Adds notes to a clip. Each note's ID is picked here, before Rust applies it. */
export function addNotes(clip: string, notes: NoteView[], gesture?: number): Promise<ProjectView> {
  return invoke<ProjectView>("add_notes", { clip, notes, gesture: gesture ?? null });
}

/**
 * Sets every value of existing notes. Changes with the same `gesture` (one
 * drag) undo as one step, together with an `addNotes` of the same notes.
 */
export function setNotes(clip: string, notes: NoteView[], gesture?: number): Promise<ProjectView> {
  return invoke<ProjectView>("set_notes", { clip, notes, gesture: gesture ?? null });
}

export function removeNotes(clip: string, notes: string[]): Promise<ProjectView> {
  return invoke<ProjectView>("remove_notes", { clip, notes });
}

/**
 * Trims the notes of the same pitch that `notes` cover, so none hides behind
 * another: when a drag of them ends, or a paste lands. It joins `gesture`'s
 * undo step, and ends the gesture.
 */
export function trimNotes(clip: string, notes: string[], gesture: number): Promise<ProjectView> {
  return invoke<ProjectView>("trim_notes", { clip, notes, gesture });
}

/** Puts back everything `gesture` (one drag) changed, as if it never happened. */
export function cancelGesture(gesture: number): Promise<ProjectView> {
  return invoke<ProjectView>("cancel_gesture", { gesture });
}

/** Plays a note briefly on a track, without changing the project, even while stopped. */
export function auditionNote(track: string, pitch: number, velocity: number): Promise<void> {
  return invoke<void>("audition_note", { track, pitch, velocity });
}

/** Plays from the play start. */
export function play(): Promise<void> {
  return invoke<void>("play");
}

/** Stops, and goes back to the play start. */
export function stop(): Promise<void> {
  return invoke<void>("stop");
}

/** Stops where the playhead is, for `resume` to carry on from. */
export function pause(): Promise<void> {
  return invoke<void>("pause");
}

/** Plays from where it paused, or from the play start if it didn't. */
export function resume(): Promise<void> {
  return invoke<void>("resume");
}

/**
 * While stopped, moves the play start to `ticks` from the start of the song.
 * While playing, jumps there, and the play start stays where it was.
 */
export function locate(ticks: number): Promise<void> {
  return invoke<void>("locate", { ticks });
}

export function setBufferSize(size: number): Promise<void> {
  return invoke<void>("set_buffer_size", { size });
}

export function onProjectChanged(handler: () => void): Promise<UnlistenFn> {
  return listen<null>(PROJECT_CHANGED, () => handler());
}

export function onEditMenu(handler: (item: EditMenuItem) => void): Promise<UnlistenFn> {
  return listen<EditMenuItem>(EDIT_MENU, (event) => handler(event.payload));
}

export function onTrackMenu(handler: (item: TrackMenuItem) => void): Promise<UnlistenFn> {
  return listen<TrackMenuItem>(TRACK_MENU, (event) => handler(event.payload));
}

export function onDevelopMenu(handler: (item: DevelopMenuItem) => void): Promise<UnlistenFn> {
  return listen<DevelopMenuItem>(DEVELOP_MENU, (event) => handler(event.payload));
}

/** Starts the stream of frames. Rust keeps only the latest subscriber. */
export async function subscribe(onFrame: (frame: Frame) => void): Promise<Channel<Frame>> {
  const channel = new Channel<Frame>(onFrame);
  await invoke("subscribe", { onFrame: channel });
  return channel;
}
