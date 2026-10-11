// The command line for `scripts/review-setup`, which runs it with Node. The
// work is in reviewSetup.ts.

import { parseArgs, reviewSetup } from "./reviewSetup.ts";

const HELP = `Usage: scripts/review-setup <pr> [--round <r>] [--commit <sha>]

Everything a review round starts from, in one go:
- the PR's title, branch, the commit under review, the ticket's Notion page ID;
- the review worktree, at a fixed path in the main checkout
  (.claude/worktrees/review-pr-<pr>), created at the commit or reset to it.
  Later rounds reuse it, so its build stays warm; scripts/check, scripts/mutate
  and the tests run there. Its local changes are discarded; target/ and
  node_modules/ are kept. tidy removes it after the merge;
- the diff stat against the merge base with main and, when there's an earlier
  review, against the commit it reviewed; which areas changed, the local
  checks that cover them, and any golden WAVs that changed;
- CI on the commit, a line per check;
- the PR body, every earlier review with its inline comments and their
  replies threaded, and the conversation's comments.

  --round <r>     the review round (1 by default)
  --commit <sha>  review the PR as it was at this commit: leaves out the
                  reviews, replies and body edits recorded from when it was
                  first reviewed or the next push on
  --help          show this
`;

const args = parseArgs(process.argv.slice(2));
if (args === "help") {
  console.log(HELP);
} else if (typeof args === "string") {
  console.error(`${args}\n\n${HELP}`);
  process.exitCode = 64;
} else {
  try {
    console.log(reviewSetup(process.cwd(), args));
  } catch (error) {
    console.error((error as Error).message);
    process.exitCode = 1;
  }
}
