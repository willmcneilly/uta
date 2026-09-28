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
  loopBars: number;
  minLoopBars: number;
  maxLoopBars: number;
  /** Where the loop starts, in ticks. */
  loopStart: number;
  /** How long the loop is, in ticks. */
  loopLength: number;
  ticksPerQuarter: number;
  /** Always 4 for now (4/4). */
  beatsPerBar: number;
  /** The limits of the synth's settings, the same for every track. */
  synthLimits: SynthLimits;
  /** The project's one track. */
  track: TrackView;
}

/** A track: its synth and its one clip. */
export interface TrackView {
  id: string;
  synth: SynthView;
  clip: ClipView;
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
  /** The playhead, in ticks from the start of the song. It stays inside the loop. */
  playhead: number;
  /** The loudest sample since the last frame, as a linear level. */
  peak: number;
  dropouts: number;
  output: OutputView;
}

/** Sent with a ProjectView after every change, including undo and redo from the menu. */
export const PROJECT_CHANGED = "project-changed";

/** Sent with the item's ID when Copy, Paste or Duplicate is chosen from the Edit menu. */
export const EDIT_MENU = "edit-menu";

export type EditMenuItem = "copy" | "paste" | "duplicate";

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

/** Changes with the same `gesture` (one drag) undo as one step. */
export function setLoopLength(bars: number, gesture?: number): Promise<ProjectView> {
  return invoke<ProjectView>("set_loop_length", { bars, gesture: gesture ?? null });
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

/** Plays a note briefly, without changing the project, even while stopped. */
export function auditionNote(pitch: number, velocity: number): Promise<void> {
  return invoke<void>("audition_note", { pitch, velocity });
}

export function play(): Promise<void> {
  return invoke<void>("play");
}

export function stop(): Promise<void> {
  return invoke<void>("stop");
}

export function setBufferSize(size: number): Promise<void> {
  return invoke<void>("set_buffer_size", { size });
}

export function onProjectChanged(handler: (project: ProjectView) => void): Promise<UnlistenFn> {
  return listen<ProjectView>(PROJECT_CHANGED, (event) => handler(event.payload));
}

export function onEditMenu(handler: (item: EditMenuItem) => void): Promise<UnlistenFn> {
  return listen<EditMenuItem>(EDIT_MENU, (event) => handler(event.payload));
}

/** Starts the stream of frames. Rust keeps only the latest subscriber. */
export async function subscribe(onFrame: (frame: Frame) => void): Promise<Channel<Frame>> {
  const channel = new Channel<Frame>(onFrame);
  await invoke("subscribe", { onFrame: channel });
  return channel;
}
