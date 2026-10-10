import type { ProjectView } from "./backend";
import { Slider } from "./design/Slider";
import { linearScale } from "./design/sliderScale";
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
  return (
    <Slider
      className="volume"
      label="Volume"
      value={project.volumeDb}
      defaultValue={project.defaultVolumeDb}
      scale={linearScale(project.minVolumeDb, project.maxVolumeDb, 0.5)}
      format={formatDb}
      onChange={onChange}
    />
  );
}
