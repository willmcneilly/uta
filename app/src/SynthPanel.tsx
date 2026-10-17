import { type KeyboardEvent, type ReactNode, useId, useRef, useState } from "react";
import type {
  ChoiceSettingView,
  FilterMode,
  LfoShape,
  NumberSettingView,
  SoundChoice,
  SynthNumber,
  SynthParam,
  SynthSettingView,
  SynthToggle,
  ToggleSettingView,
  Waveform,
} from "./backend";
import { Fader } from "./design/Fader";
import { Key } from "./design/Key";
import { Knob, type KnobSize } from "./design/Knob";
import { Menu } from "./design/Menu";
import { type Option, SegmentedChoice } from "./design/SegmentedChoice";
import { SETTING_STEPS, formatOctaves, formatOf, isCentred, settingScale } from "./synthScale";
import {
  ChoiceIcon,
  EnvelopeDrawing,
  FilterDrawing,
  LevelsDrawing,
  LfoDrawing,
  OscillatorDrawing,
} from "./synth/Drawings";
import { MODE_ICONS, SHAPE_ICONS, WAVE_ICONS } from "./synth/icons";
import { SIZES } from "./synth/sizes";
import type { StageName } from "./synth/envelopeShape";
import { DEFAULT_SAMPLE_RATE } from "./synth/filterCurve";
import "./SynthPanel.css";

// A track's synth, as Will chose it in the UTA-80 sketch (provisional:
// D-32): its stages as tabs across the top of the panel, in the order the
// sound flows through them, each with a small live drawing over its name;
// the chosen one's controls underneath, with bigger drawings. Every control
// is drawn from the setting Rust describes, so the panel holds no label,
// range, default or choice of its own: it only says which settings go on
// which page, and where. The drawings are drawn from the values the
// controls show, so they follow a drag before Rust replies.

interface Props {
  /**
   * Each of the synth's settings, as Rust describes them: its value, label,
   * unit, limits, scale and default, or its choices.
   */
  settings: SynthSettingView[];
  /** How long one cycle of the LFO lasts at the song's tempo, in seconds, as Rust works it out. */
  lfoSeconds: number;
  /** One bar at the song's tempo, in seconds. */
  barSeconds: number;
  beatsPerBar: number;
  /**
   * The engine's sample rate, which the filter's curve depends on, or null
   * before a device is open.
   */
  sampleRate: number | null;
  /** The factory sounds the Sound menu offers, as Rust lists them. */
  sounds: SoundChoice[];
  /** `gesture` is the same for every change in one drag. */
  onChange: (param: SynthParam, gesture?: number) => void;
  /** Chooses a factory sound, by its key: every setting at once. */
  onSound: (sound: string) => void;
}

/** The synth's stages, in the order the sound flows through them, as its tabs name them. */
const STAGES = [
  ["osc", "Osc"],
  ["mixer", "Mixer"],
  ["filter", "Filter"],
  ["env", "Env"],
  ["lfo", "LFO"],
  ["voice", "Voice"],
  ["mod", "Mod"],
] as const;

type Stage = (typeof STAGES)[number][0];

type NumberName = SynthNumber["name"];

/** The mixer's levels, left to right. */
const LEVELS: NumberName[] = ["osc1_level", "osc2_level", "sub_level", "noise_level"];

/** Each envelope's settings, left to right like its stages, and the stage each moves. */
const ENVELOPES = {
  Filter: [
    ["filter_attack_seconds", "attack"],
    ["filter_decay_seconds", "decay"],
    ["filter_sustain", "sustain"],
    ["filter_release_seconds", "release"],
  ],
  Amp: [
    ["attack_seconds", "attack"],
    ["decay_seconds", "decay"],
    ["sustain", "sustain"],
    ["release_seconds", "release"],
  ],
} as const satisfies Record<string, readonly (readonly [NumberName, StageName])[]>;

/** The mod slots. */
const SLOTS = [1, 2, 3] as const;

