// The parts of `scripts/dev-app` that only read text: the `tauri dev` log, the
// process table and the port search. devAppCli.ts does the starting and
// stopping.

/** Removes terminal colour and cursor codes, and turns carriage returns (cargo's progress bar) into line breaks. */
export function stripAnsi(text: string): string {
  // eslint-disable-next-line no-control-regex -- the escape character is what's being matched
  return text.replace(/\x1b\[[0-9;?]*[A-Za-z]/g, "").replace(/\r\n?/g, "\n");
}

/** What a `tauri dev` log (colour stripped) says about the build. */
export type LogState = {
  /** Cargo has launched the app. */
  ready: boolean;
  /** Why the start failed, if it did. */
  failure?: string;
  /** A few words on where the build is, for progress lines. */
  progress: string;
};

// Cargo prints this as it launches the built app: "Running `…/uta-app`".
const READY = /Running `[^`]*\/uta-app`/;

// `tauri dev` keeps running after a failed build, waiting for the next change,
// so these lines are the only sign of a failure.
const FAILURES: { pattern: RegExp; reason: string }[] = [
  { pattern: /^error: could not compile/m, reason: "The Rust build failed." },
  { pattern: /error when starting dev server/, reason: "Vite failed to start." },
  { pattern: /Port \d+ is already in use/, reason: "Vite's port was taken." },
  { pattern: /^\s*Error /m, reason: "tauri dev reported an error." },
];

export function readLog(text: string): LogState {
  const ready = READY.test(text);
  const failure = FAILURES.find(({ pattern }) => pattern.test(text))?.reason;
  return { ready, failure, progress: progressOf(text) };
}

function progressOf(text: string): string {
  if (READY.test(text)) return "launching the app";
  const building = [...text.matchAll(/Building \[[^\]]*\] (\d+)\/(\d+)/g)].at(-1);
  if (building) return `compiling ${building[1]}/${building[2]}`;
  if (text.includes("Running DevCommand")) return "starting cargo";
  return "starting Vite";
}

/** Lines in the log that report an error: Rust's, Vite's or the Tauri CLI's. */
export function errorLines(text: string): string[] {
  return text
    .split("\n")
    .filter((line) => /^error(\[E\d+\])?:|^\s*Error |\[vite\].*error/i.test(line));
}

/** The last `count` lines of the log, without blank ones or cargo's progress bar. */
export function tail(text: string, count: number): string {
  return text
    .split("\n")
    .filter((line) => line.trim() !== "" && !/^\s*Building \[/.test(line))
    .slice(-count)
    .join("\n");
}

export type Proc = { pid: number; ppid: number; command: string };

/** Parses `ps -axww -o pid=,ppid=,command=`. */
export function parsePs(output: string): Proc[] {
  const procs: Proc[] = [];
  for (const line of output.split("\n")) {
    const match = /^\s*(\d+)\s+(\d+)\s+(.*)$/.exec(line);
    if (match) procs.push({ pid: Number(match[1]), ppid: Number(match[2]), command: match[3] });
  }
  return procs;
}

/** The checkout's processes, or `UNKNOWN` for a Uta dev app whose checkout can't be told. */
export const UNKNOWN = "(unknown checkout)";

/**
 * Every running Uta dev app, by checkout: `tauri dev`, Vite and the debug
 * `uta-app`, with everything they started (cargo, esbuild). A process counts
 * only when its command names one of `checkouts` (or is a debug `uta-app`
 * binary), so another project's dev server is never included.
 */
export function findDevApps(procs: Proc[], checkouts: string[]): Map<string, Proc[]> {
  const byPid = new Map(procs.map((proc) => [proc.pid, proc]));
  const owner = new Map<number, string>();

  // Longest first, so a worktree inside the main checkout's folder wins.
  const sorted = [...checkouts].sort((a, b) => b.length - a.length);
  const checkoutIn = (command: string) =>
    sorted.find(
      (checkout) =>
        command.includes(`${checkout}/app/node_modules/.bin/`) ||
        command.includes(`${checkout}/target/`),
    );

  for (const proc of procs) {
    if (!isDevProcess(proc.command)) continue;
    let checkout = checkoutIn(proc.command);
    // A `tauri dev` started by a relative path, or an app built into a shared
    // target folder: look for an ancestor that names the checkout.
    for (let up = byPid.get(proc.ppid); !checkout && up; up = byPid.get(up.ppid)) {
      if (isDevProcess(up.command)) checkout = checkoutIn(up.command);
    }
    if (!checkout && !DEBUG_APP.test(proc.command)) continue;
    owner.set(proc.pid, checkout ?? UNKNOWN);
    // The `tauri dev` above a matched process, which may not name a path.
    for (let up = byPid.get(proc.ppid); up; up = byPid.get(up.ppid)) {
      if (TAURI_DEV.test(up.command) && !owner.has(up.pid)) owner.set(up.pid, checkout ?? UNKNOWN);
    }
  }

  // Everything below a matched process belongs to the same app.
  const children = new Map<number, Proc[]>();
  for (const proc of procs) children.set(proc.ppid, [...(children.get(proc.ppid) ?? []), proc]);
  const queue = [...owner.keys()];
  while (queue.length > 0) {
    const pid = queue.shift()!;
    for (const child of children.get(pid) ?? []) {
      if (owner.has(child.pid)) continue;
      owner.set(child.pid, owner.get(pid)!);
      queue.push(child.pid);
    }
  }

  const apps = new Map<string, Proc[]>();
  for (const [pid, checkout] of owner) {
    apps.set(checkout, [...(apps.get(checkout) ?? []), byPid.get(pid)!]);
  }
  for (const list of apps.values()) list.sort((a, b) => a.pid - b.pid);
  return apps;
}

const TAURI_DEV = /node_modules\/\.bin\/tauri dev(\s|$)/;
/** A debug build of the app, in this checkout's target folder or a shared one. */
export const DEBUG_APP = /\/debug\/uta-app(\s|$)/;

function isDevProcess(command: string): boolean {
  return (
    TAURI_DEV.test(command) ||
    // `.bin/vite`, not `.bin/vitest`: another session's tests aren't an app.
    /node_modules\/\.bin\/vite(\s|$)/.test(command) ||
    DEBUG_APP.test(command)
  );
}

/** Paths from `git worktree list --porcelain`. */
export function parseWorktrees(output: string): string[] {
  return output
    .split("\n")
    .filter((line) => line.startsWith("worktree "))
    .map((line) => line.slice("worktree ".length));
}

/**
 * The first port from `from` whose next port is free too: Vite takes the
 * port, and its hot-reload socket the one above when TAURI_DEV_HOST is set.
 */
export async function pickPort(
  isFree: (port: number) => Promise<boolean>,
  from = 1420,
  to = 1499,
): Promise<number | undefined> {
  for (let port = from; port < to; port++) {
    if ((await isFree(port)) && (await isFree(port + 1))) return port;
  }
  return undefined;
}
