// Reads a number someone typed into a drag field (RFC-005, part 7, open
// question 11): "120", "120 bpm", "1k", "250 ms", "−6".

/**
 * The units a field understands, lower case, each with what one of it is in
 * the field's own unit: a field in seconds has `{ s: 1, ms: 0.001 }`.
 */
export type Units = Record<string, number>;

const TYPED = /^([+-]?(?:\d+\.?\d*|\.\d+))(k?)([a-z]*)$/;

/**
 * The value `text` gives, in the field's own unit, or null if it can't be
 * read. Space and case don't matter, the minus can be the typographic one
 * (U+2212), a "k" after the number multiplies it by a thousand, and the unit
 * can be left off.
 */
export function parseTyped(text: string, units: Units): number | null {
  const typed = text.replace(/\s+/g, "").replace(/−/g, "-").toLowerCase();
  const match = TYPED.exec(typed);
  if (!match) return null;
  const [, number, kilo, unit] = match;
  let scale = 1;
  if (unit) {
    // "1kHz" is a kilo of a unit, unless the field knows "khz" itself.
    const known = units[kilo + unit] ?? (kilo ? undefined : units[unit]);
    if (known !== undefined) return Number(number) * known;
    if (!kilo || units[unit] === undefined) return null;
    scale = units[unit];
  }
  return Number(number) * (kilo ? 1000 : 1) * scale;
}
