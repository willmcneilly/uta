import { useEffect, useRef, useState } from "react";
import "./App.css";
import {
  type Frame,
  type OutputView,
  type ProjectView,
  type SynthParam,
  addNotes,
  auditionNote,
  cancelGesture,
  getProject,
  onEditMenu,
  onProjectChanged,
  play,
  removeNotes,
  setBufferSize,
  setLoopLength,
  setNotes,
  trimNotes,
  setSynthParam,
  setTempo,
  setVolume,
  stop,
  subscribe,
} from "./backend";
import { Meter } from "./Meter";
import { MeterLevel } from "./meterLevel";
import { type BarBeat, barBeat } from "./musicalTime";
import { Output } from "./Output";
import { FrameTime } from "./pianoRoll/FrameTime";
import { FrameStats } from "./pianoRoll/frameStats";
import { type NoteEditor, PianoRoll, type PianoRollHandle } from "./pianoRoll/PianoRoll";
import { PlayheadClock } from "./pianoRoll/playhead";
import type { RendererFactory } from "./pianoRoll/renderer";
import { SynthPanel } from "./SynthPanel";
import { Transport } from "./Transport";
import { Volume } from "./Volume";

/** The slow-changing part of a frame: what the text on screen shows. */
interface Status {
  playing: boolean;
  /** Bars and beats, so it re-renders once a beat, not sixty times a second. */
  position: BarBeat;
  dropouts: number;
  output: OutputView;
}

function toStatus(frame: Frame, project: ProjectView | null): Status {
  return {
    playing: frame.playing,
    position: barBeat(frame.playhead, project?.ticksPerQuarter ?? 960, project?.beatsPerBar ?? 4),
    dropouts: frame.dropouts,
    output: frame.output,
  };
}

function sameStatus(a: Status | null, b: Status): boolean {
  return a !== null && JSON.stringify(a) === JSON.stringify(b);
}

interface Props {
  /** Draws the piano roll. Tests pass their own; the app uses Canvas 2D. */
  createRenderer?: RendererFactory;
}

function App({ createRenderer }: Props) {
  const [project, setProject] = useState<ProjectView | null>(null);
  const [status, setStatus] = useState<Status | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [changingBuffer, setChangingBuffer] = useState(false);
  const [level] = useState(() => new MeterLevel());
  const [clock] = useState(() => new PlayheadClock());
  const [stats] = useState(() => new FrameStats());
  // The frame stream reads the latest project without re-subscribing.
  const projectRef = useRef<ProjectView | null>(null);
  const pianoRoll = useRef<PianoRollHandle>(null);

  const report = (reason: unknown) => setError(String(reason));

  useEffect(() => {
    let active = true;
    getProject()
      .then((view) => active && setProject(view))
      .catch((reason: unknown) => active && setError(String(reason)));
    // Undo, Redo and the Develop menu's changes arrive this way.
    const unlisten = onProjectChanged((view) => active && setProject(view));
    return () => {
      active = false;
      void unlisten.then((stopListening) => stopListening());
    };
  }, []);

  // Copy, Paste and Duplicate from the Edit menu act on the piano roll.
  useEffect(() => {
    const unlisten = onEditMenu((item) => pianoRoll.current?.[item]());
    return () => void unlisten.then((stopListening) => stopListening());
  }, []);

  useEffect(() => {
    projectRef.current = project;
    if (project) {
      clock.setTiming({
        bpm: project.bpm,
        ticksPerQuarter: project.ticksPerQuarter,
        loopStart: project.loopStart,
        loopLength: project.loopLength,
      });
    }
  }, [project, clock]);

  useEffect(() => {
    let active = true;
    const channel = subscribe((frame) => {
      if (!active) return;
      level.push(frame.peak);
      clock.report(frame.playhead, frame.playing, performance.now());
      const next = toStatus(frame, projectRef.current);
      setStatus((previous) => (sameStatus(previous, next) ? previous : next));
    });
    channel.catch((reason: unknown) => active && setError(String(reason)));
    return () => {
      active = false;
    };
  }, [level, clock]);

  const changeVolume = (volumeDb: number, gesture?: number) => {
    setVolume(volumeDb, gesture).then(setProject, report);
  };

  const changeTempo = (bpm: number, gesture?: number) => {
    setTempo(bpm, gesture).then(setProject, report);
  };

  const changeLoopLength = (bars: number, gesture?: number) => {
    setLoopLength(bars, gesture).then(setProject, report);
  };

  // The synth panel edits the project's one track.
  const changeSynth = (param: SynthParam, gesture?: number) => {
    if (!project) return;
    setSynthParam(project.track.id, param, gesture).then(setProject, report);
  };

  // The piano roll edits the project's one clip.
  const clip = project?.track.clip.id ?? "";
  const editor: NoteEditor = {
    add: (notes, gesture) => void addNotes(clip, notes, gesture).then(setProject, report),
    set: (notes, gesture) => void setNotes(clip, notes, gesture).then(setProject, report),
    remove: (ids) => void removeNotes(clip, ids).then(setProject, report),
    trim: (ids, gesture) => void trimNotes(clip, ids, gesture).then(setProject, report),
    cancel: (gesture) => void cancelGesture(gesture).then(setProject, report),
    audition: (pitch, velocity) => void auditionNote(pitch, velocity).catch(report),
  };

  const changeBuffer = (size: number) => {
    setChangingBuffer(true);
    setBufferSize(size)
      .catch(report)
      .finally(() => setChangingBuffer(false));
  };

  return (
    <main className="app">
      <Transport
        project={project}
        playing={status?.playing ?? false}
        position={status?.position ?? { bar: 1, beat: 1 }}
        onPlay={() => play().catch(report)}
        onStop={() => stop().catch(report)}
        onTempo={changeTempo}
        onLoopLength={changeLoopLength}
      />

      <div className="panels">
        <section className="level" aria-label="Level">
          {project && <Volume project={project} onChange={changeVolume} />}
          <Meter level={level} />
        </section>

        {status && (
          <Output
            output={status.output}
            dropouts={status.dropouts}
            busy={changingBuffer}
            onBufferSize={changeBuffer}
          />
        )}
      </div>

      {project && <SynthPanel project={project} onChange={changeSynth} />}

      {error && (
        <p className="error" role="alert">
          {error}
        </p>
      )}

      {project && (
        <PianoRoll
          ref={pianoRoll}
          project={project}
          editor={editor}
          clock={clock}
          stats={stats}
          createRenderer={createRenderer}
        />
      )}

      <FrameTime stats={stats} />
    </main>
  );
}

export default App;
