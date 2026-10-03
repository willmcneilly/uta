import { useEffect, useRef, useState } from "react";
import { flushSync } from "react-dom";
import "./App.css";
import {
  type Frame,
  type MixerView,
  type OutputView,
  type ProjectView,
  type SynthParam,
  type TrackView,
  addClip,
  addNotes,
  addStressNotes,
  addTrack,
  auditionNote,
  cancelGesture,
  duplicateTrack,
  getProject,
  locate,
  moveTrack,
  onDevelopMenu,
  onEditMenu,
  onProjectChanged,
  onTrackMenu,
  pasteClips,
  pause,
  play,
  removeClips,
  removeNotes,
  removeTrack,
  resume,
  setBufferSize,
  setClips,
  setLoop,
  setLoopEnabled,
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
import { Benchmark } from "./benchmark/Benchmark";
import {
  type BenchmarkHost,
  type BenchmarkOptions,
  type Clock,
  DEFAULT_OPTIONS,
  browserClock,
  runBenchmark,
} from "./benchmark/run";
import { Divider } from "./Divider";
import { FrameLoop } from "./frameLoop";
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
import type { TimelineRendererFactory } from "./timeline/renderer";
import {
  type ClipEditor,
  type RulerActions,
  Timeline,
  type TimelineHandle,
} from "./timeline/Timeline";
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

/** Inputs that take typing, where Space types a space rather than playing. */
const TEXT_INPUTS = new Set(["text", "search", "email", "url", "tel", "password", "number"]);

/** Whether `event` is Space or Shift+Space, pressed anywhere but a text field. */
function isTransportKey(event: KeyboardEvent): boolean {
  if (event.key !== " " || event.metaKey || event.ctrlKey || event.altKey) return false;
  const target = event.target;
  if (!(target instanceof HTMLElement)) return true;
  if (target.isContentEditable || target instanceof HTMLTextAreaElement) return false;
  return !(target instanceof HTMLInputElement && TEXT_INPUTS.has(target.type));
}

type Tab = "notes" | "sound";

/** The views the Edit menu's Copy, Paste and Duplicate can act on. */
type EditView = "timeline" | "pianoRoll";

/** The editor's height when the app opens, and its limits, in CSS pixels. */
const EDITOR_HEIGHT = 280;
const MIN_EDITOR_HEIGHT = 160;
/** What the editor always leaves above it for the transport and the tracks. */
const ABOVE_EDITOR = 260;

interface Props {
  /** Draws the piano roll. Tests pass their own; the app uses Canvas 2D. */
  createRenderer?: RendererFactory;
  /** Draws the timeline, likewise. */
  createTimelineRenderer?: TimelineRendererFactory;
  /** How Develop → Run Benchmark runs, and its clock. Tests make it quick. */
  benchmark?: { options?: BenchmarkOptions; clock?: Clock };
}

function App({ createRenderer, createTimelineRenderer, benchmark }: Props) {
  const [project, setProject] = useState<ProjectView | null>(null);
  const [status, setStatus] = useState<Status | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [changingBuffer, setChangingBuffer] = useState(false);
  const [benchmarking, setBenchmarking] = useState(false);
  // What's selected, which tab is open, how tall the editor is, and when
  // the clip light was last clicked are the UI's own: none is project data.
  const [selectedId, setSelectedId] = useState<string | null>(null);
  // The clips picked on the timeline, in the order they were picked.
  const [selectedClipIds, setSelectedClipIds] = useState<string[]>([]);
  const [tab, setTab] = useState<Tab>("notes");
  const [editorHeight, setEditorHeight] = useState(EDITOR_HEIGHT);
  const [seenClips, setSeenClips] = useState(0);
  const [windowHeight, setWindowHeight] = useState(() => window.innerHeight);
  const [level] = useState(() => new MeterLevel());
  // Held like a peak: the slowest block since the readout last showed one.
  const [slowestBlock] = useState(() => new MeterLevel());
  const [trackLevels] = useState(() => new TrackLevels());
  const [clock] = useState(() => new PlayheadClock());
  const [stats] = useState(() => new FrameStats());
  const [frames] = useState(() => new FrameLoop(stats));
  // The frame stream reads the latest project without re-subscribing.
  const projectRef = useRef<ProjectView | null>(null);
  const pianoRoll = useRef<PianoRollHandle>(null);
  const timeline = useRef<TimelineHandle>(null);
  // Which view the Edit menu acts on: the one last clicked in.
  const editView = useRef<EditView>("pianoRoll");
  const headers = useRef<HTMLElement>(null);
  // How many project-changed events have arrived, for the benchmark to wait on.
  const changeEvents = useRef(0);

  const report = (reason: unknown) => setError(String(reason));

  // A selected track that has gone (undoing its add, say) stays unselected,
  // so redoing the add doesn't select it again.
  if (project && selectedId !== null && !project.tracks.some((t) => t.id === selectedId)) {
    setSelectedId(null);
  }
  // Selected clips that have gone stay unselected too.
  const clipIds = project
    ? selectedClipIds.filter((id) => project.tracks.some((t) => t.clips.some((c) => c.id === id)))
    : selectedClipIds;
  if (clipIds.length !== selectedClipIds.length) setSelectedClipIds(clipIds);
  // The last clip picked is the one the Notes tab shows. Its track is
  // selected, wherever the clip has been moved to.
  const selectedClipId = clipIds.at(-1) ?? null;
  const clipTrack =
    project?.tracks.find((t) => t.clips.some((c) => c.id === selectedClipId)) ?? null;
  if (clipTrack && clipTrack.id !== selectedId) setSelectedId(clipTrack.id);
  // The selected track, or the top one if none is.
  const track: TrackView | null =
    clipTrack ?? project?.tracks.find((t) => t.id === selectedId) ?? project?.tracks[0] ?? null;
  // The Notes tab shows the selected clip, or the track's first if none is.
  const clip =
    clipTrack?.clips.find((c) => c.id === selectedClipId) ?? track?.clips[0] ?? null;

  useEffect(() => {
    let active = true;
    const fetchProject = () =>
      getProject()
        .then((view) => active && setProject(view))
        .catch((reason: unknown) => active && setError(String(reason)));
    void fetchProject();
    // Undo and Redo from the menu bar say only that the project changed, and
    // it's fetched the fast way. The UI's own changes come back as replies.
    const unlisten = onProjectChanged(() => {
      changeEvents.current += 1;
      void fetchProject();
    });
    return () => {
      active = false;
      void unlisten.then((stopListening) => stopListening());
    };
  }, []);

  useEffect(() => {
    projectRef.current = project;
    if (project) trackLevels.keepOnly(project.tracks.map((t) => t.id));
    if (project) {
      clock.setTiming({
        bpm: project.bpm,
        ticksPerQuarter: project.ticksPerQuarter,
        loopStart: project.loopStart,
        loopLength: project.loopLength,
        loopEnabled: project.loopEnabled,
      });
    }
  }, [project, clock, trackLevels]);

  useEffect(() => {
    const resize = () => setWindowHeight(window.innerHeight);
    window.addEventListener("resize", resize);
    return () => window.removeEventListener("resize", resize);
  }, []);

  useEffect(() => {
    let active = true;
    const channel = subscribe((frame) => {
      if (!active) return;
      level.push(frame.peak);
      slowestBlock.push(frame.slowestBlock);
      trackLevels.push(frame.trackPeaks);
      clock.report(frame.playhead, frame.playing, performance.now());
      const next = toStatus(frame, projectRef.current);
      setStatus((previous) => (sameStatus(previous, next) ? previous : next));
    });
    channel.catch((reason: unknown) => active && setError(String(reason)));
    return () => {
      active = false;
    };
  }, [level, slowestBlock, trackLevels, clock]);

  const changeVolume = (volumeDb: number, gesture?: number) => {
    setVolume(volumeDb, gesture).then(setProject, report);
  };

  const changeTempo = (bpm: number, gesture?: number) => {
    setTempo(bpm, gesture).then(setProject, report);
  };

  const changeLoopEnabled = (enabled: boolean) => {
    setLoopEnabled(enabled).then(setProject, report);
  };

  // Space plays, and stops back at the play start; Shift+Space pauses, and
  // carries on from where it paused. They read the latest status.
  const transportKey = (shift: boolean) => {
    const playing = status?.playing ?? false;
    const action = shift ? (playing ? pause : resume) : playing ? stop : play;
    action().catch(report);
  };
  const spaceBar = useRef(transportKey);
  useEffect(() => {
    spaceBar.current = transportKey;
  });

  // On the window, so it works wherever the focus is, except while typing
  // in a text field. The key's own action is prevented too, so it never
  // also presses a focused button or ticks a box (which happens on keyup).
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (!isTransportKey(event)) return;
      event.preventDefault();
      if (!event.repeat) spaceBar.current(event.shiftKey);
    };
    const onKeyUp = (event: KeyboardEvent) => {
      if (isTransportKey(event)) event.preventDefault();
    };
    window.addEventListener("keydown", onKeyDown);
    window.addEventListener("keyup", onKeyUp);
    return () => {
      window.removeEventListener("keydown", onKeyDown);
      window.removeEventListener("keyup", onKeyUp);
    };
  }, []);

  const changeMixer = (id: string, mixer: MixerView, gesture?: number) => {
    setTrackMixer(id, mixer, gesture).then(setProject, report);
  };

  const soloAlone = (id: string) => {
    soloTrackAlone(id).then(setProject, report);
  };

  /** Selects a track. The selected clip stays selected if it's on it. */
  const selectTrack = (id: string | null) => {
    setSelectedId(id);
    if (clipTrack === null || clipTrack.id !== id) setSelectedClipIds([]);
  };

  /** Selects track `id` once Rust has sent the project with it in. */
  const showAndSelect = (id: string | null) => (view: ProjectView) => {
    setProject(view);
    setSelectedId(id);
    setSelectedClipIds([]);
  };

  const newTrack = () => {
    // The Track menu's Add is disabled then too, but its shortcut may beat the update.
    if (!project || project.tracks.length >= project.maxTracks) return;
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
    "run-benchmark": () => setBenchmarking(true),
  };
  const menus = useRef(menuActions);
  useEffect(() => {
    menus.current = menuActions;
  });

  // Copy, Paste and Duplicate from the Edit menu act on the timeline or the
  // piano roll, whichever was last clicked in.
  useEffect(() => {
    const listeners = [
      onEditMenu((item) =>
        (editView.current === "timeline" ? timeline.current : pianoRoll.current)?.[item](),
      ),
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

  // The timeline draws, moves, resizes, deletes, pastes and duplicates
  // clips. New clips are selected once Rust has sent them back.
  const clipEditor: ClipEditor = {
    add: (trackId, id, start, length) =>
      void addClip(trackId, id, start, length).then((view) => {
        setProject(view);
        setSelectedClipIds([id]);
      }, report),
    set: (clips, gesture) => void setClips(clips, gesture).then(setProject, report),
    remove: (ids) => void removeClips(ids).then(setProject, report),
    paste: (clips) =>
      void pasteClips(clips).then((view) => {
        setProject(view);
        setSelectedClipIds(clips.map((clip) => clip.id));
      }, report),
    cancel: (gesture) => void cancelGesture(gesture).then(setProject, report),
  };

  // The timeline's ruler sets the loop region and moves the play start.
  const rulerActions: RulerActions = {
    loop: (startBar, bars, gesture) =>
      void setLoop(startBar, bars, gesture).then(setProject, report),
    locate: (ticks) => void locate(ticks).catch(report),
  };

  const changeBuffer = (size: number) => {
    setChangingBuffer(true);
    setBufferSize(size)
      .catch(report)
      .finally(() => setChangingBuffer(false));
  };

  // The benchmark shows each reply at once, so the frame after it is the
  // one that draws it.
  const benchmarkHost: BenchmarkHost = {
    apply: (view) => flushSync(() => setProject(view)),
    events: () => changeEvents.current,
    showClip: (id) => {
      setSelectedClipIds([id]);
      setTab("notes");
    },
  };
  const startBenchmark = (onProgress: Parameters<typeof runBenchmark>[1]) =>
    runBenchmark(
      benchmarkHost,
      onProgress,
      benchmark?.options ?? DEFAULT_OPTIONS,
      benchmark?.clock ?? browserClock,
    );

  const clipped = clipLit(status?.clips ?? 0, seenClips);
  const maxEditorHeight = Math.max(MIN_EDITOR_HEIGHT, windowHeight - ABOVE_EDITOR);

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
          onLoopEnabled={changeLoopEnabled}
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
            slowestBlock={slowestBlock}
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

      <div className="arrangement" onPointerDownCapture={() => (editView.current = "timeline")}>
        {project && (
          <TrackHeaders
            project={project}
            selected={track?.id ?? null}
            levels={trackLevels}
            onSelect={selectTrack}
            onMixer={changeMixer}
            onSoloAlone={soloAlone}
            onAdd={newTrack}
            onMove={moveTrackTo}
            scrollRef={headers}
          />
        )}
        {project && (
          <Timeline
            ref={timeline}
            project={project}
            selectedTrack={track?.id ?? null}
            // Only clips chosen on the timeline are highlighted there, so
            // Backspace never deletes the one Notes falls back to.
            selectedClips={clipIds}
            editor={clipEditor}
            ruler={rulerActions}
            clock={clock}
            frames={frames}
            headers={headers}
            onSelectTrack={(id) => {
              // A click on empty space selects the track and no clip.
              setSelectedId(id);
              setSelectedClipIds([]);
            }}
            onSelectClips={setSelectedClipIds}
            onOpenClip={(_track, clipId) => {
              setSelectedClipIds([clipId]);
              setTab("notes");
            }}
            createRenderer={createTimelineRenderer}
          />
        )}
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
        onPointerDownCapture={() => (editView.current = "pianoRoll")}
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
              frames={frames}
              createRenderer={createRenderer}
            />
          ) : (
            <p className="empty">{track.name} has no clips yet.</p>
          )}
        </div>
      </section>

      <FrameTime stats={stats} />

      {benchmarking && (
        <Benchmark run={startBenchmark} onClose={() => setBenchmarking(false)} />
      )}
    </main>
  );
}

export default App;
