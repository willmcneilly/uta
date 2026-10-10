import { type KeyboardEvent, useRef, useState } from "react";
import type { Limits, SynthLimits, SynthParam, SynthView, Waveform } from "./backend";
import { Fader } from "./design/Fader";
import type { NumberScale } from "./design/numberScale";
import {
  SETTING_STEPS,
  type Scale,
  formatAmount,
  formatHz,
  formatLevel,
  formatSeconds,
  fromPosition,
  toPosition,
} from "./synthScale";
import { EnvelopeDrawing, FilterDrawing } from "./synth/Drawings";
import type { StageName } from "./synth/envelopeShape";
import { DEFAULT_SAMPLE_RATE } from "./synth/filterCurve";
import "./SynthPanel.css";

interface Props {
  synth: SynthView;
  limits: SynthLimits;
  /** A new track's settings, which a reset sets each fader to. */
  defaults: SynthView;
  /**
   * The engine's sample rate, which the filter's curve depends on, or null
   * before a device is open.
   */
  sampleRate: number | null;
  /** `gesture` is the same for every change in one drag. */
  onChange: (param: SynthParam, gesture?: number) => void;
}

/** Each waveform, with one cycle of it drawn in a 28 by 12 box. */
const WAVEFORMS: { value: Waveform; label: string; cycle: string }[] = [
  { value: "sine", label: "Sine", cycle: "M0,6 C4.5,-2 9.5,-2 14,6 S23.5,14 28,6" },
  { value: "triangle", label: "Triangle", cycle: "M0,6 L7,0 L21,12 L28,6" },
  { value: "saw", label: "Saw", cycle: "M0,12 L14,0 L14,12 L28,0 L28,12" },
  { value: "square", label: "Square", cycle: "M0,12 L0,0 L14,0 L14,12 L28,12 L28,0" },
];

type SettingName = Exclude<SynthParam["name"], "waveform">;

const FIELDS: Record<SettingName, Exclude<keyof SynthView, "waveform">> = {
  cutoff_hz: "cutoffHz",
  resonance: "resonance",
  attack_seconds: "attackSeconds",
  decay_seconds: "decaySeconds",
  sustain: "sustain",
  release_seconds: "releaseSeconds",
};

const STAGES: Partial<Record<SettingName, StageName>> = {
  attack_seconds: "attack",
  decay_seconds: "decay",
  sustain: "sustain",
  release_seconds: "release",
};

/** A fader being dragged, and the value it shows. */
interface Dragged {
  name: SettingName;
  value: number;
}

interface SettingProps {
  label: string;
  name: SettingName;
  value: number;
  defaultValue: number;
  limits: Limits;
  scale: Scale;
  format: (value: number) => string;
  onChange: Props["onChange"];
  onDrag: (dragged: Dragged | null) => void;
}

/** A synth fader's positions, on its log or linear scale. */
function synthScale(limits: Limits, scale: Scale): NumberScale {
  return {
    min: limits[0],
    max: limits[1],
    steps: SETTING_STEPS,
    toPosition: (value) => toPosition(value, limits, scale),
    fromPosition: (position) => fromPosition(position, limits, scale),
  };
}

/**
 * One setting on a fader. It shows the value Rust last sent. An arrow key
 * moves it a hundredth of the way, and Shift+arrow a tenth.
 */
function Setting({
  label,
  name,
  value,
  defaultValue,
  limits,
  scale,
  format,
  onChange,
  onDrag,
}: SettingProps) {
  return (
    <Fader
      className="setting"
      label={label}
      value={value}
      defaultValue={defaultValue}
      scale={synthScale(limits, scale)}
      keyStep={SETTING_STEPS / 100}
      format={format}
      onChange={(setting, gesture) => onChange({ name, value: setting }, gesture)}
      onDrag={(shown) => onDrag(shown === null ? null : { name, value: shown })}
    />
  );
}

const ARROW_STEPS: Record<string, number> = {
  ArrowRight: 1,
  ArrowDown: 1,
  ArrowLeft: -1,
  ArrowUp: -1,
};

