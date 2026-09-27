---
name: teach
description: Answer Will's question about how Uta works, or why it's built the way it is, as a short, cited HTML explainer published as an Artifact. Use when Will says /teach, "teach me", "explain how X works", "why do we do X", "walk me through X", or asks any question about Uta's architecture, tools, decisions or code that deserves more than a chat reply. Covers scoping the question, researching code and decisions, verifying every claim, and publishing the explainer.
---

# Teach me

Uta is an experiment in whether Will can keep a grip on a codebase they don't read line by line. This skill keeps their mental model accurate. It answers a question about the project with a short explainer they can take in within minutes, and they can trust it because every claim points at its source.

## Who you're writing for

Will knows DAWs deeply as a user. Pitch each area differently:

- **TypeScript, React, the browser and Canvas: seasoned.** Don't explain these. Use them as the reference point for everything else.
- **Rust: new.** Explain each Rust idiom the first time it matters to the answer, in a sentence, and compare it to the TypeScript equivalent where there is one. For example:
  - `enum` with data is a discriminated union;
  - `Result` is a typed error return instead of a throw;
  - `Box<T>` is a value on the heap, behind a pointer;
  - `impl From<&A> for B` is a conversion function;
  - ownership and `std::mem::replace` mean swapping a value out without copying it;
  - `#[derive(...)]` and `#[serde(...)]` are generated code, a bit like decorators.
  
  Say why the idiom matters here, not just what it is. Skip idioms that don't affect the answer.
- **Audio programming: new.** Explain audio and real-time concepts (blocks, sample rate, dB and linear gain, why allocation clicks) the first time they appear.

Put a longer Rust aside in a collapsible section, so the main path stays short.

## The rules that make it trustworthy

- **Cite everything.** Every claim links to where it comes from: a `file:line`, an RFC section, a ticket, a PR or a commit. If you can't cite it, it doesn't go in.
- **Say what kind of knowledge each claim is.** Label each one:
  - **Decided:** an RFC, ticket or `CLAUDE.md` says so.
  - **Emerged:** the code does it, but nothing wrote it down.
  - **Drift:** the code and the docs disagree. Say exactly how.
  
  Drift is the most valuable thing an explainer can find. Never smooth it over.
- **Say what isn't built yet.** If the question assumes something that doesn't exist yet, the explainer says so up front, and says which ticket will build it.
- **Stamp it.** Each explainer shows the commit SHA and date it describes, so Will can tell when it's gone stale.
- **Never guess.** An explainer that sounds right but is wrong is worse than none, because it gives Will false confidence.

## Steps

### 1. Scope the question

Ask one round of questions with AskUserQuestion, no more than three. Skip any whose answer is obvious from how Will phrased it.

- **Angle:** *what* it is · *why* it's like this · *how* it works, step by step · *how we know* it works
- **Depth:** a 2-minute overview · a detailed look at one mechanism
- **Starting point:** "I know nothing about this area" · "I know the RFC, show me what's different in the code"

If the question is really several questions, say so and ask which one to answer first. If it's small enough for a chat reply, answer it in chat and offer the explainer in one line.

### 2. Research

Send the research to subagents so your own context stays lean. Give them the question, the scope, and these instructions:

- Read the code at `HEAD`, the RFCs (`docs/rfcs/`), the project plans (`docs/plans/`) and `CLAUDE.md`. Use the `notion` skill for tickets, and `git log` and `gh pr view` for history.
- For every step or claim, return the source (`file:line`) and the label (Decided / Emerged / Drift).
- Report what's missing: the parts of the intended design that aren't built yet, and which ticket builds them.
- Report how it's proven: the tests that cover it, by name and `file:line`.
- Mark anything they couldn't confirm as UNCONFIRMED.

Split the work by area when the question spans several (for example UI, core and engine), and run the subagents in parallel.

### 3. Verify

Before writing, check the claims. For a detailed explainer, send the draft claims to a separate subagent. Tell it to re-read every cited line and to return any claim that the source doesn't support. For an overview, spot-check the key claims yourself.

Cut or fix anything that fails. If verification turns up drift you didn't expect, keep it and make it prominent.

Check again after writing, because unsupported claims creep in while you're writing, especially in "Why it's like this". Every reason given there must be one that a doc actually gives. If it's your own inference, cut it or label it as yours.

### 4. Write the explainer

Load the `artifact-design` skill (and `artifact-diagramming` for the diagram) before writing. Write the HTML to the session scratchpad, not the repo. Explainers go stale, so they don't belong in version control.

Every explainer has the same sections, in this order, so Will learns where to look. Drop a section only if it would be empty.

1. **The answer.** Two or three sentences. If Will read only this, they'd have the right model.
2. **The picture.** One diagram of the real mechanism: the actual types, threads and queues, not generic boxes. Label where one thread hands off to another.
3. **Step by step** (for a *how* question), or **the key ideas** (for *what* and *why*). Each item is a short paragraph, its label and its citations.
4. **Why it's like this.** The decisions behind it and what they protect against, each linked to the RFC or ticket. Include the alternatives that were rejected, if the RFC records them.
5. **How we know it works.** The tests and checks, and what each one proves. Be clear about what isn't tested.
6. **Where to look.** The 3–6 files that matter, each with one line on what it does, in the order you'd read them.
7. **Gaps and drift.** What isn't built yet, what disagrees with the docs, and open questions.
8. **Check yourself.** Three questions Will should be able to answer after reading, with the answers hidden behind a click. Pick ones that test the model, not the trivia: "what would happen if…" beats "what is the name of…".

In the footer, put the commit SHA, the date, and the question as Will asked it.

Keep it short. A 2-minute overview should take two minutes to read. Put detail in collapsible sections rather than cutting what's true.

Link citations to GitHub at the stamped commit (`https://github.com/<owner>/<repo>/blob/<sha>/<path>#L<n>`), so they still point at the right code after it changes. Get the owner and repo from `git remote get-url origin`.

### 5. Publish

Publish with the Artifact tool, with icon `book`. Tell Will, in chat:
- the link;
- the answer in one or two lines;
- any drift or surprise it turned up, since that may need a ticket or an RFC amendment.

Don't fix drift as part of this skill. Suggest the fix, and let Will decide.

### Follow-up questions

If Will asks a follow-up about the same explainer, update it rather than making a new one, unless it's really a new question. Re-stamp it with the new SHA if the code has moved.
