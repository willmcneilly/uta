# Project: Make a loop

**Status:** Created · **RFC:** [RFC-002](../rfcs/rfc-002-make-a-loop.md) · **Notion:** https://app.notion.com/p/Make-a-loop-3e83af969b6f81cb89c7d758a6657e16

## Goal

Build project 1 from RFC-002. You set a tempo and a loop length, draw notes in a piano roll, shape a synth's sound and hear it loop, editing while it plays, with every change undoable. Underneath, the shared model RFC-002 settles (musical time, tracks, clips and notes with permanent IDs, sample-exact playback) is in place for Make a song and Play it in. We also find out whether a Canvas 2D piano roll is smooth enough in Tauri's web view.

## Tickets

### 1. [UTA-8] Add musical time, tracks, clips and notes to the project core (Feature)

**Goal:** The project stores music, not just a volume. It has a tempo, a loop, one track with a synth and one clip of notes, and every change to them is a command that undoes, replays and serialises. Nothing plays it yet.

**Acceptance criteria**
- [ ] Musical time in `uta-core`: positions and lengths are whole ticks at 960 per quarter note. A tempo map (one section for now, shaped to hold more) and a time signature fixed at 4/4. Converting ticks to samples and back works from the start of the tempo section, never by adding up steps.
- [ ] A new project has one track and one clip, each with a permanent ID. The track has a source (the synth's settings: waveform, cutoff, resonance, attack, decay, sustain, release), an empty effects list and a mixer strip (volume, pan, mute). The clip has a start, a length and its notes. The defaults are 120 BPM and a 4-bar loop.
- [ ] A note has a permanent ID, a pitch (0–127), a velocity (1–127), a start relative to its clip and a length. Notes outside the clip's length are kept.
- [ ] New commands, each validated with a clear error: `AddNotes` and `RemoveNotes` (each other's inverse), `SetNotes` (absolute values), `SetTempo` (20–300 BPM), `SetLoopLength` (1–16 bars) and `SetSynthParam`. The loop is stored on the transport, and `SetLoopLength` sets the loop and the clip's length together, undoing both in one step. New IDs travel inside the command. The command format goes up to 2, once, for all of them, and format 1 commands still load.
- [ ] `SetNotes`, `SetTempo`, `SetLoopLength` and `SetSynthParam` merge into one undo step through `Session::amend` when they continue the same change (for `SetNotes`: the same notes).
- [ ] Tests pass:
  - every new command undoes to the exact previous state, and round-trips through serialisation;
  - property test: random series of the new commands replay to the same project;
  - a drag's worth of `SetNotes` undoes as one step;
  - ticks to samples and back is exact over an hour of positions at several tempos and at 44.1, 48 and 96 kHz.

**Out of scope:** Playing any of it (tickets 3 and 4). Commands to add tracks or clips (Make a song). Saving.

**Depends on:** Nothing.

**Context:** RFC-002, "The shared model" points 1–3.

### 2. [UTA-9] Build the synth and play it with live notes (Feature)

**Goal:** The engine has a 16-voice synth that you can start and stop notes on directly, through the engine's command queue, the route the piano roll and keyboard will use to audition notes. Its sound is proven by measurement, and it follows the audio-thread rules.

**Acceptance criteria**
- [ ] 16 voices, all created when the processor is, so playing a note never allocates. When all are busy, a new note takes a free voice, then the oldest fading-out voice, then the oldest held one. A taken-over voice fades out over a few milliseconds first.
- [ ] Each voice has a PolyBLEP oscillator (sine, triangle, saw, square), a Simper state-variable low-pass filter (cutoff, resonance) and an envelope (attack, decay, sustain, release) worked out every sample.
- [ ] The synth's settings travel in the engine's snapshot. Changing one while notes sound glides rather than jumps, so a filter sweep doesn't click.
- [ ] `Controller` can start and stop a note directly (pitch, velocity), without going through the project.
- [ ] Sound tests pass, driving the real process path offline:
  - a range of MIDI notes measure at the expected frequency;
  - velocity changes the level by the documented amount;
  - attack, decay, sustain and release take the times they're set to;
  - the filter cuts the expected amount above the cutoff;
  - a high saw note's aliasing is below a set level;
  - no sample-to-sample jump above the limit when notes start, release, get taken over or settings sweep;
  - blocks of 32, 128 and 1024 give the same audio.
- [ ] Real-time safety: notes, voice take-overs and snapshot swaps run under `assert_no_alloc` and RealtimeSanitizer. The timing report includes all 16 voices sounding.

**Out of scope:** Playing notes from the project (ticket 3). The milestone 0 tone stays for Play until ticket 3 replaces it. Any UI.

**Depends on:** Nothing. It can run alongside ticket 1: the engine keeps its own synth settings type, and ticket 3 fills it from the project.

**Context:** RFC-002, "The shared model" points 4 and 6.

### 3. [UTA-10] Play a loop of notes from a command list (Feature)

**Goal:** The engine plays the project's notes in a loop at its tempo, every note on its exact sample. `uta render --commands` builds a project from a list of commands and renders it, so you can hear a loop before there's a piano roll.

**Acceptance criteria**
- [ ] `Snapshot::from(&Project)` holds the notes as one list of note starts and ends in samples, sorted by time and trimmed to the loop, with each clip's notes in an `Arc`. It's rebuilt when the project or the sample rate changes. It also carries the synth's settings and the master volume.
- [ ] The transport has a musical position and a loop. Within each block the engine plays up to the next note event, handles it and carries on. When the loop end falls inside a block, it goes back to the loop start in the same block.
- [ ] There's a fixed limit on note events per block. Anything over it is dropped and counted in the status, like dropouts. The status reports the playhead as a musical position.
- [ ] The notes and synth replace the milestone 0 tone. `uta render <file> --commands <json>` and `uta play --commands <json>` build a project from a command list and play it. A short demo loop's command list is committed.
- [ ] Tests pass:
  - a single note at a known tick starts on the expected sample at several tempos, at block sizes 32, 128 and 1024;
  - a note at the end of the loop plays once per pass, never twice and never cut short; a note crossing the loop end is released there; pass 1,000 starts on the exact expected sample;
  - blocks of 32, 128 and 1024 give the same audio;
  - a golden WAV of the demo loop (a human approves it in the PR);
  - under `assert_no_alloc` and RealtimeSanitizer: a render that plays notes, loops and takes over voices.

**Out of scope:** Editing while it plays (ticket 4). The window (ticket 5). Until ticket 5, the app's Play plays an empty loop, so it's silent.

**Depends on:** 1, 2.

**Context:** RFC-002, "The shared model" points 1, 5 and 7, and "Engine and tools".

### 4. [UTA-11] Edit a playing loop without stuck notes or clicks (Feature)

**Goal:** Changing the project while the loop plays is safe: notes, tempo, loop length and the sample rate can all change mid-note, and nothing sticks, clicks, skips or doubles. This is where the subtle bugs are, so it gets its own review.

**Acceptance criteria**
- [ ] Each voice remembers its note by permanent ID. When a new snapshot arrives, a voice whose note was deleted, re-pitched or moved away from the playhead is released. Stop releases every voice.
- [ ] Changing the tempo while playing keeps the playhead at the same bar and beat, converted when the snapshot swaps. Shortening the loop so the playhead is past the new end carries on from the loop start.
- [ ] Moving the engine to a new sample rate (as a device switch does) rebuilds the snapshot and keeps the loop in time.
- [ ] State the audio thread keeps outside the snapshot (voices, the bookmark) holds only plain values, never its own reference to shared data. This rule is added to `CLAUDE.md`'s audio-thread rules.
- [ ] Tests pass, rendering offline with edits sent mid-render:
  - deleting, moving or re-pitching a sounding note releases it within a few milliseconds; after Stop the output is silent;
  - a tempo change mid-loop keeps the bar and beat, with no click and no skipped or doubled note;
  - a sample-rate change keeps the loop in time;
  - no sample-to-sample jump above the limit across any of these;
  - many snapshot swaps sharing clip data run under `assert_no_alloc` and RealtimeSanitizer.

**Out of scope:** The UI for any of these edits (tickets 5–8).

**Depends on:** 3.

**Context:** RFC-002, "The shared model" points 1, 6 and 7, and "Risks & unknowns" (editing while playing).

### 5. [UTA-12] Show the loop in the window: transport and piano roll (Feature)

**Goal:** The window shows the loop and plays it: a transport bar with tempo and loop length, and a piano roll that draws the notes with a moving playhead. A stress option fills the loop with thousands of notes, so we learn early whether Canvas 2D is smooth enough in Tauri.

**Acceptance criteria**
- [ ] Transport bar: Play, Stop, tempo (20–300 BPM), loop length (1–16 bars) and the position in bars and beats. Tempo and loop length changes are undoable, one drag per undo step. The output device, buffer size, dropout count and master volume stay.
- [ ] `ProjectView` carries the tempo, loop, notes and synth settings. The frame carries the musical position.
- [ ] Piano roll: a keyboard for all 128 notes down the left, bars and beats across the top, notes as rectangles shaded by velocity, and a playhead that moves smoothly between frames by working out its place from the elapsed time. Scroll and zoom in both directions.
- [ ] It's drawn on three stacked canvases (grid, notes, top layer) that redraw only when they need to, draws only what's in view, and sits behind one drawing interface so it could swap to WebGL.
- [ ] A developer menu item adds a few thousand notes as one undoable `AddNotes`, and a frame-time readout shows how long frames take (p50 and p99).
- [ ] UI tests against the mocked back end: the view draws the notes Rust sends, and the tempo and loop length controls send the expected commands. Unit tests for the tick-to-pixel maths and the visible-range culling.

**Out of scope:** Editing notes (ticket 6) and the synth panel (ticket 8).

**Depends on:** 3.

**Context:** RFC-002, "The shared model" point 8, and "Project 1: Make a loop", "The window".

### 6. [UTA-13] Draw, move and resize notes in the piano roll (Feature)

**Goal:** You can write a loop by hand: draw notes, move them, resize them and delete them, snapped to a grid, hearing each one as you place it, while the loop plays.

**Acceptance criteria**
- [ ] Click empty space to draw a note and drag to set its length. Click a note to select it. Double-click a note, or press Backspace or Delete, to remove it.
- [ ] Drag a note to move it in time and pitch, and drag its ends to resize it. Everything snaps to a grid (1/4, 1/8, 1/16, 1/32 or off), 1/16 by default. Holding ⌘ turns snapping off for that drag.
- [ ] Each drag sends `SetNotes` steps under one gesture, so it's one undo step. Esc mid-drag puts everything back where the drag started.
- [ ] Placing a note, or dragging one to a new pitch, plays it briefly through the live note route, even while stopped.
- [ ] New notes get their permanent IDs when the command is built, before the project applies it.
- [ ] UI tests against the mocked back end: drawing, moving, resizing, deleting and Esc send the expected commands and gestures. Unit tests for hit-testing and snapping.

**Out of scope:** Multiple selection, copy and paste, and the velocity lane (ticket 7).

**Depends on:** 4, 5.

**Context:** RFC-002, "Piano roll edits", and "The shared model" points 3 and 4.

### 7. [UTA-14] Select many notes, copy, paste, duplicate and set velocity (Feature)

**Goal:** You can work on groups of notes: select them, move them together, copy, paste and duplicate them, and set how hard each note plays in a velocity lane.

**Acceptance criteria**
- [ ] Box-select, and Shift-click to add or remove a note. Dragging a selection moves it together as one undo step. Backspace deletes the selection as one undo step.
- [ ] Copy, Paste and Duplicate sit in the native Edit menu (⌘C, ⌘V, ⌘D) next to Undo and Redo, and reach the piano roll. Paste puts the notes at the playhead, snapped to the grid. Duplicate puts the copy right after the selection. Each is one `AddNotes`, so one undo step.
- [ ] A velocity lane under the notes: drag a note's bar to change its velocity, and with several selected, they change together. One drag is one undo step.
- [ ] UI tests against the mocked back end: selecting, moving a selection, copying, pasting, duplicating and velocity drags send the expected commands.

**Out of scope:** Quantise, transpose and other bulk edits.

**Depends on:** 6.

**Context:** RFC-002, "Piano roll edits".

### 8. [UTA-15] Shape the sound from a synth panel (Feature)

**Goal:** You can change the synth's sound while the loop plays: waveform, filter and envelope, each change undoable as one step per drag.

**Acceptance criteria**
- [ ] A synth panel with the waveform (sine, triangle, saw, square), filter cutoff and resonance, and attack, decay, sustain and release.
- [ ] Each control sends `SetSynthParam`. One drag is one undo step, and Undo and Redo move the panel with them.
- [ ] UI tests against the mocked back end: each control sends the expected command and shows what Rust sends back.
- [ ] Verification notes record sweeping each control while the loop plays, on Will's machine.

**Out of scope:** Any synth feature beyond RFC-002's list: more filter types, LFOs, presets.

**Depends on:** 5. It can run alongside tickets 6 and 7.

**Context:** RFC-002, "The window" (synth panel), and "The shared model" point 6.

## Order and checkpoints

```
1 ──┐
    ├──► 3 ──► 4 ─────────┐
2 ──┘     └──► 5 ──────────┴──► 6 ──► 7
               └──► 8
```

- Tickets 1 and 2 go first, in parallel. Then 3. After 3, tickets 4 and 5 can run in parallel. 6 needs both 4 and 5. 8 only needs 5, so it can run alongside 6 and 7.
- **Checkpoint after 3:** run `uta render loop.wav --commands <demo file>` and listen, or `uta play --commands <demo file>`. The first notes you hear from Uta.
- **Checkpoint after 5:** open the app, add the stress notes, press Play, then scroll and zoom while it plays, and change the tempo and loop length. **This is the web view decision point.** If it stutters, we stop and add a WebGL ticket before 6, rather than building editing on a view we'll replace.
- **Checkpoint after 6:** draw a bassline, loop it, and move, resize, delete and re-pitch notes while it plays (RFC manual checks 1 and 2).
- **Checkpoint after 8:** sweep each synth control while it plays, and undo each sweep (manual check 3).
- **Checkpoint after 7 and 8:** RFC-002's "What it looks like to you", end to end, and manual checks 4–6. That's project 1 done.

## Coverage

| RFC-002 verification item | Ticket |
|---|---|
| Timing: note on its exact sample, several tempos and block sizes | 3 |
| Loop point: once per pass, released at loop end, no drift over 1,000 passes | 3 |
| No stuck notes: delete, move, re-pitch while sounding; Stop releases all | 4 |
| No clicks: start, release, take-over | 2 |
| No clicks: looping and editing | 3, 4 |
| Pitch and level, velocity | 2 |
| Synth: envelope times, filter, aliasing | 2 |
| Block-size independence | 2, 3 |
| Golden WAV of a demo loop from a command list | 3 |
| Real-time safety: notes, loops, take-overs | 2, 3 |
| Real-time safety: snapshot swaps with shared data and edits | 4 |
| Commands: undo, save and load, replay, a drag as one undo step | 1 |
| Time conversion exact over an hour | 1 |
| Tempo change keeps bar and beat; sample-rate change keeps time | 4 |
| UI: piano roll tests against the mocked back end | 5, 6, 7 |
| Manual 1: loop without a gap or double note | 6 |
| Manual 2: edit while it plays | 6 |
| Manual 3: sweep synth knobs, undo each sweep | 8 |
| Manual 4: tempo and loop length while playing | 5 |
| Manual 5: stress notes, scroll and zoom | 5 (and again after 7) |
| Manual 6: 10 minutes at buffer 64, no dropouts | 7 (end of project) |

## Not in this project

- **Everything RFC-002 leaves out of project 1:** more than one track or clip, the timeline, saving, a metronome, other time signatures, tempo changes within the song, effects, automation, audio clips and live keyboard input. They belong to Make a song and Play it in, each with its own RFC.
- **A WebGL renderer.** Only if ticket 5's checkpoint shows Canvas 2D isn't smooth enough. Then it becomes a ticket before 6.
- **Oversampling the oscillators.** Only if ticket 2's aliasing test shows PolyBLEP isn't enough.
- **Sending only what changed to the UI.** Only if drags feel slow with many notes. Ticket 5's stress test tells us.

## Open questions

1. **Resolved: loop length changes the loop and the clip together.** RFC-002 says shortening the loop keeps notes past the clip's end, which ties the loop to the clip's length. Recommendation: the project stores the loop on the transport, and in project 1 `SetLoopLength` sets the loop and the one clip's length together, undoing both in one step. In Make a song, clips get their own lengths and the loop stays a transport setting. That's how Logic and Ableton's arrangement loop work, and it doesn't paint project 2 into a corner.
2. **Resolved: double-click deletes a note.** RFC-002 says "delete it with a click", but a single click on a note also needs to select it, for Backspace and dragging. Recommendation: double-click a note to delete it, as in Ableton's piano roll. A single click selects, and a click on empty space draws.
