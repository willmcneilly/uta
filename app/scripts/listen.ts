// Listening sessions: one question for Will, prepared by Claude, answered in
// a local page that's fair by default (RFC-009, part 3). This file reads a
// session, renders and measures its candidates, and serves the page; the
// command line is in listenCli.ts and the page's own code in listenPage.ts.
//
// A session is a folder in the listening folder (~/.uta/listening/, or
// UTA_LISTENING_DIR), never in the repo, because Will's notes are private:
//
//   2026-10-20-snare-decay/
//     session.toml          the question, the candidates, Claude's prediction
//     short.json …          each candidate's command list (or, on a canvas,
//                           short.toml, the commands added onto it)
//     short.wav …           its render, made here (and short.analysis.json)
//     images/short-level.png …   its images (uta analyse --png), for the reveal
//     verdict.toml          Will's answer, written by the page
//
// The formats are in HELP, which `scripts/listen --help` prints.

import { randomInt } from "node:crypto";
import { existsSync, readFileSync, readdirSync, statSync, writeFileSync } from "node:fs";
import type { IncomingMessage, ServerResponse } from "node:http";
import { stripTypeScriptTypes } from "node:module";
import { homedir } from "node:os";
import { basename, isAbsolute, join, resolve, sep } from "node:path";
import { fileURLToPath } from "node:url";
import { type TomlTable, type TomlValue, parseToml, stringifyToml } from "./toml.ts";

export const HELP = `Usage: scripts/listen [<session> …] [--list] [--prepare] [--port <n>] [--no-open]

Asks Will one question at a time, in a local page. Each <session> is a folder
name in the listening folder, or a path to one. With none, it asks every
session there that has no verdict yet, oldest first. For each session it
renders any candidate not yet rendered (uta render), measures each one and
draws its images (uta analyse --json --png), then serves the page, opens it
in the browser, and exits once the last question's Finish is pressed. Each
verdict's path is printed as it's written.

The listening folder is ~/.uta/listening/, or $UTA_LISTENING_DIR. It's outside
the repo on purpose: verdicts hold Will's own words.

  --list        list the sessions there, and which have a verdict, then stop
  --prepare     render and measure, print each session's loop and gains, then
                stop without serving: to check a session before handing it over
  --again       ask a session again even though it has a verdict (the old one
                is replaced)
  --port <n>    serve on this port (default: any free one)
  --no-open     print the page's address instead of opening it
  --help        show this

The page is fair by default: each candidate's gain is set so their integrated
loudness (BS.1770, from uta analyse) matches the quietest; they're lined up and
play the same loop together, so switching is instant and lands on the same
beat; and they're labelled A, B, C in a shuffled order that isn't in anything
the page receives until Finish. Then it shows which was which, Claude's
prediction, and each candidate's numbers and images.

session.toml, written by Claude before Will listens:

  question = "Which snare decay sounds most like an 808 snare?"
  kind = "pick one"        # "pick one" (2 or more candidates), "rate each"
                           # (1 to 5 each), or "listen and describe"
  match_loudness = true    # optional, default true: turn it off when a
                           # candidate is meant to be louder
  seconds = 8              # optional: how long to render (uta render --seconds)
  loop = [2.14, 4.28]      # optional: the stretch to loop, in seconds. By
                           # default the second time round the song's loop
                           # (so it starts with the first pass's tail), or
                           # with the loop off, from the start until every
                           # candidate's tail is below -60 dB (of its own
                           # peak), measured from the renders
  canvas = "chord"         # optional: render every candidate on the same
                           # part of the canvas (below)

  [prediction]             # written first, before anything is heard
  text = "B: its tail falls 40 dB in 210 ms, close to an 808's."
  choice = "short"         # pick one: the candidate expected to be picked
  measurements = ["decay_40db_s"]   # the uta analyse keys it relies on
  # rate each: [prediction.ratings] with a rating for each candidate

  [[candidates]]           # one for each; the order here is never shown
  id = "short"             # a name for Claude; Will sees only A, B, C
  commands = "short.json"  # its command list, beside session.toml

A canvas session hears every candidate on the same material, so the verdict
is about the sound or effect, not the notes. The canvas is in
examples/listen/canvas/ (its README says what each part is for): "snare" (one
909 snare hit), "chord" (a short, piano-like chord), "swell" (a sung-like
note fading in) and "pad" (a held chord). \`canvas\` names one of them, or is a
path to another song file, from session.toml's folder. Each part is a song
file whose one track is named Part, with the loop off.

On a canvas, a candidate's \`commands\` is a TOML file of [[command]] tables,
as a song file's raw commands are written (examples/demos/README.md), added
onto the end of the canvas's song file. "$track:Part" names its track. Left
out, the candidate is the canvas as it is: a dry reference. Each is compiled
(uta song) to <id>.canvas.json beside it, then rendered.

  # bright.toml: a sound, every setting at once (one left out is its default)
  [[command]]
  type = "set_synth_settings"
  track = "$track:Part"
  settings = { osc1_wave = "saw", cutoff_hz = 4000, release_seconds = 0.4 }

  # echo.toml: an effect on the part's track ({ track = "…" }, or "master"),
  # with every one of its settings
  [[command]]
  type = "add_effects"
  effects = [{ chain = { track = "$track:Part" }, index = 0, effect = { id = "9b2f6c1e-0000-4000-8000-000000000001", settings = { delay = { sync = true, division = "1/8d", time_seconds = 0.375, mode = "stereo", feedback = 0.4, low_cut_hz = 100.0, high_cut_hz = 8000.0, drive = 0.2, wobble = 0.1 } }, enabled = true, mix = 0.3 } }]

verdict.toml, written by the page when Finish is pressed:

  session = "2026-10-20-snare-decay"
  question = "…"
  kind = "pick one"
  choice = "short"           # pick one: the candidate picked, by its id
  note = "A is too ringy."   # Will's words (required for listen and describe)
  prediction_matched = true  # pick one, when the prediction names a choice
  loudness_matched = true
  finished_at = "2026-10-20T10:12:03.000Z"
  [ratings]                  # rate each: 1 to 5 for each candidate, by id
  [gain_db]                  # the gain applied to each candidate, by id
  short = -1.3
  [labels]                   # the label each candidate had
  A = "long"
  B = "short"
`;

