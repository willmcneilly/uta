import type { ProjectView } from "./backend";
import { Slider } from "./design/Slider";
import { linearScale } from "./design/sliderScale";
import { type BarBeat, formatBarBeat } from "./musicalTime";
import "./Transport.css";

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
  return (
    <section className="transport" aria-label="Transport">
      <button
        type="button"
        className="play"
        data-playing={playing || undefined}
        title="Play (Space)"
        onClick={onPlay}
      >
        Play
      </button>
      <button type="button" className="stop" title="Stop (Space)" onClick={onStop}>
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
        <Slider
          className="setting"
          label="Tempo"
          value={project.bpm}
          defaultValue={project.defaultBpm}
          scale={linearScale(project.minBpm, project.maxBpm, 1)}
          format={formatBpm}
          onChange={onTempo}
        />
      )}

      <span className="position" data-testid="position" title="Bar and beat">
        {formatBarBeat(position)}
      </span>
    </section>
  );
}
