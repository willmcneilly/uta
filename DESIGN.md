---
version: alpha
name: Drafting table
description: >-
  Uta's design system: ink on paper, faint structure, orange where the music is
  playing and blue where you've selected something. Dark colours carry a `dark-`
  prefix, and line weights are the `stroke-*` entries under spacing.
colors:
  primary: "{colors.ink}"
  paper: "#eceee9"
  sheet: "#f5f6f2"
  ink: "#22262a"
  ink-2: "#6c726f"
  ink-3: "#aeb2ae"
  line: "#22262a0e"
  line-2: "#22262a1f"
  mm-line: "#22262a0f"
  hatch: "#22262a4d"
  live: "#ff5a12"
  selected: "#2b59d6"
  selected-wash: "#2b59d61a"
  signal: "{colors.ink-2}"
  signal-hot: "{colors.ink}"
  error: "#b42318"
  clip: "{colors.error}"
  scrim: "#22262a4d"
  dark-paper: "#1f2225"
  dark-sheet: "#262a2d"
  dark-ink: "#dcd9d1"
  dark-ink-2: "#979892"
  dark-ink-3: "#5d6265"
  dark-line: "#dcd9d10b"
  dark-line-2: "#dcd9d11a"
  dark-mm-line: "#dcd9d10b"
  dark-hatch: "#dcd9d138"
  dark-live: "#ff7d42"
  dark-selected: "#86a5ff"
  dark-selected-wash: "#86a5ff24"
  dark-signal: "{colors.dark-ink-2}"
  dark-signal-hot: "{colors.dark-ink}"
  dark-error: "#ff6b5e"
  dark-clip: "{colors.dark-error}"
  dark-scrim: "#12141699"
typography:
  name:
    fontFamily: system-ui
    fontSize: 13.5px
    fontWeight: 500
    lineHeight: 1.2
  text:
    fontFamily: system-ui
    fontSize: 12.5px
    fontWeight: 400
    lineHeight: 1.4
  label:
    fontFamily: system-ui
    fontSize: 11px
    fontWeight: 400
    lineHeight: 1.2
  number:
    fontFamily: ui-monospace
    fontSize: 10px
    fontWeight: 400
    lineHeight: 1.2
  position:
    fontFamily: ui-monospace
    fontSize: 19px
    fontWeight: 400
    lineHeight: 1
  title:
    fontFamily: system-ui
    fontSize: 15px
    fontWeight: 600
    lineHeight: 1.2
rounded:
  sm: 2px
  md: 6px
  lg: 8px
spacing:
  space-1: 2px
  space-2: 4px
  space-3: 6px
  space-4: 8px
  space-5: 12px
  space-6: 16px
  space-7: 24px
  ruler-height: 24px
  track-height: 96px
  add-track-height: 44px
  track-headers-width: 272px
  keyboard-width: 56px
  velocity-lane-height: 72px
  stroke-clip: 1px
  stroke-clip-selected: 1.75px
  stroke-note: 1px
  stroke-note-selected: 1.75px
  stroke-clip-notes: 1.5px
  stroke-curve: 1.5px
  stroke-playhead: 1px
  stroke-grid: 1px
  stroke-selection-box: 1px
  hatch-gap: 5px
  mm-grid-gap: 20px
---

# Uta: Drafting table

This file is the source of Uta's look. The front matter above holds the tokens, the exact values; the sections below say why they are what they are and how to use them. `npm run tokens` (in `app/`) turns the tokens into `app/src/design/tokens.css` and `app/src/design/tokens.ts`. Never edit those two files by hand: change a token here and run the script. CI fails if they don't match.

Two things the format doesn't have, and what we do instead:
- **Themes.** Every colour has a dark twin named with a `dark-` prefix (`ink` and `dark-ink`). The script puts the light ones in `:root` and the dark ones under `prefers-color-scheme: dark`, and both again under `data-theme="light"` and `data-theme="dark"`, so a development tool can pick a theme regardless of the system.
- **Line weights.** They're the `stroke-*` entries in the spacing group. The format also requires a `primary` colour; ours points at `ink`, and the script doesn't emit it.

## Overview

Uta is an instrument you draw on. The look is a drafting table: ink on paper, structure drawn faintly, the music drawn in ink, and two coloured inks for the two things that change while you work, what's playing and what you've selected.