export const KINDS = ["pick one", "rate each", "listen and describe"] as const;
export type Kind = (typeof KINDS)[number];

/** A rating's range, for rate each. */
export const RATING_MIN = 1;
export const RATING_MAX = 5;

export interface Candidate {
  id: string;
  /**
   * The command list's path. On a canvas, the commands added onto it (a
   * TOML file of [[command]] tables), or none for the canvas as it is.
   */
  commands?: string;
}

export interface Prediction {
  text: string;
  choice?: string;
  ratings?: Record<string, number>;
  measurements: string[];
}

export interface Session {
  name: string;
  dir: string;
  question: string;
  kind: Kind;
  matchLoudness: boolean;
  seconds?: number;
  loop?: [number, number];
  /** The canvas's song file, when every candidate is rendered on it. */
  canvas?: string;
  prediction: Prediction;
  candidates: Candidate[];
}

export class SessionError extends Error {}

/** The listening folder: $UTA_LISTENING_DIR, or ~/.uta/listening. */
export function listeningDir(env: NodeJS.ProcessEnv = process.env): string {
  return env.UTA_LISTENING_DIR || join(homedir(), ".uta", "listening");
}

/** A session's folder, from its name in the listening folder or a path. */
export function sessionDir(arg: string, dir: string): string {
  const inDir = join(dir, arg);
  if (!isAbsolute(arg) && existsSync(join(inDir, "session.toml"))) return inDir;
  const path = resolve(arg);
  if (existsSync(join(path, "session.toml"))) return path;
  throw new SessionError(`No session "${arg}": there's no ${join(inDir, "session.toml")}.`);
}

export interface Listed {
  name: string;
  dir: string;
  answered: boolean;
}

/** Every session in the listening folder, oldest first (by name, which starts with its date). */
export function listSessions(dir: string): Listed[] {
  let names: string[];
  try {
    names = readdirSync(dir);
  } catch {
    return [];
  }
  return names
    .filter((name) => existsSync(join(dir, name, "session.toml")))
    .sort()
    .map((name) => ({ name, dir: join(dir, name), answered: existsSync(join(dir, name, "verdict.toml")) }));
}

