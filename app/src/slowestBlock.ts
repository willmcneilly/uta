// How the slowest block readout shows the audio thread's load.

/** How often the slowest block readout updates. */
export const SLOWEST_BLOCK_REFRESH_MS = 500;

/** `share` of the deadline as a percentage, with a decimal place below 10%. */
export function percent(share: number): string {
  const value = share * 100;
  return `${value < 10 ? value.toFixed(1) : Math.round(value)}%`;
}
