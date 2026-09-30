import { useEffect, useRef, useState } from "react";
import "./App.css";
import {
  type Frame,
  type MixerView,
  type OutputView,
  type ProjectView,
  type SynthParam,
  type TrackView,
  addNotes,
  addStressNotes,
  addTrack,
  auditionNote,
  cancelGesture,
  duplicateTrack,
  getProject,
  moveTrack,
  onDevelopMenu,
  onEditMenu,
  onProjectChanged,
  onTrackMenu,
  play,
  removeNotes,
  removeTrack,
  setBufferSize,
  setLoopLength,
  setNotes,
  setSynthParam,
  setTempo,
  setTrackMixer,
  setVolume,
  soloTrackAlone,
  stop,
  subscribe,
  trimNotes,
} from "./backend";
import { Divider } from "./Divider";
import { Meter } from "./Meter";
import { MeterLevel, TrackLevels, clipLit } from "./meterLevel";
import { type BarBeat, barBeat } from "./musicalTime";
import { Output } from "./Output";
import { FrameTime } from "./pianoRoll/FrameTime";
import { FrameStats } from "./pianoRoll/frameStats";
import { type NoteEditor, PianoRoll, type PianoRollHandle } from "./pianoRoll/PianoRoll";
import { PlayheadClock } from "./pianoRoll/playhead";
import type { RendererFactory } from "./pianoRoll/renderer";
import { SynthPanel } from "./SynthPanel";
import { TrackHeaders } from "./TrackHeaders";
import { Transport } from "./Transport";
import { Volume } from "./Volume";

/** The slow-changing part of a frame: what the text on screen shows. */
interface Status {
  playing: boolean;
  /** Bars and beats, so it re-renders once a beat, not sixty times a second. */
  position: BarBeat;
  dropouts: number;
  /** Samples the master has clipped. */
  clips: number;
  output: OutputView;
}

function toStatus(frame: Frame, project: ProjectView | null): Status {
  return {
    playing: frame.playing,
    position: barBeat(frame.playhead, project?.ticksPerQuarter ?? 960, project?.beatsPerBar ?? 4),
    dropouts: frame.dropouts,
    clips: frame.clips,
    output: frame.output,
  };
}

function sameStatus(a: Status | null, b: Status): boolean {
  return a !== null && JSON.stringify(a) === JSON.stringify(b);
}

type Tab = "notes" | "sound";

/** The editor's height when the app opens, and its limits, in CSS pixels. */
const EDITOR_HEIGHT = 280;
const MIN_EDITOR_HEIGHT = 160;
/** What the editor always leaves above it for the transport and the tracks. */
const ABOVE_EDITOR = 260;

interface Props {
  /** Draws the piano roll. Tests pass their own; the app uses Canvas 2D. */
  createRenderer?: RendererFactory;
}

