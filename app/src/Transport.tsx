import type { ProjectView } from "./backend";
import { type BarBeat, formatBarBeat } from "./musicalTime";
import { useGesture } from "./useGesture";

interface Props {
  project: ProjectView | null;
  playing: boolean;
  position: BarBeat;
  onPlay: () => void;
  onStop: () => void;
  /** `gesture` is the same for every change in one drag. */
  onTempo: (bpm: number, gesture?: number) => void;
  onLoopLength: (bars: number, gesture?: number) => void;
}

function formatBpm(bpm: number): string {
  return `${Number.isInteger(bpm) ? bpm : bpm.toFixed(1)} BPM`;
}

function formatBars(bars: number): string {
  return bars === 1 ? "1 bar" : `${bars} bars`;
}

/** Play, Stop, the tempo, the loop's length and where the playhead is. */
export function Transport({
  project,
  playing,
  position,
  onPlay,
  onStop,
  onTempo,
  onLoopLength,
}: Props) {
  const tempoGesture = useGesture();
  const loopGesture = useGesture();
  return (
    <section className="transport" aria-label="Transport">
      <button type="button" onClick={onPlay}>
        Play
      </button>
      <button type="button" onClick={onStop}>
        Stop
      </button>
      <span className="state" data-testid="transport">
        {playing ? "Playing" : "Stopped"}
      </span>

      {project && (
        <>
          <label className="setting">
            <span>Tempo</span>
            <input
              type="range"
              min={project.minBpm}
              max={project.maxBpm}
              step={1}
              value={project.bpm}
              aria-valuetext={formatBpm(project.bpm)}
              onPointerDown={tempoGesture.start}
              onChange={(event) =>
                onTempo(event.currentTarget.valueAsNumber, tempoGesture.current())
              }
            />
            <output>{formatBpm(project.bpm)}</output>
          </label>
          <label className="setting">
            <span>Loop</span>
            <input
              type="range"
              min={project.minLoopBars}
              max={project.maxLoopBars}
              step={1}
              value={project.loopBars}
              aria-valuetext={formatBars(project.loopBars)}
              onPointerDown={loopGesture.start}
              onChange={(event) =>
                onLoopLength(event.currentTarget.valueAsNumber, loopGesture.current())
              }
            />
            <output>{formatBars(project.loopBars)}</output>
          </label>
        </>
      )}

      <span className="position" data-testid="position" title="Bar and beat">
        {formatBarBeat(position)}
      </span>
    </section>
  );
}