/** Reads and checks a session's session.toml. */
export function readSession(dir: string): Session {
  const file = join(dir, "session.toml");
  let toml: TomlTable;
  try {
    toml = parseToml(readFileSync(file, "utf8"));
  } catch (error) {
    throw new SessionError(`${file}: ${(error as Error).message}`);
  }
  const fail = (message: string): never => {
    throw new SessionError(`${file}: ${message}`);
  };
  const text = (value: TomlValue | undefined, what: string): string =>
    typeof value === "string" && value.trim() !== "" ? value : fail(`${what} must be some text.`);

  const question = text(toml.question, "`question`");
  const kind = toml.kind;
  if (typeof kind !== "string" || !(KINDS as readonly string[]).includes(kind))
    fail(`\`kind\` must be one of ${KINDS.map((k) => `"${k}"`).join(", ")}.`);

  let canvas: string | undefined;
  if (toml.canvas !== undefined) {
    const name = text(toml.canvas, "`canvas`");
    canvas = /^[a-z0-9-]+$/.test(name) ? join(CANVAS_DIR, `${name}.toml`) : resolve(dir, name);
    if (!existsSync(canvas))
      fail(`\`canvas\` "${name}" isn't a part of the canvas (${canvasParts().join(", ")}) or a song file: there's no ${canvas}.`);
  }

  const list = toml.candidates;
  if (!Array.isArray(list) || list.length === 0) fail("it needs at least one [[candidates]].");
  const candidates = (list as TomlValue[]).map((entry, index): Candidate => {
    if (typeof entry !== "object" || Array.isArray(entry)) return fail(`candidate ${index + 1} must be a [[candidates]] table.`);
    const id = text(entry.id, `candidate ${index + 1}'s \`id\``);
    if (!/^[A-Za-z0-9_-]+$/.test(id)) fail(`candidate id "${id}" can only have letters, digits, - and _ (it names its files).`);
    if (canvas && entry.commands === undefined) return { id };
    const commands = resolve(dir, text(entry.commands, `candidate "${id}"'s \`commands\``));
    if (!existsSync(commands)) fail(`candidate "${id}"'s command list ${commands} doesn't exist.`);
    if (canvas) {
      const problem = onlyCommands(readFileSync(commands, "utf8"));
      if (problem) fail(`candidate "${id}"'s ${basename(commands)}: ${problem}`);
    }
    return { id, commands };
  });
  const ids = candidates.map((c) => c.id);
  if (new Set(ids).size !== ids.length) fail("each candidate needs its own `id`.");
  if (candidates.length > 26) fail("26 candidates at most, one for each letter.");
  if (kind === "pick one" && candidates.length < 2) fail("pick one needs at least two candidates.");

  const p = toml.prediction;
  if (typeof p !== "object" || Array.isArray(p)) return fail("it needs a [prediction], written before Will listens.");
  const prediction: Prediction = { text: text(p.text, "`prediction.text`"), measurements: [] };
  if (p.choice !== undefined) {
    if (typeof p.choice !== "string" || !ids.includes(p.choice)) fail(`\`prediction.choice\` must be one of the candidates' ids: ${ids.join(", ")}.`);
    prediction.choice = p.choice as string;
  }
  if (p.ratings !== undefined) {
    const ratings = p.ratings;
    if (typeof ratings !== "object" || Array.isArray(ratings)) return fail("`prediction.ratings` must be a table of ratings by candidate id.");
    for (const [id, rating] of Object.entries(ratings)) {
      if (!ids.includes(id)) fail(`\`prediction.ratings\` has "${id}", which isn't a candidate.`);
      if (!isRating(rating)) fail(`\`prediction.ratings.${id}\` must be a whole number from ${RATING_MIN} to ${RATING_MAX}.`);
    }
    prediction.ratings = ratings as Record<string, number>;
  }
  if (p.measurements !== undefined) {
    if (!Array.isArray(p.measurements) || !p.measurements.every((m) => typeof m === "string"))
      fail("`prediction.measurements` must be a list of uta analyse keys.");
    prediction.measurements = p.measurements as string[];
  }

  const matchLoudness = toml.match_loudness ?? true;
  if (typeof matchLoudness !== "boolean") fail("`match_loudness` must be true or false.");
  const seconds = toml.seconds;
  if (seconds !== undefined && !(typeof seconds === "number" && seconds > 0)) fail("`seconds` must be a number above 0.");
  const loop = toml.loop;
  if (
    loop !== undefined &&
    !(Array.isArray(loop) && loop.length === 2 && loop.every((n) => typeof n === "number") && (loop[0] as number) >= 0 && (loop[1] as number) > (loop[0] as number))
  )
    fail("`loop` must be [start, end] in seconds, with the end after the start.");

  return {
    name: basename(dir),
    dir,
    question,
    kind: kind as Kind,
    matchLoudness: matchLoudness as boolean,
    seconds: seconds as number | undefined,
    loop: loop as [number, number] | undefined,
    canvas,
    prediction,
    candidates,
  };
}

// --- The canvas -----------------------------------------------------------

/** The canvas's parts: the song files in examples/listen/canvas/. */
export const CANVAS_DIR = fileURLToPath(new URL("../../examples/listen/canvas/", import.meta.url));

export function canvasParts(dir: string = CANVAS_DIR): string[] {
  try {
    return readdirSync(dir)
      .filter((name) => name.endsWith(".toml"))
      .map((name) => name.slice(0, -".toml".length))
      .sort();
  } catch {
    return [];
  }
}

/**
 * Why a candidate's commands can't be added onto a canvas, or null: they're
 * added onto the end of its song file, so anything but [[command]] tables
 * would land in the song's last table instead.
 */
export function onlyCommands(text: string): string | null {
  let first = true;
  for (const raw of text.split("\n")) {
    const line = raw.trim();
    if (line === "" || line.startsWith("#")) continue;
    const header = /^\[\[?\s*([^\]]+?)\s*\]\]?$/.exec(line);
    if (first && !(header && line.startsWith("[[") && header[1] === "command"))
      return "it must start with a [[command]] table, since it's added onto the canvas's song file.";
    first = false;
    if (header && header[1] !== "command" && !header[1].startsWith("command."))
      return `it can only have [[command]] tables, not [${header[1]}].`;
  }
  return first ? "it has no [[command]] tables: leave `commands` out for the canvas as it is." : null;
}

/** The song file a candidate is rendered from on a canvas: the canvas, with its commands added onto the end. */
export function onCanvas(canvas: string, commands: string | undefined): string {
  const song = readFileSync(canvas, "utf8");
  if (!commands) return song;
  return `${song.trimEnd()}\n\n# Added by the listening session, from ${basename(commands)}:\n${readFileSync(commands, "utf8")}`;
}

export function isRating(value: unknown): value is number {
  return typeof value === "number" && Number.isInteger(value) && value >= RATING_MIN && value <= RATING_MAX;
}

// --- The loop -------------------------------------------------------------

/**
 * Where a command list's loop is, in seconds, from its last tempo and loop
 * commands. The defaults are a new project's (`Project::with_id` in
 * uta-core): 120 BPM, a 4-bar loop from bar 1, on. Bars are 4/4, the only
 * time signature there is.
 */
