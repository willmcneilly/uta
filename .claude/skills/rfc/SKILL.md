---
name: rfc
description: Write an RFC for Uta with Will. Use when Will wants to start a new initiative, says "let's write an RFC" or /rfc, or when a proposed piece of work is big enough to need one. Covers clarifying the goal, researching, drafting the RFC as a local markdown file, iterating on Will's feedback in Crit, and pushing to Notion on sign-off.
effort: high
---

# Writing an RFC

An RFC is how every big initiative in Uta starts. Its job is to let Will make a good decision about something they may not understand at the code level. Write for Will: a product person who knows DAWs deeply and audio programming not at all.

Workflow: draft locally → review in Crit → publish to Notion on sign-off. For Notion mechanics (wrapper, IDs, property names), use the `notion` skill.

## Does this need an RFC?

**Yes** if it adds a capability Will will notice, sets or changes an architectural decision, adds a significant dependency, or will take more than one project's worth of tickets.

**No** for bug fixes, small improvements inside an existing project's scope, or refactors that don't change a decision recorded in an RFC. Say so and suggest a ticket instead.

## Steps

### 1. Understand the goal. Don't draft yet.

First run the scratchpad review (the `scratchpad` skill), starting with the notes that touch this initiative. Notes Will folds in become part of what you clarify below.

Then run a **process check-in.** Query the Process log (see the `notion` skill) for entries that are `Trying` or `Proposed`. For each `Trying` entry, gather evidence from the project just finished: PR timings, how often Will had to step in, what went wrong. Propose a new status (Working, Reverted, or keep Trying) with a one-line reason. Also ask Will whether anything about the process got in his way. Then add a dated bullet under `## How it's going` on each entry he agrees with, and change its status. Keep it to a few minutes. Will can also ask for a check-in at any time.

Ask Will questions until you can state, in Will's words:
- what they want to be able to do, hear or see when this is done
- why now, and what it unlocks
- what's explicitly **not** part of it
- any strong opinions they already hold about how it should work

Ask a few focused questions at a time, not a questionnaire. Stop when you could write "What it looks like to you" and Will would agree with it.

### 2. Research

- Read the accepted RFCs (`docs/rfcs/`, and Notion for anything not in the repo) and the relevant code and `CLAUDE.md`, so the proposal fits what already exists.
- Where it helps, look at how other DAWs and audio projects solve this, and at libraries (maturity, licence, maintenance).
- Get each alternative to the point where you could honestly say why it loses. Don't invent straw men.

If the research changes the scope from step 1, tell Will before drafting.

Research worth keeping goes in Notion's **Research** database (see the `notion` skill), related to the RFC it served. Examples: findings the RFC only summarises, or work for a later RFC that this one only outlines. Research doesn't go in the repo. Save it when the RFC is first pushed, or sooner if scope moves part of it to another RFC.

### 3. Draft locally

The **local markdown file is the working copy**. Notion is where the RFC is published, and it's only updated when Will asks (see "Pushing to Notion").

Write `docs/rfcs/rfc-NNN-short-name.md`. For `NNN`, take the highest RFC number in Notion or in `docs/rfcs/` and add one; Notion assigns the real number when the RFC is first pushed. Start the file with:

```markdown
# RFC-NNN: <Name>

**Status:** Draft · **Author:** Will · **Notion:** not yet published
```

`Name` is a short noun phrase (e.g. "Session view clip launching"). Then the sections, in this order:

- **Summary.** Three to five sentences. Someone reading only this knows what's proposed.
- **Motivation.** Why this, why now, what it unlocks.
- **Proposal.** The approach. Explain any technical concept on first use in a sentence Will can follow. Include a diagram only if it shows something prose can't.
- **What it looks like to you.** The result in product terms: what Will does, sees and hears. No code terms.
- **Alternatives considered.** Each with why it wasn't chosen.
- **Risks & unknowns.** What could go wrong or turn out harder than expected, and what we'd do about it. Mark anything that is a guess as a guess.
- **How we'll verify it.** Concrete checks, both automated (what is tested, how) and manual (what Will should try). "Add tests" isn't enough.
- **Open questions.** Only real decisions for Will, each with your recommendation. Once one is settled, mark it `**Resolved: ...**` with the answer. Don't delete it, so the record stays.
- **New terms.** The concepts this RFC brings into Uta that aren't in the Glossary yet (query it first; see the `notion` skill). One line each: the term and a plain one-sentence definition. Will reads these alongside the proposal, so they're part of the review. Leave the section out if there are none.

Quality bar before sharing:
- The scope fits one project. If it doesn't, propose splitting it into several RFCs.
- There's no filler. A short RFC is fine when the topic is simple.
- Every claim about a library or technique comes from the research, not memory.

Give Will a two or three line summary of what's in it and what you need from them (usually the open questions), then start the review.

### 4. Iterate with Crit

Review happens in Crit, a local browser tool for leaving inline comments. Use the `crit:crit` skill for the loop. In short:
- Run `crit docs/rfcs/<file>.md` **in the background** and give Will the URL it prints.
- Wait for it to finish; it ends when Will clicks Finish Review. Don't read comments early.
- Address each comment by editing the file with Edit. Crit reloads live.
- Reply to every comment: `crit comment --reply-to <id> --author 'Claude Code' '...'`, or in bulk with `--json`. Say what you changed, or why you didn't. **Never resolve comments;** that's Will's call.
- Run `crit` again for the next round.

Also:
- Update `**Status:**` in the file to `In Discussion` once Will starts responding.
- Feedback can also arrive in chat. Treat it the same way.
- A review round that finishes with no comments counts as approval of the text, **not** acceptance of the RFC. Acceptance is step 5.

### 5. Stop at the decision

When the open questions are resolved, ask Will whether they want to accept it. Only Will accepts an RFC.
- **Accepted:** set `**Status:** Accepted (YYYY-MM-DD)` in the file, then push to Notion with `Status: Accepted` and `Date accepted` set to today.
  Then add a Glossary entry for each item under **New terms**, with `Introduced in` set to this RFC and the full body the `notion` skill describes, and link it to related terms already in the Glossary.
- **Rejected:** set it to `Rejected`, add a line at the top saying why, and push.

**Don't** break an accepted RFC into projects or tickets as part of this skill. That's a separate step, and Will starts it.

## Pushing to Notion

Push only on acceptance or rejection, or when Will asks. Never push on your own initiative mid-discussion.

- **Body:** everything below the local header. The title and status are Notion properties, not body text.
- **First push:** create the row in the RFCs data source with `Name`, `Status` and `Author: Will`, plus the body as `markdown` (see the `notion` skill). Then put the Notion link and the real `RFC-n` number in the local header, and rename the file if the number changed.
- **Later pushes:**
  1. First run `python3 .claude/skills/rfc/comments.py <page id>`. If Will has commented in Notion, bring those comments into the local file or the discussion, because the push deletes them.
  2. Then `N pages update <id>`, which replaces the whole body, and PATCH the properties.
- **After every push:** set any code blocks to `plain text` (see the `notion` skill), and tell Will it's published.

## Changing an accepted RFC

Small corrections found during implementation: add a dated note under a `## Amendments` heading at the end, and tell Will. If the core approach changes, write a new RFC that supersedes the old one, and set the old one to `Superseded`.
