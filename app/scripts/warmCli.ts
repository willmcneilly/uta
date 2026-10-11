// The command line for `scripts/warm`, which runs it with Node. The work is
// in warm.ts.

import { topLevel } from "./git.ts";
import { warm } from "./warm.ts";

const HELP = `Usage: scripts/warm

Gives this worktree a warm Rust build by cloning the main checkout's target/
with APFS copy-on-write (cp -c -R). The clone takes no extra disk until cargo
changes a file, and cargo then rebuilds only what differs: usually just the
workspace's own crates.

It clones only the parts of target/ this worktree doesn't have yet (debug,
release, and so on), so it never touches a build that's already here. The
post-checkout hook runs it for every new worktree, and scripts/check runs it
before the Rust jobs.
`;

if (process.argv.includes("--help") || process.argv.includes("-h")) console.log(HELP);
else warm(topLevel(process.cwd()));
