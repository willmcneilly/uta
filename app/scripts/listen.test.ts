// @vitest-environment node
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, utimesSync, writeFileSync } from "node:fs";
import { type Server, createServer } from "node:http";
import type { AddressInfo } from "node:net";
import { tmpdir } from "node:os";
import { basename, join } from "node:path";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import {
  type Analysis,
  CANVAS_DIR,
  type Prepared,
  IMAGES,
  TAIL_DB,
  type Tools,
  canvasParts,
  imagePath,
  listSessions,
  listening,
  listeningDir,
  loopRegion,
  matchGains,
  onlyCommands,
  prepare,
  readSession,
  readWav,
  sessionDir,
  shuffle,
  tailSeconds,
  transportOf,
  withGain,
} from "./listen";
import { parseToml } from "./toml";

/**
 * A 32-bit float stereo WAV, as `uta render` writes, of a sine at
 * `amplitude`. With a `tail`, it holds until `tail.from` seconds, then fades
 * at a steady rate in dB to 60 dB down at `tail.sixtyAt` and on below it;
 * `right` scales the right channel, and `rightSixtyAt` gives it its own fade.
 */
function floatWav(
  seconds: number,
  amplitude: number,
  sampleRate = 8000,
  tail?: { from: number; sixtyAt: number; right?: number; rightSixtyAt?: number },
): Buffer {
  const frames = Math.round(seconds * sampleRate);
  const data = Buffer.alloc(frames * 2 * 4);
  for (let i = 0; i < frames; i++) {
    const t = i / sampleRate;
    const fade = (sixtyAt: number) => (tail && t > tail.from ? 10 ** ((-60 * (t - tail.from)) / (sixtyAt - tail.from) / 20) : 1);
    const sine = amplitude * Math.sin((2 * Math.PI * 220 * i) / sampleRate);
    data.writeFloatLE(sine * fade(tail?.sixtyAt ?? 1), i * 8);
    data.writeFloatLE(sine * fade(tail?.rightSixtyAt ?? tail?.sixtyAt ?? 1) * (tail?.right ?? 1), i * 8 + 4);
  }
  const header = Buffer.alloc(44);
  header.write("RIFF", 0, "ascii");
  header.writeUInt32LE(36 + data.length, 4);
  header.write("WAVEfmt ", 8, "ascii");
  header.writeUInt32LE(16, 16);
  header.writeUInt16LE(3, 20);
  header.writeUInt16LE(2, 22);
  header.writeUInt32LE(sampleRate, 24);
  header.writeUInt32LE(sampleRate * 8, 28);
  header.writeUInt16LE(8, 32);
  header.writeUInt16LE(32, 34);
  header.write("data", 36, "ascii");
  header.writeUInt32LE(data.length, 40);
  return Buffer.concat([header, data]);
}

function samples(wav: Buffer): number[] {
  const info = readWav(wav);
  const out: number[] = [];
  for (let at = info.dataOffset; at < info.dataOffset + info.dataLength; at += 4) out.push(wav.readFloatLE(at));
  return out;
}

const commandList = (bpm: number, bars: number) =>
  JSON.stringify({
    project: "p",
    commands: [
      { format: 3, command: { type: "set_tempo", bpm } },
      { format: 3, command: { type: "set_loop_length", bars } },
    ],
  });

/**
 * Stands in for uta: "renders" a sine whose amplitude is the candidate's
 * entry in `levels` (fading to -60 dB at its `sixtyAt`, if it has one, and on
 * for 2 s after), "measures" a loudness in LUFS from it, and
 * "draws" its images: each a file holding its own name, and no harmonics for
 * a candidate with no loudness (so no pitch).
 */
function fakeUta(levels: Record<string, { amplitude: number; lufs: number | null; sixtyAt?: number }>) {
  const calls = { render: [] as string[], analyse: [] as string[], song: [] as { file: string; text: string }[] };
  const tools: Tools = {
    render(_commands, wav) {
      calls.render.push(wav);
      const { amplitude, sixtyAt } = levels[basename(wav, ".wav")];
      writeFileSync(wav, sixtyAt ? floatWav(sixtyAt + 2, amplitude, 8000, { from: 0.25, sixtyAt }) : floatWav(2, amplitude));
    },
    // "Compiles" a song file to a command list with the loop off, as the canvas's parts have it.
    song(file, out) {
      calls.song.push({ file, text: readFileSync(file, "utf8") });
      writeFileSync(out, JSON.stringify({ project: "p", commands: [{ format: 14, command: { type: "set_loop_enabled", enabled: false } }] }));
    },
    analyse(wav, images): Analysis {
      calls.analyse.push(wav);
      const id = wav.split("/").pop()?.replace(".wav", "") ?? "";
      mkdirSync(images, { recursive: true });
      for (const image of IMAGES)
        if (image !== "harmonics" || levels[id].lufs !== null) writeFileSync(join(images, `${id}-${image}.png`), `png of ${id} ${image}`);
      return { file: wav, measurements: { loudness_integrated_lufs: levels[id].lufs, decay_40db_s: 0.21 } };
    },
  };
  return { tools, calls };
}