/** A knob being dragged, and the value it shows. */
interface Dragged {
  name: NumberName;
  value: number;
}

/** A segmented choice's options, each with its drawing where it has one. */
function withDrawings<V extends string>(
  options: { value: V; label: string }[],
  icons?: Record<V, string>,
): Option<V>[] {
  return options.map((option) => ({
    ...option,
    drawing: icons && <ChoiceIcon path={icons[option.value]} />,
  }));
}

export function SynthPanel({
  settings,
  lfoSeconds,
  barSeconds,
  beatsPerBar,
  sampleRate,
  sounds,
  onChange,
  onSound,
}: Props) {
  const [stage, setStage] = useState<Stage>("osc");
  // The knob or fader being dragged: gesture state, gone when the drag ends.
  const [dragged, setDragged] = useState<Dragged | null>(null);
  // Empty mod slots opened with their +, to choose a destination: the
  // panel's own view, not the project's. A slot is used once it has one.
  const [opened, setOpened] = useState<ReadonlySet<number>>(new Set());
  const group = useId();
  const tabs = useRef(new Map<Stage, HTMLButtonElement>());

  const byName = new Map<string, SynthSettingView>(settings.map((s) => [s.name, s]));
  const find = (name: string, kind: SynthSettingView["kind"]) => {
    const setting = byName.get(name);
    if (setting?.kind !== kind) throw new Error(`Rust sent no ${kind} setting ${name}`);
    return setting;
  };
  const number = (name: NumberName) => find(name, "number") as NumberSettingView<NumberName>;
  const toggle = (name: SynthToggle) => find(name, "toggle") as ToggleSettingView<SynthToggle>;
  const choice = <V extends string>(name: string) =>
    find(name, "choice") as ChoiceSettingView<string, V>;

  /** The value a setting's control shows: the drag's, or what Rust last sent. */
  const shown = (name: NumberName) =>
    dragged?.name === name ? dragged.value : number(name).value;
  const send = (name: string, value: number | string | boolean, gesture?: number) =>
    // The setting came from Rust with this name, so its value fits it.
    onChange({ name, value } as SynthParam, gesture);

  const osc1Wave = choice<Waveform>("osc1_wave").value;
  const pulse = osc1Wave === "pulse";
  const mono = choice("voice_mode").value === "mono";
  const synced = toggle("lfo_sync").value;
  const used = SLOTS.filter((slot) => choice(`mod${slot}_destination`).value !== "off");

  // ---- Controls, each from its setting's description ----

  /**
   * A setting on a knob, labelled in a few words where its page says whose
   * it is. `inactive` draws it in the third ink, still live, where it does
   * nothing in the current settings. provisional: D-35
   */
  const knob = (
    name: NumberName,
    { size = 28, hideLabel = false, inactive = false }: { size?: KnobSize; hideLabel?: boolean; inactive?: boolean } = {},
  ) => {
    const setting = number(name);
    return (
      <span key={name} className="cell" data-inactive={inactive || undefined}>
        <Knob
          label={setting.short ?? setting.label}
          ariaLabel={setting.label}
          hideLabel={hideLabel}
          size={size}
          centred={isCentred(setting)}
          value={setting.value}
          defaultValue={setting.default}
          scale={settingScale(setting)}
          keyStep={setting.step === undefined ? SETTING_STEPS / 100 : 1}
          format={formatOf(setting)}
          onChange={(value, gesture) => send(name, value, gesture)}
          onDrag={(value) => setDragged(value === null ? null : { name, value })}
        />
      </span>
    );
  };

  /** A level on a fader stood on end, compared side by side with the others. provisional: D-26 */
  const fader = (name: NumberName) => {
    const setting = number(name);
    return (
      <Fader
        key={name}
        label={setting.short ?? setting.label}
        ariaLabel={setting.label}
        orientation="vertical"
        className="level-fader"
        value={setting.value}
        defaultValue={setting.default}
        scale={settingScale(setting)}
        keyStep={SETTING_STEPS / 100}
        format={formatOf(setting)}
        onChange={(value, gesture) => send(name, value, gesture)}
        onDrag={(value) => setDragged(value === null ? null : { name, value })}
      />
    );
  };

  /**
   * A choice as a row of options. Compact, each option is only its drawing,
   * its name kept for assistive tech, where a page's row says what it
   * chooses. provisional: D-34
   */
  const segmented = <V extends string>(
    name: string,
    icons?: Record<V, string>,
    className = "",
  ) => {
    const setting = choice<V>(name);
    return (
      <SegmentedChoice
        legend={setting.label}
        name={`${group}-${name}`}
        className={`${icons ? "compact" : ""} ${className}`.trim()}
        options={withDrawings(setting.options, icons)}
        value={setting.value}
        onChange={(value) => send(name, value)}
      />
    );
  };

  /** A switch on a key that stays down while it's on. */
  const key = (name: SynthToggle, inactive = false) => {
    const setting = toggle(name);
    return (
      <Key
        className="toggle"
        data-inactive={inactive || undefined}
        aria-label={setting.label}
        pressed={setting.value}
        onClick={() => send(name, !setting.value)}
      >
        {setting.short ?? setting.label}
      </Key>
    );
  };

  /** A choice from a menu, for one with too many options to lay out. */
  const menu = (name: string, hideLabel = false) => {
    const setting = choice(name);
    return (
      <label className="menu-cell">
        {!hideLabel && <span className="knob-label">{setting.short ?? setting.label}</span>}
        <Menu
          aria-label={setting.label}
          value={setting.value}
          onChange={(event) => send(name, event.target.value)}
        >
          {setting.options.map((option) => (
            <option key={option.value} value={option.value}>
              {option.label}
            </option>
          ))}
        </Menu>
      </label>
    );
  };

  // ---- The drawings, from the values the controls show ----

  const tuning = {
    osc1Octave: shown("osc1_octave"),
    osc2Octave: shown("osc2_octave"),
    osc2PitchSemitones: shown("osc2_pitch_semitones"),
    sync: toggle("osc2_sync").value,
  };
  const oscillator = (which: 1 | 2, size: { width: number; height: number }) => (
    <OscillatorDrawing
      which={which}
      wave={choice<Waveform>(`osc${which}_wave`).value}
      pulseWidth={shown("osc1_width")}
      tuning={tuning}
      {...size}
    />
  );
  const filter = (size: { width: number; height: number }, labels = false) => (
    <FilterDrawing
      cutoffHz={shown("cutoff_hz")}
      resonance={shown("resonance")}
      mode={choice<FilterMode>("filter_mode").value}
      envOctaves={shown("filter_env_octaves")}
      sampleRate={sampleRate ?? DEFAULT_SAMPLE_RATE}
      labels={labels}
      marked={dragged?.name === "cutoff_hz" || dragged?.name === "resonance"}
      {...size}
    />
  );
  const envelope = (which: keyof typeof ENVELOPES, size: { width: number; height: number }) => {
    const [[attack], [decay], [sustain], [release]] = ENVELOPES[which];
    const marked = ENVELOPES[which].find(([name]) => name === dragged?.name)?.[1] ?? null;
    return (
      <EnvelopeDrawing
        name={which}
        times={{
          attackSeconds: shown(attack),
          decaySeconds: shown(decay),
          sustain: shown(sustain),
          releaseSeconds: shown(release),
        }}
        marked={marked}
        {...size}
      />
    );
  };
  // While its Rate knob is dragged, the cycle follows the knob; otherwise
  // it's Rust's, which knows what a synced division is at the tempo.
  const cycleSeconds = dragged?.name === "lfo_rate_hz" ? 1 / dragged.value : lfoSeconds;
  const lfo = (size: { width: number; height: number }) => (
    <LfoDrawing
      shape={choice<LfoShape>("lfo_shape").value}
      cyclesPerBar={barSeconds / cycleSeconds}
      beatsPerBar={beatsPerBar}
      {...size}
    />
  );

  /** Each stage's small drawing, for its tab: what it's doing, at a glance. */
  const glance: Record<Stage, ReactNode> = {
    osc: oscillator(1, SIZES.tab),
    mixer: <LevelsDrawing levels={LEVELS.map(shown)} {...SIZES.tab} />,
    filter: filter(SIZES.tab),
    env: envelope("Amp", SIZES.tab),
    lfo: lfo(SIZES.tab),
    voice: (
      <span className="glance-word">
        {choice("voice_mode").options.find((o) => o.value === choice("voice_mode").value)?.label}
      </span>
    ),
    mod: <span className="glance-word">{`${used.length}/${SLOTS.length}`}</span>,
  };

  // ---- The pages ----

  const subOctave = number("sub_octave");
  const pages: Record<Stage, () => ReactNode> = {
    osc: () => (
      <div className="osc-rows">
        <span className="row-label">Osc 1</span>
        {oscillator(1, SIZES.oscillator)}
        {segmented("osc1_wave", WAVE_ICONS)}
        {knob("osc1_octave", { size: 24 })}
        {knob("osc1_width", { size: 24, inactive: !pulse })}
        <span />
        <span className="row-label">Osc 2</span>
        {oscillator(2, SIZES.oscillator)}
        {segmented("osc2_wave", WAVE_ICONS)}
        {knob("osc2_octave", { size: 24 })}
        {knob("osc2_pitch_semitones", { size: 24 })}
        {key("osc2_sync")}
      </div>
    ),
    mixer: () => (
      <div className="mixer-page">
        <div className="levels">{LEVELS.map(fader)}</div>
        {/* The sub's octave, its only other setting, as a choice of its
            whole steps, nearest first. */}
        <SegmentedChoice
          legend={subOctave.label}
          name={`${group}-sub_octave`}
          className="sub-octave"
          options={wholeSteps(subOctave).map((value) => ({
            value: String(value),
            label: formatOctaves(value),
          }))}
          value={String(subOctave.value)}
          onChange={(value) => send("sub_octave", Number(value))}
        />
      </div>
    ),
    filter: () => (
      <div className="beside-page">
        {segmented<FilterMode>("filter_mode", MODE_ICONS)}
        {filter(SIZES.filter, true)}
        <div className="page-knobs three">
          {knob("cutoff_hz")}
          {knob("resonance")}
          {knob("filter_env_octaves")}
          {knob("filter_key")}
          {knob("filter_lfo_octaves")}
        </div>
      </div>
    ),
    env: () => (
      <div className="env-rows">
        <span />
        <span />
        {ENVELOPES.Amp.map(([name]) => (
          <span key={name} className="column-label">
            {number(name).short ?? number(name).label}
          </span>
        ))}
        {(["Filter", "Amp"] as const).map((which) => (
          <EnvelopeRow key={which} label={which}>
            {envelope(which, SIZES.envelope)}
            {ENVELOPES[which].map(([name]) => knob(name, { hideLabel: true }))}
          </EnvelopeRow>
        ))}
      </div>
    ),
    lfo: () => (
      <div className="beside-page">
        {segmented<LfoShape>("lfo_shape", SHAPE_ICONS)}
        {lfo(SIZES.lfo)}
        <div className="page-knobs two">
          <span className="rate">{synced ? menu("lfo_division") : knob("lfo_rate_hz")}</span>
          <span className="keys">
            {key("lfo_sync")}
            {key("lfo_retrigger")}
          </span>
          {knob("lfo_vibrato_semitones")}
          {knob("lfo_width", { inactive: !pulse })}
        </div>
      </div>
    ),
    voice: () => (
      <div className="voice-page">
        {segmented("voice_mode", undefined, "voice-mode")}
        <div className="page-knobs">
          {knob("glide_seconds", { inactive: !mono })}
          <span className="keys">
            {key("legato", !mono)}
            {key("osc_retrigger")}
          </span>
        </div>
      </div>
    ),
    mod: () => (
      <div className="slots">
        {SLOTS.map((slot) =>
          used.includes(slot) || opened.has(slot) ? (
            <div key={slot} className="slot" role="group" aria-label={`Mod ${slot}`}>
              {menu(`mod${slot}_source`, true)}
              <span className="arrow" aria-hidden="true">
                →
              </span>
              {menu(`mod${slot}_destination`, true)}
              {knob(`mod${slot}_amount`, { size: 24, hideLabel: true })}
            </div>
          ) : (
            <div key={slot} className="slot">
              <button
                type="button"
                className="slot-add"
                aria-label={`Add mod ${slot}`}
                onClick={() => setOpened(new Set(opened).add(slot))}
              >
                +
              </button>
            </div>
          ),
        )}
      </div>
    ),
  };

  /** Opens `next`'s page. Leaving Mod shuts the slots opened with no destination. */
  const choose = (next: Stage) => {
    setStage(next);
    if (next !== "mod") setOpened(new Set());
  };

  /** Arrow keys move along the tabs, as the editor's do. */
  const onTabKey = (event: KeyboardEvent, index: number) => {
    const step = { ArrowRight: 1, ArrowLeft: -1 }[event.key];
    if (step === undefined) return;
    event.preventDefault();
    const [next] = STAGES[(index + step + STAGES.length) % STAGES.length];
    choose(next);
    tabs.current.get(next)?.focus();
  };

  return (
    <section className="synth" aria-label="Synth">
      {/* The factory sounds, at the start of the panel as the sound flows.
          It names no sound: which one a track started from isn't kept, and
          every setting can move from it. provisional: D-37 */}
      <div className="sound-row">
        <Menu
          aria-label="Sound"
          value=""
          onChange={(event) => {
            if (event.target.value) onSound(event.target.value);
          }}
        >
          <option value="" disabled>
            Sound
          </option>
          {sounds.map((sound) => (
            <option key={sound.value} value={sound.value}>
              {sound.label}
            </option>
          ))}
        </Menu>
      </div>
      <div className="stages" role="tablist" aria-label="Synth stages">
        {STAGES.map(([name, title], index) => (
          <span key={name} className="stage">
            {index > 0 && (
              <svg className="stage-arrow" viewBox="0 0 8 12" width={6} height={10} aria-hidden="true">
                <path d="M1,2 L7,6 L1,10" />
              </svg>
            )}
            <button
              ref={(tab) => {
                if (tab) tabs.current.set(name, tab);
                else tabs.current.delete(name);
              }}
              type="button"
              role="tab"
              id={`${group}-tab-${name}`}
              aria-controls={`${group}-page`}
              aria-selected={stage === name}
              tabIndex={stage === name ? 0 : -1}
              className="stage-tab"
              onClick={() => choose(name)}
              onKeyDown={(event) => onTabKey(event, index)}
            >
              <span className="glance" aria-hidden="true">
                {glance[name]}
              </span>
              <span className="stage-name">{title}</span>
            </button>
          </span>
        ))}
      </div>
      <div
        className={`stage-page page-${stage}`}
        role="tabpanel"
        id={`${group}-page`}
        aria-labelledby={`${group}-tab-${stage}`}
      >
        {pages[stage]()}
      </div>
    </section>
  );
}

/** One envelope's row: its name, then its drawing and knobs in the grid's columns. */
function EnvelopeRow({ label, children }: { label: string; children: ReactNode }) {
  return (
    <>
      <span className="row-label">{label}</span>
      {children}
    </>
  );
}

/** Each whole step of a stepped setting, nearest zero first: the sub's −1 and −2. */
function wholeSteps({ limits: [min, max], step = 1 }: NumberSettingView): number[] {
  const steps = Array.from({ length: Math.round((max - min) / step) + 1 }, (_, i) => min + i * step);
  return steps.sort((a, b) => Math.abs(a) - Math.abs(b));
}
