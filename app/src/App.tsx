import { useEffect, useState } from "react";
import "./App.css";
import {
  type Frame,
  type OutputView,
  type ProjectView,
  getProject,
  onProjectChanged,
  play,
  setBufferSize,
  setVolume,
  stop,
  subscribe,
} from "./backend";
import { Meter } from "./Meter";
import { MeterLevel } from "./meterLevel";
import { Output } from "./Output";
import { Volume } from "./Volume";

/** The slow-changing part of a frame: what the text on screen shows. */
interface Status {
  playing: boolean;
  /** Tenths of a second, so it re-renders ten times a second, not sixty. */
  tenths: number;
  dropouts: number;
  output: OutputView;
}

function toStatus(frame: Frame): Status {
  return {
    playing: frame.playing,
    tenths: Math.floor(frame.positionSeconds * 10),
    dropouts: frame.dropouts,
    output: frame.output,
  };
}

function sameStatus(a: Status | null, b: Status): boolean {
  return a !== null && JSON.stringify(a) === JSON.stringify(b);
}

function App() {
  const [project, setProject] = useState<ProjectView | null>(null);
  const [status, setStatus] = useState<Status | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [changingBuffer, setChangingBuffer] = useState(false);
  const [level] = useState(() => new MeterLevel());

  const report = (reason: unknown) => setError(String(reason));

  useEffect(() => {
    let active = true;
    getProject()
      .then((view) => active && setProject(view))
      .catch((reason: unknown) => active && setError(String(reason)));
    // Undo and Redo from the menu arrive this way.
    const unlisten = onProjectChanged((view) => active && setProject(view));
    return () => {
      active = false;
      void unlisten.then((stopListening) => stopListening());
    };
  }, []);

  useEffect(() => {
    let active = true;
    const channel = subscribe((frame) => {
      if (!active) return;
      level.push(frame.peak);
      const next = toStatus(frame);
      setStatus((previous) => (sameStatus(previous, next) ? previous : next));
    });
    channel.catch((reason: unknown) => active && setError(String(reason)));
    return () => {
      active = false;
    };
  }, [level]);

  const changeVolume = (volumeDb: number, gesture?: number) => {
    setVolume(volumeDb, gesture).then(setProject, report);
  };

  const changeBuffer = (size: number) => {
    setChangingBuffer(true);
    setBufferSize(size)
      .catch(report)
      .finally(() => setChangingBuffer(false));
  };

  return (
    <main className="app">
      <section className="transport" aria-label="Transport">
        <button type="button" onClick={() => play().catch(report)}>
          Play
        </button>
        <button type="button" onClick={() => stop().catch(report)}>
          Stop
        </button>
        <span className="state" data-testid="transport">
          {status?.playing ? "Playing" : "Stopped"}
        </span>
        <span className="position" data-testid="position">
          {((status?.tenths ?? 0) / 10).toFixed(1)} s
        </span>
      </section>

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

      {error && (
        <p className="error" role="alert">
          {error}
        </p>
      )}
    </main>
  );
}

export default App;
