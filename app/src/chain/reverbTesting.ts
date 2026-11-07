import type { EffectView, NumberSettingView, Scale, Unit } from "../backend";

// A Reverb as Rust describes it (`ReverbParam::describe` in uta-core), for
// the UI tests: its twenty settings in RFC-010's order, then Mix. The
// `sounds` text is shortened; the rest is Rust's.

export interface ReverbValues {
  enabled: boolean;
  mix: number;
  mode: string;
  predelay_seconds: number;
  decay_seconds: number;
  depth: number;
  high_cut_hz: number;
  low_cut_hz: number;
  early_size_seconds: number;
  early_cross: number;
  diffusion: number;
  early_send: number;
  early_mod_rate_hz: number;
  early_mod_depth: number;
  late_size: number;
  late_cross: number;
  late_mod_rate_hz: number;
  late_mod_depth: number;
  bass_mult: number;
  bass_xover_hz: number;
  high_mult: number;
  high_xover_hz: number;
}

/** A new Reverb's settings. */
export const NEW_REVERB: ReverbValues = {
  enabled: true,
  mix: 0.25,
  mode: "room",
  predelay_seconds: 0.01,
  decay_seconds: 2,
  depth: 0.5,
  high_cut_hz: 20000,
  low_cut_hz: 20,
  early_size_seconds: 0.03,
  early_cross: 1,
  diffusion: 0.8,
  early_send: 0.9,
  early_mod_rate_hz: 0.5,
  early_mod_depth: 0,
  late_size: 0.5,
  late_cross: 1,
  late_mod_rate_hz: 0.5,
  late_mod_depth: 0,
  bass_mult: 1,
  bass_xover_hz: 500,
  high_mult: 0.5,
  high_xover_hz: 4000,
};

type NumberName = Exclude<keyof ReverbValues, "enabled" | "mix" | "mode">;

/** Each number setting's label, short name, unit, scale and limits, in Rust's order. */
const NUMBERS: [NumberName, string, string | undefined, Unit, Scale, [number, number]][] = [
  ["predelay_seconds", "Predelay", undefined, "seconds", "linear", [0, 0.5]],
  ["decay_seconds", "Decay", undefined, "seconds", "log", [0.1, 30]],
  ["depth", "Depth", undefined, "fraction", "linear", [0, 1]],
  ["high_cut_hz", "High cut", undefined, "hz", "log", [1000, 20000]],
  ["low_cut_hz", "Low cut", undefined, "hz", "log", [20, 1000]],
  ["early_size_seconds", "Early size", "Size", "seconds", "log", [0.002, 1]],
  ["early_cross", "Early cross", "Cross", "fraction", "linear", [0, 1]],
  ["diffusion", "Diffusion", undefined, "fraction", "linear", [0, 1]],
  ["early_send", "Early send", "Send", "fraction", "linear", [0, 1]],
  ["early_mod_rate_hz", "Early mod rate", "Mod rate", "hz", "log", [0.05, 5]],
  ["early_mod_depth", "Early mod depth", "Mod depth", "fraction", "linear", [0, 1]],
  ["late_size", "Late size", "Size", "fraction", "linear", [0, 1]],
  ["late_cross", "Late cross", "Cross", "fraction", "linear", [0, 1]],
  ["late_mod_rate_hz", "Late mod rate", "Mod rate", "hz", "log", [0.05, 5]],
  ["late_mod_depth", "Late mod depth", "Mod depth", "fraction", "linear", [0, 1]],
  ["bass_mult", "Bass mult", "Low ×", "times", "log", [0.25, 4]],
  ["bass_xover_hz", "Bass xover", "Low until", "hz", "log", [100, 2000]],
  ["high_mult", "High mult", "High ×", "times", "log", [0.1, 2]],
  ["high_xover_hz", "High xover", "High from", "hz", "log", [1000, 16000]],
];

/** A Reverb with ID `id`, its settings `NEW_REVERB`'s but for `values`. */
export function reverbView(id: string, values: Partial<ReverbValues> = {}): EffectView {
  const v = { ...NEW_REVERB, ...values };
  const d = NEW_REVERB;
  const numbers = NUMBERS.map(
    ([name, label, short, unit, scale, limits]): NumberSettingView => ({
      name,
      kind: "number",
      label,
      ...(short === undefined ? {} : { short }),
      sounds: `${label}.`,
      ...(name === "low_cut_hz" ? { offAtLowest: true } : {}),
      value: v[name],
      unit,
      scale,
      limits,
      default: d[name],
    }),
  );
  return {
    id,
    name: "Reverb",
    enabled: v.enabled,
    kind: "reverb",
    mix: {
      name: "mix",
      kind: "number",
      label: "Mix",
      sounds: "Mix.",
      value: v.mix,
      unit: "fraction",
      scale: "linear",
      limits: [0, 1],
      default: d.mix,
    },
    settings: [
      {
        name: "mode",
        kind: "choice",
        label: "Mode",
        sounds: "Mode.",
        value: v.mode,
        options: [{ value: "room", label: "Room" }],
        default: d.mode,
      },
      ...numbers,
    ],
  };
}