let root: string;

beforeEach(() => {
  root = mkdtempSync(join(tmpdir(), "uta-listen-"));
});
afterEach(() => rmSync(root, { recursive: true, force: true }));

interface Draft {
  kind?: string;
  ids?: string[];
  extra?: string;
  prediction?: string;
}

/** Writes a session folder with a command list for each candidate. */
function writeSession(name: string, draft: Draft = {}): string {
  const dir = join(root, name);
  mkdirSync(dir, { recursive: true });
  const ids = draft.ids ?? ["zebra-dark", "yak-bright", "xerus-plain"];
  for (const id of ids) writeFileSync(join(dir, `${id}.json`), commandList(120, 1));
  const candidates = ids.map((id) => `[[candidates]]\nid = "${id}"\ncommands = "${id}.json"\n`).join("\n");
  const prediction =
    draft.prediction ??
    `[prediction]\ntext = "The secret-prediction: zebra-dark's tail is shortest."\nchoice = "${ids[0]}"\nmeasurements = ["decay_40db_s"]\n`;
  writeFileSync(
    join(dir, "session.toml"),
    `question = "Which snare sounds most like an 808?"\nkind = "${draft.kind ?? "pick one"}"\n${draft.extra ?? ""}\n${prediction}\n${candidates}`,
  );
  return dir;
}

const LEVELS = {
  "zebra-dark": { amplitude: 0.5, lufs: -20 },
  "yak-bright": { amplitude: 0.25, lufs: -26 },
  "xerus-plain": { amplitude: 0.35, lufs: -23 },
  only: { amplitude: 0.3, lufs: -22 },
};

describe("the session format", () => {
  it("reads a session", () => {
    const session = readSession(writeSession("s"));
    expect(session).toMatchObject({
      name: "s",
      question: "Which snare sounds most like an 808?",
      kind: "pick one",
      matchLoudness: true,
      prediction: { choice: "zebra-dark", measurements: ["decay_40db_s"] },
    });
    expect(session.candidates.map((c) => c.id)).toEqual(["zebra-dark", "yak-bright", "xerus-plain"]);
    expect(session.candidates[0].commands).toBe(join(root, "s", "zebra-dark.json"));
  });

  it("refuses a session without a prediction written first", () => {
    expect(() => readSession(writeSession("s", { prediction: "" }))).toThrow(/needs a \[prediction\]/);
  });

  it("refuses a kind it doesn't know, a pick of one, and a prediction of no candidate", () => {
    expect(() => readSession(writeSession("a", { kind: "vote" }))).toThrow(/`kind` must be one of/);
    expect(() => readSession(writeSession("b", { ids: ["only"] }))).toThrow(/at least two candidates/);
    expect(() => readSession(writeSession("c", { prediction: '[prediction]\ntext = "x"\nchoice = "nobody"' }))).toThrow(
      /`prediction.choice` must be one of/,
    );
  });

  it("lives in ~/.uta/listening, or UTA_LISTENING_DIR", () => {
    expect(listeningDir({})).toMatch(/\.uta\/listening$/);
    expect(listeningDir({ UTA_LISTENING_DIR: "/somewhere/private" })).toBe("/somewhere/private");
  });

  it("finds a session by name in the listening folder, and lists which have a verdict", () => {
    writeSession("2026-10-20-b");
    writeSession("2026-10-19-a");
    writeFileSync(join(root, "2026-10-19-a", "verdict.toml"), "");
    expect(sessionDir("2026-10-20-b", root)).toBe(join(root, "2026-10-20-b"));
    expect(() => sessionDir("missing", root)).toThrow(/No session "missing"/);
    expect(listSessions(root).map(({ name, answered }) => [name, answered])).toEqual([
      ["2026-10-19-a", true],
      ["2026-10-20-b", false],
    ]);
  });
});