The principles, in the order they settle arguments:
- **An app, not a replica.** Honest software controls, with no fake metal, screws or bevels.
- **Draw the sound.** A control shows what it does before a label says it: the filter draws its curve, the envelope its shape.
- **Light at heart.** Ink on paper. The dark theme is the same drawing with the lights down, in layered tones, never white lines on black.
- **Industrial, not clinical.** Precise and a little technical, like a drawing with dimensions on it, but warm rather than sterile.
- **Uncluttered.** Each thing on the screen earns its place. Detail appears when it's useful, not all the time.
- **Alive.** Uta moves with the music, and is still when it stops.
- **Every interaction is considered.** Hover, press, drag, focus, disabled and empty states are all designed, not left to defaults.
- **Opinionated and fun.** It should feel like someone made choices.

## Colors

Two inks on paper. Everything is drawn in black and grey ink, except for two colours that each mean one thing.

- **Paper (`paper`):** the page everything sits on, a cool off-white. In dark, graphite.
- **Sheet (`sheet`):** a lighter sheet laid on the paper, for drawing surfaces such as the timeline and the piano roll, and for panels.
- **Ink (`ink`):** content. Text, notes, clips, anything you made. In dark, a warm off-white, never pure white.
- **Ink 2 (`ink-2`):** secondary text and marks: labels, units, the less important half of a pair.
- **Ink 3 (`ink-3`):** structure you need to find but shouldn't notice: bar lines, the black keys, quiet outlines.
- **Lines (`line`, `line-2`):** the faintest structure, as ink at low opacity so they sit on any surface: sub-beat lines and washes (`line`), borders, beat lines and tracks (`line-2`).
- **Millimetre grid (`mm-line`):** the fine grid behind the piano roll.
- **Hatching (`hatch`):** the hatching under drawn curves such as the filter response and the envelope.
- **Live (`live`):** orange. Only what is playing now: the playhead and the notes sounding. Nothing else is orange.
- **Selected (`selected`, `selected-wash`):** blue. Only what you've selected, and the focus ring. The wash is the same blue, faint, for areas such as a selected track or the selection box.
- **Meter (`signal`, `signal-hot`, `clip`):** a level meter is ink: `ink-2` for an ordinary level, `ink` once it's loud (above -18 dB). Only clipping gets a colour, the error red on the clip light, because it's a warning.
- **Error (`error`):** messages that something went wrong, and clipping.
- **Scrim (`scrim`):** behind a dialog, to set it apart from the app.

Colours are named for what they mean, never for where they're used: `--ink-2`, not `--roll-ruler-text`. A component that needs a new meaning gets a new token here, with a sentence in this list.

## Typography

The system fonts for now: SF Pro through `system-ui` for words and SF Mono through `ui-monospace` for numbers, until tuning in the app settles on a typeface. The font that ships must be free (such as SIL Open Font License) or a system font.

**Trying fonts.** In development builds (`npm run tauri dev`) the tuning panel offers the system fonts, the free fonts kept in the repo in `app/src/design/fonts/`, and the font files (`.woff2`, `.woff`, `.ttf`, `.otf`) in a folder outside the repo named by the `UTA_TRIAL_FONTS` environment variable, for trial fonts whose licences only cover testing: `UTA_TRIAL_FONTS=~/Fonts/trials npm run tauri dev`. The development server reads them from that folder for the panel and never copies them; nothing from it may be committed. Each file is offered under its name up to the first hyphen, so `CommitMono-Bold.otf` is the family `CommitMono` at weight 700.

- **Name (`name`):** track and clip names, the things you made.
- **Text (`text`):** ordinary interface text: settings, messages, buttons.
- **Label (`label`):** small captions on the canvases, such as clip labels and key names.
- **Number (`number`):** values in readouts, such as dB, Hz and milliseconds. Monospaced, so they don't jump as they change.
- **Position (`position`):** the transport's bar and beat readout, the largest number on screen.
- **Title (`title`):** panel and dialog titles.

## Layout

Spacing comes from a short scale, `space-1` (2px) to `space-7` (24px). Most gaps are `space-4` (8px); panels pad by `space-5` (12px).

The canvases have fixed sizes that the code works from, kept here at their current values: `ruler-height` (both rulers), `track-height` and `add-track-height` (timeline rows), `track-headers-width`, `keyboard-width` and `velocity-lane-height` (the piano roll). Changing one moves the hit areas with it.

## Elevation & Depth

