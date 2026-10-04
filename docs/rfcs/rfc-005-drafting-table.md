# RFC-005: Drafting table

**Status:** Accepted (2026-10-04) · **Author:** Will · **Notion:** https://app.notion.com/p/Drafting-table-3ef3af969b6f816f9639ec5af0e668eb

## Summary

Bring the design direction chosen in the design rounds ("Drafting table" with two inks) into the app, and give Uta a design system that records it. A `DESIGN.md` file at the root of the repo, in Google's DESIGN.md format, holds the tokens and the reasons for them. It's the single source: a small script turns its tokens into the CSS variables and canvas values the app uses. In development builds, a tuning panel edits those tokens live on the real components and saves them back to `DESIGN.md`. A first pass then cleans up every area of the screen in the new style, and the parts that repeat become shared components. A log of provisional design decisions and one CI check keep the system changing on purpose rather than drifting.

## Motivation

- **Today's look is defaults.** It uses the system font, a stock blue accent and grey on grey. There are 36 colour variables, all named after where they're used (`--roll-bar-line`, `--clip-fill`) rather than what they mean. Outside them are about 80 raw pixel values, 3 raw colours, and two slightly different font stacks. The canvases hard-code their fonts and line widths, and the meter hard-codes its green, amber and red ([App.css](../../app/src/App.css), [Meter.tsx:72](../../app/src/Meter.tsx), [pianoRoll/canvasRenderer.ts:151](../../app/src/pianoRoll/canvasRenderer.ts)).
- **The direction is chosen.** Two design rounds compared four directions, then refined one. The result is Drafting table with two inks, and it now needs to meet the real components. Tuning it further on a copy of the app has stopped paying off: spacing, density and type only settle against the real thing.
- **More features are coming.** Each new control or view needs something to build on. Without a system, every ticket invents its own look, and agents fill the gaps with the average-looking defaults we're trying to leave behind.

**When:** after the Responsive at song scale project (RFC-004) finishes. Its UTA-25 replaces the built-in sliders in the same components this restyles (open question 1, resolved).

## The direction, in brief

These are the decisions from the design rounds that this RFC carries into the app. `DESIGN.md` records them in full.

- **Principles:**
  - An app, not a replica: honest software controls, with no fake metal, screws or bevels.
  - Draw the sound: a control shows what it does before a label says it.
  - Light at heart: ink on paper. The dark theme is the same drawing with the lights down, in layered tones, never white lines on black.
  - Industrial, not clinical.
  - Uncluttered.
  - Alive: Uta moves with the music, and is still when it stops.
  - Every interaction is considered: hover, press, drag, focus and empty states are all designed.
  - Opinionated and fun.
- **Ink weights carry the hierarchy.** Structure is faint, content is ink, and the selection is drawn heaviest. Depth comes from line weight, not shadows.
- **Two inks.** Orange means live: the playhead and the notes sounding now. Blue means selected. Everything else is drawn in black and grey ink.
- **Signature details:** hatching under curves such as the filter response and the envelope; a millimetre grid behind the piano roll; dimension lines and value tags that appear when they're useful, not all the time.
- **Light first.** Both themes get equal care, and the theme still follows the system setting.
- **Still open, to settle by tuning in the real app:** type, and the fine detail of both themes.

## Proposal

### 1. DESIGN.md is the source of truth

