import type { EffectView, NumberSettingView } from "../backend";

// A Delay as Rust describes it (`DelayParam::describe` in uta-core), for
// the UI tests: its nine settings in the panel's order, then Mix. The
// `sounds` text is shortened; the rest is Rust's.

/** The eighteen note lengths, as Rust lists them: triplet, straight, dotted, from 1/32 to 1/1. */
export const DIVISIONS = [32, 16, 8, 4, 2, 1].flatMap((d) => [
  { value: `1/${d}t`, label: `1/${d} triplet` },
  { value: `1/${d}`, label: `1/${d}` },
  { value: `1/${d}d`, label: `1/${d} dotted` },
]);

export interface DelayValues {
  enabled: boolean;
  mix: number;
  sync: boolean;
  division: string;
  timeSeconds: number;
  mode: "stereo" | "ping_pong";
  feedback: number;
  lowCutHz: number;
  highCutHz: number;
  drive: number;
  wobble: number;
  /** What Rust works out from the time and the tempo. */
  echoSeconds: number;
}

/** A new Delay's settings, at 120 BPM. */
export const NEW_DELAY: DelayValues = {
  enabled: true,
  mix: 0.3,
  sync: true,
  division: "1/8d",
  timeSeconds: 0.375,
  mode: "stereo",
  feedback: 0.4,
  lowCutHz: 100,
  highCutHz: 8000,
  drive: 0.2,
  wobble: 0.1,
  echoSeconds: 0.375,
};

function number<N extends string>(
  name: N,
  label: string,
  value: number,
  unit: NumberSettingView["unit"],
  scale: NumberSettingView["scale"],
  limits: [number, number],
  fallback: number,
): NumberSettingView<N> {
  return { name, kind: "number", label, sounds: `${label}.`, value, unit, scale, limits, default: fallback };
}

/** A Delay with ID `id`, its settings `NEW_DELAY`'s but for `values`. */
export function delayView(id: string, values: Partial<DelayValues> = {}): EffectView {
  const v = { ...NEW_DELAY, ...values };
  const d = NEW_DELAY;
  return {
    id,
    name: "Delay",
    enabled: v.enabled,
    kind: "delay",
    mix: number("mix", "Mix", v.mix, "fraction", "linear", [0, 1], d.mix),
    settings: [
      { name: "sync", kind: "toggle", label: "Sync", sounds: "Sync.", value: v.sync, default: d.sync },
      {
        name: "division",
        kind: "choice",
        label: "Time",
        sounds: "Time.",
        value: v.division,
        options: DIVISIONS,
        default: d.division,
      },
      number("time_seconds", "Time", v.timeSeconds, "seconds", "log", [0.001, 5], d.timeSeconds),
      {
        name: "mode",
        kind: "choice",
        label: "Mode",
        sounds: "Mode.",
        value: v.mode,
        options: [
          { value: "stereo", label: "Stereo" },
          { value: "ping_pong", label: "Ping-pong" },
        ],
        default: d.mode,
      },
      number("feedback", "Feedback", v.feedback, "fraction", "linear", [0, 1.1], d.feedback),
      number("low_cut_hz", "Low cut", v.lowCutHz, "hz", "log", [20, 2000], d.lowCutHz),
      number("high_cut_hz", "High cut", v.highCutHz, "hz", "log", [500, 20000], d.highCutHz),
      number("drive", "Drive", v.drive, "fraction", "linear", [0, 1], d.drive),
      number("wobble", "Wobble", v.wobble, "fraction", "linear", [0, 1], d.wobble),
    ],
    echoSeconds: v.echoSeconds,
  };
}
