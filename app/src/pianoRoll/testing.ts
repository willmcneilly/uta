// Test helpers: a project view and a renderer that records what it's asked
// to draw instead of drawing.

import type { NoteView, ProjectView } from "../backend";
import type { PlacedNote } from "./notes";
import type { GridScene, PianoRollRenderer, RendererFactory } from "./renderer";
import type { Viewport } from "./viewport";

export function projectView(overrides: Partial<ProjectView> = {}, notes: NoteView[] = []): ProjectView {
  return {
    volumeDb: -12,
    minVolumeDb: -60,
    maxVolumeDb: 0,
    canUndo: false,
    canRedo: false,
    bpm: 120,
    minBpm: 20,
    maxBpm: 300,
    loopBars: 4,
    minLoopBars: 1,
    maxLoopBars: 16,
    loopStart: 0,
    loopLength: 4 * 3840,
    ticksPerQuarter: 960,
    beatsPerBar: 4,
    track: {
      id: "track-1",
      synth: {
        waveform: "saw",
        cutoffHz: 20_000,
        resonance: 0,
        attackSeconds: 0.005,
        decaySeconds: 0.2,
        sustain: 0.7,
        releaseSeconds: 0.2,
      },
      clip: { id: "clip-1", start: 0, length: 4 * 3840, notes },
    },
    ...overrides,
  };
}

export class RecordingRenderer implements PianoRollRenderer {
  sizes: [number, number][] = [];
  grids: GridScene[] = [];
  notes: { view: Viewport; notes: readonly PlacedNote[] }[] = [];
  tops: number[] = [];

  resize(width: number, height: number): void {
    this.sizes.push([width, height]);
  }
  drawGrid(scene: GridScene): void {
    this.grids.push(scene);
  }
  drawNotes(view: Viewport, notes: readonly PlacedNote[]): void {
    this.notes.push({ view, notes });
  }
  drawTop(_view: Viewport, playhead: number): void {
    this.tops.push(playhead);
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
