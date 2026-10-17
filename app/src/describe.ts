// Puts each setting's value from the outline together with its description,
// which Rust sends once for the whole project rather than with every track
// (RFC-004, part 2, "Growing later"). What comes out is what the panels draw
// from: a setting with its label, range and default, all Rust's.

import type {
  ChoiceSettingView,
  DescriptionTable,
  Descriptions,
  DrumSettingView,
  EffectOutline,
  EffectSettingView,
  EffectView,
  KickModel,
  NumberSettingView,
  SourceOutline,
  SourceView,
  SynthSettingView,
} from "./backend";

/** A setting as a track carries it: its name and value. */
interface Value {
  name: string;
  value: number | string | boolean;
}

/**
 * `setting` with its description from `table`. Rust describes every setting
 * it sends, so one missing is a bug on its side, and says so.
 */
function described<S>(table: DescriptionTable | undefined, setting: Value): S {
  const description = table?.[setting.name];
  if (!description) throw new Error(`Rust sent ${setting.name} without describing it`);
  // The description's kind matches the value's: Rust made both.
  return { ...description, ...setting } as S;
}

/** A track's source, each setting with its description. */
export function describeSource(source: SourceOutline, descriptions: Descriptions): SourceView {
  if (source.kind === "synth") {
    return {
      kind: "synth",
      settings: source.settings.map((setting) =>
        described<SynthSettingView>(descriptions.synth, setting),
      ),
    };
  }
  // The kick's settings, and so their descriptions, are its model's.
  const model = source.kit.rows.find((row) => row.model)?.model?.value;
  if (!model) throw new Error("Rust sent a kit without its kick's model");
  const kit = descriptions.drums[model];
  return {
    kind: "drums",
    kit: {
      rows: source.kit.rows.map((row) => {
        const table = kit[row.sound];
        return {
          ...row,
          settings: row.settings.map((setting) => described<DrumSettingView>(table, setting)),
          model: row.model && described<ChoiceSettingView<"model", KickModel>>(table, row.model),
        };
      }),
    },
  };
}

/** An effect, its Mix and each of its settings with its description. */
export function describeEffect(effect: EffectOutline, descriptions: Descriptions): EffectView {
  const table = descriptions.effects[effect.kind];
  return {
    ...effect,
    mix: described<NumberSettingView<"mix">>(table, effect.mix),
    settings: effect.settings.map((setting) => described<EffectSettingView>(table, setting)),
  };
}
