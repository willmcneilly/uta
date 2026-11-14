// The screen lock: one session at a time drives the Mac's screen with
// computer use, so parallel tickets don't click into each other's windows.
// The command line is in screenCli.ts.
//
// The lock is a folder (mkdir is atomic, so two sessions can't both take it)
// holding owner.json: who has it and since when. A lock older than its stale
// limit is from a session that never gave it back, and is taken over.

import { mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";

/** How long a lock lasts without being taken again before anyone may take it over. */
export const STALE_MINUTES = 20;

export type Owner = { who: string; since: string };

/** The lock's folder: $UTA_SCREEN_LOCK, or ~/.uta/screen.lock. */
export function lockDir(env: NodeJS.ProcessEnv = process.env): string {
  return env.UTA_SCREEN_LOCK || join(homedir(), ".uta", "screen.lock");
}

/** Who holds the lock, or undefined when nobody does. */
export function holder(dir: string): Owner | undefined {
  try {
    return JSON.parse(readFileSync(join(dir, "owner.json"), "utf8")) as Owner;
  } catch {
    return undefined;
  }
}

const isStale = (owner: Owner | undefined, now: Date, staleMinutes: number) =>
  !owner || now.getTime() - new Date(owner.since).getTime() > staleMinutes * 60_000;

/**
 * Takes the lock for `who`, or says who has it. Taking it again as its holder
 * renews it; a stale lock (or one with no owner, left half-made) is taken over.
 */
export function take(dir: string, who: string, now: Date = new Date(), staleMinutes = STALE_MINUTES): { taken: true; tookOver?: Owner } | { taken: false; holder: Owner } {
  const write = () => writeFileSync(join(dir, "owner.json"), JSON.stringify({ who, since: now.toISOString() }) + "\n");
  try {
    mkdirSync(join(dir, ".."), { recursive: true });
    mkdirSync(dir);
    write();
    return { taken: true };
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code !== "EEXIST") throw error;
  }
  const owner = holder(dir);
  if (owner?.who === who) {
    write();
    return { taken: true };
  }
  if (isStale(owner, now, staleMinutes)) {
    write();
    return owner ? { taken: true, tookOver: owner } : { taken: true };
  }
  return { taken: false, holder: owner as Owner };
}

/** Gives the lock back. Only its holder can, unless `force`. */
export function give(dir: string, who: string, force = false): string {
  const owner = holder(dir);
  if (!owner) {
    rmSync(dir, { recursive: true, force: true });
    return "Nobody had the screen.";
  }
  if (owner.who !== who && !force) throw new Error(`The screen is ${owner.who}'s, not ${who}'s. Pass --force to take it back anyway.`);
  rmSync(dir, { recursive: true, force: true });
  return `Gave the screen back (it was ${owner.who}'s).`;
}