[DESIGN.md](https://github.com/google-labs-code/design.md) is an open format from Google Labs (Apache-2.0) for describing a design system to coding agents. It does for design what `CLAUDE.md` does for engineering. One markdown file has two parts:
- **Front matter:** YAML at the top, holding the **tokens**, which are the exact values: colours, type, spacing.
- **Body:** prose sections in a fixed order (Overview, Colors, Typography, Layout, Elevation & Depth, Shapes, Components, Do's and Don'ts), saying why the values are what they are and how to use them.

It comes with a command-line tool (`@google/design.md`, version 0.4.0) that lints the file, diffs two versions and exports tokens.

Ours would map like this:

| Section | What goes there |
| --- | --- |
| Overview | The principles above. |
| Colors | Every colour by role: paper, sheet, ink 1–3, lines, live, selected, and the meter's signal and clip colours. |
| Typography | A token for each role: track names, labels, numbers, the position readout, panel titles. |
| Layout | A spacing scale. The fixed sizes the canvases use, such as row heights and ruler height, start as tokens and stay where they are. |
| Elevation & Depth | Flat. Hierarchy comes from ink weight; there are no shadows. |
| Shapes | Line weights, hatching, the millimetre grid, corner radius. |
| Components | Filled in as components are extracted (part 5). |
| Do's and Don'ts | The "never" list: skeuomorphic knobs and bevels, neon on black, decorative gradients and glows, grey-on-grey with no hierarchy, leftover defaults, jargon labels. |

**Where the format doesn't fit, and what we do instead.** The format is labelled alpha and has two gaps that matter to us:
- **No themes.** We name dark colours with a `dark-` prefix (`ink` and `dark-ink`). The format accepts any colour name, so the file still lints cleanly.
- **No line weights.** We keep them as `stroke-*` entries in the spacing group, which accepts any named value. The format also requires a `primary` colour, which we point at `ink`.

Our script reads the YAML itself rather than depending on the format's own exporter. If the format changes, only our script and the file's layout change.

### 2. Tokens in the app

- **A script turns the tokens into code.** A small Node script (`npm run tokens`) reads `DESIGN.md`'s front matter and writes two files:
  - `app/src/design/tokens.css`: CSS variables, with light values in `:root` and dark values under `prefers-color-scheme: dark`. The dark values are also written under a `data-theme="dark"` attribute on the page, and the light ones under `data-theme="light"`, so something can choose a theme regardless of the system setting. Only the tuning panel does that (part 3); the app itself still follows the system.
  - `app/src/design/tokens.ts`: the same values for the canvases, typed.

  Both are committed. CI regenerates them and fails if they differ from what's committed, so a token can't change in one place and not the other.
- **Names say what a colour means, not where it's used:** `--ink`, `--ink-2`, `--live`, `--selected`, not `--roll-bar-line`. `App.css` and the components move to these names.
- **The canvases use the same tokens.** The timeline and piano roll already read their colours from CSS variables when they're built and when the colour scheme changes. That extends to line widths and fonts, which are hard-coded today. In development builds they also re-read whenever the tuning panel changes a token.
- **The meter's colours become tokens too,** and it follows theme changes like the canvases do. Today it reads its colours once.

### 3. A tuning panel, in development builds only

When Uta runs with `npm run tauri dev`, a tuning panel opens beside the real app. It's the same panel as in the design sandbox: colours for both themes, type for each role, line weights, hatching and grid spacing. It also has a **theme switch** (System, Light, Dark), so you can tune either theme without changing your Mac's setting. The switch only affects the development build you're looking at, and isn't saved. Changes show immediately, on the CSS and the canvases, while the song plays. **Save** writes the values back into `DESIGN.md`'s front matter, leaving the prose untouched, and regenerates the token files. Every tuning session ends as an ordinary diff you can read and commit.

How it stays out of release builds:
- The panel is loaded only when the build is a development one (Vite's `import.meta.env.DEV`), so a release build doesn't contain it at all.
- Saving goes through the Vite dev server that `tauri dev` already runs. Release builds have no such server, so there's nothing to save to.
- The panel uses [Tweakpane](https://tweakpane.github.io/docs/) (MIT), as a development dependency only. The design sandbox already uses it.

**Fonts while tuning:** the panel offers free fonts kept in the repo, the system fonts, and any font files in a folder outside the repo that you point it at. That folder is for trial fonts from foundries. Their licences only cover testing, so they never go in this public repo.

### 4. First pass: clean up every area

This restyles each area in Drafting table, using only tokens. It's a cleanup, not a redesign: no new features or controls, and the layout keeps its structure. Spacing changes where the style needs it, and layouts can change later once more of the parts exist. One ticket per area:
1. Top bar and transport, including the master meter and loop switch.
2. Track headers: name, volume, mute and solo, meter.
3. Timeline (canvas): clips, ruler, grid, loop band, playhead, selection.
4. Piano roll (canvas): keys, grid and millimetre grid, notes, velocity lane, playhead.
5. Synth panel: the filter and envelope drawn with hatching, and the waveform choice.
6. Everything else: tabs, divider, output panel, frame readout, empty and error states.
7. The window opens filling the screen, then remembers its size and position (from the scratchpad, open question 4).

Doing the whole screen in one pass matters because the direction is about relationships. A track header tuned on its own can look right and still be too loud next to the timeline.

### 5. Extract what repeats

After the first pass, the parts that appear more than once become shared components in `app/src/design/`. Likely candidates are the round mute and solo keys, value readouts, the value tag shown while dragging, rulers and section titles. Each gets an entry in `DESIGN.md`'s Components section. The rule from then on: extract on the second use, not before. Components designed in isolation tend to be generic; ones pulled out of a real screen keep its character.

### 6. Keeping it deliberate

- **A provisional decisions log.** When a ticket needs something `DESIGN.md` doesn't cover (a new component, a layout pattern, a new token, or bending a rule), the agent builds the best version it can from the principles, so the ticket isn't blocked. It then:
  1. marks the code `provisional: D-n`,
  2. logs the decision: what was needed, what it chose and why, the principles it relied on, the alternatives, a screenshot and the ticket,
  3. lists it in the PR under "Design decisions".

  Between projects, alongside the scratchpad review, you decide each one: **adopt** it into `DESIGN.md`, **revise** it (it goes back to the sandbox for a proper round), or **reject** it in favour of an existing pattern. The log is a Notion database in your workspace, so the reasoning stays private and only the outcomes reach the public repo (open question 6).
- **Skills.**
  - `work-ticket`: read `DESIGN.md` before UI work, and log a provisional decision when it's needed.
  - `review-pr`: check that UI changes use tokens, and that anything new is logged.
  - `scratchpad`: the between-projects review also goes through open design decisions.
- **One CI check: no colour written directly in the code.** A colour that isn't a token fails the build. This covers `#hex`, `rgb()` and `hsl()` values and named colours, in the CSS (using [Stylelint](https://stylelint.io)'s built-in rules) and in the TypeScript (an ESLint rule). The generated token files are the only exception. CI also runs `designmd lint DESIGN.md`. Sizes and spacing aren't checked yet; they can be added once their tokens settle (open question 5).
- **`CLAUDE.md`** gets a short Design section pointing to `DESIGN.md` and the log.

### Not in this project

- Brand and logo, the app icon and a wordmark.
- Layout changes beyond what the restyle needs.
- A setting in the app for choosing light or dark. Only the development tuning panel can override the system.
- New features in any component.
- Screenshot comparison tests.
- The design sandbox's own process, which stays in the private design repo.

## What it looks like to you

- **You open Uta and it looks like the Two inks sandbox, with your real song.** In light: paper, black ink, faint structure, orange where the music is playing and blue where you've selected something. In dark: graphite tones and a warm off-white ink. Hatching sits under the filter and envelope curves, and a millimetre grid behind the piano roll.
- **Everything works as before.** Nothing moves or gains features, but every area looks like it belongs to the same instrument.
- **With `npm run tauri dev`, the tuning panel sits beside the real app.** Switch it to Dark to work on the dark theme without touching your Mac's setting. Drag the ink colour and the timeline, piano roll and buttons all change together while the song plays. Try a font from your trial folder on the real track names. Save, and `git diff` shows exactly which tokens you changed in `DESIGN.md`.
- **A release build has no panel.**
- **Later, when a ticket adds something new,** its PR has a "Design decisions" section, and between projects you go through the log and decide what joins the system.

## Alternatives considered

- **Design components one at a time first,** for example in Storybook, then assemble the screen. The direction depends on how things relate on one screen, so components judged alone look right and add up wrong. They also tend towards generic parts. Storybook may still be useful later as a catalogue of the extracted components.
- **Keep the tokens in `tokens.json` and use `DESIGN.md` only for prose.** That gives two files describing the same values, which drift apart. Agents also get the values and the reasons together when they're in one file. The cost is that the panel has to edit YAML without disturbing the prose, which is a contained problem (see Risks).
- **Adopt Tailwind,** which the DESIGN.md exporter targets. It would replace the styling approach across every component and add a large dependency, and the canvases would still need the values separately. Plain CSS variables fit how the app is written today and how the canvases already read colours.
- **A typed styling library** such as vanilla-extract. It gives typed tokens, but adds a build step and changes every component's styling. The generated `tokens.ts` gives the canvases typed values without that.
- **Keep tuning in the design sandbox.** It stopped paying off because it doesn't match the real components.
- **Keep the tuning panel in release builds** as a hidden setting. You want it in development builds only, and a release build shouldn't ship a tool that writes to the repo.

## Risks & unknowns

- **The format is young.** It's labelled alpha, at version 0.4.0, from Google Labs. The front matter could change shape. Because our script reads the YAML itself, a change costs a script edit and a file reshuffle, not a rewrite. The linter is the part most likely to shift. If it starts failing on our naming conventions, we pin its version.
- **Saving must never damage the prose.** Saving replaces only the front matter, between the `---` lines, and tests check that the body comes out unchanged byte for byte. If YAML comments or ordering get lost on save, we write the front matter in a fixed order and keep the comments in the prose instead.
- **Fonts and the public repo.** Only free fonts (such as those under the SIL Open Font License) or the system fonts can be committed and shipped from this repo. A commercial font would need a licence that allows embedding in an app, and a way to keep the file out of the public repo. That would need its own decision (open question 3).
- **The CI check could catch false positives,** such as a `#` and hex digits inside an unrelated string. It only looks at styling code: CSS files, and colour strings passed to the canvases. It has a documented escape comment for genuine exceptions. *Guess:* this will be rare.
- **The restyle could break behaviour quietly,** for example a hit area that shrinks when an outline gets thinner. The existing UI tests catch logic but not looks, because they render without a real browser. Each area's PR has before and after screenshots and steps for you to try.
- **Canvas redraw cost while tuning.** Re-reading tokens and rebuilding the renderers on every panel change only happens in development builds. Release builds read the tokens when they start and when the colour scheme changes, as now.

## How we'll verify it

**Automated, on every PR:**
- Unit tests for the token script:
  - Light and dark colours each come out in the right place.
  - References like `{colors.ink}` resolve.
  - Type and stroke tokens come out as CSS variables and typed TypeScript values.
  - A missing or malformed token gives a clear error.
- Unit tests for saving:
  - Only the front matter changes, and the prose is byte-for-byte identical.
  - Saving the same values twice changes nothing.
- CI fails when:
  - the generated token files differ from what the script produces from `DESIGN.md`,
  - `designmd lint DESIGN.md` reports an error,
  - a colour is written directly in CSS or TypeScript outside the generated files. A test plants one in a fixture and checks that the rule catches it.
- A release-build check: the bundle built by `tauri build` contains neither Tweakpane nor the save endpoint's code.
- The existing UI tests pass unchanged. A test that changes a token checks that the timeline and piano roll renderers read the new value.

**Manual, by you:**
1. With the system in light mode, open a release build next to the Two inks sandbox, both playing the demo song. Each area should read as the same direction.
2. Switch the system to dark. The app follows, in graphite tones, with no pure white lines. Then, in a development build, use the tuning panel's theme switch: Light and Dark override the system, and System hands back to it.
3. In `npm run tauri dev`, open the tuning panel and change the ink, a line weight and the font for names. The CSS and both canvases follow immediately while the song plays.
4. Save, then check `git diff DESIGN.md`. Only token lines should have changed.
5. Open a release build and confirm there's no tuning panel.
6. Use each area as before (play, select, drag clips and notes, mute and solo, the synth), and check that nothing has become harder to hit or read.

## Open questions

1. **When does this start?** Recommendation: after the Responsive at song scale project, because UTA-25 replaces the built-in sliders in the same components. **Resolved: after RFC-004's project finishes (Will, 2026-10-04).**
2. **Which theme is the default?** Recommendation: keep following the system setting, and leave a switch inside the app out of this project. The design rounds favour light, and tuning both themes in the real app is part of this project. **Resolved: the app follows the system. The tuning panel gets a theme switch for development builds, so either theme can be tuned without changing the Mac's setting (Will, 2026-10-04).**
3. **Do fonts have to be free?** Recommendation: try anything while tuning, using the trial folder. The font that ships must be free (OFL) or a system font, unless you choose to buy a licence that allows embedding in an app. If you do, we amend this RFC with how the font file stays out of the public repo. **Resolved: as recommended (Will, 2026-10-04).**
4. **Fold in the scratchpad note "Open the window filling the screen, then remember its size and position"?** Recommendation: yes, as a small ticket in this project. The restyle is the moment the window's starting size matters. **Resolved: yes (Will, 2026-10-04).**
5. **How strict is the CI check?** Recommendation: colours only, failing the build, from the start. Add sizes and spacing later, first as warnings, once their tokens have settled. **Resolved: colours only until we're further along (Will, 2026-10-04).**
6. **Where does the provisional decisions log live?** Recommendation: a Notion database in your workspace, next to the scratchpad, so the reasoning stays private and only outcomes reach the public repo. The alternative is a `docs/design/log.md` in this repo, which is visible to anyone. **Resolved: Notion (Will, 2026-10-04).**
7. **Glossary area.** The Glossary's `Area` options have no home for design terms. Recommendation: add a "Design" area for the new terms below. **Resolved: add it (Will, 2026-10-04).**

## New terms

- **Design system:** the shared set of decisions, values and parts that make an app look and behave consistently, written down so new work follows it.
- **Design token:** a named design value, such as a colour, a size or a font, used everywhere instead of the raw value, so changing it in one place changes it everywhere.
- **DESIGN.md:** a markdown file at the root of the repo, in Google's open format, that holds Uta's design tokens and the reasons behind them, for people and agents to read.
- **Ink weight:** how heavily something is drawn. Uta's design uses it to rank what matters: faint structure, ink for content, heaviest for the selection.
- **Tuning panel:** a set of live controls, in development builds only, for adjusting design tokens on the real app and saving them to DESIGN.md.
- **Provisional design decision:** a design choice made during a ticket for something the design system doesn't cover yet, marked in the code and logged so it's adopted, revised or rejected later.
