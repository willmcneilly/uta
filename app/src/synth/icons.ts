// The small drawings on the synth panel's choices, as SVG paths in a 22 by
// 10 box: what each option does, read quicker than its name. ChoiceIcon
// (Drawings.tsx) draws them. provisional: D-34

import type { FilterMode, LfoShape, Waveform } from "../backend";

/** One cycle of each oscillator wave. */
export const WAVE_ICONS: Record<Waveform, string> = {
  sine: "M0,5 C3.5,-1.5 7.5,-1.5 11,5 S18.5,11.5 22,5",
  triangle: "M0,10 L11,0 L22,10",
  saw: "M0,10 L11,0 L11,10 L22,0 L22,10",
  pulse: "M0,10 L0,0 L11,0 L11,10 L22,10 L22,0",
};

/** Each filter mode's shape. */
export const MODE_ICONS: Record<FilterMode, string> = {
  low_pass: "M0,2 L12,2 C16,2 17,0 18,4 L22,10",
  band_pass: "M0,10 L7,4 C9,1 13,1 15,4 L22,10",
  high_pass: "M0,10 L4,4 C5,0 6,2 10,2 L22,2",
};

/** One cycle of each LFO shape. */
export const SHAPE_ICONS: Record<LfoShape, string> = {
  sine: "M0,5 C3.5,-1.5 7.5,-1.5 11,5 S18.5,11.5 22,5",
  triangle: "M0,5 L5.5,0 L16.5,10 L22,5",
  saw: "M0,10 L11,0 L11,10 L22,0 L22,10",
  square: "M0,10 L0,0 L11,0 L11,10 L22,10 L22,0",
  sample_and_hold: "M0,6 L5,6 L5,1 L10,1 L10,9 L15,9 L15,3 L22,3",
};
