import { type KeyboardEvent, useRef } from "react";
import type { Limits, SynthLimits, SynthParam, SynthView, Waveform } from "./backend";
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
import { useGesture } from "./useGesture";

interface Props {
  synth: SynthView;
  limits: SynthLimits;
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
  limits: Limits;
  scale: Scale;
  format: (value: number) => string;
  onChange: Props["onChange"];
}

/** One setting on a slider. It shows the value Rust last sent. */
function Slider({ label, name, value, limits, scale, format, onChange }: SliderProps) {
  const gesture = useGesture();
  return (
    <label className="setting">
      <span>{label}</span>
      <input
        type="range"
        min={0}
        max={SLIDER_STEPS}
        step={1}
        value={toPosition(value, limits, scale)}
        aria-valuetext={format(value)}
        onPointerDown={gesture.start}
        onChange={(event) =>
          onChange(
            { name, value: fromPosition(event.currentTarget.valueAsNumber, limits, scale) },
            gesture.current(),
          )
        }
      />
      <output>{format(value)}</output>
    </label>
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
export function SynthPanel({ synth, limits, onChange }: Props) {
  return (
    <section className="synth" aria-label="Synth">
      <WaveformPicker waveform={synth.waveform} onChange={onChange} />

      <fieldset>
        <legend>Filter</legend>
        <Slider
          label="Cutoff"
          name="cutoff_hz"
          value={synth.cutoffHz}
          limits={limits.cutoffHz}
          scale="log"
          format={formatHz}
          onChange={onChange}
        />
        <Slider
          label="Resonance"
          name="resonance"
          value={synth.resonance}
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
          limits={limits.envelopeSeconds}
          scale="log"
          format={formatSeconds}
          onChange={onChange}
        />
        <Slider
          label="Decay"
          name="decay_seconds"
          value={synth.decaySeconds}
          limits={limits.envelopeSeconds}
          scale="log"
          format={formatSeconds}
          onChange={onChange}
        />
        <Slider
          label="Sustain"
          name="sustain"
          value={synth.sustain}
          limits={limits.sustain}
          scale="linear"
          format={formatLevel}
          onChange={onChange}
        />
        <Slider
          label="Release"
          name="release_seconds"
          value={synth.releaseSeconds}
          limits={limits.envelopeSeconds}
          scale="log"
          format={formatSeconds}
          onChange={onChange}
        />
      </fieldset>
    </section>
  );
}
