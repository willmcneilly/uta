import type { EffectSettingView } from "../backend";
import type { TailSettings } from "./tail";

// The Reverb's settings as its tail drawing takes them, from the settings as
// the panel holds them: the panel's drawing and the preset menu's.

/** A knob or the drawing's end being dragged, and the value it shows. */
export interface Dragged {
  name: string;
  value: number;
}

/**
 * The settings the drawing draws from: as Rust sent them, but for the one
 * being dragged, which it follows before Rust replies. The preset menu draws
 * each factory sound's tail from them too.
 */
export function tailSettings(
  setting: (name: string) => EffectSettingView | undefined,
  dragged: Dragged | null,
): TailSettings {
  const number = (name: string) => {
    if (dragged?.name === name) return dragged.value;
    const found = setting(name);
    return found?.kind === "number" ? found.value : 0;
  };
  const lowCut = setting("low_cut_hz");
  const lowCutOff =
    lowCut?.kind === "number" && lowCut.offAtLowest === true && number("low_cut_hz") <= lowCut.limits[0];
  return {
    predelaySeconds: number("predelay_seconds"),
    decaySeconds: number("decay_seconds"),
    depth: number("depth"),
    highCutHz: number("high_cut_hz"),
    lowCutHz: lowCutOff ? null : number("low_cut_hz"),
    earlySizeSeconds: number("early_size_seconds"),
    diffusion: number("diffusion"),
    earlySend: number("early_send"),
    lateSize: number("late_size"),
    bassMult: number("bass_mult"),
    bassXoverHz: number("bass_xover_hz"),
    highMult: number("high_mult"),
    highXoverHz: number("high_xover_hz"),
  };
}
