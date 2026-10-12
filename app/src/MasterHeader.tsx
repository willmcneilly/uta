import type { ProjectView } from "./backend";
import { Fader } from "./design/Fader";
import { linearScale } from "./design/numberScale";
import { Meter } from "./Meter";
import type { MeterLevel } from "./meterLevel";
import "./MasterHeader.css";

/**
 * The meter's width, in CSS pixels: under the volume fader's rail, as wide
 * as it is, leaving room for the clip light at its end. provisional: D-30
 */
const METER_WIDTH = 168;
const METER_HEIGHT = 4;

interface Props {
  project: ProjectView;
  selected: boolean;
  level: MeterLevel;
  /** Whether the master has clipped since the light was last clicked. */
  clipped: boolean;
  /**
   * Selects the master. `open` is true for a click on the row itself, which
   * opens its chain on the Sound tab; using its volume or clip light, or
   * tabbing onto them, leaves the editor's tab as it is.
   */
  onSelect: (open: boolean) => void;
  /** `gesture` is the same for every change in one drag. */
  onVolume: (volumeDb: number, gesture?: number) => void;
  /** Puts the clip light out. */
  onClipSeen: () => void;
}

function formatDb(db: number): string {
  return `${db.toFixed(1)} dB`;
}

/**
 * The master's header row, below the tracks: its volume, meter and clip
 * light. Select it to edit the master's chain on the Sound tab.
 * provisional: D-30
 */
export function MasterHeader({
  project,
  selected,
  level,
  clipped,
  onSelect,
  onVolume,
  onClipSeen,
}: Props) {
  return (
    <section
      className={`master-header${selected ? " selected" : ""}`}
      aria-label="Master"
      aria-current={selected ? "true" : undefined}
      onPointerDown={(event) =>
        onSelect(!(event.target instanceof Element && event.target.closest("button, [role=slider]")))
      }
      // Tabbing onto its volume or clip light selects it too.
      onFocus={() => onSelect(false)}
    >
      <span className="master-name">Master</span>
      <Fader
        className="master-volume"
        label="Vol"
        ariaLabel="Master volume"
        value={project.volumeDb}
        defaultValue={project.defaultVolumeDb}
        scale={linearScale(project.minVolumeDb, project.maxVolumeDb, 0.5)}
        format={formatDb}
        onChange={onVolume}
      />
      <Meter level={level} label="Level meter" width={METER_WIDTH} height={METER_HEIGHT} />
      <button
        type="button"
        className="clip-light"
        data-lit={clipped}
        aria-label={clipped ? "Master clipped. Click to reset" : "Master hasn't clipped"}
        title={clipped ? "The master clipped. Click to reset." : "Lights if the master clips"}
        onClick={onClipSeen}
      />
    </section>
  );
}