export interface Transport {
  bpm: number;
  loopEnabled: boolean;
  loopStart: number;
  loopLength: number;
}

export function transportOf(commandList: unknown): Transport {
  let bpm = 120;
  let startBar = 0;
  let bars = 4;
  let loopEnabled = true;
  const commands = (commandList as { commands?: { command?: Record<string, unknown> }[] }).commands ?? [];
  for (const { command } of commands) {
    if (!command) continue;
    if (command.type === "set_tempo" && typeof command.bpm === "number") bpm = command.bpm;
    else if (command.type === "set_loop_length" && typeof command.bars === "number") bars = command.bars;
    else if (command.type === "set_loop" && typeof command.start_bar === "number" && typeof command.bars === "number") {
      startBar = command.start_bar;
      bars = command.bars;
    } else if (command.type === "set_loop_enabled" && typeof command.enabled === "boolean") loopEnabled = command.enabled;
  }
  const bar = (4 * 60) / bpm;
  return { bpm, loopEnabled, loopStart: startBar * bar, loopLength: bars * bar };
}

/**
 * The stretch every candidate loops, in seconds: the session's own `loop`,
 * or the second time round the song's loop (which starts with the first
 * pass's tail, so it loops without a gap), or the first time round if the
 * render stops before that. With the loop off, it's from the start until
 * the last candidate's tail is below TAIL_DB (`tails`, from `tailSeconds`),
 * not the silence a render rings out into; the shorter renders are padded
 * with silence to it. The candidates must share a loop, so they stay lined
 * up.
 */
export function loopRegion(
  session: Pick<Session, "loop">,
  transports: Transport[],
  seconds: number[],
  tails: number[],
): { start: number; end: number; bpm: number | null } {
  const shortest = Math.min(...seconds);
  const [first] = transports;
  const same = transports.every(
    (t) => t.bpm === first.bpm && t.loopEnabled === first.loopEnabled && t.loopStart === first.loopStart && t.loopLength === first.loopLength,
  );
  const bpm = same ? first.bpm : null;
  if (session.loop) {
    const [start, end] = session.loop;
    if (end > shortest + 1e-9) throw new SessionError(`\`loop\` ends at ${end} s, after the shortest render (${shortest.toFixed(2)} s).`);
    return { start, end, bpm };
  }
  if (!same) throw new SessionError("The candidates' tempo or loop differ, so they can't loop together: set `loop` in session.toml.");
  if (!first.loopEnabled) {
    const last = Math.max(...tails);
    return { start: 0, end: last > 0 ? last : shortest, bpm };
  }
  for (const pass of [1, 0]) {
    const start = first.loopStart + pass * first.loopLength;
    const end = start + first.loopLength;
    if (end <= shortest + 1e-9) return { start, end, bpm };
  }
  return { start: 0, end: shortest, bpm };
}

// --- Loudness and labels --------------------------------------------------

/**
 * The gain for each candidate, in dB, that brings its integrated loudness to
 * the quietest one's, so nothing is turned up into clipping. A candidate with
 * no loudness (silence) gets none, as does every one when matching is off.
 */
export function matchGains(loudness: (number | null)[], match: boolean): number[] {
  const measured = loudness.filter((l): l is number => l !== null && Number.isFinite(l));
  if (!match || measured.length === 0) return loudness.map(() => 0);
  const target = Math.min(...measured);
  return loudness.map((l) => (l !== null && Number.isFinite(l) ? round(target - l, 2) : 0));
}

function round(value: number, places: number): number {
  const scale = 10 ** places;
  return Math.round(value * scale) / scale + 0;
}

/** A shuffled copy (Fisher–Yates), with `random(n)` giving a whole number below n. */
export function shuffle<T>(items: T[], random: (n: number) => number = randomInt): T[] {
  const out = [...items];
  for (let i = out.length - 1; i > 0; i--) {
    const j = random(i + 1);
    [out[i], out[j]] = [out[j], out[i]];
  }
  return out;
}

export const LABELS = "ABCDEFGHIJKLMNOPQRSTUVWXYZ".split("");

// --- WAVs -----------------------------------------------------------------

export interface WavInfo {
  sampleRate: number;
  channels: number;
  frames: number;
  /** Where the samples start, and how many bytes they are. */
  dataOffset: number;
  dataLength: number;
}

/** The layout of a 32-bit float WAV, the kind `uta render` writes. */
export function readWav(bytes: Buffer): WavInfo {
  if (bytes.toString("ascii", 0, 4) !== "RIFF" || bytes.toString("ascii", 8, 12) !== "WAVE") throw new Error("not a WAV file");
  let at = 12;
  let format: { tag: number; channels: number; sampleRate: number; bits: number } | null = null;
  while (at + 8 <= bytes.length) {
    const id = bytes.toString("ascii", at, at + 4);
    const size = bytes.readUInt32LE(at + 4);
    const body = at + 8;
    if (id === "fmt ") {
      format = {
        tag: bytes.readUInt16LE(body),
        channels: bytes.readUInt16LE(body + 2),
        sampleRate: bytes.readUInt32LE(body + 4),
        bits: bytes.readUInt16LE(body + 14),
      };
      // WAVE_FORMAT_EXTENSIBLE keeps the real format in its sub-format.
      if (format.tag === 0xfffe && size >= 26) format.tag = bytes.readUInt16LE(body + 24);
    } else if (id === "data") {
      if (!format) throw new Error("a WAV's data came before its format");
      if (format.tag !== 3 || format.bits !== 32) throw new Error("only 32-bit float WAVs (what uta render writes) can be matched");
      const dataLength = Math.min(size, bytes.length - body);
      return { sampleRate: format.sampleRate, channels: format.channels, frames: dataLength / 4 / format.channels, dataOffset: body, dataLength };
    }
    at = body + size + (size % 2);
  }
  throw new Error("a WAV with no data");
}

