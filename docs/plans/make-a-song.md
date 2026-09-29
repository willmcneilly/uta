# Project: Make a song

**Status:** Created · **RFC:** [RFC-003](../rfcs/rfc-003-make-a-song.md) · **Notion:** https://app.notion.com/p/Make-a-song-3ea3af969b6f811ab12dd0b602b37004

## Goal

Build RFC-003. You add synth tracks, arrange clips along a timeline, edit the selected clip in the piano roll below, and balance the tracks from their headers. You can play the whole song or a loop region, and notes that are already under way sound when playback starts mid-note. Underneath, each track has its own synth and slot on the audio thread, the output is stereo with a hard clip on the master, and snapshots match clips by ID.

## Where the code starts

Checked against `main` at `46676a2`, after UTA-16:

- **Core.** `SetLoopLength` writes `tracks[0].clips[0].length`. The first track's and clip's IDs are derived from the project ID, and command lists such as `examples/demo-loop.json` depend on that. Note IDs are only checked for uniqueness within a clip. `MixerStrip` has volume, pan and mute but no solo, and no command sets it. The master allows up to +6 dB in the core. The proptest strategies in `testing.rs` and most tests target track 0 and clip 0.
- **Trimming (UTA-16).** `Clip::trims_under` works out trims inside one clip, and the app joins them to the drag's undo step with `Session::join`. It never looks at other clips, so overlapping clips can't trim each other as long as it stays that way. The new commands fit `Session`'s existing `apply`/`amend`/`join`/`withdraw` without changes to it.
- **Engine.** `Snapshot` takes the synth settings from the first track and matches clips by their position in a flattened list. There's one merged event list and one bookmark, one 16-voice `Synth`, and mono output copied to every channel (`frame.fill`) with no clip. The engine clamps the master at 0 dB and never reads `MixerStrip`. Live `NoteOn`/`NoteOff` carry no track, and `Status` has one peak.
- **App.** `ProjectView` holds one `track` with one `clip`. `App.tsx` hard-codes that clip for the piano roll, the synth panel edits that track, and the stress notes go to `tracks[0].clips[0]`. The master volume control tops out at 0 dB.

## Tickets

### 1. [UTA-17] Add tracks, clips, the mixer and the loop region to the project core (Feature)

**Goal:** The project can describe a song: many tracks, each with a name and a full mixer strip, clips anywhere on them, and a loop region you can switch off. Every change is a command that undoes, replays and serialises. Nothing plays it yet beyond the first track.

