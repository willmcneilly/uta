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
  /** Switches the loop on or off. */
  onLoopEnabled: (enabled: boolean) => void;
}

function formatBpm(bpm: number): string {
  return `${Number.isInteger(bpm) ? bpm : bpm.toFixed(1)} BPM`;
}

/**
 * Play, Stop, the loop switch, the tempo and where the playhead is. The loop
 * region is set on the timeline's ruler.
 */
export function Transport({
  project,
  playing,
  position,
  onPlay,
  onStop,
  onTempo,
  onLoopEnabled,
}: Props) {
  const tempoGesture = useGesture();
  return (
    <section className="transport" aria-label="Transport">
      <button type="button" title="Play (Space)" onClick={onPlay}>
        Play
      </button>
      <button type="button" title="Stop (Space)" onClick={onStop}>
        Stop
      </button>
      {project && (
        <button
          type="button"
          className="loop-switch"
          aria-pressed={project.loopEnabled}
          title={
            project.loopEnabled ? "Switch the loop off" : "Switch the loop on"
          }
          onClick={() => onLoopEnabled(!project.loopEnabled)}
        >
          Loop
        </button>
      )}
      <span className="state" data-testid="transport">
        {playing ? "Playing" : "Stopped"}
      </span>

      {project && (
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
      )}

      <span className="position" data-testid="position" title="Bar and beat">
        {formatBarBeat(position)}
      </span>
    </section>
  );
}
