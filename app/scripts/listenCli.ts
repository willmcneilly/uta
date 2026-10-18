// The command line for `scripts/listen`, which runs it with Node. The work is
// in listen.ts.

import { execFileSync, spawn } from "node:child_process";
import { createServer } from "node:http";
import type { AddressInfo } from "node:net";
import { fileURLToPath } from "node:url";
import {
  HELP,
  type Prepared,
  SessionError,
  type Tools,
  listSessions,
  listening,
  listeningDir,
  prepare,
  readSession,
  sessionDir,
} from "./listen.ts";

const REPO = fileURLToPath(new URL("../..", import.meta.url));

/** `uta render` and `uta analyse --json`, from $UTA_BIN or this checkout's release build. */
function utaTools(): Tools {
  let uta = process.env.UTA_BIN;
  const binary = () => {
    if (!uta) {
      console.error("Building uta (cargo build --release -p uta-cli)…");
      execFileSync("cargo", ["build", "--release", "-q", "-p", "uta-cli"], { cwd: REPO, stdio: "inherit" });
      uta = `${REPO}target/release/uta`;
    }
    return uta;
  };
  return {
    render(commands, wav, seconds) {
      const args = ["render", wav, "--commands", commands, ...(seconds ? ["--seconds", String(seconds)] : [])];
      execFileSync(binary(), args, { stdio: ["ignore", "ignore", "inherit"] });
    },
    analyse(wav) {
      return JSON.parse(execFileSync(binary(), ["analyse", wav, "--json"], { encoding: "utf8" }));
    },
  };
}

function parseArgs(argv: string[]) {
  const args = { sessions: [] as string[], list: false, again: false, open: true, port: 0, help: false };
  for (let i = 0; i < argv.length; i++) {
    const arg = argv[i];
    if (arg === "--help" || arg === "-h") args.help = true;
    else if (arg === "--list") args.list = true;
    else if (arg === "--again") args.again = true;
    else if (arg === "--no-open") args.open = false;
    else if (arg === "--port") {
      args.port = Number(argv[++i]);
      if (!Number.isInteger(args.port) || args.port < 0) return `--port needs a port number.`;
    } else if (arg.startsWith("-")) return `Unknown option ${arg}.`;
    else args.sessions.push(arg);
  }
  return args;
}

async function main() {
  const args = parseArgs(process.argv.slice(2));
  if (typeof args === "string") {
    console.error(`${args}\n\n${HELP}`);
    process.exitCode = 64;
    return;
  }
  if (args.help) {
    console.log(HELP);
    return;
  }
  const dir = listeningDir();
  if (args.list) {
    const sessions = listSessions(dir);
    if (sessions.length === 0) console.log(`No sessions in ${dir}.`);
    for (const s of sessions) console.log(`${s.answered ? "answered" : "waiting "}  ${s.name}`);
    return;
  }

  const dirs = args.sessions.length
    ? args.sessions.map((name) => sessionDir(name, dir))
    : listSessions(dir)
        .filter((s) => !s.answered)
        .map((s) => s.dir);
  if (dirs.length === 0) {
    console.log(`Nothing to listen to: every session in ${dir} has a verdict.`);
    return;
  }
  const tools = utaTools();
  const prepared: Prepared[] = [];
  for (const sessionPath of dirs) {
    const session = readSession(sessionPath);
    if (!args.again && listSessions(dir).some((s) => s.dir === sessionPath && s.answered)) {
      console.log(`Skipping ${session.name}: it has a verdict (use --again to ask it again).`);
      continue;
    }
    prepared.push(prepare(session, tools, (line) => console.error(line)));
  }
  if (prepared.length === 0) return;

  const { handle, done } = listening(prepared, { onVerdict: (path) => console.log(`Wrote ${path}`) });
  const server = createServer(handle);
  await new Promise<void>((resolve) => server.listen(args.port, "127.0.0.1", resolve));
  const url = `http://127.0.0.1:${(server.address() as AddressInfo).port}/`;
  const count = prepared.length === 1 ? "1 question" : `${prepared.length} questions`;
  console.log(`Listening page for ${count}: ${url}`);
  if (args.open) spawn("open", [url], { stdio: "ignore", detached: true }).unref();

  await done;
  server.close();
  server.closeAllConnections();
}

main().catch((error: unknown) => {
  if (error instanceof SessionError) {
    console.error(error.message);
    process.exitCode = 1;
  } else throw error;
});
