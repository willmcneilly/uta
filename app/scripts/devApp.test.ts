// @vitest-environment node
import { describe, expect, it } from "vitest";
import {
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

const MAIN = "/Users/will/Sites/uta";
const TREE = "/Users/will/Sites/uta/.claude/worktrees/a";
const SHARED = "/Users/will/Sites/uta/.claude/worktrees/b";
const OTHER = "/Users/will/Sites/deadwax";

describe("stripAnsi", () => {
  it("removes colour and cursor codes and splits cargo's progress bar", () => {
    const raw = "\x1b[1m\x1b[32m   Compiling\x1b[0m uta-core\r    Building [==> ] 3/9\x1b[K\r\n";
    expect(stripAnsi(raw)).toBe("   Compiling uta-core\n    Building [==> ] 3/9\n");
  });
});

describe("readLog", () => {
  const vite = "  VITE v8.3.1  ready in 406 ms\n     Running DevCommand (`cargo  run`)\n";

  it("is ready once cargo launches uta-app", () => {
    const log = `${vite}    Finished \`dev\` profile\n     Running \`${TREE}/target/debug/uta-app\`\n`;
    expect(readLog(log)).toEqual({ ready: true, failure: undefined, progress: "launching the app" });
  });

  it("reports the latest compile progress", () => {
    const log = `${vite}    Building [=> ] 12/370: syn\n    Building [==> ] 124/370: syn\n`;
    expect(readLog(log)).toMatchObject({ ready: false, progress: "compiling 124/370" });
  });

  it("says what it's waiting on before cargo starts", () => {
    expect(readLog("> vite\n").progress).toBe("starting Vite");
    expect(readLog(vite).progress).toBe("starting cargo");
  });

  // tauri dev keeps running after these, so the log is the only sign.
  it.each([
    ['error[E0308]: mismatched types\nerror: could not compile `uta-app` (bin "uta-app")', "The Rust build failed."],
    ["error when starting dev server:\nError: Port 1421 is already in use", "Vite failed to start."],
    ["       Error failed to get cargo metadata", "tauri dev reported an error."],
  ])("fails on %j", (log, reason) => {
    expect(readLog(log).failure).toBe(reason);
  });

  it("doesn't fail on a single Rust error before cargo gives up", () => {
    expect(readLog("error[E0308]: mismatched types\n").failure).toBeUndefined();
  });
});

describe("errorLines", () => {
  it("picks out Rust, Tauri and Vite errors, and nothing else", () => {
    const log = [
      "warning: unused variable",
      "error[E0308]: mismatched types",
      "error: could not compile `uta-app`",
      "       Error failed to rebuild",
      "7:41:02 PM [vite] Internal server error: Failed to resolve import",
      "  terror is a word",
    ].join("\n");
    expect(errorLines(log)).toHaveLength(4);
  });
});

describe("tail", () => {
  it("keeps the last lines that aren't blank or cargo's progress bar", () => {
    expect(tail("a\n\nb\n    Building [==> ] 369/370: uta-app(bin)\nc\n\n", 2)).toBe("b\nc");
  });
});

describe("parseWorktrees", () => {
  it("reads the paths from git's porcelain output", () => {
    const out = `worktree ${MAIN}\nHEAD abc\nbranch refs/heads/main\n\nworktree ${TREE}\nHEAD def\ndetached\n`;
    expect(parseWorktrees(out)).toEqual([MAIN, TREE]);
  });
});

describe("findDevApps", () => {
  // Shapes taken from real `ps -axww -o pid=,ppid=,command=` output.
  const ps = parsePs(
    [
      // This worktree's app, started by scripts/dev-app (tauri dev reparented to launchd).
      `  100     1 /usr/local/bin/node ${TREE}/app/node_modules/.bin/tauri dev --config {"build":{"devUrl":"http://localhost:1421"}}`,
      `  101   100 npm run dev`,
      `  102   101 sh -c vite`,
      `  103   102 node ${TREE}/app/node_modules/.bin/vite`,
      `  104   103 ${TREE}/app/node_modules/@esbuild/darwin-arm64/bin/esbuild --service=0.25.0 --ping`,
      `  105   100 ${TREE}/target/debug/uta-app`,
      // The main checkout's app, started by hand with a relative path.
      `  200    50 npm run tauri dev`,
      `  201   200 node node_modules/.bin/tauri dev`,
      `  202   201 node ${MAIN}/app/node_modules/.bin/vite`,
      `  203   201 ${MAIN}/target/debug/uta-app`,
      // Another project's dev app on the same machine.
      `  300    60 npm run tauri dev`,
      `  301   300 node ${OTHER}/app/node_modules/.bin/tauri dev`,
      `  302   301 node ${OTHER}/app/node_modules/.bin/vite`,
      `  303   301 ${OTHER}/target/debug/deadwax-app`,
      // Another session's tests in this worktree, and an editor open on it.
      `  400    70 node ${TREE}/app/node_modules/.bin/vitest run`,
      `  401    70 /Applications/Zed.app/Contents/MacOS/zed ${TREE}`,
      // An app built into a shared target folder, so only its tauri dev names the checkout.
      `  600     1 node ${SHARED}/app/node_modules/.bin/tauri dev`,
      `  601   600 cargo run --no-default-features --color always --`,
      `  602   601 /Users/will/Library/Caches/uta-target/debug/uta-app`,
      // An orphaned app whose tauri dev is gone, built into a shared target folder.
      `  500     1 /Users/will/Library/Caches/uta-target/debug/uta-app`,
    ].join("\n"),
  );
  const apps = findDevApps(ps, [MAIN, TREE, SHARED]);
  const pids = (checkout: string) => apps.get(checkout)?.map((p) => p.pid);

  it("finds a checkout's whole tree, from tauri dev down", () => {
    expect(pids(TREE)).toEqual([100, 101, 102, 103, 104, 105]);
  });

  it("finds an app started by hand, including the tauri dev that names no path", () => {
    // npm run tauri dev (200) is above it and exits by itself.
    expect(pids(MAIN)).toEqual([201, 202, 203]);
  });

  it("leaves other projects, test runs and editors alone", () => {
    const all = [...apps.values()].flat().map((p) => p.pid);
    for (const pid of [300, 301, 302, 303, 400, 401]) expect(all).not.toContain(pid);
  });

  it("keeps a debug uta-app it can't place, under an unknown checkout", () => {
    expect(pids(UNKNOWN)).toEqual([500]);
  });

  it("places an app in a shared target folder by the tauri dev above it", () => {
    expect(pids(SHARED)).toEqual([600, 601, 602]);
  });
});

describe("pickPort", () => {
  const taken = (busy: number[]) => async (port: number) => !busy.includes(port);

  it("takes 1420 when it and 1421 are free", async () => {
    expect(await pickPort(taken([]))).toBe(1420);
  });

  it("moves past a held port, needing the one above it free too", async () => {
    expect(await pickPort(taken([1420]))).toBe(1421);
    expect(await pickPort(taken([1420, 1422]))).toBe(1423);
  });

  it("gives up when nothing in the range is free", async () => {
    expect(await pickPort(taken([1420, 1421, 1422]), 1420, 1423)).toBeUndefined();
  });
});
