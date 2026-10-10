import { type KeyboardEvent, useRef } from "react";
import type { Limits, SynthLimits, SynthParam, SynthView, Waveform } from "./backend";
import { Slider as SliderControl } from "./design/Slider";
import type { SliderScale } from "./design/sliderScale";
import {
  SLIDER_STEPS,
  type Scale,
  formatAmount,
  formatHz,
  formatLevel,
  formatSeconds,
  fromPosition,
  toPosition,
} from "./synthScale";
import "./SynthPanel.css";

interface Props {
  synth: SynthView;
  limits: SynthLimits;
  /** A new track's settings, which double-click resets each slider to. */
  defaults: SynthView;
  /** `gesture` is the same for every change in one drag. */
  onChange: (param: SynthParam, gesture?: number) => void;
}

const WAVEFORMS: { value: Waveform; label: string }[] = [
  { value: "sine", label: "Sine" },
  { value: "triangle", label: "Triangle" },
  { value: "saw", label: "Saw" },
  { value: "square", label: "Square" },
];

type SliderName = Exclude<SynthParam["name"], "waveform">;

interface SliderProps {
  label: string;
  name: SliderName;
  value: number;
  defaultValue: number;
  limits: Limits;
  scale: Scale;
  format: (value: number) => string;
  onChange: Props["onChange"];
}

/** A synth slider's positions, on its log or linear scale. */
function synthScale(limits: Limits, scale: Scale): SliderScale {
  return {
    min: limits[0],
    max: limits[1],
    steps: SLIDER_STEPS,
    toPosition: (value) => toPosition(value, limits, scale),
    fromPosition: (position) => fromPosition(position, limits, scale),
  };
}

/**
 * One setting on a slider. It shows the value Rust last sent. An arrow key
 * moves it a hundredth of the way, and Shift+arrow a tenth.
 */
function Slider({ label, name, value, defaultValue, limits, scale, format, onChange }: SliderProps) {
  return (
    <SliderControl
      className="setting"
      label={label}
      value={value}
      defaultValue={defaultValue}
      scale={synthScale(limits, scale)}
      keyStep={SLIDER_STEPS / 100}
      format={format}
      onChange={(setting, gesture) => onChange({ name, value: setting }, gesture)}
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
 * The waveform, as a radio group. WebKit on macOS doesn't focus a radio
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
      {WAVEFORMS.map(({ value, label }, index) => (
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
          {label}
        </label>
      ))}
    </fieldset>
  );
}

/** A track's synth: waveform, filter and envelope. */
export function SynthPanel({ synth, limits, defaults, onChange }: Props) {
  return (
    <section className="synth" aria-label="Synth">
      <WaveformPicker waveform={synth.waveform} onChange={onChange} />

      <fieldset>
        <legend>Filter</legend>
        <Slider
          label="Cutoff"
          name="cutoff_hz"
          value={synth.cutoffHz}
          defaultValue={defaults.cutoffHz}
          limits={limits.cutoffHz}
          scale="log"
          format={formatHz}
          onChange={onChange}
        />
        <Slider
          label="Resonance"
          name="resonance"
          value={synth.resonance}
          defaultValue={defaults.resonance}
          limits={limits.resonance}
          scale="linear"
          format={formatAmount}
          onChange={onChange}
        />
      </fieldset>

      <fieldset className="envelope">
        <legend>Envelope</legend>
        <Slider
          label="Attack"
          name="attack_seconds"
          value={synth.attackSeconds}
          defaultValue={defaults.attackSeconds}
          limits={limits.envelopeSeconds}
          scale="log"
          format={formatSeconds}
          onChange={onChange}
        />
        <Slider
          label="Decay"
          name="decay_seconds"
          value={synth.decaySeconds}
          defaultValue={defaults.decaySeconds}
          limits={limits.envelopeSeconds}
          scale="log"
          format={formatSeconds}
          onChange={onChange}
        />
        <Slider
          label="Sustain"
          name="sustain"
          value={synth.sustain}
          defaultValue={defaults.sustain}
          limits={limits.sustain}
          scale="linear"
          format={formatLevel}
          onChange={onChange}
        />
        <Slider
          label="Release"
          name="release_seconds"
          value={synth.releaseSeconds}
          defaultValue={defaults.releaseSeconds}
          limits={limits.envelopeSeconds}
          scale="log"
          format={formatSeconds}
          onChange={onChange}
        />
      </fieldset>
    </section>
  );
}
