import { type ReactNode, useState } from "react";
import type {
  NumberSettingView,
  SynthNumber,
  SynthParam,
  SynthSettingView,
  Unit,
  Waveform,
} from "./backend";
import { Group } from "./design/Group";
import { Knob } from "./design/Knob";
import { SegmentedChoice } from "./design/SegmentedChoice";
import {
  SETTING_STEPS,
  formatAmount,
  formatHz,
  formatLevel,
  formatSeconds,
  settingScale,
} from "./synthScale";
import { EnvelopeDrawing, FilterDrawing } from "./synth/Drawings";
import type { StageName } from "./synth/envelopeShape";
import { DEFAULT_SAMPLE_RATE } from "./synth/filterCurve";
import "./SynthPanel.css";

interface Props {
  /**
   * Each of the synth's settings, as Rust describes them: its value, label,
   * unit, limits, scale and default, or its choices.
   */
  settings: SynthSettingView[];
  /**
   * The engine's sample rate, which the filter's curve depends on, or null
   * before a device is open.
   */
  sampleRate: number | null;
  /** `gesture` is the same for every change in one drag. */
  onChange: (param: SynthParam, gesture?: number) => void;
}

/** One cycle of a wave, drawn in a 28 by 12 box. provisional: D-11 */
function cycle(path: string) {
  return (
    <svg className="cycle" viewBox="-1 -1 30 14" width={30} height={14}>
      <path d={path} />
    </svg>
  );
}

/** One cycle of each waveform, drawn above its name. */
const CYCLES: Record<Waveform, ReactNode> = {
  sine: cycle("M0,6 C4.5,-2 9.5,-2 14,6 S23.5,14 28,6"),
  triangle: cycle("M0,6 L7,0 L21,12 L28,6"),
  saw: cycle("M0,12 L14,0 L14,12 L28,0 L28,12"),
  square: cycle("M0,12 L0,0 L14,0 L14,12 L28,12 L28,0"),
};

/** How each unit reads on the synth's knobs. */
const FORMATS: Record<Unit, (value: number) => string> = {
  hz: formatHz,
  seconds: formatSeconds,
  db: (db) => `${db.toFixed(1)} dB`,
  fraction: formatLevel,
  amount: formatAmount,
};

type SettingName = SynthNumber["name"];

/** The settings each drawing shows, in the order of their knobs. */
const FILTER: SettingName[] = ["cutoff_hz", "resonance"];
const ENVELOPE: SettingName[] = ["attack_seconds", "decay_seconds", "sustain", "release_seconds"];

const STAGES: Partial<Record<SettingName, StageName>> = {
  attack_seconds: "attack",
  decay_seconds: "decay",
  sustain: "sustain",
  release_seconds: "release",
};

/** A knob being dragged, and the value it shows. */
interface Dragged {
  name: SettingName;
  value: number;
}

/**
 * A track's synth: waveform, filter and envelope. Each control is drawn from
 * the setting Rust describes, so the panel holds no label, range or default
 * of its own. The filter and the envelope are drawn above their knobs, from
 * the values the knobs show, so the drawings follow a drag before Rust
 * replies.
 */
export function SynthPanel({ settings, sampleRate, onChange }: Props) {
  // The knob being dragged: gesture state, gone when the drag ends.
  const [dragged, setDragged] = useState<Dragged | null>(null);
  const numbers = new Map<string, NumberSettingView<SettingName>>();
  for (const setting of settings) if (setting.kind === "number") numbers.set(setting.name, setting);
  const number = (name: SettingName) => {
    const setting = numbers.get(name);
    if (!setting) throw new Error(`Rust sent no ${name} setting`);
    return setting;
  };
  /** The value a setting's knob shows: the drag's, or what Rust last sent. */
  const shown = (name: SettingName) =>
    dragged?.name === name ? dragged.value : number(name).value;
  const waveform = settings.find((setting) => setting.kind === "choice");

  /**
   * One setting on a knob. It shows the value Rust last sent. An arrow key
   * moves it a hundredth of the way, and Shift+arrow a tenth.
   */
  const knob = (name: SettingName) => {
    const setting = number(name);
    return (
      <Knob
        key={name}
        label={setting.label}
        value={setting.value}
        defaultValue={setting.default}
        scale={settingScale(setting)}
        keyStep={SETTING_STEPS / 100}
        format={FORMATS[setting.unit]}
        onChange={(value, gesture) => onChange({ name, value }, gesture)}
        onDrag={(value) => setDragged(value === null ? null : { name, value })}
      />
    );
  };

  return (
    <section className="synth" aria-label="Synth">
      {waveform && (
        <SegmentedChoice
          legend={waveform.label}
          name={waveform.name}
          options={waveform.options.map((option) => ({
            ...option,
            drawing: CYCLES[option.value],
          }))}
          value={waveform.value}
          onChange={(value) => onChange({ name: "waveform", value })}
        />
      )}

      <Group title="Filter" className="filter">
        <FilterDrawing
          cutoffHz={shown("cutoff_hz")}
          resonance={shown("resonance")}
          sampleRate={sampleRate ?? DEFAULT_SAMPLE_RATE}
          marked={dragged !== null && FILTER.includes(dragged.name)}
        />
        <div className="knobs">{FILTER.map(knob)}</div>
      </Group>

      <Group title="Envelope" className="envelope">
        <EnvelopeDrawing
          times={{
            attackSeconds: shown("attack_seconds"),
            decaySeconds: shown("decay_seconds"),
            sustain: shown("sustain"),
            releaseSeconds: shown("release_seconds"),
          }}
          marked={dragged ? (STAGES[dragged.name] ?? null) : null}
        />
        <div className="knobs">{ENVELOPE.map(knob)}</div>
      </Group>
    </section>
  );
}
