import { useId } from "react";
import type { EffectSettingView, Unit } from "../backend";
import { Key } from "../design/Key";
import { Knob } from "../design/Knob";
import { Menu } from "../design/Menu";
import { SegmentedChoice } from "../design/SegmentedChoice";
import {
  SETTING_STEPS,
  formatAmount,
  formatHz,
  formatLevel,
  formatOctaves,
  formatSeconds,
  formatSemitones,
  settingScale,
} from "../synthScale";

// One of an effect's controls, drawn from Rust's description of the setting
// alone, following RFC-005's control rule: a knob for a number, a segmented
// choice (or a menu, past five) for a choice, a key for a toggle. An effect's
// panel says where each goes; the label, range, default and choices are
// Rust's.

interface Props {
  /** What the effect is called, for the control's accessible name: "Delay". */
  owner: string;
  setting: EffectSettingView;
  /** Its class, for its place in the panel's layout. */
  className?: string;
  /** `gesture` is the same for every change in one drag. */
  onChange: (value: number | string | boolean, gesture?: number) => void;
}

/** How each unit reads on an effect's knobs. */
const FORMATS: Record<Unit, (value: number) => string> = {
  hz: formatHz,
  seconds: formatSeconds,
  db: (db) => `${db.toFixed(1)} dB`,
  fraction: formatLevel,
  amount: formatAmount,
  semitones: formatSemitones,
  octaves: formatOctaves,
};

/** The most choices a segmented choice holds; more go in a menu. */
const MOST_SEGMENTS = 5;

/** The control RFC-005's rule picks for `setting`. */
export function SettingControl({ owner, setting, className, onChange }: Props) {
  const ariaLabel = `${owner} ${setting.label.toLowerCase()}`;
  // Each panel's radios are a group of their own, however many Delays there are.
  const group = useId();
  switch (setting.kind) {
    case "number":
      return (
        <Knob
          label={setting.label}
          ariaLabel={ariaLabel}
          className={className}
          size={28}
          value={setting.value}
          defaultValue={setting.default}
          scale={settingScale(setting)}
          keyStep={SETTING_STEPS / 100}
          format={FORMATS[setting.unit]}
          onChange={onChange}
        />
      );
    case "choice":
      return setting.options.length <= MOST_SEGMENTS ? (
        <SegmentedChoice
          legend={setting.label}
          name={`${group}-${setting.name}`}
          className={className}
          options={setting.options}
          value={setting.value}
          onChange={(value) => onChange(value)}
        />
      ) : (
        <label className={["effect-menu", className].filter(Boolean).join(" ")}>
          <span>{setting.label}</span>
          <Menu aria-label={ariaLabel} value={setting.value} onChange={(event) => onChange(event.target.value)}>
            {setting.options.map((option) => (
              <option key={option.value} value={option.value}>
                {option.label}
              </option>
            ))}
          </Menu>
        </label>
      );
    case "toggle":
      return (
        <Key
          aria-label={ariaLabel}
          className={className}
          pressed={setting.value}
          onClick={() => onChange(!setting.value)}
        >
          {setting.label}
        </Key>
      );
  }
}