/**
 * The waveform, as a radio group drawn as keys, each with one cycle of its
 * wave (provisional: D-11). WebKit on macOS doesn't focus a radio
 * button when it's clicked, and leaves radios out of the Tab order, so the
 * group focuses them itself and handles the arrow keys itself.
 */
function WaveformPicker({
  waveform,
  onChange,
}: {
  waveform: Waveform;
  onChange: Props["onChange"];
}) {
  const radios = useRef(new Map<Waveform, HTMLInputElement>());

  const onKeyDown = (event: KeyboardEvent<HTMLInputElement>, index: number) => {
    const step = ARROW_STEPS[event.key];
    if (step === undefined) return;
    event.preventDefault();
    const next = WAVEFORMS[(index + step + WAVEFORMS.length) % WAVEFORMS.length].value;
    radios.current.get(next)?.focus();
    onChange({ name: "waveform", value: next });
  };

  return (
    <fieldset className="waveform">
      <legend>Waveform</legend>
      {WAVEFORMS.map(({ value, label, cycle }, index) => (
        <label key={value}>
          <input
            ref={(radio) => {
              if (radio) radios.current.set(value, radio);
              else radios.current.delete(value);
            }}
            type="radio"
            name="waveform"
            value={value}
            checked={waveform === value}
            // An explicit tabIndex puts it in WebKit's Tab order.
            tabIndex={waveform === value ? 0 : -1}
            onClick={(event) => event.currentTarget.focus()}
            onKeyDown={(event) => onKeyDown(event, index)}
            onChange={() => onChange({ name: "waveform", value })}
          />
          <svg className="cycle" viewBox="-1 -1 30 14" width={30} height={14} aria-hidden="true">
            <path d={cycle} />
          </svg>
          {label}
        </label>
      ))}
    </fieldset>
  );
}

/**
 * A track's synth: waveform, filter and envelope. The filter and the envelope
 * are drawn above their faders, from the values the faders show, so the
 * drawings follow a drag before Rust replies.
 */
export function SynthPanel({ synth, limits, defaults, sampleRate, onChange }: Props) {
  // The fader being dragged: gesture state, gone when the drag ends.
  const [dragged, setDragged] = useState<Dragged | null>(null);
  const shown: SynthView = dragged ? { ...synth, [FIELDS[dragged.name]]: dragged.value } : synth;
  const setting = (
    label: string,
    name: SettingName,
    limit: Limits,
    scale: Scale,
    format: (value: number) => string,
  ) => (
    <Setting
      label={label}
      name={name}
      value={synth[FIELDS[name]]}
      defaultValue={defaults[FIELDS[name]]}
      limits={limit}
      scale={scale}
      format={format}
      onChange={onChange}
      onDrag={setDragged}
    />
  );

  return (
    <section className="synth" aria-label="Synth">
      <WaveformPicker waveform={synth.waveform} onChange={onChange} />

      <fieldset className="filter">
        <legend>Filter</legend>
        <FilterDrawing
          cutoffHz={shown.cutoffHz}
          resonance={shown.resonance}
          sampleRate={sampleRate ?? DEFAULT_SAMPLE_RATE}
          marked={dragged?.name === "cutoff_hz" || dragged?.name === "resonance"}
        />
        {setting("Cutoff", "cutoff_hz", limits.cutoffHz, "log", formatHz)}
        {setting("Resonance", "resonance", limits.resonance, "linear", formatAmount)}
      </fieldset>

      <fieldset className="envelope">
        <legend>Envelope</legend>
        <EnvelopeDrawing times={shown} marked={dragged ? (STAGES[dragged.name] ?? null) : null} />
        {setting("Attack", "attack_seconds", limits.envelopeSeconds, "log", formatSeconds)}
        {setting("Decay", "decay_seconds", limits.envelopeSeconds, "log", formatSeconds)}
        {setting("Sustain", "sustain", limits.sustain, "linear", formatLevel)}
        {setting("Release", "release_seconds", limits.envelopeSeconds, "log", formatSeconds)}
      </fieldset>
    </section>
  );
}
