import { useState } from "react";
import type { Limits, SynthLimits, SynthParam, SynthView, Waveform } from "./backend";
import { Group } from "./design/Group";
import { Knob } from "./design/Knob";
import type { NumberScale } from "./design/numberScale";
import { type Option, SegmentedChoice } from "./design/SegmentedChoice";
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
  /** A new track's settings, which a reset sets each knob to. */
  defaults: SynthView;
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

/** Each waveform, with one cycle of it drawn. */
const WAVEFORMS: Option<Waveform>[] = [
  { value: "sine", label: "Sine", drawing: cycle("M0,6 C4.5,-2 9.5,-2 14,6 S23.5,14 28,6") },
  { value: "triangle", label: "Triangle", drawing: cycle("M0,6 L7,0 L21,12 L28,6") },
  { value: "saw", label: "Saw", drawing: cycle("M0,12 L14,0 L14,12 L28,0 L28,12") },
  { value: "square", label: "Square", drawing: cycle("M0,12 L0,0 L14,0 L14,12 L28,12 L28,0") },
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

/** A knob being dragged, and the value it shows. */
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

/** A synth knob's positions, on its log or linear scale. */
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
 * One setting on a knob. It shows the value Rust last sent. An arrow key
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
    <Knob
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

/**
 * A track's synth: waveform, filter and envelope. The filter and the envelope
 * are drawn above their knobs, from the values the knobs show, so the
 * drawings follow a drag before Rust replies.
 */
export function SynthPanel({ synth, limits, defaults, sampleRate, onChange }: Props) {
  // The knob being dragged: gesture state, gone when the drag ends.
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
      <SegmentedChoice
        legend="Waveform"
        name="waveform"
        options={WAVEFORMS}
        value={synth.waveform}
        onChange={(value) => onChange({ name: "waveform", value })}
      />

      <Group title="Filter" className="filter">
        <FilterDrawing
          cutoffHz={shown.cutoffHz}
          resonance={shown.resonance}
          sampleRate={sampleRate ?? DEFAULT_SAMPLE_RATE}
          marked={dragged?.name === "cutoff_hz" || dragged?.name === "resonance"}
        />
        <div className="knobs">
          {setting("Cutoff", "cutoff_hz", limits.cutoffHz, "log", formatHz)}
          {setting("Resonance", "resonance", limits.resonance, "linear", formatAmount)}
        </div>
      </Group>

      <Group title="Envelope" className="envelope">
        <EnvelopeDrawing times={shown} marked={dragged ? (STAGES[dragged.name] ?? null) : null} />
        <div className="knobs">
          {setting("Attack", "attack_seconds", limits.envelopeSeconds, "log", formatSeconds)}
          {setting("Decay", "decay_seconds", limits.envelopeSeconds, "log", formatSeconds)}
          {setting("Sustain", "sustain", limits.sustain, "linear", formatLevel)}
          {setting("Release", "release_seconds", limits.envelopeSeconds, "log", formatSeconds)}
        </div>
      </Group>
    </section>
  );
}