function App({ createRenderer }: Props) {
  const [project, setProject] = useState<ProjectView | null>(null);
  const [status, setStatus] = useState<Status | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [changingBuffer, setChangingBuffer] = useState(false);
  // What's selected, which tab is open, how tall the editor is, and when
  // the clip light was last clicked are the UI's own: none is project data.
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [tab, setTab] = useState<Tab>("notes");
  const [editorHeight, setEditorHeight] = useState(EDITOR_HEIGHT);
  const [seenClips, setSeenClips] = useState(0);
  const [level] = useState(() => new MeterLevel());
  const [trackLevels] = useState(() => new TrackLevels());
  const [clock] = useState(() => new PlayheadClock());
  const [stats] = useState(() => new FrameStats());
  // The frame stream reads the latest project without re-subscribing.
  const projectRef = useRef<ProjectView | null>(null);
  const pianoRoll = useRef<PianoRollHandle>(null);

  const report = (reason: unknown) => setError(String(reason));

  // The selected track, or the top one if it has gone (undoing its add, say).
  const track: TrackView | null =
    project?.tracks.find((t) => t.id === selectedId) ?? project?.tracks[0] ?? null;
  // The Notes tab shows the track's first clip until the timeline can
  // select clips.
  const clip = track?.clips[0] ?? null;

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
      trackLevels.push(frame.trackPeaks);
      clock.report(frame.playhead, frame.playing, performance.now());
      const next = toStatus(frame, projectRef.current);
      setStatus((previous) => (sameStatus(previous, next) ? previous : next));
    });
    channel.catch((reason: unknown) => active && setError(String(reason)));
    return () => {
      active = false;
    };
  }, [level, trackLevels, clock]);

  const changeVolume = (volumeDb: number, gesture?: number) => {
    setVolume(volumeDb, gesture).then(setProject, report);
  };

  const changeTempo = (bpm: number, gesture?: number) => {
    setTempo(bpm, gesture).then(setProject, report);
  };

  const changeLoopLength = (bars: number, gesture?: number) => {
    setLoopLength(bars, gesture).then(setProject, report);
  };

  const changeMixer = (id: string, mixer: MixerView, gesture?: number) => {
    setTrackMixer(id, mixer, gesture).then(setProject, report);
  };

  const soloAlone = (id: string) => {
    soloTrackAlone(id).then(setProject, report);
  };

  /** Selects `id` once Rust has sent the project with it in. */
  const showAndSelect = (id: string | null) => (view: ProjectView) => {
    setProject(view);
    setSelectedId(id);
  };

  const newTrack = () => {
    const id = crypto.randomUUID();
    addTrack(id).then(showAndSelect(id), report);
  };

  const moveTrackTo = (id: string, index: number) => {
    moveTrack(id, index).then(setProject, report);
  };

  // The Track and Develop menus act on the selected track and the clip in
  // the Notes tab. The listeners read the latest of them.
  const menuActions = {
    "add-track": newTrack,
    "duplicate-track": () => {
      if (!track) return;
      const id = crypto.randomUUID();
      duplicateTrack(track.id, id).then(showAndSelect(id), report);
    },
    "delete-track": () => {
      if (!project || !track) return;
      // The track below takes its place, or the one above if it was last.
      const index = project.tracks.indexOf(track);
      const next = project.tracks[index + 1] ?? project.tracks[index - 1] ?? null;
      removeTrack(track.id).then(showAndSelect(next?.id ?? null), report);
    },
    "add-stress-notes": () => {
      if (clip) addStressNotes(clip.id).then(setProject, report);
    },
  };
  const menus = useRef(menuActions);
  useEffect(() => {
    menus.current = menuActions;
  });

  // Copy, Paste and Duplicate from the Edit menu act on the piano roll.
  useEffect(() => {
    const listeners = [
      onEditMenu((item) => pianoRoll.current?.[item]()),
      onTrackMenu((item) => menus.current[item]()),
      onDevelopMenu((item) => menus.current[item]()),
    ];
    return () => {
      for (const unlisten of listeners) void unlisten.then((stopListening) => stopListening());
    };
  }, []);

  // The synth panel edits the selected track's sound.
  const changeSynth = (param: SynthParam, gesture?: number) => {
    if (!track) return;
    setSynthParam(track.id, param, gesture).then(setProject, report);
  };

  // The piano roll edits the clip in the Notes tab, and plays notes on its track.
  const clipId = clip?.id ?? "";
  const trackId = track?.id ?? "";
  const editor: NoteEditor = {
    add: (notes, gesture) => void addNotes(clipId, notes, gesture).then(setProject, report),
    set: (notes, gesture) => void setNotes(clipId, notes, gesture).then(setProject, report),
    remove: (ids) => void removeNotes(clipId, ids).then(setProject, report),
    trim: (ids, gesture) => void trimNotes(clipId, ids, gesture).then(setProject, report),
    cancel: (gesture) => void cancelGesture(gesture).then(setProject, report),
    audition: (pitch, velocity) => void auditionNote(trackId, pitch, velocity).catch(report),
  };

  const changeBuffer = (size: number) => {
    setChangingBuffer(true);
    setBufferSize(size)
      .catch(report)
      .finally(() => setChangingBuffer(false));
  };

  const clipped = clipLit(status?.clips ?? 0, seenClips);
  const maxEditorHeight = Math.max(MIN_EDITOR_HEIGHT, window.innerHeight - ABOVE_EDITOR);

  return (
    <main className="app">
      <div className="top">
        <Transport
          project={project}
          playing={status?.playing ?? false}
          position={status?.position ?? { bar: 1, beat: 1 }}
          onPlay={() => play().catch(report)}
          onStop={() => stop().catch(report)}
          onTempo={changeTempo}
          onLoopLength={changeLoopLength}
        />

        <section className="master" aria-label="Master">
          {project && <Volume project={project} onChange={changeVolume} />}
          <div className="master-meter">
            <Meter level={level} label="Level meter" width={120} />
            <button
              type="button"
              className="clip-light"
              data-lit={clipped}
              aria-label={clipped ? "Master clipped. Click to reset" : "Master hasn't clipped"}
              title={clipped ? "The master clipped. Click to reset." : "Lights if the master clips"}
              onClick={() => setSeenClips(status?.clips ?? 0)}
            />
          </div>
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

      {error && (
        <p className="error" role="alert">
          {error}
        </p>
      )}

      <div className="arrangement">
        {project && (
          <TrackHeaders
            project={project}
            selected={track?.id ?? null}
            levels={trackLevels}
            onSelect={setSelectedId}
            onMixer={changeMixer}
            onSoloAlone={soloAlone}
            onAdd={newTrack}
            onMove={moveTrackTo}
          />
        )}
        <section className="timeline" aria-label="Timeline" />
      </div>

      <Divider
        height={Math.min(editorHeight, maxEditorHeight)}
        min={MIN_EDITOR_HEIGHT}
        max={maxEditorHeight}
        onChange={setEditorHeight}
      />

      <section
        className="editor"
        aria-label="Editor"
        style={{ height: Math.min(editorHeight, maxEditorHeight) }}
      >
        <div className="tabs" role="tablist" aria-label="Editor">
          {(["notes", "sound"] as const).map((name) => (
            <button
              key={name}
              type="button"
              role="tab"
              id={`tab-${name}`}
              aria-controls={`panel-${name}`}
              aria-selected={tab === name}
              tabIndex={tab === name ? 0 : -1}
              onClick={() => setTab(name)}
            >
              {name === "notes" ? "Notes" : "Sound"}
            </button>
          ))}
          {track && <span className="editing">{track.name}</span>}
        </div>

        <div
          className="tab-panel"
          role="tabpanel"
          id={`panel-${tab}`}
          aria-labelledby={`tab-${tab}`}
        >
          {!project || !track ? (
            <p className="empty">No track selected. Add one with + Add track.</p>
          ) : tab === "sound" ? (
            <SynthPanel synth={track.synth} limits={project.synthLimits} onChange={changeSynth} />
          ) : clip ? (
            <PianoRoll
              ref={pianoRoll}
              project={project}
              clip={clip}
              editor={editor}
              clock={clock}
              stats={stats}
              createRenderer={createRenderer}
            />
          ) : (
            <p className="empty">{track.name} has no clips yet.</p>
          )}
        </div>
      </section>

      <FrameTime stats={stats} />
    </main>
  );
}

export default App;
