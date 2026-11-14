// @vitest-environment node
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { STALE_MINUTES, give, holder, lockDir, take } from "./screen";

let root: string;
let dir: string;
beforeEach(() => {
  root = mkdtempSync(join(tmpdir(), "uta-screen-"));
  dir = join(root, "screen.lock");
});
afterEach(() => rmSync(root, { recursive: true, force: true }));

const at = (minutes: number) => new Date(Date.UTC(2026, 10, 14, 12, minutes));

describe("the screen lock", () => {
  it("lives in ~/.uta/screen.lock, or UTA_SCREEN_LOCK", () => {
    expect(lockDir({})).toMatch(/\.uta\/screen\.lock$/);
    expect(lockDir({ UTA_SCREEN_LOCK: "/elsewhere" })).toBe("/elsewhere");
  });

  it("goes to one session at a time, and to the next once it's given back", () => {
    expect(take(dir, "UTA-105", at(0))).toEqual({ taken: true });
    expect(take(dir, "UTA-106", at(1))).toEqual({ taken: false, holder: { who: "UTA-105", since: at(0).toISOString() } });
    expect(give(dir, "UTA-105")).toBe("Gave the screen back (it was UTA-105's).");
    expect(holder(dir)).toBeUndefined();
    expect(take(dir, "UTA-106", at(2))).toEqual({ taken: true });
  });

  it("renews the lock for its holder, which keeps it from going stale", () => {
    take(dir, "UTA-105", at(0));
    take(dir, "UTA-105", at(STALE_MINUTES - 1));
    expect(take(dir, "UTA-106", at(STALE_MINUTES + 1)).taken).toBe(false);
  });

  it("takes over a stale lock, saying whose it was", () => {
    take(dir, "UTA-101", at(0));
    expect(take(dir, "UTA-106", at(STALE_MINUTES + 1))).toEqual({ taken: true, tookOver: { who: "UTA-101", since: at(0).toISOString() } });
    expect(holder(dir)?.who).toBe("UTA-106");
  });

  it("only gives back its holder's lock, unless forced", () => {
    take(dir, "UTA-105", at(0));
    expect(() => give(dir, "UTA-106")).toThrow("The screen is UTA-105's, not UTA-106's. Pass --force to take it back anyway.");
    expect(give(dir, "UTA-106", true)).toBe("Gave the screen back (it was UTA-105's).");
    expect(give(dir, "UTA-106")).toBe("Nobody had the screen.");
  });
});