**Acceptance criteria**
- [ ] Model: a track has a name and a mixer strip with volume (−60 to +6 dB), pan, mute and solo. A clip has a start, a length, and a content offset and content length, which are fixed at "no offset, same length as the clip" and not used yet. The transport has a loop start, a loop length and an on/off switch. The project reports where the song ends: one bar after the last clip ends.
- [ ] A track's name is stored in the track, chosen when the command is built: "Synth n", one more than the highest number in use, for duplicates too (open question 2). A new project has one synth track, "Synth 1", with an empty 4-bar clip, and the loop switched on over those 4 bars. The first track's and clip's IDs are still derived from the project ID, so existing command lists find them.
- [ ] New commands, each validated with a clear error: `AddTracks` and `RemoveTracks` (each other's inverse; a removal's inverse carries the whole track, its clips and its place in the order), `MoveTrack`, `SetTrackMixer`, `AddClips`, `RemoveClips` and `SetClips` (start, length and track, so a move to another track is one command), `SetLoop` (start and length in whole bars) and `SetLoopEnabled`. At most 32 tracks. `SetClips` and `SetTrackMixer` continue a drag through `Session::amend`.
- [ ] The command format goes up to 3. Format 2 lists still load, with `SetLoopLength` keeping its old meaning (the loop and the first track's first clip). `examples/demo-loop.json` loads unchanged.
- [ ] Note IDs are unique across the project: adding a note whose ID is used anywhere, by `AddNotes`, `AddClips` or `AddTracks`, is refused. The core has helpers that copy a clip or a whole track with new IDs for it, its clips and every note, for the app to build duplicates and pastes from.
- [ ] Clips can overlap on a track. Clip commands never trim notes, and `trims_under` stays within one clip.
- [ ] Tests pass:
  - every new command undoes to the exact previous state and round-trips through serialisation;
  - property tests: random series of all commands, old and new, across several tracks and clips, replay to the same project (the strategies in `testing.rs` cover many tracks and clips, not only the first);
  - a drag's worth of `SetClips` or `SetTrackMixer` undoes as one step; undoing a track delete restores its clips, sound, mixer and place exactly;
  - a note ID already used in another clip or track is refused; copied clips and tracks share no IDs with the originals;
  - trimming after an edit in one clip leaves an overlapping clip's notes alone.

**Out of scope:** Playing more than the first track (tickets 2 to 4). Any UI (tickets 5 to 8). A second kind of `Source`: it stays synth-only, so the irrefutable `let Source::Synth(..)` patterns are RFC-004's to change. Renaming tracks.

**Depends on:** Nothing. The engine and app keep using the first track until tickets 2 to 5 change them.

**Context:** RFC-003, "The shared model, extended" points 1 to 3, "Tracks" (limit, names, the new project), "Clips on the timeline" (overlaps), "Playing a song" (the loop region and the song's end), and "Saving later".

### 2. [UTA-18] Mix in stereo, with a hard clip on the master (Feature)

**Goal:** The engine's output is stereo. The first track's volume, pan and mute are applied, and a hard clip on the master means nothing past full scale ever reaches the device. It also confirms early that the compensated pan law leaves today's sound, and the golden WAV, unchanged.

**Acceptance criteria**
- [ ] The track's signal gets its volume, then the "−3 dB compensated" constant-power pan law (centre: unchanged in both speakers; hard to one side: +3 dB in that speaker, silent in the other), then mute. The master volume comes next, then a hard clip at full scale.
- [ ] The snapshot carries the first track's mixer strip. Track volume goes up to +6 dB, not clamped at 0 dB. The master keeps its current range and 0 dB ceiling in the engine. Volume, pan and mute changes glide rather than jump.
- [ ] A stereo device gets left and right. A mono device gets their average, so a centred track sounds as it does now. Channels past the second are silent.
- [ ] `Status` carries a running count of clips on the master.
- [ ] **The golden WAV is unchanged**, compared as usual, without regenerating it, and the existing sound tests pass unchanged. If it does change, stop and tell Will before regenerating.
- [ ] Tests pass:
  - volume changes the track's level by the expected dB; pan follows the law at centre, hard left, hard right and two points in between; mute silences it;
  - volume, pan and mute changes have no sample-to-sample jump above the click limit;
  - many loud notes never produce a sample past full scale, and the clip count goes up; a quiet mix passes through the hard clip bit for bit;
  - under `assert_no_alloc` and RealtimeSanitizer: a render with mixer changes and clipping.

**Out of scope:** More than one track, and solo (ticket 3). Any UI, including the clip light (ticket 5). A lookahead limiter.

**Depends on:** Nothing. It can run alongside ticket 1. Tests set the mixer strip in the snapshot directly, since no command sets it until ticket 1.

**Context:** RFC-003, "The mixer in the track headers" (pan law), "The master and headroom", "Risks & unknowns" (mono to stereo), and resolved open question 1.

### 3. [UTA-19] Play several tracks from a command list (Feature)

**Goal:** Every track plays through its own synth and its own slot on the audio thread, mixed with its volume, pan, mute and solo. `uta render --commands` and `uta play --commands` play a committed multi-track demo, so you hear a song-shaped arrangement before the timeline exists.

**Acceptance criteria**
- [ ] 32 track slots, each with its own 16 voices, a bookmark and a track buffer, all set aside when the stream starts. The control side gives each track a slot when it's added and keeps it for the track's life. A deleted track's voices fade out before its slot is reused. The audio thread holds only slot numbers, never references to snapshot data.
- [ ] Each track has its own event list, built from its clips. Snapshots match clips by ID: unchanged clips share their notes through the same `Arc`, even after tracks are reordered, and a new snapshot rebuilds only the tracks whose clips changed. The limit on events per block applies to each track. Overlapping clips on one track both play.
- [ ] Each track's own synth settings and mixer strip are used. A track plays if it isn't muted, and either no track is soloed or it is. Mute and solo changes glide like volume.
- [ ] `Status` carries a fixed array of 32 track peaks, the master peak and the master clip count. Live `NoteOn`/`NoteOff` carry a slot. The app keeps working on its first track until ticket 5.
- [ ] A multi-track demo (for example bass, chords and a lead, panned apart, with different sounds) is committed as `examples/demo-song.json`, with a golden WAV that a human approves in the PR. The timing report adds a many-tracks case (for example 32 tracks, 8 voices each) at p50 and p99.
- [ ] Tests pass:
  - two tracks with known tones render at the expected levels; the solo rule holds, including mute winning over solo;
  - adding, deleting, duplicating and reordering tracks while playing: no click and no stuck note; a deleted track's notes release; reordering doesn't change the audio at all;
  - unchanged clips share notes with the previous snapshot (by pointer), matched by ID after a reorder; editing one track rebuilds only that track's events;
  - blocks of 32, 128 and 1024 give the same audio with several tracks;
  - under `assert_no_alloc` and RealtimeSanitizer: a render that plays several tracks and adds, removes and reorders tracks and clips mid-render.

**Out of scope:** Playing past the loop, jumps and note chasing (ticket 4). The app showing more than one track (ticket 5).

**Depends on:** 1, 2.

**Context:** RFC-003, "The shared model, extended" points 4, 5, 7 and 8, "Tracks" (own voices), "The mixer in the track headers" (solo), and "Risks & unknowns" (CPU with many tracks).

### 4. [UTA-20] Play a whole song, and chase notes already under way (Feature)

**Goal:** The engine plays a song, not only a loop: from a start point to the song's end with the loop off, or round the loop region with it on. Starting, jumping or wrapping in the middle of a note starts that note straight away.

**Acceptance criteria**
- [ ] The engine follows the project's loop region and switch. With the loop off, Play runs from the play start to the song's end, stops there and goes back to the play start. Stop always goes back to where Play was last pressed.
- [ ] The control side can set the play start while stopped and jump while playing. A jump lands on the exact sample.
- [ ] Note chasing: on Play, on a jump and on a wrap round the loop, any note that should already be sounding starts straight away with an ordinary note on, and ends where it would have ended. The snapshot carries an index so the look-back is bounded, and chased notes count towards the track's events per block. Edits don't chase: a note moved under the playhead waits for its next pass.
- [ ] With the loop on, starting before the loop region plays into it and then goes round it. Starting after the region plays on to the song's end and stops, as if the loop were off (open question 1).
- [ ] `uta render` with the loop off renders the song from the top to its end, and the default length with the loop on stays twice round the loop. `uta play --from <bar>` starts playback mid-song, so chasing can be heard before the ruler exists.
- [ ] Tests pass:
  - with the loop off, playback stops at the song's end and returns to the play start; Stop returns to the play start; a jump while playing lands on the exact sample;
  - starting, jumping and wrapping the loop in the middle of a note starts it at once, at the expected pitch, and it ends where it would have ended; a chased note starts from its attack with no jump above the click limit; a note moved under the playhead by an edit doesn't start until its next pass;
  - under `assert_no_alloc` and RealtimeSanitizer: a render that plays several tracks, chases notes, wraps the loop, jumps, and adds, removes and reorders tracks and clips mid-render.

**Out of scope:** The loop switch, the ruler and following the playhead in the app (ticket 8). Chasing into the envelope (RFC-003 open question 2).

**Depends on:** 3.

**Context:** RFC-003, "Playing a song", "Risks & unknowns" (note chasing needs a fast lookup), and resolved open question 2.

### 5. [UTA-21] Show every track, and mix them from the track headers (Feature)

**Goal:** The window shows every track in a header column, each with volume, pan, mute, solo and a meter. You can add, delete, duplicate and reorder tracks, and the panel below edits the selected track's notes and sound. You can build and balance a layered loop in the app.

**Acceptance criteria**
- [ ] `ProjectView` carries every track in order (ID, name, mixer strip, synth settings, and each clip with its notes), the loop region and switch, and the song's end. The frame carries each track's peak, the master's peak and the clip count.
- [ ] The window has the transport on top, the track headers on the left of the (still empty) timeline area, and a bottom panel with **Notes** and **Sound** tabs, with a divider you can drag between them.
- [ ] Each header has the track's name, volume (−60 to +6 dB), pan, mute, solo (⌥-click solos it alone) and a meter that holds its peak for about a second, then falls at the IEC 60268-18 rate (20 dB in about 1.7 s). The master meter has a clip light that stays lit until you click it. Each drag is one undo step.
- [ ] **+ Add track** under the headers, and a new **Track** menu with Add, Delete and Duplicate. Duplicate copies the sound, mixer and every clip, with new IDs, using ticket 1's helpers. Dragging a header reorders the tracks. Each is one undo step.
- [ ] Clicking a header selects the track. Notes shows its first clip, or says it has none, until the timeline can select clips (ticket 6). Sound shows its synth. Auditioned notes and the stress notes go to the clip in the Notes tab.
- [ ] UI tests against the mocked back end: the headers draw what Rust sends; the mixer controls, Add, Delete, Duplicate and reordering send the expected commands; the tabs follow the selected track. Unit tests for the meter's hold and fall.

**Out of scope:** The timeline canvas and clip editing (ticket 6). The custom slider (the separate chore). A separate mixer view. Renaming and colouring tracks.

**Depends on:** 3. It can run alongside ticket 4.

**Context:** RFC-003, "The window", "Tracks", "The mixer in the track headers", "The master and headroom" (the clip light), and "The shared model, extended" points 6 and 7.

### 6. [UTA-22] Arrange clips on a timeline (Feature)

**Goal:** A canvas timeline next to the track headers shows every clip with a preview of its notes. You can draw, move and resize clips, and selecting one opens it in the piano roll.

**Acceptance criteria**
- [ ] The timeline is drawn like the piano roll: three stacked canvases (grid, clips, and a top layer for the playhead and drags) behind one drawing interface, drawing only what's in view. It has a ruler of bars, scrolls and zooms, and lines up with the track headers. Each clip shows a preview of its notes. Where clips overlap, the later one is drawn on top.
- [ ] Drag across empty space on a track to make a clip that long, or double-click for a one-bar clip. Drag a clip to move it along its track or onto another. Drag its right edge to resize it. Everything snaps to bars by default, with beat and off as other choices, and ⌘ turns snapping off for one drag.
- [ ] Each drag is one undo step, and Esc mid-drag puts everything back. Moving a clip never trims notes in the clips around it.
- [ ] Click a clip to select it, and Backspace deletes it. Selecting or double-clicking a clip opens it in the Notes tab and selects its track, so Sound shows that track's synth.
- [ ] The piano roll shows the selected clip in song time, with the space outside the clip shaded and the song's playhead.
- [ ] UI tests against the mocked back end: creating, moving (including to another track), resizing, deleting and Esc send the expected commands; the timeline draws what Rust sends. Unit tests for hit-testing (including overlaps) and snapping.

**Out of scope:** Selecting several clips, copy, paste and duplicate (ticket 7). The loop region on the ruler, ruler clicks and following the playhead (ticket 8). Resizing a clip's left edge, and clips that repeat their contents.

**Depends on:** 5.

**Context:** RFC-003, "The window", "Clips on the timeline", and "Risks & unknowns" (two busy canvases).

### 7. [UTA-23] Select, copy, paste and duplicate clips (Feature)

**Goal:** You can work on groups of clips as you do with notes: select several, move them together, and copy, paste, duplicate and delete them, so a 4-bar idea becomes a 16-bar song in a few keystrokes.

**Acceptance criteria**
- [ ] Box-select and Shift-click on the timeline. Dragging a selection moves it together, across tracks too, as one undo step. Backspace deletes the selection as one undo step.
- [ ] ⌘C, ⌘V and ⌘D in the Edit menu go to whichever view you last clicked in, the timeline or the piano roll.
- [ ] Paste puts clips on the selected track at the playhead, snapped. Duplicate puts the copy straight after the original. Clips copied from several tracks keep their layout: the topmost track's clips land on the selected track and the rest keep their distance below it. Any that would fall past the last track go on the last track, and pasting never creates tracks (open question 3). Each is one `AddClips`, so one undo step.
- [ ] Every copy gets new IDs for the clip and every note, using ticket 1's helpers, so copies are independent.
- [ ] UI tests against the mocked back end: selecting, moving a selection, copying, pasting, duplicating and deleting send the expected commands, and the Edit menu reaches the right view.

**Out of scope:** Linked copies. Anything the piano roll's selection doesn't already do.

**Depends on:** 6. It can run alongside ticket 8.

**Context:** RFC-003, "Clips on the timeline" (select, copy, paste, duplicate; copies are independent) and "Risks & unknowns" (focus and the Edit menu).

### 8. [UTA-24] Set the loop region, play the song through, and follow the playhead (Feature)

**Goal:** You set the loop region on the ruler and switch it on or off, choose where playback starts by clicking the ruler, and play the whole song through. The timeline and piano roll turn pages to follow the playhead.

**Acceptance criteria**
- [ ] Drag on the ruler to set the loop region, snapped to bars. A loop button in the transport switches it on or off. Both are undoable, one drag per step. The transport's loop length control goes, since the region replaces it.
- [ ] Clicking the ruler moves the play start, and clicking it while playing jumps there. Stop returns to the play start. With the loop off, playback stops at the song's end and returns to the play start. The position readout shows the song position.
- [ ] When the playhead reaches the right edge of the timeline, the view turns a page. Following pauses when you scroll or edit, and resumes on the next Play. The piano roll follows the same way.
- [ ] UI tests against the mocked back end: the loop region, the loop button and ruler clicks send the expected commands and engine calls; the views draw the loop region and playhead Rust sends. Unit tests for follow paging.

**Out of scope:** The metronome (Play it in). Markers, and showing the song's end on the ruler.

**Depends on:** 4, 6.

**Context:** RFC-003, "Playing a song" and "The window" (the transport).

## Chore outside the project

Not part of Make a song, but created in the same batch, with no project, as `Ready`: UTA-25.

### [UTA-25] Replace the built-in sliders with one that takes focus on click (Chore)

**Goal:** Every slider in the app works from the keyboard as soon as you click it, and can be set finely or reset. The web view's built-in range inputs don't take focus on click, so arrow keys only work after tabbing to them, and the track headers add many more sliders.

**Acceptance criteria**
- [ ] One slider component, used by every slider in the app when it lands: the transport, the synth panel, and the track headers if ticket 5 has merged.
- [ ] Clicking or dragging it gives it focus. Arrow keys step it, and Shift+arrow takes bigger steps. ⌥-drag moves it in fine steps. Double-click resets it to its default.
- [ ] `ProjectView` carries each control's default (master volume, tempo, synth settings, and the track mixer if present), so the UI never hard-codes them.
- [ ] A drag is still one undo step. It's accessible as a slider (role, value and range for assistive tech).
- [ ] UI tests: click focuses, arrow and Shift+arrow steps, ⌥-drag fine steps, double-click reset, each sending the expected commands.

**Out of scope:** Knobs, and any control that isn't a slider today.

**Depends on:** Nothing. Best picked up before or alongside ticket 5, so the headers are built on it; if it lands after, it swaps theirs.

**Context:** RFC-003, "Alternatives considered" (a custom slider in this project).

## Order and checkpoints

```
1 ─┐
   ├─► 3 ─┬─► 4 ─────────────┐
2 ─┘      └─► 5 ─► 6 ─┬──────┴─► 8
                      └─► 7
```

- Tickets 1 and 2 go first, in parallel. 3 needs both. After 3, tickets 4 (engine) and 5 (app) can run in parallel. 6 follows 5. 7 and 8 both follow 6, and 8 also needs 4.
- The slider chore can run any time. Doing it before or alongside 5 saves reworking the headers.
- **Checkpoint after 2:** the golden WAV is unchanged, so the pan law is proven not to change today's sound. `uta render loop.wav --commands examples/demo-loop.json` should sound exactly as before.
- **Checkpoint after 3:** `uta render song.wav --commands examples/demo-song.json`, or `uta play --commands examples/demo-song.json`. The first time Uta plays several tracks, each with its own sound, panned apart.
- **Checkpoint after 4:** `uta play --commands examples/demo-song.json --from 3` starts mid-song, and a held chord sounds straight away (manual check 4, early). With the loop off, the song plays to its end and stops. A good moment for the 10-minute run at `--buffer 64` (manual check 8, early).
- **Checkpoint after 5:** in the app, duplicate the first track, give each a different sound, and balance them with volume, pan, mute and solo, including ⌥-click and a muted soloed track (manual check 2). Push the levels until the clip light comes on (manual check 3).
- **Checkpoint after 6:** draw clips across tracks and move and resize them while it plays. Watch the frame readout with both canvases animating (the "two busy canvases" risk).
- **Checkpoint after 7 and 8:** RFC-003's "What it looks like to you", end to end, and manual checks 1 to 8. **Manual check 7 decides whether "send the UI only what changed" becomes a ticket.** That's the project done.

## Coverage

| RFC-003 verification item | Ticket |
|---|---|
| Commands: undo, serialisation, random replay of old and new | 1 |
| Commands: a drag of `SetClips` or `SetTrackMixer` is one undo step | 1 |
| Commands: undoing a track delete restores it exactly | 1 |
| Commands: format 2 lists and `demo-loop.json` still load and render the same | 1 (load), 2 (render) |
| Note IDs: a used ID is refused; duplicates get new IDs | 1 (and 5, 7 use it) |
| Mixing: volume in dB, pan law, mute, glides | 2 |
| Mixing: two tracks at expected levels, solo rule with mute winning | 3 |
| Headroom: never past full scale, clip count, quiet mix bit for bit | 2 |
| Tracks and slots: add, delete, reorder, duplicate while playing; deleted notes release; reorder doesn't change audio | 3 |
| Snapshots: clips shared by pointer, matched by ID after reorder; only the edited track rebuilt | 3 |
| Song playback: stops at the end, Stop returns, exact jumps | 4 |
| Note chasing: start, jump, wrap; attack from scratch; edits don't chase | 4 |
| Sound: block sizes with several tracks; multi-track golden WAV | 3 |
| Real-time safety: tracks, chasing, wraps, edits mid-render | 3 (tracks), 4 (all together) |
| Timing report: many-tracks case | 3 |
| UI: track headers and mixer controls | 5 |
| UI: creating, moving, resizing, deleting clips; hit-testing, snapping | 6 |
| UI: duplicating clips, Edit menu routing | 7 |
| UI: reordering tracks | 5 |
| UI: loop region, ruler clicks, follow paging | 8 |
| Manual 1: a song of several tracks played through with the loop off | 7 and 8 |
| Manual 2: volume, pan, solo, mute | 5 |
| Manual 3: clip light, never past full scale | 5 |
| Manual 4: start in the middle of a chord | 4 (from the CLI), 8 (from the ruler) |
| Manual 5: edit clips and tracks while playing | 7 |
| Manual 6: follow the playhead | 8 |
| Manual 7: stress notes, scroll, zoom, drag a volume and a clip | end of project |
| Manual 8: 10 minutes at buffer 64 | 4 (CLI), end of project (app) |

## Not in this project

- **Sending the UI only what changed** (RFC-003, shared model point 6). Only if manual check 7 shows lag when dragging a track's volume or a clip with the stress notes in. Then it becomes a ticket, designed as the RFC describes: an outline sent in full, and each clip's notes only when they change.
- **The custom slider.** A separate chore, above, created with this batch but outside the project.
- **The master's volume range.** RFC-003 leaves the master unchanged: the core allows up to +6 dB, while the engine and UI stop at 0 dB. Tracks get −60 to +6 dB throughout.
- **A second kind of track source** and the irrefutable `let Source::Synth(..)` patterns it breaks. That's RFC-004 (drums).
- **Everything RFC-003 leaves out:** drums, the metronome, saving, renaming and colouring tracks, a separate mixer view, effects, sends and buses, automation, tempo changes and time signatures other than 4/4, audio clips, clips that repeat their contents, trimming a clip's left edge, recording, a lookahead limiter and chasing into the envelope.

## Open questions

1. **Resolved: starting outside the loop region with the loop on.** Will agreed with the recommendation. RFC-003 didn't say. Recommendation: start before the region and it plays into the loop, then goes round it; start after the region and it plays on to the song's end and stops, as if the loop were off. As far as I know that's how Ableton behaves, and it means clicking anywhere on the ruler and pressing Play always plays from there.
2. **Resolved: track names are stored in the track.** Will agreed with the recommendation. Recommendation: store the name in the track, chosen when the command is built: "Synth n", one more than the highest number in use, for duplicates too. Deleting "Synth 2" then doesn't rename "Synth 3", and renaming later is just a command. The alternative, working the name out from the track's place, renames tracks whenever you reorder them.
3. **Resolved: pasting clips copied from several tracks.** Will agreed with the recommendation. RFC-003 says paste goes on the selected track. Recommendation: the clips keep their layout, with the topmost track's clips landing on the selected track and the rest keeping their distance below it. Any that would fall past the last track go on the last track. Pasting doesn't create tracks.
