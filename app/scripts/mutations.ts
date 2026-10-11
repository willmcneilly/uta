// The standard ways `scripts/mutate --named` breaks the code, so reviewers
// don't each invent them. Each `find` must match its file exactly once; a
// test checks that against the current code, so a refactor that moves one
// fails here rather than in a review.

export type Mutation = {
  name: string;
  /** What it breaks, and what should catch it. */
  what: string;
  /** Relative to the top of the checkout. */
  file: string;
  find: string;
  replace: string;
  /** The tests that should fail, run when no command is given. */
  command: readonly string[];
};

export const MUTATIONS: readonly Mutation[] = [
  {
    name: "rt-alloc",
    what: "allocates (and frees) at the top of Processor::process; the real-time tests' assert_no_alloc must abort",
    file: "crates/uta-engine/src/processor.rs",
    find: "        let started = Instant::now();\n",
    replace: "        let started = Instant::now();\n        std::hint::black_box(vec![0u8; 64]);\n",
    command: ["cargo", "test", "-p", "uta-engine", "--test", "realtime"],
  },
];

export const named = (name: string) => MUTATIONS.find((mutation) => mutation.name === name);