/** Where a loop with the loop off ends: once every candidate is this far below its own peak. */
export const TAIL_DB = -60;

/**
 * How long a float WAV goes on before it's below `db` of its own peak for
 * good, in seconds: the end of its last sample at or above that, on either
 * channel. 0 for silence.
 */
export function tailSeconds(bytes: Buffer, db: number = TAIL_DB): number {
  const info = readWav(bytes);
  const end = info.dataOffset + info.dataLength;
  let peak = 0;
  for (let at = info.dataOffset; at < end; at += 4) peak = Math.max(peak, Math.abs(bytes.readFloatLE(at)));
  if (peak === 0) return 0;
  const threshold = peak * 10 ** (db / 20);
  for (let at = end - 4; at >= info.dataOffset; at -= 4)
    if (Math.abs(bytes.readFloatLE(at)) >= threshold) return (Math.floor((at - info.dataOffset) / 4 / info.channels) + 1) / info.sampleRate;
  return 0;
}

/**
 * A copy of a float WAV with its gain changed, so the page is sent the
 * matched sound and nothing else, and padded with silence to at least
 * `frames`, so it lasts the whole loop.
 */
export function withGain(bytes: Buffer, gainDb: number, frames = 0): Buffer {
  const info = readWav(bytes);
  const padding = Math.max(0, Math.ceil(frames) - info.frames) * info.channels * 4;
  const dataEnd = info.dataOffset + info.dataLength;
  const out = padding === 0 ? Buffer.from(bytes) : Buffer.concat([bytes.subarray(0, dataEnd), Buffer.alloc(padding), bytes.subarray(dataEnd)]);
  if (padding > 0) {
    out.writeUInt32LE(info.dataLength + padding, info.dataOffset - 4);
    out.writeUInt32LE(out.length - 8, 4);
  }
  if (gainDb === 0) return out;
  const scale = 10 ** (gainDb / 20);
  for (let at = info.dataOffset; at < dataEnd; at += 4) out.writeFloatLE(out.readFloatLE(at) * scale, at);
  return out;
}

// --- Preparing a session --------------------------------------------------

export interface Tools {
  /** Renders a command list to a WAV, as `uta render`. */
  render(commands: string, wav: string, seconds?: number): void;
  /** Compiles a song file to a command list, as `uta song --out`. */
  song(file: string, out: string): void;
  /** Measures a WAV, as `uta analyse --json`, and draws its images into `images`, as `--png`. */
  analyse(wav: string, images: string): Analysis;
}

export interface Analysis {
  measurements: Record<string, number | null>;
  [key: string]: unknown;
}

export interface Prepared {
  session: Session;
  candidates: {
    id: string;
    wav: string;
    analysis: Analysis;
    gainDb: number;
    seconds: number;
    /** How long it sounds before it's below TAIL_DB for good. */
    tailSeconds: number;
    sampleRate: number;
    /** The images drawn of it, of IMAGES, in that order: `uta analyse` draws the harmonics only when there's a pitch. */
    images: Image[];
  }[];
  loop: { start: number; end: number; bpm: number | null };
}

/**
 * The images the reveal shows of each candidate, as `uta analyse --png` names
 * them after the WAV (`<id>-level.png`), in the order shown: the spectrogram
 * and level curve first, as RFC-009's reveal has them.
 */
export const IMAGES = ["spectrogram-long", "level", "spectrogram-short", "clicks", "harmonics"] as const;
export type Image = (typeof IMAGES)[number];

/** Where a candidate's image is: in the session's images folder. */
export const imagePath = (dir: string, id: string, image: Image) => join(dir, "images", `${id}-${image}.png`);

const REPO = fileURLToPath(new URL("../../", import.meta.url));
const relativeToRepo = (path: string) => (path.startsWith(REPO) ? path.slice(REPO.length).split(sep).join("/") : path);

const newer = (file: string, than: string[]) => existsSync(file) && than.every((other) => statSync(file).mtimeMs >= statSync(other).mtimeMs);

/** The command list a candidate is rendered from: its own, or on a canvas, the one compiled beside it. */
export const renderedFrom = (session: Session, candidate: Candidate): string =>
  session.canvas ? join(session.dir, `${candidate.id}.canvas.json`) : (candidate.commands as string);

/**
 * Renders each candidate that isn't rendered yet (or whose command list,
 * canvas or session changed since), measures each, and works out the gains
 * and the loop. On a canvas, each candidate's commands are added onto it
 * and compiled first.
 */