Flat. Hierarchy comes from ink weight, not shadows: structure is faint, content is ink, and the selection is drawn heaviest. A surface is set apart by a lighter sheet or a faint line, never a drop shadow. The one exception is the scrim behind a dialog.

## Shapes

- **Line weights (`stroke-*`):** clips and notes are outlined at `stroke-clip` and `stroke-note`, and drawn heavier when selected (`stroke-clip-selected`, `stroke-note-selected`). Notes inside a clip's preview use `stroke-clip-notes`, curves `stroke-curve`, and the playhead `stroke-playhead`. Grid lines, ruler ticks and the lines between rows and tracks use `stroke-grid`, and the edge of the selection box `stroke-selection-box`.
- **Hatching:** under a curve, diagonal lines in `hatch` every `hatch-gap`.
- **Millimetre grid:** behind the piano roll's bar and beat lines, in `mm-line` every `mm-grid-gap`.
- **Corners:** small. `rounded-sm` for meters and small marks, `rounded-md` for controls, `rounded-lg` for panels.

## Components

Parts that appear more than once become shared components in `app/src/design/`, each with an entry here and its own tests.

**The rule: extract on the second use, not before.** The first time something is needed, build it where it's used. The second time, pull it out into `app/src/design/`, write its entry here, and have both places use it. Components designed on their own, before anything needs them, tend to be generic; ones pulled out of a real screen keep its character.

Each entry says what the component is for, its tokens and its states. One still marked provisional is a decision Will hasn't adopted yet (see the provisional decisions log).

### Choosing a control

The first row that fits wins:

| Control | Use when | Examples |
|---|---|---|
| **Fader** | Values compared side by side | Track and master volume |
| **Toggle** | On or off | Mute, solo, loop |
| **Segmented choice** | Five or fewer named options, all visible | Waveform |
| **Menu** | More options than that, or a list that changes | Snap, output device |
| **Drawing you can drag** | A shape: an envelope, a filter curve | None yet |
| **Drag field** | A crowded place where the exact number matters | Tempo |
| **Knob** | A setting in a compact group that stands on its own | Pan, the synth's settings |

Faders follow the mouse along their length. Knobs and drag fields are dragged up and down, never in a circle.

### Setting a number

The fader, the knob and the drag field all set a number, so they behave the same way, from one shared piece (`useNumberControl`) with one shared set of tests:
- **Drag** from wherever you press. Nothing jumps on press: the control moves from where it is, once the pointer has moved a few pixels, so a wobble during a click doesn't nudge it. A drag is one undo step.
- **Shift-drag** moves it in fine steps, a tenth of the speed. Shift can be pressed or let go part-way, and it carries on from where it is.
- **Click** gives it focus. The arrow keys step it, Shift+arrow by ten steps, Page Up and Page Down by ten, Home and End to the ends.
- **Reset** to its default with double-click, ⌥-click, or Delete or Backspace while it has focus (with ⌘, Ctrl or ⌥ held they're left to the app's own shortcuts, such as ⌘⌫ to delete a track). A reset is one undo step, and does nothing when it's already there.
- **The scroll wheel** leaves it alone, so scrolling a panel never changes a sound by accident.
- **During a drag** it shows where the mouse has taken it, not what the project last said, then the project's value once you let go.
- **The value and its unit** are always readable in the `number` type, whose digits don't shift as they change. To assistive tech it's a slider, with its range, and its value read out with the unit ("-6.0 dB").

### Fader

For values compared side by side, such as volume on every track. A label, the fader, and its value.
- **Drawing:** a faint rail (`ink-3` at `stroke-grid`), a tick where a reset sets it (`ink-3`), and an `ink` thumb pointing up at the value from under the rail. The hit area is 20px tall, with room (`space-3`) for the thumb at each end, and `rounded-md` corners for the focus ring.
- **Value:** beside it, in the `number` type in `ink`. The label is in the `text` type in `ink-2`.
- **States:** at rest as drawn. Hover and drag darken the rail to `ink-2`. With focus, the thumb is `selected`, because the arrow keys now move it. The focus ring (`selected` at `stroke-clip-selected`) shows for focus from the keyboard, or once a key is pressed after a click. The cursor is a left-right resize arrow.
- **Behaviour:** as in Setting a number, dragged left and right along the rail.

### Drag field

