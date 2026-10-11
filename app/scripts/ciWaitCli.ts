// The command line for `scripts/ci-wait`, which runs it with Node. The work
// is in ciWait.ts.

import { setTimeout as sleep } from "node:timers/promises";
import { ciWait, parseArgs } from "./ciWait.ts";
import { checkRuns, pull, pullHead } from "./github.ts";

const HELP = `Usage: scripts/ci-wait <pr> [--commit <sha>] [--timeout <minutes>]

Waits until CI has finished on the PR's head commit, prints a line per check
(its result, the commit it ran on, its time and link), and exits:

  0  every check passed on that commit
  1  a check failed (it stops at the first failure, without waiting for the rest)
  2  the checks aren't for that commit: none started within 90 seconds of
     waiting, or the PR's head moved while it waited
  3  it timed out with CI still running: run it again to keep waiting

  --commit <sha>      wait for this commit's checks instead of the head's
  --timeout <min>     give up after this long (default 9, under the shell's
                      10-minute limit)
  --help              show this
`;

async function main() {
  const args = parseArgs(process.argv.slice(2));
  if (args === "help") {
    console.log(HELP);
    return;
  }
  if (typeof args === "string") {
    console.error(`${args}\n\n${HELP}`);
    process.exitCode = 64;
    return;
  }
  const commits = () => pull(args.pr).commits.map((commit) => commit.oid);
  // A short SHA has to be widened to match the check runs' full one.
  const commit = args.commit && commits().find((sha) => sha.startsWith(args.commit ?? ""));
  if (args.commit && !commit) {
    console.error(`${args.commit} isn't one of PR #${args.pr}'s commits.`);
    process.exitCode = 64;
    return;
  }
  process.exitCode = await ciWait(
    args.pr,
    { commit, timeoutMs: args.timeoutMinutes * 60_000, graceMs: 90_000, intervalMs: 15_000 },
    {
      head: () => pullHead(args.pr),
      commits,
      runs: checkRuns,
      sleep: (ms) => sleep(ms),
      now: () => Date.now(),
      print: (text) => console.log(text),
    },
  );
}

await main();
