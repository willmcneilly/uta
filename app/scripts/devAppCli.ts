// `scripts/dev-app`: starts, stops and checks the Uta app in development mode
// (`tauri dev`) for the checkout this file is in. See `--help`.

import { execFileSync, spawn, spawnSync } from "node:child_process";
import { closeSync, existsSync, mkdirSync, openSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { createServer } from "node:net";
import { join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import {
  DEBUG_APP,
  type Proc,
  UNKNOWN,
  errorLines,
  findDevApps,
  parsePs,
  parseWorktrees,
  pickPort,
  readLog,
  stripAnsi,
  tail,
} from "./devApp.ts";

const HELP = `Usage: scripts/dev-app <command> [--timeout <seconds>]

Runs the Uta app in development mode (tauri dev, with hot reload) for the
checkout this script is in, from any directory.

Commands:
  start   Stop every other Uta dev app (any checkout of this repository, never
          another project's), pick a free port from 1420 up, start tauri dev
          detached so it outlives this shell, and wait until the app is open.
          Prints the branch, commit, port and pid. On a failed build it stops
          what it started and exits 1 with the end of the log.
  wait    Wait for an app that start left building (after its time ran out).
  status  Is this checkout's app running, from which commit, on which port,
          and has its log reported errors since it started. Exits 1 if not
          running.
  stop    Stop this checkout's app: tauri dev, Vite, cargo and the app.

Options:
  --timeout <seconds>  How long start and wait wait for the app (default 480).
                       After that they exit 2 and leave it building.

Log and state: target/dev-app/ in the checkout.`;

const checkout = resolve(fileURLToPath(new URL("../..", import.meta.url)));
const appDir = join(checkout, "app");
const stateDir = join(checkout, "target", "dev-app");
const logPath = join(stateDir, "tauri-dev.log");
const statePath = join(stateDir, "state.json");

type State = { pid: number; port: number; branch: string; commit: string; startedAt: string };

function git(...args: string[]): string {
  return execFileSync("git", ["-C", checkout, ...args], { encoding: "utf8" }).trim();
}

function checkouts(): string[] {
  try {
    return [...new Set([checkout, ...parseWorktrees(git("worktree", "list", "--porcelain"))])];
  } catch {
    return [checkout];
  }
}

function devApps(): Map<string, Proc[]> {
  const ps = execFileSync("ps", ["-axww", "-o", "pid=,ppid=,command="], { encoding: "utf8" });
  return findDevApps(parsePs(ps), checkouts());
}

function alive(pid: number): boolean {
  try {
    process.kill(pid, 0);
    return true;
  } catch {
    return false;
  }
}

const sleep = (ms: number) => new Promise((done) => setTimeout(done, ms));

/** Stops every process in `procs`: SIGTERM, then SIGKILL after 5 seconds. Returns any that survive. */
async function stopAll(procs: Proc[]): Promise<Proc[]> {
  for (const signal of ["SIGTERM", "SIGKILL"] as const) {
    for (const proc of procs.filter((p) => alive(p.pid))) {
      try {
        process.kill(proc.pid, signal);
      } catch {
        // Already gone.
      }
    }
    for (let waited = 0; waited < 5000 && procs.some((p) => alive(p.pid)); waited += 100) await sleep(100);
  }
  return procs.filter((p) => alive(p.pid));
}

function describe(where: string): string {
  return where === UNKNOWN ? "an unknown checkout" : shortPath(where);
}

/** Relative to the current folder when that's shorter, absolute otherwise. */
function shortPath(path: string): string {
  const rel = relative(process.cwd(), path);
  return rel.startsWith("..") ? path : rel || ".";
}

/** True when nothing listens on `port` on either loopback address. */
function isFree(port: number): Promise<boolean> {
  const tryHost = (host: string) =>
    new Promise<boolean>((done) => {
      const server = createServer();
      server.once("error", (error: NodeJS.ErrnoException) =>
        // A machine without IPv6 can't have anything listening on ::1.
        done(error.code === "EADDRNOTAVAIL" || error.code === "EAFNOSUPPORT"),
      );
      server.listen({ port, host, exclusive: true }, () => server.close(() => done(true)));
    });
  return tryHost("127.0.0.1").then((free) => free && tryHost("::1"));
}

function readState(): State | undefined {
  try {
    return JSON.parse(readFileSync(statePath, "utf8")) as State;
  } catch {
    return undefined;
  }
}

function readLogText(): string {
  return existsSync(logPath) ? stripAnsi(readFileSync(logPath, "utf8")) : "";
}

function appRunning(): boolean {
  return (devApps().get(checkout) ?? []).some((p) => DEBUG_APP.test(p.command));
}

async function start(timeout: number): Promise<number> {
  if (!existsSync(join(appDir, "node_modules"))) {
    console.log("Installing the app's npm packages (npm ci)…");
    const install = spawnSync("npm", ["ci"], { cwd: appDir, stdio: ["ignore", "inherit", "inherit"] });
    if (install.status !== 0) return fail("npm ci failed.");
  }

  for (const [where, procs] of devApps()) {
    const whose = where === checkout ? "this checkout's app" : `the Uta app from ${describe(where)}`;
    console.log(`Stopping ${whose} (pids ${procs.map((p) => p.pid).join(", ")}).`);
    const left = await stopAll(procs);
    if (left.length > 0) return fail(`Couldn't stop pids ${left.map((p) => p.pid).join(", ")}.`);
  }

  const port = await pickPort(isFree);
  if (port === undefined) return fail("No free port pair between 1420 and 1499.");

  mkdirSync(stateDir, { recursive: true });
  const log = openSync(logPath, "w");
  const devUrl = `http://localhost:${port}`;
  const child = spawn(
    process.execPath,
    [join(appDir, "node_modules", ".bin", "tauri"), "dev", "--config", JSON.stringify({ build: { devUrl } })],
    // Detached: its own session, so it outlives the shell that ran this.
    { cwd: appDir, detached: true, stdio: ["ignore", log, log], env: { ...process.env, UTA_DEV_PORT: String(port) } },
  );
  child.unref();
  closeSync(log);
  if (child.pid === undefined) return fail("Couldn't start tauri dev.");

  const state: State = {
    pid: child.pid,
    port,
    branch: git("branch", "--show-current") || "(detached)",
    commit: git("rev-parse", "--short", "HEAD"),
    startedAt: new Date().toISOString(),
  };
  writeFileSync(statePath, `${JSON.stringify(state, null, 2)}\n`);
  console.log(`Starting tauri dev on port ${port} (pid ${child.pid}). Log: ${shortPath(logPath)}`);
  return waitFor(state, timeout);
}

async function waitFor(state: State, timeout: number): Promise<number> {
  const began = Date.now();
  const seconds = () => Math.round((Date.now() - began) / 1000);
  let lastReport = 0;
  let settled = false;
  for (;;) {
    const text = readLogText();
    const { ready, failure, progress } = readLog(text);
    const up = ready && appRunning();
    const quit = settled && !up ? "The app quit as it started." : undefined;
    if (failure || quit || !alive(state.pid)) {
      console.error(`\n${failure ?? quit ?? "tauri dev exited."} The last 30 lines of the log:\n`);
      console.error(tail(text, 30));
      await stopAll(devApps().get(checkout) ?? []);
      rmSync(statePath, { force: true });
      console.error("\nStopped what it started.");
      return 1;
    }
    if (up && !settled) {
      // Once more after a moment, so an app that quits on launch isn't reported as open.
      settled = true;
      await sleep(2000);
      continue;
    }
    if (up) {
      console.log(`The app is open: ${state.branch} at ${state.commit}, port ${state.port}, pid ${state.pid} (${seconds()}s).`);
      return 0;
    }
    if (seconds() >= timeout) {
      console.error(`Not open after ${timeout}s (${progress}). It's still running: scripts/dev-app wait, or status.`);
      return 2;
    }
    if (seconds() - lastReport >= 10) {
      lastReport = seconds();
      console.log(`  ${lastReport}s  ${progress}`);
    }
    await sleep(500);
  }
}

async function wait(timeout: number): Promise<number> {
  const state = readState();
  if (!state || !alive(state.pid)) return fail("Nothing is starting: run scripts/dev-app start.");
  return waitFor(state, timeout);
}

function status(): number {
  const apps = devApps();
  const mine = apps.get(checkout) ?? [];
  const state = readState();
  for (const [where, procs] of apps) {
    if (where !== checkout) console.log(`Also running: the Uta app from ${describe(where)} (pids ${procs.map((p) => p.pid).join(", ")}).`);
  }
  if (mine.length === 0) {
    console.log("Not running from this checkout.");
    return 1;
  }

  const ours = state && mine.some((p) => p.pid === state.pid) ? state : undefined;
  if (ours) {
    const minutes = Math.round((Date.now() - Date.parse(ours.startedAt)) / 60000);
    console.log(`Running: ${ours.branch} at ${ours.commit}, port ${ours.port}, pid ${ours.pid}, started ${minutes} min ago.`);
    const head = git("rev-parse", "--short", "HEAD");
    if (head !== ours.commit) {
      console.log(`HEAD is now ${head}. tauri dev reloads changes by itself; scripts/dev-app start for a clean start.`);
    }
  } else {
    console.log(`Running, but not started by scripts/dev-app (pids ${mine.map((p) => p.pid).join(", ")}).`);
  }
  console.log(appRunning() ? "The app window's process is up." : "The app's process isn't up: still building, or the build failed.");

  const errors = ours ? errorLines(readLogText()) : [];
  if (errors.length === 0) {
    if (ours) console.log("No errors in the log since it started.");
  } else {
    console.log(`${errors.length} error lines in the log since it started. The last ones:`);
    console.log(errors.slice(-5).join("\n"));
  }
  return 0;
}

async function stop(): Promise<number> {
  const mine = devApps().get(checkout) ?? [];
  rmSync(statePath, { force: true });
  if (mine.length === 0) {
    console.log("Nothing running from this checkout.");
    return 0;
  }
  const left = await stopAll(mine);
  if (left.length > 0) return fail(`Couldn't stop pids ${left.map((p) => p.pid).join(", ")}.`);
  console.log(`Stopped this checkout's app (pids ${mine.map((p) => p.pid).join(", ")}).`);
  return 0;
}

function fail(message: string): number {
  console.error(message);
  return 1;
}

async function main(args: string[]): Promise<number> {
  const [command] = args;
  const at = args.indexOf("--timeout");
  const timeout = at === -1 ? 480 : Number(args[at + 1]);
  if (!command || args.includes("--help") || args.includes("-h")) {
    console.log(HELP);
    return command ? 0 : 1;
  }
  if (!Number.isFinite(timeout) || timeout <= 0) return fail("--timeout takes a number of seconds.");
  switch (command) {
    case "start":
      return start(timeout);
    case "wait":
      return wait(timeout);
    case "status":
      return status();
    case "stop":
      return stop();
    default:
      console.error(`Unknown command: ${command}\n\n${HELP}`);
      return 1;
  }
}

process.exitCode = await main(process.argv.slice(2));