export function prepare(session: Session, tools: Tools, log: (line: string) => void = () => {}): Prepared {
  const sessionFile = join(session.dir, "session.toml");
  const candidates = session.candidates.map((candidate) => {
    const wav = join(session.dir, `${candidate.id}.wav`);
    const commands = renderedFrom(session, candidate);
    const sources = [sessionFile, ...(session.canvas ? [session.canvas] : []), ...(candidate.commands ? [candidate.commands] : [])];
    if (!newer(wav, sources) || !existsSync(commands)) {
      if (session.canvas) {
        const song = join(session.dir, `${candidate.id}.canvas.toml`);
        writeFileSync(song, onCanvas(session.canvas, candidate.commands));
        log(`Compiling ${session.name}/${candidate.id} on the canvas (${relativeToRepo(session.canvas)})…`);
        tools.song(song, commands);
      }
      log(`Rendering ${session.name}/${candidate.id}…`);
      tools.render(commands, wav, session.seconds);
    }
    const analysisFile = join(session.dir, `${candidate.id}.analysis.json`);
    let analysis: Analysis;
    // Every image but the harmonics is always drawn, so those say whether they're all up to date.
    const always = IMAGES.filter((image) => image !== "harmonics").map((image) => imagePath(session.dir, candidate.id, image));
    if (newer(analysisFile, [wav]) && always.every((image) => newer(image, [wav])))
      analysis = JSON.parse(readFileSync(analysisFile, "utf8")) as Analysis;
    else {
      log(`Measuring ${session.name}/${candidate.id}…`);
      analysis = tools.analyse(wav, join(session.dir, "images"));
      writeFileSync(analysisFile, JSON.stringify(analysis, null, 2) + "\n");
    }
    const bytes = readFileSync(wav);
    const info = readWav(bytes);
    const images = IMAGES.filter((image) => newer(imagePath(session.dir, candidate.id, image), [wav]));
    const seconds = info.frames / info.sampleRate;
    return { id: candidate.id, wav, analysis, images, sampleRate: info.sampleRate, seconds, tailSeconds: tailSeconds(bytes) };
  });
  if (new Set(candidates.map((c) => c.sampleRate)).size > 1) throw new SessionError(`${session.name}: the renders' sample rates differ.`);
  const gains = matchGains(
    candidates.map((c) => c.analysis.measurements?.loudness_integrated_lufs ?? null),
    session.matchLoudness,
  );
  const transports = session.candidates.map((c) => transportOf(JSON.parse(readFileSync(renderedFrom(session, c), "utf8"))));
  let loop;
  try {
    loop = loopRegion(
      session,
      transports,
      candidates.map((c) => c.seconds),
      candidates.map((c) => c.tailSeconds),
    );
  } catch (error) {
    throw new SessionError(`${session.name}: ${(error as Error).message}`);
  }
  return { session, candidates: candidates.map((c, i) => ({ ...c, gainDb: gains[i] })), loop };
}

// --- The verdict ----------------------------------------------------------

/** What the page sends when Finish is pressed, by label. */
export interface Answer {
  /** Which question the page was showing (its index in the queue), so a stale page can't answer the next one. */
  question?: number;
  choice?: string;
  ratings?: Record<string, number>;
  note?: string;
}

/** The verdict, by candidate id, or why the answer won't do. */
export function verdictFor(prepared: Prepared, labels: string[], answer: Answer, now: Date): TomlTable | string {
  const { session } = prepared;
  const idOf = (label: string) => {
    const index = labels.indexOf(label);
    return index < 0 ? undefined : prepared.candidates[index].id;
  };
  const note = typeof answer.note === "string" ? answer.note.trim() : "";
  const verdict: TomlTable = { session: session.name, question: session.question, kind: session.kind };
  if (session.kind === "pick one") {
    const choice = typeof answer.choice === "string" ? idOf(answer.choice) : undefined;
    if (!choice) return "Pick one first.";
    verdict.choice = choice;
  }
  if (session.kind === "listen and describe" && note === "") return "Say what you hear first.";
  verdict.note = note;
  if (session.kind === "pick one" && session.prediction.choice) verdict.prediction_matched = verdict.choice === session.prediction.choice;
  verdict.loudness_matched = session.matchLoudness;
  verdict.finished_at = now.toISOString();
  if (session.kind === "rate each") {
    const ratings: TomlTable = {};
    for (const label of labels) {
      const rating = answer.ratings?.[label];
      if (!isRating(rating)) return `Rate ${label} first.`;
      ratings[idOf(label) as string] = rating;
    }
    verdict.ratings = ratings;
  }
  verdict.gain_db = Object.fromEntries(prepared.candidates.map((c) => [c.id, c.gainDb]));
  verdict.labels = Object.fromEntries([...labels].sort().map((label) => [label, idOf(label) as string]));
  return verdict;
}

// --- The server -----------------------------------------------------------

export interface ServeOptions {
  /** A whole number below n, for the shuffle. */
  random?: (n: number) => number;
  now?: () => Date;
  /** Called with each verdict's path as it's written. */
  onVerdict?: (path: string) => void;
}

export interface Listening {
  handle(request: IncomingMessage, response: ServerResponse): void;
  /** Resolves with each verdict's path once the last question is finished. */
  done: Promise<string[]>;
}

