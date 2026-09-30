// Where a dragged track header lands, kept apart from the component so it
// can be tested without layout.

/**
 * The index the track at `from` moves to when it's dropped at `y`: how many
 * of the other headers have their middle above `y`. `middles` are each
 * header's vertical middle, in order from the top, in the same coordinates
 * as `y`.
 */
export function dropIndex(middles: readonly number[], from: number, y: number): number {
  return middles.filter((middle, index) => index !== from && middle < y).length;
}

/** Formats a pan position: "C" in the middle, then "L 50" or "R 100". */
export function formatPan(pan: number): string {
  const amount = Math.round(Math.abs(pan) * 100);
  if (amount === 0) return "C";
  return `${pan < 0 ? "L" : "R"} ${amount}`;
}
