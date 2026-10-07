# Project: Drafting table

**Status:** Created · **RFC:** [RFC-005](../rfcs/rfc-005-drafting-table.md) · **Notion:** https://app.notion.com/p/Drafting-table-3f23af969b6f8191828cc48f8acc810d

## Goal

Build RFC-005. Uta looks like the Two inks sandbox, with your real song: paper and black ink, faint structure, orange where the music is playing and blue where you've selected something, in both themes. `DESIGN.md` holds every token and the reasons for it, and a script turns it into the CSS variables and canvas values the app uses. In development builds a tuning panel edits those tokens live on the real app and saves them back. One CI check and a log of provisional decisions keep the system changing on purpose. Every slider is UTA-25's slider, built once, in the new style.

## Where the code starts

Checked against `main` at `9762414`, after UTA-33. RFC-004's project is Done.

- **Styles.** One [App.css](../../app/src/App.css) (599 lines) holds every rule. Its 36 colour variables are named after where they're used (`--roll-bar-line`, `--clip-fill`), with dark values under `prefers-color-scheme` at line 43.
- **Canvases.** The piano roll reads its colours with `readTheme` ([pianoRoll/colours.ts:35](../../app/src/pianoRoll/colours.ts)) and the timeline with `readClipTheme` ([timeline/canvasRenderer.ts:44](../../app/src/timeline/canvasRenderer.ts)), each with a raw hex fallback per colour. They re-read when `useColourScheme` reports a change. Fonts (`"11px -apple-system, …"`) and line widths are hard-coded in both renderers. Note colour comes from velocity, on a blue gradient.
- **Meter.** [Meter.tsx](../../app/src/Meter.tsx) reads `--meter-track` once, and its green, amber and red are hex values at line 72.
- **Sliders.** Five `<input type="range">`: `Volume`, `Transport`, the `Slider` inside `SynthPanel`, and two in `TrackHeaders`. UTA-31's helper (`dragSteps.ts`) already sends only the latest step of a drag.
- **Synth panel.** Sliders and a waveform fieldset. Nothing draws the filter or the envelope yet.
- **Window.** `tauri.conf.json` opens it at 1024 × 700, and nothing remembers its size.
- **Checks.** ESLint, no Stylelint. CI has a frontend job (typecheck, lint, test) and an app build job (`tauri build`).
- **The sandbox** (private design repo, `live/`) has the starting values in `tokens.json` (both themes' colours, type, line weights, hatch and grid spacing), the Tweakpane panel in `tune.js`, the Two inks drawing in `quiet.js`, and fader, filter-graph and envelope drawings in `views.js`. They're references to port, not code to copy as is.
- **Glossary.** RFC-005's six terms and the "Design" area are already in Notion.

## Tickets

11 tickets: three build the system, UTA-25 builds the slider, six restyle the screen and one extracts shared components. That's above the usual 4–8 because the RFC is big, not because the tickets are small: it's a design system and a restyle of the whole screen, it asks for one ticket per area, and UTA-25 joined it. The window ticket (area 7) is folded into "the rest" to keep it to 11.

**Every restyle ticket (5–10) also has these criteria,** written out in full on each Notion ticket:
- [ ] It uses tokens only, and passes ticket 3's colour check. Anything `DESIGN.md` doesn't cover is built from the principles, marked `provisional: D-n`, logged, and listed in the PR under "Design decisions".
- [ ] Hover, press, drag, focus, disabled and empty states are designed for each control in the area ("every interaction is considered").
- [ ] The existing UI tests pass unchanged. No hit area gets smaller; the PR lists the hit sizes it changed, before and after.
- [ ] The PR has before and after screenshots in both themes, and "For Will to try" steps for the area.

### 1. [UTA-34] Make DESIGN.md the source of the app's colours (Feature)

**Goal:** `DESIGN.md` exists at the root with every token and the prose behind it. `npm run tokens` turns it into a CSS file and a TypeScript file, and the whole app, canvases and meter included, takes its colours from them under names that say what they mean. Uta opens in paper, ink and two inks, in both themes, though nothing is restyled yet beyond colour.

**Acceptance criteria**
- [ ] `DESIGN.md` in Google's format: front matter with colours by role for both themes (dark ones prefixed `dark-`), a type token per role, a spacing scale and the canvases' fixed sizes at today's values, `stroke-*` line weights, corner radius, hatch and grid spacing. The prose sections follow the format's order, with the principles in Overview and the "never" list in Do's and Don'ts. Values start from the sandbox's `tokens.json`. Type starts on the system fonts, through `system-ui` and `ui-monospace` (resolved open question 1).
- [ ] `npm run tokens` writes `app/src/design/tokens.css` (light in `:root`, dark under `prefers-color-scheme: dark`, and both again under `data-theme`) and `app/src/design/tokens.ts` (the same values, typed). Both are committed.
- [ ] Every colour in the app uses a meaning name (`--ink`, `--ink-2`, `--live`, `--selected`, …). No `--roll-*` or `--clip-*` names remain, and the canvases' raw fallbacks are gone. The meter's signal and clip colours are tokens, and it follows theme changes like the canvases.
- [ ] `App.css` is split into a stylesheet beside each area's component, so tickets 5–10 can run in parallel without colliding.
- [ ] CI regenerates the token files and fails if they differ from what's committed, and runs `designmd lint DESIGN.md` (`@google/design.md` pinned at 0.4.0).
- [ ] Unit tests for the script: light and dark land in the right place; `{colors.ink}` references resolve; type and stroke tokens come out as CSS variables and typed values; a missing or malformed token gives a clear error. The existing UI tests pass unchanged.

**Out of scope:** Line widths and fonts on the canvases (2). Restyling any area beyond its colours (5–10). The colour check (3).

**Depends on:** Nothing.

**Context:** RFC-005, Proposal parts 1 and 2, and "How we'll verify it" (the token script's tests, the CI checks).

### 2. [UTA-35] Tune the design live in development builds (Feature)

**Goal:** With `npm run tauri dev`, a tuning panel sits beside the real app. Changing a token changes the CSS and both canvases at once, while the song plays, and Save writes it back into `DESIGN.md`. A release build doesn't contain the panel.

**Acceptance criteria**
- [ ] A Tweakpane panel, loaded only when `import.meta.env.DEV`: colours for both themes, type for each role, line weights, hatch and grid spacing, and a theme switch (System, Light, Dark) that sets `data-theme` for this session only. Tweakpane is a dev dependency.
- [ ] The canvases take their line widths and fonts from tokens, so none are hard-coded in the renderers. In development builds the canvases and the meter re-read the tokens on every panel change; release builds read them at start and when the colour scheme changes, as now.
- [ ] Save goes through an endpoint on the Vite dev server. It replaces only the front matter between the `---` lines, then reruns the token script.
- [ ] Fonts: the system fonts, free fonts kept in the repo, and font files from a folder outside the repo named by an environment variable (documented in `DESIGN.md`'s prose). Nothing from that folder is ever copied into the repo.
- [ ] Tests: saving leaves the prose byte-for-byte identical; saving the same values twice changes nothing; after a token changes, the timeline and piano roll renderers read the new value.
- [ ] CI: the bundle `tauri build` produces contains neither Tweakpane nor the save endpoint's code.

**Out of scope:** Choosing the font that ships. A theme setting in the app. Restyling any area.

**Depends on:** 1.

**Context:** RFC-005, Proposal parts 2 ("The canvases use the same tokens") and 3, Risks ("Saving must never damage the prose", "Canvas redraw cost while tuning"), resolved open questions 2 and 3, and "How we'll verify it" (saving tests, the release-build check, the renderer test).

### 3. [UTA-36] Keep the design system deliberate (Chore)

**Goal:** A colour written directly in the code fails the build, and there's a place and a process for design decisions a ticket has to make before `DESIGN.md` covers them. Tickets 4–11 work under both.

**Acceptance criteria**
- [ ] Stylelint, with its built-in rules, fails on `#hex`, `rgb()`, `hsl()` and named colours in CSS, except `tokens.css`. An ESLint rule does the same for colour strings in TypeScript, except `tokens.ts`. Both have one documented escape comment, and `npm run lint` runs both.
- [ ] Tests plant a colour in a CSS fixture and a TypeScript fixture and check that each rule catches it.
- [ ] A **Design decisions** database in Will's Notion, next to the Scratchpad: ID (`D-n`), what was needed, what was chosen and why, principles relied on, alternatives, screenshot, ticket, and status (Open · Adopted · Revised · Rejected). Its IDs go in the `notion` skill.
- [ ] Skills: `work-ticket` reads `DESIGN.md` before UI work and logs provisional decisions (mark the code, log it, list it in the PR); `review-pr` checks UI changes use tokens and anything new is logged; `scratchpad`'s between-projects review also goes through open decisions.
- [ ] `CLAUDE.md` gets a short Design section pointing to `DESIGN.md`, the log and the colour check.

**Out of scope:** Checking sizes and spacing (resolved open question 5).

**Depends on:** 1, since the check only passes once every colour is a token.

**Context:** RFC-005, Proposal part 6, resolved open questions 5 and 6, and "How we'll verify it" (the colour check and its fixture test).

### 4. [UTA-25] Replace the built-in sliders with one that follows the mouse and takes focus on click (Feature)

UTA-25 already existed, in Backlog with no project. It joined this project on 2026-10-07 with its criteria unchanged, gained one for the style, and now depends on 1 and 3 (UTA-30 and UTA-31 are Done).

**Goal:** Every slider in the app is one component, drawn in Drafting table. It stays under your mouse during a drag, works from the keyboard as soon as you click it, and can be set finely or reset.

**Acceptance criteria**
- [ ] One slider component, used by every slider in the app: the transport, the synth panel and the track headers.
- [ ] From press to release, it shows its own value, where the mouse is, and sends steps through UTA-31's helper. On release it shows the project's value again. That value is gesture state, gone when the drag ends.
- [ ] Clicking or dragging it gives it focus. Arrow keys step it, and Shift+arrow takes bigger steps. ⌥-drag moves it in fine steps. Double-click resets it to its default.
- [ ] The outline carries each control's default (master volume, tempo, synth settings and the track mixer), so the UI never hard-codes them.
- [ ] A drag is still one undo step. It's accessible as a slider (role, value and range for assistive tech).
- [ ] UI tests: it shows its own value during a drag and the project's after; click focuses; arrow and Shift+arrow step; ⌥-drag takes fine steps; double-click resets, each sending the expected commands.
- [ ] *New:* drawn in Drafting table from tokens only, with the sandbox's track faders as the reference. Anything `DESIGN.md` doesn't cover is logged as a provisional decision.

**Out of scope:** Knobs, and any control that isn't a slider today. Restyling the areas around it (5, 6, 9).

**Depends on:** 1 and 3.

**Context:** RFC-004, Proposal part 3 and resolved open question 2; RFC-005, resolved open question 1 (amended 2026-10-05). RFC-004's manual check 2, second half: the slider stays under the mouse while dragging at check 7.

### 5. [UTA-37] Restyle the top bar and transport (Feature)

**Goal:** The top bar reads as Drafting table: transport, position, tempo, master volume, master meter and loop switch.

**Acceptance criteria** (plus the shared ones above)
- [ ] Play and Stop, the loop switch, tempo and master volume (UTA-25's slider) in ink, with the live and selected inks used only for what's playing and what's selected.
- [ ] The position readout uses the position type token.
- [ ] The master meter uses the signal and clip tokens and keeps its behaviour.

**Out of scope:** New transport controls. Moving things around beyond what the style needs.

**Depends on:** 3 and 4.

**Context:** RFC-005, Proposal part 4, area 1.

### 6. [UTA-38] Restyle the track headers (Feature)

**Goal:** Each track header reads as Drafting table: name, volume, mute and solo, meter. Next to the timeline it's quieter than the clips.

**Acceptance criteria** (plus the shared ones above)
- [ ] Names use the track-name type token. Mute and solo are round keys, and their on states read at a glance in both themes.
- [ ] The volume is UTA-25's slider, and the meter matches the master meter's style.
- [ ] The selected track is marked with the selected ink, at the selection's ink weight.

**Out of scope:** Track colours, renaming in place, or any new header control.

**Depends on:** 3 and 4.

**Context:** RFC-005, Proposal part 4, area 2.

### 7. [UTA-39] Restyle the timeline (Feature)

**Goal:** The timeline canvas reads as Drafting table: clips, ruler, grid, loop band, playhead and selection, ranked by ink weight.

**Acceptance criteria** (plus the shared ones above)
- [ ] Structure (grid, ruler ticks) is faint, clips and their note previews are ink, and a selected clip is drawn heaviest, in the selected ink.
- [ ] The playhead is the live ink. The loop band is redrawn in the new style.
- [ ] Every value comes from tokens through ticket 2's reader, so the panel tunes the timeline live.
- [ ] Drawing stays within RFC-004's frame budget: the benchmark at heavy and wide shows no regression against `main` (report in the PR).

**Out of scope:** Dimension lines or value tags on drags, which would be new behaviour. Overlapping clips (a raw scratchpad note).

**Depends on:** 2 and 3.

**Context:** RFC-005, Proposal part 4, area 3, and "The direction, in brief" (ink weights, two inks).

### 8. [UTA-40] Restyle the piano roll (Feature)

**Goal:** The piano roll reads as Drafting table: keys, a millimetre grid behind the grid, notes in ink, the velocity lane, the playhead, and the notes sounding now in orange.

**Acceptance criteria** (plus the shared ones above)
- [ ] A millimetre grid sits behind the bar and beat lines, at the grid-spacing token.
- [ ] Notes are ink, with velocity shown by ink weight rather than the blue gradient, since blue now means selected. Selected notes are drawn heaviest, in the selected ink.
- [ ] The playhead is the live ink, and the notes under it while playing are drawn in the live ink, worked out from the playhead position the UI already receives (resolved open question 3).
- [ ] Keys and the velocity lane are restyled. Every value comes from tokens through ticket 2's reader.
- [ ] The benchmark at check 7 shows no regression against `main` (report in the PR).

**Out of scope:** ⌥-drag duplicate and resizing all selected notes (raw scratchpad notes). Dimension lines or value tags on drags.

**Depends on:** 2 and 3.

**Context:** RFC-005, Proposal part 4, area 4, and "The direction, in brief" (two inks, the millimetre grid).

### 9. [UTA-41] Restyle the synth panel and draw its filter and envelope (Feature)

**Goal:** The synth panel reads as Drafting table, and draws what it does: the filter's response and the envelope's shape, with hatching under each curve, changing as you move their sliders.

**Acceptance criteria** (plus the shared ones above)
- [ ] A drawing of the filter's response from the cutoff and resonance, and of the envelope from attack, decay, sustain and release, each with hatching under the curve at the hatch token. Both are read-only: the sliders stay the controls. The UI works out the filter curve from the exact formula for the engine's filter (resolved open question 2).
- [ ] The drawings follow the sliders during a drag, from the values the slider shows.
- [ ] The sliders are UTA-25's, and the waveform choice is restyled.
- [ ] A Rust test drives the engine's real filter with sine waves at a set of frequencies and settings, and writes the measured gains to a committed fixture (regenerated like the golden WAVs). A TypeScript test checks the drawn curve against it, within 0.5 dB, so the drawing can't drift from the sound.
- [ ] The approach and why are written down where the next person will look: a comment at the top of the curve's module and on the Rust test (works out the curve in the UI from the exact formula; why not ask Rust or measure it live; why the test exists), linking the Research page from resolved open question 2.
- [ ] Tests: the envelope drawing's shape for known values (each stage has the right length and level).

**Out of scope:** Dragging the curves themselves (the sandbox's interactive filter graph is a new control). New synth parameters (the synth's own scratchpad note).

**Depends on:** 3 and 4.

**Context:** RFC-005, Proposal part 4, area 5, and the principle "Draw the sound".

### 10. [UTA-42] Restyle the rest, and open the window filling the screen (Feature)

**Goal:** Everything the other tickets don't cover reads as Drafting table, and the window opens filling the screen, then remembers its size and position.

**Acceptance criteria** (plus the shared ones above)
- [ ] Tabs, the divider, the output panel, the frame and slowest-block readouts, the benchmark report, and the empty and error states, all restyled.
- [ ] On first launch the window fills the screen (maximised, not full screen). After that it opens at the size and position it was closed at, using Tauri 2's window-state plugin.
- [ ] If the remembered position is off every screen (a display unplugged), it opens filling the main screen instead.

**Out of scope:** The app icon and wordmark. A theme setting.

**Depends on:** 3.

**Context:** RFC-005, Proposal part 4, areas 6 and 7, and resolved open question 4.

### 11. [UTA-43] Extract the parts that repeat into shared components (Chore)

**Goal:** The parts that appear more than once after the first pass are shared components in `app/src/design/`, each with an entry in `DESIGN.md`'s Components section. The screen looks the same as before.

**Acceptance criteria**
- [ ] Each part used more than once becomes one component: likely the round mute and solo keys, value readouts, the value tag shown while dragging, rulers and section titles. The PR lists each, with where it's used.
- [ ] Each has a `DESIGN.md` Components entry (what it's for, its tokens, its states) and its own UI test.
- [ ] Provisional decisions it settles are noted in the PR, for the between-projects review.
- [ ] `DESIGN.md` states the rule: extract on the second use, not before.
- [ ] The existing UI tests pass unchanged, and before and after screenshots match.

**Out of scope:** Storybook or any component catalogue. Changing how anything looks.

**Depends on:** 5, 6, 7, 8, 9 and 10.

**Context:** RFC-005, Proposal part 5.

## Order and checkpoints

```
                  ┌─► 2 Tuning panel ─────────────────┬─► 7 Timeline ────┐
1 DESIGN.md ──────┤                                   └─► 8 Piano roll ──┤
  and tokens      ├─► 3 Keep it deliberate ─┬─────────────► 10 The rest ─┤
                  │                         │                            ├─► 11 Extract
                  └─────────────────────────┴─► 4 Slider ─┬─► 5 Top bar ─┤
                                                (UTA-25)  ├─► 6 Headers ─┤
                                                          └─► 9 Synth ───┘
```

- 1 comes first. Then 2, 3 and 4 run alongside each other (4 waits for 3, which is small).
- 7 and 8 need 2 (the canvases' line widths and fonts). 5, 6 and 9 need the slider. 10 needs only 3. Ticket 1's split of `App.css` lets 5–10 run in parallel.
- Because the direction is about how areas look together, the checkpoint after the restyles judges them side by side, and fixes go back into the area tickets' follow-ups or ticket 11.
- **Checkpoint after 1:** open the app. It's in paper, ink and two inks in both themes (switch the Mac's setting), with the old layout and type.
- **Checkpoint after 2:** in `npm run tauri dev`, the tuning panel. Manual checks 3, 4 and 5: change the ink, a line weight and the name font while the song plays; Save and read `git diff DESIGN.md`; a release build has no panel. And the second half of manual check 2: the panel's theme switch overrides the system, and System hands back. From here Will can tune type and both themes at any time.
- **Checkpoint after 4:** drag sliders on the check-7 song. Each stays under the mouse (RFC-004's manual check 2, second half), and clicks, arrows, ⌥-drag and double-click all work.
- **Checkpoint after 5–10:** the whole screen restyled. Manual checks 1, 2 and 6: the release build beside the Two inks sandbox in light, then dark; use every area as before.
- **End of project, after 11:** the same checks again on the extracted components, and the first between-projects review of the Design decisions log.

## Coverage

| RFC-005 verification item | Ticket |
|---|---|
| Token script: light and dark in the right place | 1 |
| Token script: `{colors.ink}` references resolve | 1 |
| Token script: type and stroke tokens as CSS variables and typed TypeScript | 1 |
| Token script: missing or malformed token gives a clear error | 1 |
| Saving: only the front matter changes, prose byte-for-byte | 2 |
| Saving: the same values twice changes nothing | 2 |
| CI: generated files differ from the script's output | 1 |
| CI: `designmd lint DESIGN.md` | 1 |
| CI: a raw colour in CSS or TypeScript, with a fixture test | 3 |
| Release build contains neither Tweakpane nor the save endpoint | 2 |
| Existing UI tests pass unchanged | every ticket |
| A token change reaches the timeline and piano roll renderers | 2 |
| Manual 1: release build beside the sandbox, light | after 5–10 |
| Manual 2: dark follows the system; panel's theme switch | after 5–10 (system), 2 (switch) |
| Manual 3: tune ink, a line weight, a font while playing | 2 |
| Manual 4: `git diff DESIGN.md` shows only token lines | 2 |
| Manual 5: no panel in a release build | 2 |
| Manual 6: every area works and nothing got harder to hit or read | 5–10, end of project |
| Proposal 1: DESIGN.md as source of truth | 1 |
| Proposal 2: tokens in the app, canvases and meter | 1 (colours), 2 (lines and fonts) |
| Proposal 3: tuning panel, theme switch, fonts | 2 |
| Proposal 4: first pass, areas 1–7 | 5, 6, 7, 8, 9, 10 (areas 6 and 7 together) |
| Proposal 5: extract what repeats | 11 |
| Proposal 6: log, skills, colour check, `CLAUDE.md` | 3 |
| Open question 1: UTA-25 joins | 4 |
| Open question 4: the window | 10 |
| Open question 7: the Glossary's Design area | done when the RFC was accepted |

## Not in this project

- **Everything RFC-005 leaves out:** brand, logo, app icon and wordmark; layout changes beyond what the restyle needs; a theme setting in the app; new features in any component; screenshot comparison tests; the sandbox's own process.
- **Checking sizes and spacing in CI.** Colours only, until the tokens settle (resolved open question 5).
- **Interactive drawings,** such as dragging the filter curve, and dimension lines or value tags on canvas drags. Both would be new behaviour. If a ticket thinks one is needed, it's a provisional decision, not part of the diff.
- **Scratchpad notes.** None of the seven raw notes fits a restyle that adds no features, so I'd keep all seven Raw for a later review:
  - vertical track scrolling feels janky at check 7, and the graphics stall when undo brings back many clips (both about drawing cost; a performance follow-up, not a restyle);
  - overlapping clips hide each other (Will chose to live with it for now);
  - the synth sounds basic and digital (a sound RFC of its own);
  - ⌥-drag duplicates the selected notes, and resize all selected notes together (a small piano-roll editing project);
  - optimise build artifacts across worktrees (tooling, outside any project).

## Open questions

1. **Resolved: system fonts for now.** Will agreed with the recommendation. The question was what type `DESIGN.md` starts on. Type is still open, to settle by tuning, and the sandbox's Archivo and Azeret Mono read as the agent defaults you wanted to avoid. Recommendation: start on the system fonts (SF Pro and SF Mono, through `system-ui` and `ui-monospace`), and keep no font files in the repo until tuning picks one. The candidates (Martian, Commit Mono, Departure Mono, iA Writer, all OFL) already sit in the design repo's fonts folder, which the panel reads through the trial folder. The chosen free font then goes in the repo with its licence, in its own small PR.
2. **Resolved: the UI works it out, and a test ties it to the engine.** Will agreed to start with the recommendation and see how it works in practice. The question was who works out the filter's response curve. "Draw the sound" needs a curve from the cutoff and resonance, and the UI never works out project data. Recommendation: the UI draws it from the standard formula for the engine's 12 dB filter, as presentation, like turning a velocity into a colour. A test checks a few points against the engine's own filter, so the drawing can't drift from the sound. The alternative is a Rust command that returns the curve, which is a round trip on every slider step.

   **How plugins usually do it** (researched 2026-10-07): the editor works out the curve itself, by evaluating the filter's transfer function at each frequency it draws, rather than asking the audio side. JUCE's advice is that working out the coefficients is cheap, so duplicating it in the UI is fine, and the UI's copy should stay separate from the audio thread's, which smooths its values ([JUCE forum](https://forum.juce.com/t/drawing-a-filter-response/29128), [JUCE forum](https://forum.juce.com/t/simplest-way-to-draw-single-iir-filter-curve-in-audioplugin/28747)). Measuring the real filter with an FFT is too slow for a display that follows a drag at 60 frames a second ([HISE forum](https://forum.hise.audio/post/81600)). The known risk is the one our test is for: HISE drew some filter types from a generic approximation and the drawing stopped matching the sound, until it let each filter supply its exact formula ([HISE forum](https://forum.hise.audio/post/81517)). So ticket 9 draws from the exact formula for the engine's filter, and a Rust test measures the real filter so the TypeScript curve is checked against what you hear.

   **Where it's recorded:** these findings are in the Research page [Drawing a filter's response curve](https://app.notion.com/p/Drawing-a-filter-s-response-curve-3f23af969b6f81f9a4d9f08a1bc97317), linked to RFC-005. Ticket 9 links it from the code comments that explain the approach.
3. **Resolved: yes, in the piano roll only.** Will agreed with the recommendation. The question was whether to light up the notes sounding now. The RFC says orange means "the playhead and the notes sounding now", but nothing highlights sounding notes today. Recommendation: yes, in the piano roll only (ticket 8), worked out from the playhead position the UI already receives and the notes it already holds, so no engine change. Not in the timeline, where previews are too small to read it.
