// @vitest-environment node
import { mkdirSync, mkdtempSync, readFileSync, rmSync, utimesSync, writeFileSync } from "node:fs";
import { type Server, createServer } from "node:http";
import type { AddressInfo } from "node:net";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import {
  type Analysis,
  type Prepared,
  type Tools,
  listSessions,
  listening,
  listeningDir,
  loopRegion,
  matchGains,
  prepare,
  readSession,
  readWav,
  sessionDir,
  shuffle,
  transportOf,
} from "./listen";
import { parseToml } from "./toml";

/** A 32-bit float stereo WAV, as `uta render` writes, of a sine at `amplitude`. */
function floatWav(seconds: number, amplitude: number, sampleRate = 8000): Buffer {
  const frames = Math.round(seconds * sampleRate);
  const data = Buffer.alloc(frames * 2 * 4);
  for (let i = 0; i < frames; i++) {
    const sample = amplitude * Math.sin((2 * Math.PI * 220 * i) / sampleRate);
    data.writeFloatLE(sample, i * 8);
    data.writeFloatLE(sample, i * 8 + 4);
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
 * Stands in for uta: "renders" a sine whose amplitude is the command list's
 * file name's entry in `levels`, and "measures" a loudness in LUFS from it.
 */
function fakeUta(levels: Record<string, { amplitude: number; lufs: number | null }>) {
  const calls = { render: [] as string[], analyse: [] as string[] };
  const tools: Tools = {
    render(commands, wav) {
      calls.render.push(wav);
      const id = commands.split("/").pop()?.replace(".json", "") ?? "";
      writeFileSync(wav, floatWav(2, levels[id].amplitude));
    },
    analyse(wav): Analysis {
      calls.analyse.push(wav);
      const id = wav.split("/").pop()?.replace(".wav", "") ?? "";
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
    expect(loopRegion({}, [transport, transport], [8.2, 8.3])).toEqual({ start: 4, end: 8, bpm: 120 });
    expect(loopRegion({}, [transport], [6])).toEqual({ start: 0, end: 4, bpm: 120 });
    expect(loopRegion({ loop: [1, 2] }, [transport], [6])).toEqual({ start: 1, end: 2, bpm: 120 });
  });

  it("refuses candidates whose loops differ, unless the session says what to loop", () => {
    const a = transportOf(JSON.parse(commandList(120, 2)));
    const b = transportOf(JSON.parse(commandList(100, 2)));
    expect(() => loopRegion({}, [a, b], [9, 9])).toThrow(/tempo or loop differ/);
    expect(loopRegion({ loop: [0, 3] }, [a, b], [9, 9])).toEqual({ start: 0, end: 3, bpm: null });
  });

  it("shuffles", () => {
    expect(shuffle([1, 2, 3], () => 0)).toEqual([2, 3, 1]);
  });
});

describe("preparing a session", () => {
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
  const finish = (body: unknown, headers: Record<string, string> = {}) =>
    fetch(`${base}/api/finish`, { method: "POST", headers: { "Content-Type": "application/json", ...headers }, body: JSON.stringify(body) });

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
    expect((await finish({ note: "  " })).status).toBe(400);
    expect((await finish({ note: "The tail clicks." })).status).toBe(200);
    expect(parseToml(readFileSync(join(root, "describe", "verdict.toml"), "utf8"))).toMatchObject({
      kind: "listen and describe",
      note: "The tail clicks.",
      labels: { A: "only" },
    });
    expect(await done).toEqual([join(root, "rate", "verdict.toml"), join(root, "describe", "verdict.toml")]);
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
  await fetch(`${base}/api/finish`, { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify(answer) });
  const [file] = await listen.done;
  server.closeAllConnections();
  await new Promise((resolve) => server.close(resolve));
  return parseToml(readFileSync(file, "utf8"));
}