interface Asked {
  prepared: Prepared;
  /** labels[i] is the label of prepared.candidates[i]. */
  labels: string[];
  /** The candidates' indexes in label order: A first. */
  order: number[];
  audio: Map<string, Buffer>;
  verdict?: string;
}

const PAGE_SCRIPT = fileURLToPath(new URL("./listenPage.ts", import.meta.url));
const TOKENS = fileURLToPath(new URL("../src/design/tokens.css", import.meta.url));

/**
 * The page's server, for the prepared sessions in order. Before a session's
 * Finish, nothing it sends says which candidate is which: the session is sent
 * by label, and each label's audio is its render with the matching gain
 * already applied.
 */
export function listening(prepared: Prepared[], options: ServeOptions = {}): Listening {
  const random = options.random ?? randomInt;
  const now = options.now ?? (() => new Date());
  const asked: Asked[] = prepared.map((p) => {
    const order = shuffle(
      p.candidates.map((_, i) => i),
      random,
    );
    const labels: string[] = [];
    order.forEach((candidate, position) => (labels[candidate] = LABELS[position]));
    return { prepared: p, labels, order, audio: new Map() };
  });
  let current = 0;
  let finish: (paths: string[]) => void = () => {};
  const done = new Promise<string[]>((resolve) => (finish = resolve));

  const sessionState = () => {
    const ask = asked[current];
    const { session, loop } = ask.prepared;
    return {
      index: current,
      total: asked.length,
      question: session.question,
      kind: session.kind,
      labels: ask.order.map((_, position) => LABELS[position]),
      loop,
      loudnessMatched: session.matchLoudness,
      ratingRange: [RATING_MIN, RATING_MAX],
    };
  };

  // The images go in the reply itself, as data URLs: after the last
  // question's Finish the server stops, so the page couldn't fetch them later.
  const reveal = (ask: Asked) => {
    const { session } = ask.prepared;
    const dataUrl = (id: string, image: Image) => `data:image/png;base64,${readFileSync(imagePath(session.dir, id, image)).toString("base64")}`;
    return {
      verdict: ask.verdict,
      prediction: session.prediction,
      candidates: ask.order.map((index, position) => {
        const c = ask.prepared.candidates[index];
        const label = LABELS[position];
        const images = c.images.map((image) => ({ name: image, src: dataUrl(c.id, image) }));
        return { label, id: c.id, gainDb: c.gainDb, measurements: c.analysis.measurements ?? {}, images };
      }),
      next: current < asked.length,
    };
  };

  const handle = (request: IncomingMessage, response: ServerResponse): void => {
    const path = (request.url ?? "").split("?")[0];
    const get = request.method === "GET";
    if (get && path === "/") return send(response, 200, "text/html; charset=utf-8", PAGE);
    if (get && path === "/page.js") return send(response, 200, "text/javascript; charset=utf-8", stripTypeScriptTypes(readFileSync(PAGE_SCRIPT, "utf8")));
    if (get && path === "/tokens.css") return send(response, 200, "text/css; charset=utf-8", readFileSync(TOKENS, "utf8"));
    if (current >= asked.length) return json(response, 410, { error: "That was the last question." });
    const ask = asked[current];
    if (get && path === "/api/session") return json(response, 200, sessionState());
    const audio = /^\/audio\/([A-Z])$/.exec(path);
    if (get && audio) {
      const position = LABELS.indexOf(audio[1]);
      const index = ask.order[position];
      if (index === undefined) return json(response, 404, { error: "No such candidate." });
      const label = audio[1];
      if (!ask.audio.has(label)) {
        const c = ask.prepared.candidates[index];
        const { loop } = ask.prepared;
        ask.audio.set(label, withGain(readFileSync(c.wav), c.gainDb, loop.end * c.sampleRate));
      }
      return send(response, 200, "audio/wav", ask.audio.get(label) as Buffer);
    }
    if (request.method === "POST" && path === "/api/finish") {
      void readAnswer(request, response).then((answer) => {
        if (!answer) return;
        // Another tab, or a second press, may have finished this question while the answer was arriving.
        if (answer.question !== asked.indexOf(ask) || current >= asked.length || asked[current] !== ask)
          return json(response, 409, { error: "That question has already been answered. Reload the page for the next one." });
        const verdict = verdictFor(ask.prepared, ask.labels, answer, now());
        if (typeof verdict === "string") return json(response, 400, { error: verdict });
        const file = join(ask.prepared.session.dir, "verdict.toml");
        writeFileSync(file, stringifyToml(verdict));
        ask.verdict = file;
        options.onVerdict?.(file);
        current++;
        json(response, 200, reveal(ask));
        if (current >= asked.length) response.on("finish", () => finish(asked.map((a) => a.verdict as string)));
      });
      return;
    }
    json(response, 404, { error: "Not here." });
  };
  return { handle, done };
}

const MAX_BODY = 1 << 16;

