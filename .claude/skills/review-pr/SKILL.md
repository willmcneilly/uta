---
name: review-pr
description: Get an independent review of a Uta ticket's PR when Will asks for one by hand. Use when Will says /review-pr, "review UTA-n" or "review PR #n". Runs the review-round workflow, which starts a fresh pr-reviewer agent, and reports its verdict. Reviews normally happen inside the work-ticket review loop, without Will asking.
---

# Reviewing a PR

Reviews normally run inside the `work-ticket` review loop: the implementer starts each round itself, and Will hears only the outcome. Use this skill when Will asks for a review directly, for example of a PR that was opened before the loop existed, or for a second opinion.

The review itself is in the `pr-reviewer` agent (`.claude/agents/pr-reviewer.md`). It's started only by the `review-round` workflow (`.claude/workflows/review-round.js`), which builds the reviewer's prompt from the PR number alone. That's what keeps the reviewer independent: nobody can brief it.

## Steps

1. **Find the PR number,** from what Will said or from the ticket's `PR` property.
2. **Work out the round.** If the PR already has reviews starting `🤖 Claude (review`, this is a re-review: count them and add one.
3. **Run the workflow:** the Workflow tool with `name: "review-round"` and `args` `{"pr": <n>}`, or `{"pr": <n>, "round": <r>}`. Add nothing else. If you implemented this PR in this session, that's fine, because the workflow is what keeps the reviewer separate.
4. **Tell Will:**
   - the verdict, with the review link;
   - the blocking findings in plain language;
   - what needs him, from the result's `for_will` list and `golden_changed` flag, keeping the High and Low ratings. If nothing is High, say so.

**Don't fix the code here.** If the review asks for changes, the implementer fixes them (the `work-ticket` skill). **Don't merge either.** Merging happens on Will's go-ahead, following step 10 of `work-ticket`.