For a crowded place where the exact number matters, such as tempo. A label, and the value with its unit, which you drag or type. Provisional (D-18) until it's adopted.
- **Drawing:** the value and unit in the `number` type in `ink`, centred over a hairline (`ink-3` at `stroke-grid`, inset `space-3`), in a box `space-7` (24px) tall and at least 64px wide, with `rounded-md` corners for the focus ring. No outline at rest: it isn't a form. The label is in the `text` type in `ink-2`.
- **States:** at rest as drawn. Hover and drag darken the hairline to `ink-2`. With focus, the hairline is `selected`, because the arrow keys now move it; the value stays in `ink`, since blue text reads as a link. The focus ring shows as on the fader. The cursor is an up-down resize arrow.
- **Typing:** a click without dragging opens it for typing: a text box over it in the same type and place, the value selected to type over, a `selected` caret and selection wash (`selected-wash`), and the hairline heavier (`stroke-clip-selected`) in `selected`. Return sets it, as one undo step; Escape leaves it as it was, and so does anything it can't read; clicking away sets it. It reads units and shorthand ("120", "120 bpm", "1k", "250 ms", "−6"), and a value out of range goes to the nearest end.
- **Behaviour:** as in Setting a number, dragged up and down, a step for every 2px. The first click of a double-click opens the box and the second closes it and resets, so the box shows for a moment.

### Knob

