import { useId } from "react";
import type { EffectSettingView } from "../backend";
import { Key } from "../design/Key";
import { Knob } from "../design/Knob";
import { Menu } from "../design/Menu";
import { SegmentedChoice } from "../design/SegmentedChoice";
import { SETTING_STEPS, formatOf, settingScale } from "../synthScale";

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
  /**
   * Whether it's labelled with Rust's short name, where the panel's group
   * already says whose it is: "Size" under Early for "Early size".
   */
  short?: boolean;
  /** `gesture` is the same for every change in one drag. */
  onChange: (value: number | string | boolean, gesture?: number) => void;
  /** A knob's value as it's dragged, before Rust replies, then null when the drag ends. */
  onDrag?: (value: number | null) => void;
}

/** The most choices a segmented choice holds; more go in a menu. */
const MOST_SEGMENTS = 5;

/** The control RFC-005's rule picks for `setting`. */
export function SettingControl({ owner, setting, className, short = false, onChange, onDrag }: Props) {
  const label = (short && setting.short) || setting.label;
  const ariaLabel = `${owner} ${setting.label.toLowerCase()}`;
  // Each panel's radios are a group of their own, however many Delays there are.
  const group = useId();
  switch (setting.kind) {
    case "number":
      return (
        <Knob
          label={label}
          ariaLabel={ariaLabel}
          className={className}
          size={28}
          value={setting.value}
          defaultValue={setting.default}
          scale={settingScale(setting)}
          keyStep={SETTING_STEPS / 100}
          format={formatOf(setting)}
          onChange={onChange}
          onDrag={onDrag}
        />
      );
    case "choice":
      return setting.options.length <= MOST_SEGMENTS ? (
        <SegmentedChoice
          legend={label}
          name={`${group}-${setting.name}`}
          className={className}
          options={setting.options}
          value={setting.value}
          onChange={(value) => onChange(value)}
        />
      ) : (
        <label className={["effect-menu", className].filter(Boolean).join(" ")}>
          <span>{label}</span>
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
          {label}
        </Key>
      );
  }
}
