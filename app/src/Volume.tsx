import type { ProjectView } from "./backend";
import { useGesture } from "./useGesture";
import "./Volume.css";

interface Props {
  project: ProjectView;
  /** `gesture` is the same for every change in one drag. */
  onChange: (volumeDb: number, gesture?: number) => void;
}

function formatDb(db: number): string {
  return `${db.toFixed(1)} dB`;
}

/** The master volume. It shows the project's volume, as Rust last sent it. */
export function Volume({ project, onChange }: Props) {
  const gesture = useGesture();
  const shown = Math.min(project.maxVolumeDb, Math.max(project.minVolumeDb, project.volumeDb));
  return (
    <label className="volume">
      <span>Volume</span>
      <input
        type="range"
        min={project.minVolumeDb}
        max={project.maxVolumeDb}
        step={0.5}
        value={shown}
        aria-valuetext={formatDb(project.volumeDb)}
        onPointerDown={gesture.start}
        onChange={(event) => onChange(event.currentTarget.valueAsNumber, gesture.current())}
      />
      <output>{formatDb(project.volumeDb)}</output>
    </label>
  );
}
