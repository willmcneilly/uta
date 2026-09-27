import { useRef } from "react";
import type { ProjectView } from "./backend";

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
  const gestures = useRef(0);
  const gesture = useRef<number | undefined>(undefined);

  const startDrag = () => {
    gestures.current += 1;
    gesture.current = gestures.current;
    // The pointer can be released outside the control.
    window.addEventListener(
      "pointerup",
      () => {
        gesture.current = undefined;
      },
      { once: true },
    );
  };

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
        onPointerDown={startDrag}
        onChange={(event) => onChange(event.currentTarget.valueAsNumber, gesture.current)}
      />
      <output>{formatDb(project.volumeDb)}</output>
    </label>
  );
}