describe("matching and lining up", () => {
  it("brings every candidate down to the quietest one's integrated loudness", () => {
    expect(matchGains([-20, -26, -23], true)).toEqual([-6, 0, -3]);
  });

  it("applies no gain when matching is off, or to a silent candidate", () => {
    expect(matchGains([-20, -26, -23], false)).toEqual([0, 0, 0]);
    expect(matchGains([-20, null, -23], true)).toEqual([-3, 0, 0]);
  });

  it("loops the second time round the song's loop, the same stretch for every candidate", () => {
    const transport = transportOf(JSON.parse(commandList(120, 2)));
    expect(transport).toEqual({ bpm: 120, loopEnabled: true, loopStart: 0, loopLength: 4 });
    expect(loopRegion({}, [transport, transport], [8.2, 8.3], [8.2, 8.3])).toEqual({ start: 4, end: 8, bpm: 120 });
    expect(loopRegion({}, [transport], [6], [6])).toEqual({ start: 0, end: 4, bpm: 120 });
    expect(loopRegion({ loop: [1, 2] }, [transport], [6], [6])).toEqual({ start: 1, end: 2, bpm: 120 });
  });

  it("reads the loop from set_loop and set_loop_enabled, and loops the whole render with the loop off", () => {
    const list = (...commands: Record<string, unknown>[]) => ({ commands: commands.map((command) => ({ format: 3, command })) });
    const moved = transportOf(list({ type: "set_tempo", bpm: 120 }, { type: "set_loop", start_bar: 1, bars: 2 }));
    expect(moved).toEqual({ bpm: 120, loopEnabled: true, loopStart: 2, loopLength: 4 });
    // Into the loop at 2 s, then the second time round it, from 6 s.
    expect(loopRegion({}, [moved], [10.2], [10.2])).toEqual({ start: 6, end: 10, bpm: 120 });
    const off = transportOf(list({ type: "set_loop_enabled", enabled: false }));
    expect(off).toMatchObject({ loopEnabled: false });
    // With the loop off, until the last tail is below -60 dB, however long the renders ring on after it.
    expect(loopRegion({}, [off, off], [7.5, 7.25], [2.5, 3.1])).toEqual({ start: 0, end: 3.1, bpm: 120 });
    expect(loopRegion({}, [off, off], [7.5, 7.25], [0, 0]), "all silent").toEqual({ start: 0, end: 7.25, bpm: 120 });
    // A new project's defaults: 120 BPM, a 4-bar loop from bar 1.
    expect(transportOf(list())).toEqual({ bpm: 120, loopEnabled: true, loopStart: 0, loopLength: 8 });
  });

  it("refuses candidates whose loops differ, unless the session says what to loop", () => {
    const a = transportOf(JSON.parse(commandList(120, 2)));
    const b = transportOf(JSON.parse(commandList(100, 2)));
    expect(() => loopRegion({}, [a, b], [9, 9], [9, 9])).toThrow(/tempo or loop differ/);
    expect(loopRegion({ loop: [0, 3] }, [a, b], [9, 9], [9, 9])).toEqual({ start: 0, end: 3, bpm: null });
  });

  it("measures where a render's tail falls below -60 dB of its own peak, at any level", () => {
    // A held tone, fading 60 dB over 1.5 s from 0.5 s, in a render that rings on to 5 s.
    for (const amplitude of [0.9, 0.05])
      expect(tailSeconds(floatWav(5, amplitude, 8000, { from: 0.5, sixtyAt: 2 })), `at ${amplitude}`).toBeCloseTo(2, 2);
    expect(tailSeconds(floatWav(5, 0.5, 8000, { from: 0, sixtyAt: 3.2 }))).toBeCloseTo(3.2, 2);
    // -40 dB is reached two thirds of the way down.
    expect(tailSeconds(floatWav(5, 0.5, 8000, { from: 0.5, sixtyAt: 2 }), -40)).toBeCloseTo(1.5, 2);
    // The louder channel decides it: on its own, the right (20 dB down) would be under -60 dB of the peak a third sooner.
    expect(tailSeconds(floatWav(5, 0.5, 8000, { from: 0.5, sixtyAt: 2, right: 0.1 }))).toBeCloseTo(2, 2);
    // Either channel can ring on longest, and the peak is the louder channel's.
    expect(tailSeconds(floatWav(5, 0.5, 8000, { from: 0.5, sixtyAt: 1, rightSixtyAt: 2.5 }))).toBeCloseTo(2.5, 2);
    // The right is 20 dB louder: measured against the left's peak, it would ring on to 2.5 s.
    expect(tailSeconds(floatWav(5, 0.05, 8000, { from: 0.5, sixtyAt: 2, right: 10 }))).toBeCloseTo(2, 2);
    expect(tailSeconds(floatWav(5, 0, 8000))).toBe(0);
    expect(TAIL_DB).toBe(-60);
  });

  it("pads a render with silence to the loop's length, and leaves one that's long enough alone", () => {
    const wav = floatWav(1, 0.5, 8000);
    const padded = withGain(wav, -6, 8000 * 2.5);
    const info = readWav(padded);
    expect(info.frames).toBe(8000 * 2.5);
    expect(padded.readUInt32LE(4)).toBe(padded.length - 8);
    const sent = samples(padded);
    const rendered = samples(wav);
    rendered.forEach((sample, i) => expect(sent[i]).toBeCloseTo(sample * 10 ** (-6 / 20), 6));
    expect(sent.slice(rendered.length).every((sample) => sample === 0)).toBe(true);
    expect(withGain(wav, 0, 4000).equals(wav)).toBe(true);
  });

  it("shuffles", () => {
    expect(shuffle([1, 2, 3], () => 0)).toEqual([2, 3, 1]);
  });
});

