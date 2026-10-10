import type { DrumParam, DrumSettingView, DrumSound, DrumUnit, KitRowView } from "./backend";
import { Fader } from "./design/Fader";
import { Knob } from "./design/Knob";
import { SegmentedChoice } from "./design/SegmentedChoice";
import { type NumberScale, linearScale } from "./design/numberScale";
import { SETTING_STEPS, formatSeconds, fromPosition, toPosition } from "./synthScale";
import "./DrumPanel.css";

// A drum track's Sound tab: a strip per sound, as on the 808's front panel
// (RFC-006, "In the window"). Everything in a strip comes from the outline:
// its name, which settings it has, and each one's label, limits, default
// and unit, so a sound that arrives later needs nothing here. Each sound's
// Level is a fader stood on end, so the Levels line up like a mixer, and
// its other settings are knobs (RFC-005, part 7).

interface Props {
  /** The kit's rows, kick first. */
  rows: KitRowView[];
  /** `gesture` is the same for every change in one drag. */
  onChange: (sound: DrumSound, param: DrumParam, gesture?: number) => void;
  /** Plays `pitch`, the note of the sound whose name was clicked. */
  onAudition: (pitch: number) => void;
}

/** How each unit reads. Hz stay whole below 10 kHz, so "1000 Hz" fits a small knob. */
const FORMATS: Record<DrumUnit, (value: number) => string> = {
  hz: (hz) => (hz < 10_000 ? `${Math.round(hz)} Hz` : `${(hz / 1000).toFixed(1)} kHz`),
  seconds: formatSeconds,
  db: (db) => `${db.toFixed(1)} dB`,
  fraction: (fraction) => `${Math.round(fraction * 100)}%`,
};

/** How far one step of a Level goes, in dB, as on a track's volume. */
const LEVEL_STEP = 0.5;

/**
 * A setting's positions. Frequencies and times move on a log scale, the way
 * you hear them, as the synth's do; a fraction moves on a linear one, and a
 * level in half-dB steps, as a track's volume does.
 */
function scaleOf({ unit, limits }: DrumSettingView): NumberScale {
  if (unit === "db") return linearScale(...limits, LEVEL_STEP);
  const scale = unit === "fraction" ? "linear" : "log";
  return {
    min: limits[0],
    max: limits[1],
    steps: SETTING_STEPS,
    toPosition: (value) => toPosition(value, limits, scale),
    fromPosition: (position) => fromPosition(position, limits, scale),
  };
}

/** Each sound's Level is its fader; the rest are knobs. */
const isLevel = (setting: DrumSettingView) => setting.name === "level_db";

interface StripProps extends Omit<Props, "rows"> {
  row: KitRowView;
  /** What the sound it shares settings with is called, if it shares any. */
  sharedWith?: string;
}

/** "Tune", "Tune and Tone", "Tune, Tone and Decay". */
function listed(labels: string[]): string {
  return labels.length < 2 ? (labels[0] ?? "") : `${labels.slice(0, -1).join(", ")} and ${labels.at(-1)}`;
}

/**
 * One sound: its name, which plays it, its knobs two to a row, and its
 * Level beside them. Where its other settings belong to another sound, it
 * says so where they'd be.
 */
function Strip({ row, sharedWith, onChange, onAudition }: StripProps) {
  const level = row.settings.find(isLevel);
  const knobs = row.settings.filter((setting) => !isLevel(setting));
  const control = (setting: DrumSettingView) => ({
    ariaLabel: `${row.name} ${setting.label.toLowerCase()}`,
    value: setting.value,
    defaultValue: setting.default,
    scale: scaleOf(setting),
    format: FORMATS[setting.unit],
    onChange: (value: number, gesture?: number) =>
      onChange(row.sound, { name: setting.name, value }, gesture),
  });

  return (
    <section className="drum-strip" aria-label={row.name}>
      <button
        type="button"
        className="sound-name"
        title={`Play the ${row.name.toLowerCase()}`}
        onClick={() => onAudition(row.pitch)}
      >
        {row.name}
      </button>
      {row.settings.length === 0 ? (
        <p className="strip-empty">No controls yet</p>
      ) : (
        <div className="strip-knobs">
          {row.model && (
            // provisional: D-28
            <SegmentedChoice
              className="strip-model"
              legend={row.model.label}
              name={`${row.sound}-model`}
              options={row.model.options}
              value={row.model.value}
              onChange={(value) => onChange(row.sound, { name: "model", value })}
            />
          )}
          {knobs.map((setting) => (
            <Knob
              key={setting.name}
              label={setting.label}
              size={24}
              keyStep={SETTING_STEPS / 100}
              {...control(setting)}
            />
          ))}
          {row.shares && sharedWith && (
            <p className="strip-shared">
              {listed(row.shares.labels)}: the {sharedWith.toLowerCase()}’s
            </p>
          )}
        </div>
      )}
      {level && (
        <Fader
          className="strip-level"
          orientation="vertical"
          label={level.label}
          {...control(level)}
        />
      )}
    </section>
  );
}

/** The kit's sounds side by side, kick on the left. provisional: D-27 */
export function DrumPanel({ rows, onChange, onAudition }: Props) {
  return (
    <div className="drums" role="group" aria-label="Drum kit">
      {rows.map((row) => (
        <Strip
          key={row.sound}
          row={row}
          sharedWith={rows.find((other) => other.sound === row.shares?.sound)?.name}
          onChange={onChange}
          onAudition={onAudition}
        />
      ))}
    </div>
  );
}
