import { useEffect, useLayoutEffect, useReducer, useRef, useState } from "react";
import { flushSync } from "react-dom";
import "./design/tokens.css";
import "./App.css";
import {
  type ClipNotes,
  type ClipOutline,
  type Frame,
  type MixerView,
  type OutputView,
  type ProjectView,
  type SynthParam,
  type TrackKind,
  type TrackView,
  type Update,
  addClip,
  addNotes,
  addStressNotes,
  addTrack,
  auditionNote,
  cancelGesture,
  duplicateTrack,
  getNotes,
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
import { DragSteps } from "./dragSteps";
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
import { EMPTY_CACHE, inOutline, missingNotes, receive } from "./projectCache";
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
  // The project as Rust sent it: the latest outline and the notes held for
  // each clip. Only what Rust sends changes it (projectCache.ts).
  const [cache, dispatch] = useReducer(receive, EMPTY_CACHE);
  const project = cache.project;
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
  // Every control's drags send their steps through it, one at a time, latest wins.
  const [drags] = useState(() => new DragSteps());
  // The frame stream reads the latest project without re-subscribing.
  const projectRef = useRef<ProjectView | null>(null);
  const pianoRoll = useRef<PianoRollHandle>(null);
  const timeline = useRef<TimelineHandle>(null);
  // Which view the Edit menu acts on: the one last clicked in.
  const editView = useRef<EditView>("pianoRoll");
  const headers = useRef<HTMLElement>(null);
  // How many project-changed events have arrived, for the benchmark to wait on.
  const changeEvents = useRef(0);
  // The cache as of the latest render, for the benchmark to read once it's drawn.
  const cacheRef = useRef(cache);
  useLayoutEffect(() => {
    cacheRef.current = cache;
  });
  // The clips whose notes are being fetched, at the revision wanted.
  const fetching = useRef(new Set<string>());

  const report = (reason: unknown) => setError(String(reason));
  const show = (update: Update) => dispatch({ type: "update", update });

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
        .then((update) => active && dispatch({ type: "update", update }))
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

  // Fetches the notes of clips the cache doesn't hold at the outline's
  // revision: every clip after the web view reloads, or one whose update was
  // ignored because a newer one arrived first. Several at once go into the
  // cache together.
  useEffect(() => {
    const key = (clip: ClipOutline) => `${clip.id}@${clip.notesRevision}`;
    const wanted = missingNotes(cache).filter((clip) => !fetching.current.has(key(clip)));
    if (wanted.length === 0) return;
    for (const clip of wanted) fetching.current.add(key(clip));
    void Promise.allSettled(wanted.map((clip) => getNotes(clip.id))).then((results) => {
      for (const clip of wanted) fetching.current.delete(key(clip));
      const notes: ClipNotes[] = [];
      results.forEach((result, index) => {
        if (result.status === "fulfilled") {
          notes.push(result.value);
        } else if (inOutline(cacheRef.current, wanted[index].id)) {
          // A clip deleted while its notes were on their way is no error.
          setError(String(result.reason));
        }
      });
      dispatch({ type: "notes", notes });
    });
  }, [cache]);

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
    drags.step(gesture, () => setVolume(volumeDb, gesture).then(show, report));
  };

  const changeTempo = (bpm: number, gesture?: number) => {
    drags.step(gesture, () => setTempo(bpm, gesture).then(show, report));
  };

  const changeLoopEnabled = (enabled: boolean) => {
    setLoopEnabled(enabled).then(show, report);
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
    drags.step(gesture, () => setTrackMixer(id, mixer, gesture).then(show, report));
  };

  const soloAlone = (id: string) => {
    soloTrackAlone(id).then(show, report);
  };

  /** Selects a track. The selected clip stays selected if it's on it. */
  const selectTrack = (id: string | null) => {
    setSelectedId(id);
    if (clipTrack === null || clipTrack.id !== id) setSelectedClipIds([]);
  };

  /** Selects track `id` once Rust has sent the project with it in. */
  const showAndSelect = (id: string | null) => (update: Update) => {
    show(update);
    setSelectedId(id);
    setSelectedClipIds([]);
  };

  const newTrack = (kind: TrackKind) => {
    // The Track menu's Add is disabled then too, but its shortcut may beat the update.
    if (!project || project.tracks.length >= project.maxTracks) return;
    const id = crypto.randomUUID();
    addTrack(id, kind).then(showAndSelect(id), report);
  };

  const moveTrackTo = (id: string, index: number) => {
    moveTrack(id, index).then(show, report);
  };

  // The Track and Develop menus act on the selected track and the clip in
  // the Notes tab. The listeners read the latest of them.
  const menuActions = {
    "add-synth-track": () => newTrack("synth"),
    "add-drum-track": () => newTrack("drums"),
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
      if (clip) addStressNotes(clip.id).then(show, report);
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
    const id = track.id;
    drags.step(gesture, () => setSynthParam(id, param, gesture).then(show, report));
  };

  // The piano roll edits the clip in the Notes tab, and plays notes on its track.
  const clipId = clip?.id ?? "";
  const trackId = track?.id ?? "";
  // A drag's steps go through `drags`, and so does everything else that
  // edits notes, so nothing overtakes a step that's still waiting.
  const editor: NoteEditor = {
    add: (notes, gesture) => drags.then(() => addNotes(clipId, notes, gesture).then(show, report)),
    set: (notes, gesture) =>
      drags.step(gesture, () => setNotes(clipId, notes, gesture).then(show, report)),
    remove: (ids) => drags.then(() => removeNotes(clipId, ids).then(show, report)),
    trim: (ids, gesture) => drags.then(() => trimNotes(clipId, ids, gesture).then(show, report)),
    cancel: (gesture) => drags.cancel(gesture, () => cancelGesture(gesture).then(show, report)),
    audition: (pitch, velocity) => void auditionNote(trackId, pitch, velocity).catch(report),
  };

  // The timeline draws, moves, resizes, deletes, pastes and duplicates
  // clips. New clips are selected once Rust has sent them back.
  // Its drags go through `drags` too, as do its other edits.
  const clipEditor: ClipEditor = {
    add: (trackId, id, start, length) =>
      drags.then(() =>
        addClip(trackId, id, start, length).then((update) => {
          show(update);
          setSelectedClipIds([id]);
        }, report),
      ),
    set: (clips, gesture) => drags.step(gesture, () => setClips(clips, gesture).then(show, report)),
    remove: (ids) => drags.then(() => removeClips(ids).then(show, report)),
    paste: (clips) =>
      drags.then(() =>
        pasteClips(clips).then((update) => {
          show(update);
          setSelectedClipIds(clips.map((clip) => clip.id));
        }, report),
      ),
    cancel: (gesture) => drags.cancel(gesture, () => cancelGesture(gesture).then(show, report)),
  };

  // The timeline's ruler sets the loop region and moves the play start.
  const rulerActions: RulerActions = {
    loop: (startBar, bars, gesture) =>
      drags.step(gesture, () => setLoop(startBar, bars, gesture).then(show, report)),
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
    apply: (update) => flushSync(() => show(update)),
    project: () => cacheRef.current.project,
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
            track.source.kind === "synth" ? (
              <SynthPanel
                synth={track.source.synth}
                limits={project.synthLimits}
                defaults={project.synthDefaults}
                sampleRate={status?.output.sampleRate ?? null}
                onChange={changeSynth}
              />
            ) : (
              <p className="empty">
                The drum panel is coming. For now, play the kit from its labels on the Notes tab.
              </p>
            )
          ) : clip ? (
            <PianoRoll
              ref={pianoRoll}
              project={project}
              clip={clip}
              lanes={track.source.kind === "drums" ? track.source.kit.rows : undefined}
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