describe("preparing a session", () => {
  it("renders every candidate again when session.toml changes, since it can change how long they render", () => {
    const dir = writeSession("s");
    const { tools, calls } = fakeUta(LEVELS);
    prepare(readSession(dir), tools);
    const later = new Date(Date.now() + 5000);
    utimesSync(join(dir, "session.toml"), later, later);
    prepare(readSession(dir), tools);
    expect(calls.render).toHaveLength(6);
  });

  it("renders and measures each candidate once, and again when its command list changes", () => {
    const dir = writeSession("s");
    const { tools, calls } = fakeUta(LEVELS);
    const prepared = prepare(readSession(dir), tools);
    expect(calls.render).toHaveLength(3);
    expect(prepared.candidates.map((c) => c.gainDb)).toEqual([-6, 0, -3]);
    expect(prepared.loop).toEqual({ start: 0, end: 2, bpm: 120 });

    prepare(readSession(dir), tools);
    expect(calls.render).toHaveLength(3);
    expect(calls.analyse).toHaveLength(3);

    const later = new Date(Date.now() + 5000);
    writeFileSync(join(dir, "yak-bright.json"), commandList(120, 1));
    utimesSync(join(dir, "yak-bright.json"), later, later);
    prepare(readSession(dir), tools);
    expect(calls.render).toEqual([...calls.render.slice(0, 3), join(dir, "yak-bright.wav")]);
  });

  it("draws each candidate's images beside its render, and draws them again if one is missing", () => {
    const dir = writeSession("s", { ids: ["zebra-dark", "only"] });
    const { tools, calls } = fakeUta({ ...LEVELS, only: { amplitude: 0.3, lufs: null } });
    const prepared = prepare(readSession(dir), tools);
    expect(prepared.candidates[0].images).toEqual(["spectrogram-long", "level", "spectrogram-short", "clicks", "harmonics"]);
    expect(prepared.candidates[1].images).toEqual(["spectrogram-long", "level", "spectrogram-short", "clicks"]);
    expect(readFileSync(imagePath(dir, "zebra-dark", "level"), "utf8")).toBe("png of zebra-dark level");

    for (const image of ["spectrogram-long", "level", "spectrogram-short", "clicks"] as const) {
      rmSync(imagePath(dir, "only", image));
      prepare(readSession(dir), tools);
      expect(calls.analyse.at(-1), image).toBe(join(dir, "only.wav"));
    }
    expect(calls.analyse).toHaveLength(6);
  });

  it("leaves out an image older than its render: one the render no longer has", () => {
    // zebra-dark is drawn with harmonics, then re-rendered with no pitch: its old harmonics image stays on disk.
    const dir = writeSession("s", { ids: ["zebra-dark", "only"] });
    const levels = { ...LEVELS, only: { amplitude: 0.3, lufs: -22 } };
    prepare(readSession(dir), fakeUta(levels).tools);
    const earlier = new Date(Date.now() - 60_000);
    utimesSync(imagePath(dir, "zebra-dark", "harmonics"), earlier, earlier);
    const later = new Date(Date.now() + 5000);
    utimesSync(join(dir, "zebra-dark.json"), later, later);
    const prepared = prepare(readSession(dir), fakeUta({ ...levels, "zebra-dark": { amplitude: 0.5, lufs: null } }).tools);
    expect(existsSync(imagePath(dir, "zebra-dark", "harmonics"))).toBe(true);
    expect(prepared.candidates[0].images).toEqual(["spectrogram-long", "level", "spectrogram-short", "clicks"]);
    expect(prepared.candidates[1].images).toContain("harmonics");
  });
});

