import type { ProjectView } from "./backend";
import { DragField } from "./design/DragField";
import { Key } from "./design/Key";
import { linearScale } from "./design/numberScale";
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
      <Key mark="play" playing={playing} title="Play (Space)" onClick={onPlay}>
        Play
      </Key>
      <Key mark="stop" title="Stop (Space)" onClick={onStop}>
        Stop
      </Key>
      {project && (
        <Key
          mark="loop"
          pressed={project.loopEnabled}
          title={
            project.loopEnabled ? "Switch the loop off" : "Switch the loop on"
          }
          onClick={() => onLoopEnabled(!project.loopEnabled)}
        >
          Loop
        </Key>
      )}
      {/* Play's mark already shows it, so this is for screen readers only. */}
      <span className="state" role="status" data-testid="transport">
        {playing ? "Playing" : "Stopped"}
      </span>

      {project && (
        <DragField
          className="setting"
          label="Tempo"
          value={project.bpm}
          defaultValue={project.defaultBpm}
          scale={linearScale(project.minBpm, project.maxBpm, 1)}
          format={formatBpm}
          units={{ bpm: 1 }}
          onChange={onTempo}
        />
      )}

      <span className="position" data-testid="position" title="Bar and beat">
        {formatBarBeat(position)}
      </span>
    </section>
  );
}