async function readAnswer(request: IncomingMessage, response: ServerResponse): Promise<Answer | null> {
  // Only the page itself may answer: a JSON body and a matching origin keep
  // other web pages from posting a verdict.
  const origin = request.headers.origin;
  if (origin !== undefined && origin !== `http://${request.headers.host}`) {
    json(response, 403, { error: "Only the listening page can answer." });
    return null;
  }
  if (!request.headers["content-type"]?.startsWith("application/json")) {
    json(response, 415, { error: "Send JSON." });
    return null;
  }
  let body = "";
  for await (const chunk of request) {
    body += chunk;
    if (body.length > MAX_BODY) {
      json(response, 413, { error: "Too long." });
      return null;
    }
  }
  try {
    const answer = JSON.parse(body) as unknown;
    if (typeof answer === "object" && answer !== null && !Array.isArray(answer)) return answer as Answer;
  } catch {
    // Falls through to the error below.
  }
  json(response, 400, { error: "Send the answer as a JSON object." });
  return null;
}

function send(response: ServerResponse, status: number, type: string, body: string | Buffer): void {
  response.writeHead(status, { "Content-Type": type, "Cache-Control": "no-store" });
  response.end(body);
}

function json(response: ServerResponse, status: number, body: unknown): void {
  send(response, status, "application/json", JSON.stringify(body));
}

// The page's markup and look. Its behaviour is in listenPage.ts. It follows
// DESIGN.md's Key and Segmented choice: `live` (orange) marks the candidate
// you're hearing, `selected` (blue) your pick. provisional: D-38
const PAGE = `<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Listening</title>
<link rel="stylesheet" href="/tokens.css">
<style>
  * { box-sizing: border-box; }
  body { margin: 0; background: var(--paper); color: var(--ink); font: var(--font-text); }
  main { max-width: calc(30 * var(--space-7)); margin: 0 auto; padding: var(--space-7) var(--space-6); display: grid; gap: var(--space-7); }
  .progress { font: var(--font-label); color: var(--ink-2); }
  h1 { font: var(--font-title); margin: var(--space-2) 0 0; }
  h2 { font: var(--font-label); color: var(--ink-2); margin: 0 0 var(--space-3); text-transform: none; }
  .row { display: flex; flex-wrap: wrap; gap: var(--space-4); align-items: center; }
  button.key { font: var(--font-text); color: var(--ink); background: none; min-height: var(--space-7); min-width: var(--space-7);
    padding: 0 var(--space-4); border: var(--stroke-grid) solid var(--line-2); border-radius: var(--rounded-md); cursor: pointer; }
  button.key:hover { background: var(--line); }
  button.key:active { background: var(--line-2); }
  button.key:focus-visible { outline: var(--stroke-clip-selected) solid var(--selected); outline-offset: 0; }
  button.key:disabled { border-color: var(--line); color: var(--ink-3); background: none; cursor: default; }
  button.key.primary { border-color: var(--ink); }
  button.key .mark { display: inline-block; margin-right: var(--space-3); }
  .playing button.key.play .mark { color: var(--live); }
  .segments { display: inline-flex; border: var(--stroke-grid) solid var(--ink-3); border-radius: var(--rounded-md); }
  .segments button { font: var(--font-text); color: var(--ink-2); background: none; border: var(--stroke-grid) solid transparent;
    border-radius: var(--rounded-md); min-width: calc(2 * var(--space-7)); min-height: var(--space-7); padding: 0 var(--space-5); cursor: pointer; }
  .segments button:hover { background: var(--line); color: var(--ink); }
  .segments button:active { background: var(--line-2); }
  .segments button:focus-visible { outline: var(--stroke-clip-selected) solid var(--selected); }
  .segments.hear button[aria-checked="true"] { border-color: var(--live); color: var(--ink); }
  .segments.pick button[aria-checked="true"] { border-color: var(--selected); color: var(--selected); background: var(--selected-wash); }
  .position { font: var(--font-position); font-family: var(--font-position-family), monospace; color: var(--ink-3); min-width: 5ch; }
  .playing .position { color: var(--ink); }
  .hint { font: var(--font-label); color: var(--ink-2); }
  .rate { display: grid; grid-template-columns: 2ch auto; gap: var(--space-4); align-items: center; font: var(--font-text); }
  textarea { width: 100%; min-height: calc(3 * var(--space-7)); font: var(--font-text); color: var(--ink); background: var(--sheet);
    border: var(--stroke-grid) solid var(--line-2); border-radius: var(--rounded-md); padding: var(--space-4); resize: vertical; }
  textarea:focus-visible { outline: var(--stroke-clip-selected) solid var(--selected); }
  .error { color: var(--error); }
  table { border-collapse: collapse; width: 100%; font: var(--font-number); font-family: var(--font-number-family), monospace; }
  th, td { text-align: right; padding: var(--space-2) var(--space-4); border-bottom: var(--stroke-grid) solid var(--line-2); }
  th:first-child, td:first-child { text-align: left; }
  thead th { font: var(--font-name); }
  tr.relied td { color: var(--ink); }
  td { color: var(--ink-2); }
  .prediction { background: var(--sheet); border-radius: var(--rounded-md); padding: var(--space-5); }
  .images { display: grid; gap: var(--space-4); }
  .images h3 { font: var(--font-name); margin: var(--space-4) 0 0; }
  .images img { display: block; width: 100%; height: auto; border-radius: var(--rounded-md); }
  .images summary { font: var(--font-label); color: var(--ink-2); cursor: pointer; }
  [hidden] { display: none !important; }
</style>
</head>
<body>
<main id="app"><p class="hint">Loading…</p></main>
<script type="module" src="/page.js"></script>
</body>
</html>
`;