describe("a canvas session", () => {
  const CANVAS = `name = "Canvas: test"\ntempo = 120\nloop = false\n\n[[track]]\nname = "Part"\nkind = "synth"\npattern.a = "C4"\n\n[[section]]\nname = "a"\nbars = 1\n`;
  const ECHO = `# An echo on the part.\n[[command]]\ntype = "add_effects"\neffects = [{ chain = { track = "$track:Part" }, index = 0, effect = { id = "9b2f6c1e-0000-4000-8000-000000000001", settings = { delay = { sync = true, division = "1/8d", time_seconds = 0.375, mode = "stereo", feedback = 0.4, low_cut_hz = 100.0, high_cut_hz = 8000.0, drive = 0.2, wobble = 0.1 } }, mix = 0.3 } }]\n`;
  const SOUND = `[[command]]\ntype = "set_synth_settings"\ntrack = "$track:Part"\nsettings = { cutoff_hz = 4000 }\n`;

  /** A session on a canvas of its own, canvas.toml beside it: "dry" is the canvas as it is, "echo" and "bright" add commands. */
  function writeCanvasSession(name: string, extra = "") {
    const dir = join(root, name);
    mkdirSync(dir, { recursive: true });
    writeFileSync(join(dir, "canvas.toml"), CANVAS);
    writeFileSync(join(dir, "echo.toml"), ECHO);
    writeFileSync(join(dir, "bright.toml"), SOUND);
    writeFileSync(
      join(dir, "session.toml"),
      `question = "Which echo?"\nkind = "rate each"\ncanvas = "canvas.toml"\n${extra}\n[prediction]\ntext = "x"\n\n` +
        `[[candidates]]\nid = "dry"\n\n[[candidates]]\nid = "echo"\ncommands = "echo.toml"\n\n[[candidates]]\nid = "bright"\ncommands = "bright.toml"\n`,
    );
    return dir;
  }
  const TAILS = { dry: { amplitude: 0.5, lufs: -20, sixtyAt: 0.8 }, echo: { amplitude: 0.5, lufs: -21, sixtyAt: 3.4 }, bright: { amplitude: 0.5, lufs: -19, sixtyAt: 1.1 } };

  it("has the four parts in examples/listen/canvas, and a session names one by its name", () => {
    expect(canvasParts()).toEqual(["chord", "pad", "snare", "swell"]);
    const dir = writeCanvasSession("s");
    writeFileSync(join(dir, "session.toml"), readFileSync(join(dir, "session.toml"), "utf8").replace('canvas = "canvas.toml"', 'canvas = "chord"'));
    expect(readSession(dir).canvas).toBe(join(CANVAS_DIR, "chord.toml"));
  });

  it("refuses a canvas that isn't a part or a song file, saying what the parts are", () => {
    const dir = writeCanvasSession("s");
    writeFileSync(join(dir, "session.toml"), readFileSync(join(dir, "session.toml"), "utf8").replace('canvas = "canvas.toml"', 'canvas = "drums"'));
    expect(() => readSession(dir)).toThrow(/"drums" isn't a part of the canvas \(chord, pad, snare, swell\)/);
  });

  it("takes a candidate with no commands as the canvas as it is, but only on a canvas", () => {
    expect(readSession(writeCanvasSession("s")).candidates).toEqual([
      { id: "dry" },
      { id: "echo", commands: join(root, "s", "echo.toml") },
      { id: "bright", commands: join(root, "s", "bright.toml") },
    ]);
    const plain = writeSession("plain");
    writeFileSync(join(plain, "session.toml"), readFileSync(join(plain, "session.toml"), "utf8").replace('commands = "zebra-dark.json"\n', ""));
    expect(() => readSession(plain)).toThrow(/candidate "zebra-dark"'s `commands` must be some text/);
  });

  it("refuses commands that would land in the canvas's song instead of adding onto it", () => {
    expect(onlyCommands(ECHO)).toBeNull();
    expect(onlyCommands(`[[command]]\ntype = "set_tempo"\nbpm = 90\n[command.extra]\nx = 1\n`)).toBeNull();
    expect(onlyCommands(`tempo = 90\n[[command]]\ntype = "set_tempo"\n`)).toMatch(/must start with a \[\[command\]\] table/);
    expect(onlyCommands(`[[command]]\ntype = "set_tempo"\n[[track]]\nname = "More"\n`)).toMatch(/not \[track\]/);
    expect(onlyCommands("# Nothing here.\n")).toMatch(/no \[\[command\]\] tables/);
    const dir = writeCanvasSession("s");
    writeFileSync(join(dir, "echo.toml"), `bars = 4\n${ECHO}`);
    expect(() => readSession(dir)).toThrow(/candidate "echo"'s echo.toml: it must start with a \[\[command\]\] table/);
  });

  it("renders each candidate through the same canvas, with its commands added onto the end", () => {
    const dir = writeCanvasSession("s");
    const { tools, calls } = fakeUta(TAILS);
    prepare(readSession(dir), tools);
    expect(calls.song.map((c) => c.file)).toEqual(["dry", "echo", "bright"].map((id) => join(dir, `${id}.canvas.toml`)));
    const [dry, echo, bright] = calls.song.map((c) => c.text);
    expect(dry).toBe(CANVAS);
    for (const [text, commands] of [
      [echo, ECHO],
      [bright, SOUND],
    ]) {
      expect(text.startsWith(CANVAS.trimEnd())).toBe(true);
      expect(text.endsWith(commands)).toBe(true);
    }
    expect(calls.render).toEqual(["dry", "echo", "bright"].map((id) => join(dir, `${id}.wav`)));
  });

  it("renders again only what changed: a candidate's commands, or every candidate when the canvas changes", () => {
    const dir = writeCanvasSession("s");
    const { tools, calls } = fakeUta(TAILS);
    prepare(readSession(dir), tools);
    prepare(readSession(dir), tools);
    expect(calls.render).toHaveLength(3);
    // A compiled command list gone missing is compiled and rendered again.
    rmSync(join(dir, "bright.canvas.json"));
    prepare(readSession(dir), tools);
    expect(calls.render.slice(3)).toEqual([join(dir, "bright.wav")]);
    expect(existsSync(join(dir, "bright.canvas.json"))).toBe(true);
    const later = new Date(Date.now() + 5000);
    utimesSync(join(dir, "echo.toml"), later, later);
    prepare(readSession(dir), tools);
    expect(calls.render.slice(4)).toEqual([join(dir, "echo.wav")]);
    const evenLater = new Date(Date.now() + 10_000);
    utimesSync(join(dir, "canvas.toml"), evenLater, evenLater);
    prepare(readSession(dir), tools);
    expect(calls.render.slice(5)).toEqual(["dry", "echo", "bright"].map((id) => join(dir, `${id}.wav`)));
  });

  it("loops from the start until the last candidate's tail is below -60 dB, not the silence the renders ring on into", () => {
    const prepared = prepare(readSession(writeCanvasSession("s")), fakeUta(TAILS).tools);
    // Each render rings on 2 s past its own -60 dB point.
    expect(prepared.candidates.map((c) => c.seconds)).toEqual([2.8, 5.4, 3.1]);
    expect(prepared.candidates.map((c) => c.tailSeconds.toFixed(2))).toEqual(["0.80", "3.40", "1.10"]);
    expect(prepared.loop.start).toBe(0);
    expect(prepared.loop.end).toBeCloseTo(3.4, 2);
  });
});

describe("the listening page's server", () => {
  let server: Server;
  let base: string;
  let done: Promise<string[]>;

  async function serve(prepared: Prepared[], random?: (n: number) => number) {
    const listen = listening(prepared, { random, now: () => new Date("2026-10-20T10:12:03Z") });
    done = listen.done;
    server = createServer(listen.handle);
    await new Promise<void>((resolve) => server.listen(0, "127.0.0.1", resolve));
    base = `http://127.0.0.1:${(server.address() as AddressInfo).port}`;
  }

  afterEach(async () => {
    server?.closeAllConnections();
    await new Promise((resolve) => server?.close(resolve));
  });

  const prepared = (name: string, draft: Draft = {}) => prepare(readSession(writeSession(name, draft)), fakeUta(LEVELS).tools);
  /** Presses Finish on question 1 (index 0), unless the answer says which. */
  const finish = (body: Record<string, unknown>, headers: Record<string, string> = {}) =>
    fetch(`${base}/api/finish`, {
      method: "POST",
      headers: { "Content-Type": "application/json", ...headers },
      body: JSON.stringify({ question: 0, ...body }),
    });

  /** Everything the page can fetch before Finish, by path: its status, headers and body, as text. */
  async function everythingSent(): Promise<Record<string, string>> {
    const paths = ["/", "/page.js", "/tokens.css", "/api/session", "/audio/A", "/audio/B", "/audio/C", "/audio/D"];
    const parts = await Promise.all(
      paths.map(async (path) => {
        const response = await fetch(base + path);
        const body = Buffer.from(await response.arrayBuffer()).toString("latin1");
        return [path, `${response.status} ${JSON.stringify([...response.headers])}\n${body}`];
      }),
    );
    return Object.fromEntries(parts);
  }

  it("sends each label's audio with the gain its measured loudness asks for", async () => {
    // With this random, the order is the session's: A is zebra-dark, B yak-bright, C xerus-plain.
    await serve([prepared("s")], (n) => n - 1);
    for (const [label, id, gainDb] of [
      ["A", "zebra-dark", -6],
      ["B", "yak-bright", 0],
      ["C", "xerus-plain", -3],
    ] as const) {
      const sent = samples(Buffer.from(await (await fetch(`${base}/audio/${label}`)).arrayBuffer()));
      const rendered = samples(readFileSync(join(root, "s", `${id}.wav`)));
      const scale = 10 ** (gainDb / 20);
      expect(sent.length).toBe(rendered.length);
      sent.forEach((sample, i) => expect(sample).toBeCloseTo(rendered[i] * scale, 6));
    }
  });

  it("pads each candidate's audio with silence to the shared loop's end", async () => {
    const dir = join(root, "c");
    mkdirSync(dir);
    writeFileSync(join(dir, "canvas.toml"), "# A canvas.\n");
    for (const id of ["short", "long"]) writeFileSync(join(dir, `${id}.toml`), `[[command]]\ntype = "set_tempo"\nbpm = 120\n`);
    writeFileSync(
      join(dir, "session.toml"),
      `question = "Which?"\nkind = "pick one"\ncanvas = "canvas.toml"\n[prediction]\ntext = "x"\n[[candidates]]\nid = "short"\ncommands = "short.toml"\n[[candidates]]\nid = "long"\ncommands = "long.toml"\n`,
    );
    const levels = { short: { amplitude: 0.5, lufs: -20, sixtyAt: 0.5 }, long: { amplitude: 0.5, lufs: -20, sixtyAt: 3 } };
    const p = prepare(readSession(dir), fakeUta(levels).tools);
    // short renders 2.5 s; the loop runs to long's tail, at 3 s.
    expect(p.candidates[0].seconds).toBe(2.5);
    await serve([p], (n) => n - 1);
    const session = await (await fetch(`${base}/api/session`)).json();
    expect(session.loop.end).toBeCloseTo(3, 2);
    for (const label of ["A", "B"]) {
      const info = readWav(Buffer.from(await (await fetch(`${base}/audio/${label}`)).arrayBuffer()));
      expect(info.frames / info.sampleRate, label).toBeGreaterThanOrEqual(session.loop.end);
    }
  });

  it("applies no gain when the session turns matching off", async () => {
    await serve([prepared("s", { extra: "match_loudness = false" })], (n) => n - 1);
    const session = await (await fetch(`${base}/api/session`)).json();
    expect(session.loudnessMatched).toBe(false);
    const sent = Buffer.from(await (await fetch(`${base}/audio/A`)).arrayBuffer());
    expect(sent.equals(readFileSync(join(root, "s", "zebra-dark.wav")))).toBe(true);
    await finish({ choice: "A", note: "" });
    const verdict = parseToml(readFileSync(join(root, "s", "verdict.toml"), "utf8"));
    expect(verdict.gain_db).toEqual({ "zebra-dark": 0, "yak-bright": 0, "xerus-plain": 0 });
    expect(verdict.loudness_matched).toBe(false);
  });

  it("shuffles the labels", async () => {
    const first = new Set<string>();
    for (let run = 0; run < 60; run++) {
      const listen = listening([prepared("s")]);
      // Which candidate got A is in the verdict, so finish each run and read it.
      const verdict = await answerWith(listen, { choice: "A", note: "" });
      first.add((verdict.labels as Record<string, string>).A);
    }
    // 60 runs leave a candidate never first with odds of about 1 in 10^10.
    expect(first).toEqual(new Set(["zebra-dark", "yak-bright", "xerus-plain"]));
  });

  it("keeps which candidate is which out of everything the page receives before Finish", async () => {
    await serve([prepared("s")]);
    const sent = await everythingSent();
    for (const [path, text] of Object.entries(sent))
      for (const secret of ["zebra-dark", "yak-bright", "xerus-plain", "secret-prediction", "decay_40db_s", root])
        expect(text.includes(secret), `${path} has "${secret}"`).toBe(false);
    const session = await (await fetch(`${base}/api/session`)).json();
    expect(session).toEqual({
      index: 0,
      total: 1,
      question: "Which snare sounds most like an 808?",
      kind: "pick one",
      labels: ["A", "B", "C"],
      loop: { start: 0, end: 2, bpm: 120 },
      loudnessMatched: true,
      ratingRange: [1, 5],
    });

    const reveal = await (await finish({ choice: "B", note: "A is too ringy." })).json();
    expect(JSON.stringify(reveal)).toContain("zebra-dark");
    expect(reveal.prediction.text).toContain("secret-prediction");
    expect(reveal.candidates.map((c: { label: string }) => c.label)).toEqual(["A", "B", "C"]);
    expect(reveal.candidates[0].measurements).toEqual({ loudness_integrated_lufs: expect.any(Number), decay_40db_s: 0.21 });
  });

  it("sends each candidate's images with the reveal, and nothing before", async () => {
    // A is zebra-dark, B yak-bright, C xerus-plain.
    await serve([prepared("s")], (n) => n - 1);
    const before = JSON.stringify(await everythingSent());
    expect(before).not.toContain("png of");
    expect(before).not.toContain(Buffer.from("png of").toString("base64").slice(0, 6));

    const reveal = await (await finish({ choice: "A", note: "" })).json();
    const b = reveal.candidates[1];
    expect(b.images.map((image: { name: string }) => image.name)).toEqual([...IMAGES]);
    for (const { name, src } of b.images) {
      const [type, data] = src.split(",");
      expect(type).toBe("data:image/png;base64");
      expect(Buffer.from(data, "base64").toString()).toBe(`png of yak-bright ${name}`);
    }
  });

  it("writes verdict.toml with every field when Finish is pressed, and stops", async () => {
    await serve([prepared("s")], (n) => n - 1);
    const response = await finish({ choice: "A", note: "B is too ringy." });
    expect(response.status).toBe(200);
    expect(await done).toEqual([join(root, "s", "verdict.toml")]);
    expect(parseToml(readFileSync(join(root, "s", "verdict.toml"), "utf8"))).toEqual({
      session: "s",
      question: "Which snare sounds most like an 808?",
      kind: "pick one",
      choice: "zebra-dark",
      note: "B is too ringy.",
      prediction_matched: true,
      loudness_matched: true,
      finished_at: "2026-10-20T10:12:03.000Z",
      gain_db: { "zebra-dark": -6, "yak-bright": 0, "xerus-plain": -3 },
      labels: { A: "zebra-dark", B: "yak-bright", C: "xerus-plain" },
    });
    expect((await fetch(`${base}/api/session`)).status).toBe(410);
  });

  it("writes the ratings for rate each, and the note for listen and describe", async () => {
    await serve(
      [prepared("rate", { kind: "rate each", prediction: '[prediction]\ntext = "x"' }), prepared("describe", { kind: "listen and describe", ids: ["only"], prediction: '[prediction]\ntext = "x"' })],
      (n) => n - 1,
    );
    expect((await finish({ ratings: { A: 4, B: 2 }, note: "" })).status).toBe(400);
    expect((await finish({ ratings: { A: 4, B: 2, C: 9 }, note: "" })).status).toBe(400);
    expect((await finish({ ratings: { A: 4, B: 2, C: 5 }, note: "" })).status).toBe(200);
    expect(parseToml(readFileSync(join(root, "rate", "verdict.toml"), "utf8"))).toMatchObject({
      kind: "rate each",
      ratings: { "zebra-dark": 4, "yak-bright": 2, "xerus-plain": 5 },
    });

    // The next question in the queue.
    expect(await (await fetch(`${base}/api/session`)).json()).toMatchObject({ index: 1, total: 2, kind: "listen and describe", labels: ["A"] });
    expect((await finish({ question: 1, note: "  " })).status).toBe(400);
    expect((await finish({ question: 1, note: "The tail clicks." })).status).toBe(200);
    expect(parseToml(readFileSync(join(root, "describe", "verdict.toml"), "utf8"))).toMatchObject({
      kind: "listen and describe",
      note: "The tail clicks.",
      labels: { A: "only" },
    });
    expect(await done).toEqual([join(root, "rate", "verdict.toml"), join(root, "describe", "verdict.toml")]);
  });

  it("refuses a second Finish for a question, so a stale page can't answer the next one", async () => {
    await serve([prepared("first"), prepared("second")], (n) => n - 1);
    const [one, two] = await Promise.all([finish({ choice: "A", note: "one" }), finish({ choice: "B", note: "two" })]);
    expect([one.status, two.status].sort()).toEqual([200, 409]);
    expect(existsSync(join(root, "second", "verdict.toml"))).toBe(false);
    expect((await finish({ choice: "A", note: "late" })).status).toBe(409);
    expect((await finish({ question: 1, choice: "A", note: "" })).status).toBe(200);
  });

  it("refuses an answer without a pick, or from another page", async () => {
    await serve([prepared("s")]);
    expect((await finish({ note: "" })).status).toBe(400);
    expect((await finish({ choice: "Z", note: "" })).status).toBe(400);
    expect((await finish({ choice: "A" }, { Origin: "http://elsewhere.test" })).status).toBe(403);
  });
});

/** Finishes a session straight through the handler, as the page would, and reads its verdict. */
async function answerWith(listen: ReturnType<typeof listening>, answer: unknown) {
  const server = createServer(listen.handle);
  await new Promise<void>((resolve) => server.listen(0, "127.0.0.1", resolve));
  const base = `http://127.0.0.1:${(server.address() as AddressInfo).port}`;
  await fetch(`${base}/api/finish`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ question: 0, ...(answer as object) }),
  });
  const [file] = await listen.done;
  server.closeAllConnections();
  await new Promise((resolve) => server.close(resolve));
  return parseToml(readFileSync(file, "utf8"));
}
