import { useState } from "react";
import type { DelayParam, EffectSettingView, EffectView } from "../backend";
import type { FrameLoop } from "../frameLoop";
import { EchoDrawing } from "./EchoDrawing";
import { SettingControl } from "./SettingControl";
import "./DelayPanel.css";

// The Delay's panel: the drawing of its echoes on the left, Time with Sync
// beside it and Mode under them on the right, and the rest as knobs in a row
// underneath, Mix last. Time is a menu of note lengths when synced and a
// knob in seconds when not. Every control is drawn from Rust's description
// of its setting, so the panel keeps no label, range or default of its own;
// it only says where each goes. provisional: D-31

interface Props {
  effect: EffectView;
  /** One bar at the song's tempo, in seconds. */
  barSeconds: number;
  beatsPerBar: number;
  /** Where the song is now, in seconds, while it plays; null when it's stopped. */
  songSeconds: ((now: number) => number) | null;
  /** The app's drawing loop, which moves the drawing's light. */
  frames: FrameLoop;
  /** `gesture` is the same for every change in one drag. */
  onSetting: (param: DelayParam, gesture?: number) => void;
  onMix: (mix: number, gesture?: number) => void;
}

/** The settings placed beside the drawing. Every other one is a knob in the row. */
const BESIDE = ["sync", "division", "time_seconds", "mode"];

export function DelayPanel({
  effect,
  barSeconds,
  beatsPerBar,
  songSeconds,
  frames,
  onSetting,
  onMix,
}: Props) {
  // Pointing at the panel, or a control in it focused (as it is while a knob
  // is dragged): with the song stopped, the drawing plays its echoes.
  const [pointing, setPointing] = useState(false);
  const [focused, setFocused] = useState(false);

  const setting = (name: string) => effect.settings.find((s) => s.name === name);
  const number = (name: string) => {
    const found = setting(name);
    return found?.kind === "number" ? found.value : 0;
  };
  const control = (found: EffectSettingView | undefined, className?: string) =>
    found && (
      <SettingControl
        key={found.name}
        owner={effect.name}
        setting={found}
        className={className}
        onChange={(value, gesture) => onSetting({ name: found.name, value }, gesture)}
      />
    );

  const sync = setting("sync");
  const synced = sync?.kind === "toggle" && sync.value;
  const rest = effect.settings.filter((s) => !BESIDE.includes(s.name));

  return (
    <div
      className="delay-panel"
      onPointerEnter={() => setPointing(true)}
      onPointerLeave={() => setPointing(false)}
      onFocus={() => setFocused(true)}
      onBlur={(event) => {
        if (!event.currentTarget.contains(event.relatedTarget)) setFocused(false);
      }}
    >
      <div className="delay-top">
        <EchoDrawing
          settings={{
            spacing: effect.echoSeconds,
            feedback: number("feedback"),
            drive: number("drive"),
            pingPong: setting("mode")?.value === "ping_pong",
          }}
          wobble={number("wobble")}
          enabled={effect.enabled}
          barSeconds={barSeconds}
          beatsPerBar={beatsPerBar}
          songSeconds={songSeconds}
          preview={pointing || focused}
          frames={frames}
        />
        <div className="delay-beside">
          <div className="delay-time">
            {control(synced ? setting("division") : setting("time_seconds"), "delay-time-control")}
            {control(sync, "delay-sync")}
          </div>
          {control(setting("mode"), "delay-mode")}
        </div>
      </div>
      <div className="delay-knobs">
        {rest.map((s) => control(s))}
        <SettingControl
          owner={effect.name}
          setting={effect.mix}
          onChange={(value, gesture) => typeof value === "number" && onMix(value, gesture)}
        />
      </div>
    </div>
  );
}
