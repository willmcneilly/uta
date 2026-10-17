import { describe, expect, it } from "vitest";
import type { Descriptions, SourceOutline } from "./backend";
import { delayView } from "./chain/delayTesting";
import { describeEffect, describeSource } from "./describe";
import { KICK_909_SETTINGS, KICK_MODEL, drumTrackView, outlineOf, trackView } from "./pianoRoll/testing";

// Rust sends each setting's description once, and each track only its
// settings' values (RFC-004, part 2, "Growing later"); the panels draw from
// the two put back together.

describe("describeSource and describeEffect", () => {
  it("put each synth, drum and effect setting back together with its description", () => {
    const synth = trackView("t1", "Synth 1");
    const drums = drumTrackView("t2", "Drums 1");
    const delay = delayView("e1", { mix: 0.5 });
    const project = { tracks: [synth, drums], masterEffects: [delay] };
    const { descriptions, tracks, masterEffects } = outlineOf(project);

    // What the outline carries is only names and values.
    for (const track of tracks) {
      const settings =
        track.source.kind === "synth"
          ? track.source.settings
          : track.source.kit.rows.flatMap((row) => [...row.settings, ...(row.model ? [row.model] : [])]);
      for (const setting of settings) expect(Object.keys(setting)).toEqual(["name", "value"]);
    }
    expect(masterEffects[0].mix).toEqual({ name: "mix", value: 0.5 });

    expect(describeSource(tracks[0].source, descriptions)).toEqual(synth.source);
    expect(describeSource(tracks[1].source, descriptions)).toEqual(drums.source);
    expect(describeEffect(masterEffects[0], descriptions)).toEqual(delay);
  });

  it("describes the kick from its model's table", () => {
    const descriptions: Descriptions = {
      synth: {},
      drums: {
        "808": { kick: { tune_hz: { ...KICK_909_SETTINGS[0], label: "808 Tune" }, model: KICK_MODEL } },
        "909": { kick: { tune_hz: KICK_909_SETTINGS[0], model: KICK_MODEL } },
      },
      effects: { delay: {} },
    };
    const kit = (model: "808" | "909"): SourceOutline => ({
      kind: "drums",
      kit: {
        rows: [
          {
            sound: "kick",
            name: "Kick",
            pitch: 36,
            settings: [{ name: "tune_hz", value: 55 }],
            shares: null,
            model: { name: "model", value: model },
          },
        ],
      },
    });
    const tune = (model: "808" | "909") => {
      const source = describeSource(kit(model), descriptions);
      if (source.kind !== "drums") throw new Error("a kit");
      return source.kit.rows[0].settings[0];
    };
    expect(tune("909")).toEqual({ ...KICK_909_SETTINGS[0], value: 55 });
    expect(tune("808").label).toBe("808 Tune");
  });

  it("says so when Rust sends a setting it didn't describe", () => {
    const { descriptions } = outlineOf({ tracks: [], masterEffects: [] });
    expect(() =>
      describeSource({ kind: "synth", settings: [{ name: "cutoff_hz", value: 800 }] }, descriptions),
    ).toThrow("Rust sent cutoff_hz without describing it");
  });
});
