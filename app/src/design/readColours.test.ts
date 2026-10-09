/* eslint-disable uta/no-raw-colour -- the colours here are test data */
import { afterEach, describe, expect, it } from "vitest";
import { readColours } from "./readColours";
import { colors } from "./tokens";

describe("readColours", () => {
  afterEach(() => document.documentElement.removeAttribute("style"));

  it("reads each colour from its CSS variable", () => {
    document.documentElement.style.setProperty("--ink-2", "#123456");
    expect(readColours(document.body).ink2).toBe("#123456");
  });

  it("falls back to the light value without the stylesheet", () => {
    const colours = readColours(document.body);
    expect(colours.live).toBe(colors.light.live);
    expect(Object.keys(colours)).toEqual(Object.keys(colors.light));
  });
});
