// The command line for `scripts/mutate`, which runs it with Node. The work is
// in mutate.ts, and the named mutations in mutations.ts.

import { resolve } from "node:path";
import { topLevel } from "./git.ts";
import { USAGE, mutate } from "./mutate.ts";
import { MUTATIONS, named } from "./mutations.ts";

const HELP = `Usage: scripts/mutate <file> <find> <replace> -- <command...>
       scripts/mutate --named <name> [-- <command...>]
       scripts/mutate --list

Breaks the code on purpose to prove a test can fail: replaces <find> with
<replace> in <file> (the text must occur exactly once), runs the command,
then puts the file back, even when the run is interrupted. A run that was
killed outright is put right by the next run.

It prints the command's output, then one of:

  CAUGHT       the command failed: exit 0
  MISSED       the command passed with the code broken: exit 1
  DIDN'T BUILD cargo couldn't compile the mutated code, so no test judged it:
               exit 2
  INTERRUPTED  the file is restored: exit 130

The file and the command are relative to where you run it; a named mutation's
file is relative to the top of the checkout. Pass a multi-line <find> with
the shell's $'...' quoting. --named runs the mutation's own tests when no
command is given.

  --named <name>  use a standard mutation (--list shows them)
  --list          list the standard mutations
  --help          show this
`;

function main(argv: readonly string[]): Promise<number> | number {
  if (argv.includes("--help") || argv.includes("-h")) {
    console.log(HELP);
    return 0;
  }
  if (argv[0] === "--list") {
    for (const mutation of MUTATIONS) {
      console.log(`${mutation.name}  ${mutation.what}\n  ${mutation.file}; runs ${mutation.command.join(" ")}`);
    }
    return 0;
  }
  const dash = argv.indexOf("--");
  const before = dash === -1 ? argv : argv.slice(0, dash);
  const command = dash === -1 ? [] : argv.slice(dash + 1);
  const root = topLevel(process.cwd());
  const run = { root, cwd: process.cwd() };
  if (before[0] === "--named") {
    const mutation = named(before[1] ?? "");
    if (!mutation || before.length !== 2) {
      console.error(`Name one of: ${MUTATIONS.map((m) => m.name).join(", ")}\n\n${HELP}`);
      return USAGE;
    }
    const { file, find, replace } = mutation;
    return execute({ ...run, file: resolve(root, file), find, replace, command: command.length ? command : mutation.command });
  }
  if (before.length !== 3 || command.length === 0) {
    console.error(HELP);
    return USAGE;
  }
  const [file, find, replace] = before;
  return execute({ ...run, file: resolve(process.cwd(), file), find, replace, command });
}

async function execute(run: Parameters<typeof mutate>[0]): Promise<number> {
  try {
    const outcome = await mutate(run);
    console.log(outcome.verdict);
    return outcome.code;
  } catch (error) {
    console.error(`Couldn't run ${run.command[0]}: ${(error as Error).message}. The file is restored.`);
    return USAGE;
  }
}

process.exitCode = await main(process.argv.slice(2));
