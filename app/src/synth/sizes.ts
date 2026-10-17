// The synth panel's drawings' sizes.

/**
 * Each drawing's size in CSS pixels, without its labels, fitted to its page
 * of the panel, and the tabs' small ones. provisional: D-33
 */
export const SIZES = {
  oscillator: { width: 96, height: 30 },
  filter: { width: 200, height: 72 },
  envelope: { width: 150, height: 44 },
  lfo: { width: 180, height: 60 },
  tab: { width: 44, height: 18 },
} as const;
