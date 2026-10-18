import { describe, expect, it } from "vitest";
import { parseToml, stringifyToml } from "./toml";

describe("parseToml", () => {
  it("reads the values a session uses", () => {
    const toml = parseToml(`
# A comment
question = "Which is darker? # not a comment"
kind = 'pick one'
match_loudness = false
seconds = 8.5
count = 1_000
loop = [2.14, 4.28]
names = [
  "a", # one
  "b",
]

[prediction]
text = """
Line one,
and "two"."""
measurements = ["decay_40db_s"]

[prediction.ratings]
short = 4

[[candidates]]
id = "short"
commands = "short.json"

[[candidates]]
id = "long"
commands = "long.json"
`);
    expect(toml).toEqual({
      question: "Which is darker? # not a comment",
      kind: "pick one",
      match_loudness: false,
      seconds: 8.5,
      count: 1000,
      loop: [2.14, 4.28],
      names: ["a", "b"],
      prediction: { text: 'Line one,\nand "two".', measurements: ["decay_40db_s"], ratings: { short: 4 } },
      candidates: [
        { id: "short", commands: "short.json" },
        { id: "long", commands: "long.json" },
      ],
    });
  });

  it("reads escapes", () => {
    expect(parseToml(String.raw`a = "tab\there \"q\" \u00e9"`)).toEqual({ a: 'tab\there "q" é' });
    expect(parseToml('a = """one \\\n    two"""')).toEqual({ a: "one two" });
    expect(parseToml(String.raw`a = 'C:\no\escapes'`)).toEqual({ a: String.raw`C:\no\escapes` });
  });

  it("says which line is wrong", () => {
    expect(() => parseToml('a = 1\nb = bare')).toThrow(/line 2: `bare` isn't a value/);
    expect(() => parseToml("a = 1\na = 2")).toThrow(/line 2: `a` is set twice/);
    expect(() => parseToml('x = "open')).toThrow(/line 1: a string never closes/);
    expect(() => parseToml("t = { a = 1 }")).toThrow(/inline tables/);
    expect(() => parseToml("just words")).toThrow(/line 1: expected `key = value`/);
  });
});

describe("stringifyToml", () => {
  it("writes what parseToml reads back", () => {
    const table = {
      session: "2026-10-20-snare",
      note: 'Line one\nline "two" \\ back',
      choice: "short",
      prediction_matched: true,
      gain_db: { short: -1.25, "odd key": 0 },
      labels: { A: "long", B: "short" },
      list: [1, "two", false],
      candidates: [{ id: "a" }, { id: "b" }],
    };
    expect(parseToml(stringifyToml(table))).toEqual(table);
  });

  it("puts values before tables, in the order given", () => {
    expect(stringifyToml({ a: 1, t: { b: 2 }, c: "x" })).toBe('a = 1\nc = "x"\n\n[t]\nb = 2\n');
  });
});
