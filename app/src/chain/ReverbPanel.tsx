import { useEffect, useId, useRef, useState } from "react";
import type { EffectParam, EffectSettingView, EffectView } from "../backend";
import { Group } from "../design/Group";
import type { FrameLoop } from "../frameLoop";
import { SettingControl } from "./SettingControl";
import { type Band, TailDrawing } from "./TailDrawing";
import { type Dragged, tailSettings } from "./tailSettings";
import "./ReverbPanel.css";

// The Reverb's panel. Its core view is shaped like the Delay's: the drawing
// of its tail on the left, Mode beside it, and the core knobs in a row
// underneath, Mix last. A pull tab on the panel's right edge opens it wider,
// to the Early and Late groups beside the core, each two rows of knobs under
// a section title. Whether it's open is the panel's own view, which the row
// remembers, never the project's. Every control is drawn from Rust's
// description of its setting, so the panel keeps no label, range or default
// of its own; it only says where each goes. provisional: D-41

interface Props {
  effect: EffectView;
  /** Whether it's open wide, to the Early and Late groups. */
  open: boolean;
  onOpen: (open: boolean) => void;
  /** One bar at the song's tempo, in seconds. */
  barSeconds: number;
  /** Where the song is now, in seconds, while it plays; null when it's stopped. */
  songSeconds: ((now: number) => number) | null;
  /** The app's drawing loop, which moves the drawing's light. */
  frames: FrameLoop;
  /** `gesture` is the same for every change in one drag. */
  onSetting: (param: EffectParam, gesture?: number) => void;
  onMix: (mix: number, gesture?: number) => void;
}

/** The core view's knobs, in order, after Mode. */
const CORE = ["predelay_seconds", "decay_seconds", "late_size", "depth", "high_cut_hz", "low_cut_hz"];
/** The Early group's knobs, in order. */
const EARLY = [
  "early_size_seconds",
  "early_cross",
  "diffusion",
  "early_send",
  "early_mod_rate_hz",
  "early_mod_depth",
];
/** The Late group's knobs, in order. Any setting placed nowhere else goes after them. */
const LATE = [
  "bass_mult",
  "bass_xover_hz",
  "high_mult",
  "high_xover_hz",
  "late_cross",
  "late_mod_rate_hz",
  "late_mod_depth",
];
/** The settings placed in the core and the Early group: not in the Late group. */
const PLACED = ["mode", ...CORE, ...EARLY];
/** The settings that move the low or the high band, whose own edge the drawing shows as they're touched. */
const BANDS: Record<string, Band> = {
  bass_mult: "low",
  bass_xover_hz: "low",
  low_cut_hz: "low",
  high_mult: "high",
  high_xover_hz: "high",
  high_cut_hz: "high",
};
/** How long a touched band's edge stays, in milliseconds. */
const TOUCHED_MS = 1600;

/** The pull tab's chevron: › to open, ‹ to close. */
function Chevron({ open }: { open: boolean }) {
  return (
    <svg viewBox="0 0 8 12" width={6} height={9} aria-hidden="true">
      <path d={open ? "M6,2 L2,6 L6,10" : "M2,2 L6,6 L2,10"} />
    </svg>
  );
}

export function ReverbPanel({ effect, open, onOpen, barSeconds, songSeconds, frames, onSetting, onMix }: Props) {
  const advanced = useId();
  // Pointing at the panel, or a control in it focused (as it is while a knob
  // is dragged): with the song stopped, the drawing plays through.
  const [pointing, setPointing] = useState(false);
  const [focused, setFocused] = useState(false);
  // The band whose setting was just touched, shown for a moment.
  const [touched, setTouched] = useState<Band | null>(null);
  // The knob or the drawing's end being dragged: gesture state, gone when the drag ends.
  const [dragged, setDragged] = useState<Dragged | null>(null);
  const untouch = useRef<ReturnType<typeof setTimeout> | null>(null);
  useEffect(
    () => () => {
      if (untouch.current) clearTimeout(untouch.current);
    },
    [],
  );
  const touch = (name: string) => {
    const band = BANDS[name];
    if (!band) return;
    setTouched(band);
    if (untouch.current) clearTimeout(untouch.current);
    untouch.current = setTimeout(() => setTouched(null), TOUCHED_MS);
  };

  const setting = (name: string) => effect.settings.find((s) => s.name === name);
  const control = (found: EffectSettingView | undefined, className?: string) =>
    found && (
      <SettingControl
        key={found.name}
        owner={effect.name}
        setting={found}
        className={className}
        short
        onChange={(value, gesture) => {
          touch(found.name);
          onSetting({ name: found.name, value }, gesture);
        }}
        onDrag={(value) => setDragged(value === null ? null : { name: found.name, value })}
      />
    );

  const decay = setting("decay_seconds");
  const early = EARLY.map(setting);
  const late = [
    ...LATE.map(setting),
    ...effect.settings.filter((s) => !PLACED.includes(s.name) && !LATE.includes(s.name)),
  ];
  const label = `${open ? "Hide" : "Show"} the ${effect.name}'s early and late settings`;

  return (
    <div
      className="reverb-panel"
      data-open={open || undefined}
      onPointerEnter={() => setPointing(true)}
      onPointerLeave={() => setPointing(false)}
      onFocus={() => setFocused(true)}
      onBlur={(event) => {
        if (!event.currentTarget.contains(event.relatedTarget)) setFocused(false);
      }}
    >
      <div className="reverb-core">
        <div className="reverb-top">
          <TailDrawing
            settings={tailSettings(setting, dragged)}
            enabled={effect.enabled}
            barSeconds={barSeconds}
            songSeconds={songSeconds}
            preview={pointing || focused}
            touched={touched}
            decayLimits={decay?.kind === "number" ? decay.limits : [0, 0]}
            onDecay={(seconds, gesture) => onSetting({ name: "decay_seconds", value: seconds }, gesture)}
            onDrag={(seconds) => setDragged(seconds === null ? null : { name: "decay_seconds", value: seconds })}
            frames={frames}
          />
          <div className="reverb-beside">{control(setting("mode"), "reverb-mode")}</div>
        </div>
        <div className="reverb-knobs">
          {CORE.map((name) => control(setting(name)))}
          <SettingControl
            owner={effect.name}
            setting={effect.mix}
            onChange={(value, gesture) => typeof value === "number" && onMix(value, gesture)}
          />
        </div>
      </div>
      {open && (
        <div className="reverb-advanced" id={advanced}>
          <Group title="Early" className="reverb-group">
            <div className="reverb-grid reverb-early">{early.map((found) => control(found))}</div>
          </Group>
          <Group title="Late" className="reverb-group">
            <div className="reverb-grid reverb-late">{late.map((found) => control(found))}</div>
          </Group>
        </div>
      )}
      <button
        type="button"
        className="pull-tab"
        aria-expanded={open}
        aria-controls={open ? advanced : undefined}
        aria-label={label}
        title={label}
        onClick={() => onOpen(!open)}
      >
        <Chevron open={open} />
      </button>
    </div>
  );
}
