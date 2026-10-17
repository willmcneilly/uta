import type { DelayParam, EffectSettingView, EffectView, Unit } from "../backend";
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

// An effect's controls, drawn from Rust's description of its settings alone:
// a knob for a number, a segmented choice (or a menu, past five) for a
// choice, a key for a toggle, then Mix. It knows no effect by name, so a
// setting Rust adds shows up here without a change. The Delay's own panel,
// laid out around its drawing, replaces it for the Delay (UTA-76).

interface Props {
  effect: EffectView;
  /** `gesture` is the same for every change in one drag. */
  onSetting: (param: DelayParam, gesture?: number) => void;
  onMix: (mix: number, gesture?: number) => void;
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

/** The effect's settings, then its Mix, left to right. */
export function EffectControls({ effect, onSetting, onMix }: Props) {
  const control = (setting: EffectSettingView) => {
    const ariaLabel = `${effect.name} ${setting.label.toLowerCase()}`;
    switch (setting.kind) {
      case "number":
        return (
          <Knob
            key={setting.name}
            label={setting.label}
            ariaLabel={ariaLabel}
            size={28}
            value={setting.value}
            defaultValue={setting.default}
            scale={settingScale(setting)}
            keyStep={SETTING_STEPS / 100}
            format={FORMATS[setting.unit]}
            onChange={(value, gesture) => onSetting({ name: setting.name, value }, gesture)}
          />
        );
      case "choice":
        return setting.options.length <= MOST_SEGMENTS ? (
          <SegmentedChoice
            key={setting.name}
            legend={setting.label}
            name={`${effect.id}-${setting.name}`}
            options={setting.options}
            value={setting.value}
            onChange={(value) => onSetting({ name: setting.name, value })}
          />
        ) : (
          <label key={setting.name} className="effect-menu">
            <span>{setting.label}</span>
            <Menu
              aria-label={ariaLabel}
              value={setting.value}
              onChange={(event) => onSetting({ name: setting.name, value: event.target.value })}
            >
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
            key={setting.name}
            aria-label={ariaLabel}
            pressed={setting.value}
            onClick={() => onSetting({ name: setting.name, value: !setting.value })}
          >
            {setting.label}
          </Key>
        );
    }
  };

  const { mix } = effect;
  return (
    <div className="effect-controls">
      {effect.settings.map(control)}
      <Knob
        label={mix.label}
        ariaLabel={`${effect.name} ${mix.label.toLowerCase()}`}
        size={28}
        value={mix.value}
        defaultValue={mix.default}
        scale={settingScale(mix)}
        keyStep={SETTING_STEPS / 100}
        format={FORMATS[mix.unit]}
        onChange={onMix}
      />
    </div>
  );
}
