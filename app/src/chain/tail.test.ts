import { describe, expect, it } from "vitest";
import { type TailSettings, decayFor, endSeconds } from "./tail";

// What the Reverb's drawing works out from its settings: here, the Decay a
// drag of the drawing's end asks for.

/** A new Reverb's settings, as the drawing takes them. */
const NEW: TailSettings = {
  predelaySeconds: 0.01,
  decaySeconds: 2,
  depth: 0.5,
  highCutHz: 20000,
  lowCutHz: null,
  earlySizeSeconds: 0.03,
  diffusion: 0.8,
  earlySend: 0.9,
  lateSize: 0.5,
  bassMult: 1,
  bassXoverHz: 500,
  highMult: 0.5,
  highXoverHz: 4000,
};
const LIMITS: [number, number] = [0.1, 30];

describe("decayFor", () => {
  it("gives back the same Decay for the end where it is, and a longer one further out", () => {
    expect(decayFor(NEW, endSeconds(NEW), LIMITS)).toBeCloseTo(2, 6);
    const longer = decayFor(NEW, endSeconds(NEW) * 2, LIMITS);
    expect(longer).toBeGreaterThan(3.5);
    // Set to it, the tail ends where the pointer was.
    expect(endSeconds({ ...NEW, decaySeconds: longer })).toBeCloseTo(endSeconds(NEW) * 2, 6);
  });

  it("stays within Rust's limits", () => {
    expect(decayFor(NEW, 1000, LIMITS)).toBe(30);
    expect(decayFor(NEW, 0, LIMITS)).toBe(0.1);
  });

  it("leaves Decay as it is when there's no tail to scale: Depth at 0", () => {
    const early = { ...NEW, depth: 0 };
    for (const at of [0.5, 0.9, 1, 1.1, 2].map((share) => endSeconds(early) * share)) {
      expect(decayFor(early, at, LIMITS)).toBe(2);
    }
    const gated = { ...early, earlySizeSeconds: 0.4, earlySend: 1 };
    expect(decayFor(gated, endSeconds(gated) * 1.1, LIMITS)).toBe(2);
  });
});
