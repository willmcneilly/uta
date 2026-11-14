// The command line for `scripts/screen`, which runs it with Node. The work is
// in screen.ts.

import { STALE_MINUTES, give, holder, lockDir, take } from "./screen.ts";

const HELP = `Usage: scripts/screen take <who> [--wait <minutes>] | give <who> [--force] | status

The screen lock: one session at a time uses computer use (screenshots, clicks)
on the Mac, so tickets running side by side don't drive each other's windows.
<who> names the session, such as UTA-105.

  take <who>    take the lock, or renew it if it's yours. If another session
                has it, wait for it (checking every 5 seconds) for up to
                --wait minutes (default 30), then give up. Run it in the
                background and carry on with other work: it exits when it
                has the lock.
  give <who>    give it back, as soon as you're done with the screen
  status        who has it, and since when

A lock not taken again for ${STALE_MINUTES} minutes is stale: the next take has it.
Take it again to keep it for longer.

The lock is ~/.uta/screen.lock, or $UTA_SCREEN_LOCK.

Exit status: 0 done, 1 not taken (waited too long) or not yours, 2 bad usage.`;

async function main(argv: string[]): Promise<number> {
  const [command, who, ...rest] = argv;
  if (!command || command === "--help" || command === "-h") {
    console.log(HELP);
    return command ? 0 : 2;
  }
  const dir = lockDir();
  if (command === "status") {
    const owner = holder(dir);
    console.log(owner ? `The screen is ${owner.who}'s, since ${owner.since}.` : "Nobody has the screen.");
    return 0;
  }
  if (!who || who.startsWith("--")) {
    console.error(`${command} needs <who>.\n\n${HELP}`);
    return 2;
  }
  if (command === "give") {
    try {
      console.log(give(dir, who, rest.includes("--force")));
      return 0;
    } catch (error) {
      console.error((error as Error).message);
      return 1;
    }
  }
  if (command !== "take") {
    console.error(`Unknown command: ${command}\n\n${HELP}`);
    return 2;
  }
  const at = rest.indexOf("--wait");
  const minutes = at === -1 ? 30 : Number(rest[at + 1]);
  if (!(minutes >= 0)) {
    console.error(`--wait needs a number of minutes.\n\n${HELP}`);
    return 2;
  }
  const until = Date.now() + minutes * 60_000;
  let said = false;
  for (;;) {
    const result = take(dir, who);
    if (result.taken) {
      console.log(`The screen is ${who}'s.${result.tookOver ? ` (Took over ${result.tookOver.who}'s stale lock.)` : ""} Give it back with: scripts/screen give ${who}`);
      return 0;
    }
    if (Date.now() >= until) {
      console.error(`The screen is still ${result.holder.who}'s (since ${result.holder.since}). Gave up after ${minutes} minutes.`);
      return 1;
    }
    if (!said) console.error(`The screen is ${result.holder.who}'s. Waiting for it…`);
    said = true;
    await new Promise((resolve) => setTimeout(resolve, 5000));
  }
}

process.exitCode = await main(process.argv.slice(2));