For a setting in a compact group that stands on its own, such as pan or the synth's cutoff. A label, the knob, and its value underneath. Provisional (D-19) until it's adopted.
- **Drawing:** flat, with no bevel, shadow or metal. A thin arc for the range, 270° from bottom left to bottom right (`ink-3` at `stroke-grid`); the value as an `ink` arc and an `ink` indicator from near the centre out to the arc (`stroke-curve`); and a tick outside the arc where a reset sets it (`ink-3`). The value's arc runs from the start of the range, or from the middle for a centred setting such as pan. The dial is 32px across.
- **Value:** underneath, in the `number` type in `ink`. The dial and the value are one hit area, at least as big as the mute and solo keys. The label is above, in the `text` type in `ink-2`, and can be left out where the place around it says what it is (a track's pan, under mute and solo).
- **States:** as the fader's. Hover and drag darken the range arc to `ink-2`. With focus, the indicator is `selected`. The focus ring (`selected` at `stroke-clip-selected`, `rounded-md` corners) shows for focus from the keyboard, or once a key is pressed after a click. The cursor is an up-down resize arrow.
- **Behaviour:** as in Setting a number, dragged up and down, never in a circle. 200px of drag go from one end to the other, whatever the knob's size.

### Segmented choice

For five or fewer named options, all visible, such as the waveform. Provisional (D-20) until it's adopted.
- **Drawing:** a row of options in one `ink-3` outline (`stroke-grid`, `rounded-md` corners). Each option is its name in the `text` type in `ink-2`, with a drawing above it where one helps, such as a cycle of each wave. The chosen one is outlined in `ink`, over the row's outline, with its name and drawing in `ink`. Each option is at least `space-7` tall.
- **States:** hover shades an option with `line` and darkens it to `ink`; pressing shades it with `line-2`. The focus ring is `selected` at `stroke-clip-selected`, round the option with focus. Disabled options are `ink-3`, with no shading.
- **Behaviour:** a click chooses an option and gives it focus. The arrow keys move the choice, round from one end to the other. Only the chosen option is in the Tab order. To assistive tech it's a radio group named by its label.

### Key

For a single action, or a setting that's on or off with a word on it: Play, Stop, Loop, + Add track, the benchmark dialog's buttons (`Key`). Provisional (D-9) until it's adopted.
- **Drawing:** its words in the `text` type in `ink`, in a faint outline (`line-2` at `stroke-grid`) with `rounded-md` corners, padded `space-4` either side. At least `space-7` (24px) square to hit. Where it helps, a small mark drawn before the words in the words' ink: a triangle for Play, a square for Stop, and for Loop the ruler's dimension line in small.
- **Primary:** of a few side by side, the one that goes ahead is outlined in `ink`.
- **States:** hover shades it with `line`, pressing with `line-2`. Focus from the keyboard rings it in `selected` at `stroke-clip-selected`. Disabled, its outline is `line` and its words `ink-3`, with no shade. Play's mark is `live` while the song plays.
- **On and off:** a key that stays down, such as Loop, is a toggle button. Off, its words are `ink-2` and its mark `ink-3`. On, it's outlined in `ink` and shaded with `line-2`; hover lightens the outline to `ink-2`, and pressing lifts it to the hover shade, as if letting it up.

### Round key

For a small on-or-off setting marked with a letter, repeated on every row: a track's mute and solo (`RoundKey`). Provisional (D-12) until it's adopted.
- **Drawing:** an 18px circle (`ink-3` at `stroke-grid`) with its letter in the `number` type in `ink-2`, in a `space-7` square to hit. The square stays square, because a round button wouldn't respond in its corners.
- **States:** hover darkens the ring to `ink-2` and shades it with `line`; pressing shades it with `line-2`. On, it's filled `ink` with the letter in `paper`; hover lifts the fill to `ink-2`, and pressing presses it back to `ink`. Focus from the keyboard rings the circle, not the square, in `selected`. Disabled, the ring is `line-2` and the letter `ink-3`.
- **Behaviour:** a toggle button, named in full for assistive tech ("Mute Bass").

### Menu

For more options than a segmented choice holds, or a list that changes: snap, the buffer size (`Menu`). It opens the system's own list. Provisional (D-3) until it's adopted.
- **Drawing:** the chosen option in the `text` type in `ink`, with no outline, and a small chevron (`ink-2` at `stroke-clip`) in place of the native arrow. `space-7` tall, at least 64px wide, `rounded-md` corners.
- **States:** hover shades it with `line`, pressing with `line-2`. Focus from the keyboard rings it in `selected`. Disabled, its words and chevron are `ink-3`, with no shade.

### Tool chip

For the small tools over a canvas: snap and zoom on the timeline and the piano roll (`ToolChip`, with `ZoomKeys`). Provisional (D-3) until it's adopted.
- **Drawing:** a chip on its own `sheet`, edged in `line-2` at `stroke-grid`, `rounded-md` corners, with its caption ("Snap", "Time") in the `label` type in `ink-2`. Inside, a menu or flat keys in `ink` in the `text` type, each at least `space-7` square, with the smaller `rounded-sm` corners. Zoom is a − key and a + key either side of what they zoom.
- **States:** the keys inside shade with `line` on hover and `line-2` when pressed, ring in `selected` with focus, and are `ink-3` with no shade when disabled.
- **Behaviour:** the caption is for the eye; each control inside is named for assistive tech ("Clip snap", "Zoom in pitch"), and a chip of keys is a group.

### Section title

For naming a group of controls in a panel: the synth's Waveform, Filter and Envelope (`Group`).
- **Drawing:** the title in the `text` type in `ink-2`, `space-3` above what it names. No frame round the group: the space around it sets it apart.
- **Behaviour:** the group is a fieldset named by its title, or a radio group named by it, as the segmented choice is.

### Ruler

The bar ruler across the top of the timeline and the piano roll, drawn on their canvases (`drawRuler`), `ruler-height` tall. Provisional (D-2) until it's adopted.
- **Drawing:** on the canvas's `sheet`, with a line along its foot (`line-2` at `stroke-grid`). Bar ticks (`ink-3`, 8px) and beat ticks (`line-2`, 4px) stand on its foot, and each bar is numbered beside its tick in the `label` type in `ink-2`. Beat ticks are left out when they'd be closer than 6px, and bar numbers are kept at least 32px apart.
- **The loop region** is a dimension line along its foot: a line at `stroke-clip` from the loop's start to its end, with an end tick and an arrowhead at each end (the arrowheads only where there's room). While the loop is on, it's `ink` over a wash of `line` across the region; while it's off, it's `ink-3` with no wash.

### Playhead

Where the song is, on both canvases (`drawPlayhead`): a line in `live` at `stroke-playhead` from top to bottom, with a small triangle pointing down from the top of the ruler, centred on it.

### Selection box

The box dragged out to select, on both canvases, and the clip being drawn on the timeline (`drawSelectionBox`): a `selected-wash`, edged inside the box in `selected` at `stroke-selection-box`, so the edge never spills past it.

## Do's and Don'ts

- Do use tokens for every colour. Name a new colour for what it means and add it here first.
- Do keep orange for what's playing and blue for what's selected, and nothing else.
- Do give both themes equal care. Dark is the same drawing in layered tones.
- Don't draw skeuomorphic knobs, bevels, screws or brushed metal.
- Don't use neon on black, or white lines on black.
- Don't use decorative gradients, glows or drop shadows.
- Don't leave grey on grey with no hierarchy: rank things by ink weight.
- Don't leave defaults in place: every control's states are designed.
- Don't use jargon labels where a drawing or a plain word would do.
