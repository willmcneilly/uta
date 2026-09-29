# RFC-003: Make a song: tracks, a timeline and a mixer

**Status:** Accepted (2026-09-29) · **Author:** Will · **Notion:** https://app.notion.com/p/Make-a-song-tracks-a-timeline-and-a-mixer-3ea3af969b6f81dfb35bf67176f99f82

## Summary

Uta moves from one loop to a whole song. You add tracks, each with its own synth, and arrange clips along a timeline. The selected clip opens in the piano roll in a panel below, next to its track's sound controls. Each track has volume, pan, mute, solo and a meter. A loop region you can switch on or off replaces the fixed loop. Underneath, the engine gets its own synth for each track and a mixer, a hard clip on the master so summed tracks can't push the output past full scale, clips tracked by ID, and notes that start partway through ("note chasing") when playback begins mid-note. Saving, drums and the metronome are left out on purpose. This RFC keeps the model ready for saving without committing to a file format, and a short RFC-004 adds a drum machine as a second kind of track source.

## Motivation

- **Make a loop proved the foundations.** The piano roll ran at a steady 60 frames a second with 3,000 notes (17.0 / 18.0 ms at p50 / p99), sound is tested by measurement, and editing while playing doesn't click or leave notes stuck. The riskiest unknown from RFC-001, whether the web view is fast enough, is settled.
- **One loop isn't a song.** To make music you need more than one part, and a way to arrange parts over time. It's the next step towards something worth keeping, which is also when saving becomes worth designing.
- **Make a loop left known gaps that multiple tracks make urgent.** None of them matter with one track, and all of them do with many:
  - Nothing stops the output going past full scale. Four full-velocity notes already reach it.
  - Each track stores a volume, pan and mute, but the engine ignores them.
  - The engine matches clips between snapshots by their position in a list, not by ID.
- **Doing this before drums keeps each RFC one project.** Everything here is needed whatever the tracks play. Drums then need only a new kind of source and a drum editor (see "Next: drums").

## Proposal

### The window

```
┌──────────────────────────────────────────────────────────────────┐
│ Transport: ▶ ■  Loop ⟳  Tempo  Position  Master  Output            │
├──────────────┬───────────────────────────────────────────────────┤
│ Track headers│ Ruler (bars, loop region, playhead)                │
│ Synth 1 M S ▮│ ▭▭▭▭ clip ▭▭▭▭      ▭▭▭▭ clip ▭▭▭▭                  │
│ Synth 2 M S ▮│         ▭▭▭▭▭▭▭▭ clip ▭▭▭▭▭▭▭▭                      │
│ + Add track  │                                                    │
├──────────────┴───────────────────────────────────────────────────┤
│ [Notes] [Sound]   bottom panel for the selected clip's track      │
│ piano roll, or that track's synth controls                        │
└──────────────────────────────────────────────────────────────────┘
```

- **Timeline above, editor below,** as in Logic and Ableton. You can drag the boundary between them.
- **The bottom panel has two tabs** for the selected track. **Notes** is the piano roll for the selected clip. **Sound** is the synth panel from Make a loop, now showing that track's settings.
- **The transport** keeps Play, Stop, tempo, position, the master volume and the output device. It adds the loop switch.

### Tracks

- **Up to 32 tracks.** The audio thread sets aside room for 32 when the stream starts, so adding a track never allocates. 32 is a constant, like the 16 voices.
- **Every track in this project is a synth track.** Each has its own synth settings and its own 16 voices, so a pad with a long release can't steal voices from the bass. The model's track "source" can already hold other kinds, and RFC-004 adds drums.
- **Names are automatic** ("Synth 1", "Synth 2"). Renaming and colours are left for later.
- **Add** with a button under the headers or from a new **Track** menu. **Delete** and **Duplicate** are in the Track menu too. Duplicate copies the sound, the mixer settings and every clip, with new IDs. **Reorder** by dragging a header.
- **A new project** has one synth track with an empty 4-bar clip, and the loop switched on over those 4 bars. It opens much like Make a loop does today. Once RFC-004 lands, a new project gets a drum track as well.

### The mixer in the track headers

- **Each header has** volume, pan, mute, solo and a level meter. There's no separate mixer view. A mixer view with channel strips fits better when effects arrive.
- **Volume** runs from −60 dB to +6 dB, 0 dB by default.
- **Pan** uses the "−3 dB compensated" constant-power law. It's Logic's default, and Ableton's panner behaves the same way. A centred track plays at the same level it does today, and a track panned hard to one side is 3 dB louder in that speaker. Today's output is mono, copied to both speakers at full level. Centred, that's exactly what this law gives, so existing tests and the golden WAV shouldn't change.
- **Solo is additive:** click Solo on several tracks to hear them together. ⌥-click solos one track on its own, as in Logic.
- **The rule for what you hear:** a track plays if it isn't muted, and either no track is soloed or it is. So mute always wins, even on a soloed track. DAWs don't agree on this (Reaper lets solo win, Audition lets mute win). This rule is the easiest to explain, and the Mute button always means silent.
- **Mixer changes are commands.** Volume, pan, mute and solo are undoable, and one drag is one undo step, as with every other slider.
- **Meters:** the audio thread reports each track's peak level once per block, as a plain number. The UI holds the peak for about a second, then lets it fall. The fall rate follows the digital peak meter standard (IEC 60268-18: 20 dB in about 1.7 s).

### The master and headroom

- **Inside the mix, signals are floating-point numbers,** so adding tracks together never clips. It only clips where the sound leaves for the device, beyond full scale. Logic works the same way.
- **The master gets a safety stage at the end.** Nothing past full scale ever reaches the device, and a clip light on the master meter stays lit until you click it. This is a safety net, not a mastering tool.
- **The safety stage is a hard clip** at full scale (open question 1). It adds no delay, has no state and is trivial to prove. When it acts you hear distortion, which is also the warning. A lookahead limiter comes later as the first effect.
- **Headroom stays as it is:** each voice peaks at −12 dB, and the master defaults to −12 dB. A few tracks playing together at those levels stay well below full scale. When they don't, the clip light tells you.

### Clips on the timeline

- **The timeline is drawn on canvas,** built the same way as the piano roll: three stacked layers (grid, clips, and a top layer for the playhead and drags), behind one drawing interface, drawing only what's in view. Each clip shows a small preview of its notes.
- **Create:** drag across empty space on a track to make a clip that long, or double-click for a one-bar clip.
- **Move:** drag a clip along its track, or onto another track (for now every track is a synth track; see "Next: drums").
- **Resize:** drag the right edge. The left edge is left for later (see open question 4).
- **Everything snaps to bars by default,** with beat and off as other choices. Holding ⌘ turns snapping off for one drag, as in the piano roll.
- **Select, copy, paste, duplicate and delete** work like they do in the piano roll: box-select, Shift-click, ⌘C, ⌘V, ⌘D, Backspace. Edit menu actions go to whichever view you last clicked in. Paste puts clips on the selected track at the playhead. Duplicate puts the copy straight after the original.
- **Copies are independent.** A copy's notes are its own, with new IDs.
- **Clips can overlap on the same track,** and both play, as in Logic. The clip that starts later is drawn on top. Ableton instead trims the clip underneath, but that means a move quietly changes other clips, which makes undo harder to follow.
- **Double-click a clip,** or select it, to open it in the Notes tab. The piano roll shows that clip. Space outside the clip is shaded, and the playhead shows the song's position.

### Playing a song

- **The loop becomes a region on the ruler.** Drag on the ruler to set it, and switch it on or off with the loop button, like Logic's cycle. Its start and length snap to bars. The loop region and the switch are project data, so they're undoable.
- **With the loop off, Play runs from the playhead to the end of the song.** The end is worked out automatically as one bar after the last clip ends. Playback stops there and goes back to where it started.
- **Stop goes back to where you last pressed Play,** like Ableton and Logic's "return to last play start". That keeps you hearing the same passage while you work. Clicking the ruler moves the start point, and clicking it while playing jumps there.
- **The timeline follows the playhead:** when the playhead reaches the right edge, the view turns a page. Following pauses when you scroll or edit, as in Ableton, and resumes on the next Play. The piano roll follows the same way.
- **Note chasing.** Today a note only sounds if the playhead crosses its start. So if you press Play at bar 3 and a pad chord started at bar 2 and holds until bar 5, you hear nothing from that chord: its start has already gone by. "Chasing" means the engine looks back when playback starts, jumps or wraps round the loop, finds any notes that should already be sounding, and starts them straight away for the rest of their length.
  - **A chased note starts with an ordinary "note on",** so it plays its attack from scratch. That's what DAWs do with plugin instruments: as far as I know, the plugin formats have no way to say "start this note 2 seconds in". A pad that should be sustaining fades in instead. That's slightly misleading, but standard and simple. Our own synth could jump a voice to where its envelope would be, but that's extra work we're not doing now (open question 2).
  - **Effects can't be chased.** Once effects exist, a reverb or delay tail from notes before the start point won't be there, because nothing played them. As far as I know, no DAW reconstructs that in live playback, and nobody can for a plugin. Starting mid-song is a close preview. `uta render` always starts from the top, so renders are exact.
  - **Who else chases:** Ableton and Reaper both appear to chase notes by default. Logic has a separate switch for drum-style tracks, and RFC-004 will need an exception too.
  - **Edits don't trigger chasing.** A note moved *under* a playing playhead by an edit still waits for its next pass, as it does now, so dragging notes around while playing doesn't make them fire.

### The shared model, extended

These are the changes under the surface, in the same terms as RFC-002's "shared model".

**1. New commands.** They follow RFC-002's patterns: IDs are chosen before the command runs and travel inside it, each command's inverse is another command, and absolute "set" commands let the steps of a drag merge into one undo step.
- `AddTracks` and `RemoveTracks`: each is the other's inverse. The removal's inverse carries the whole track, so undoing a delete brings back its clips and sound. Duplicate is an `AddTracks` of a copy.
- `MoveTrack`, to reorder.
- `SetTrackMixer`: volume, pan, mute and solo, as absolute values.
- `AddClips`, `RemoveClips` and `SetClips`, mirroring the note commands. `SetClips` sets each clip's start, length and track, so moving a clip to another track is one command.
- `SetLoop` (start and length) and `SetLoopEnabled`.
- The command format goes up to 3. Format 2 command lists still load, and `SetLoopLength` keeps its old meaning in them: it sets the loop and the length of the first track's first clip. So `examples/demo-loop.json` and the golden WAV still work.

**2. Clips reserve room for repeating and trimming.** Each clip gets a content offset and a content length, separate from its position and length on the timeline, fixed for now at "no offset, same length as the clip" (open question 4). Nothing uses them yet; they mean repeating a clip's contents and trimming its left edge can come later without changing clips' shape.

**3. Note IDs are unique across the whole project, not just within their clip.** The engine identifies a sounding note by its ID, so two notes with the same ID on one track could release each other. Copying a clip or a track gives every note a new ID, chosen app-side when the command is built.

**4. Each track has a fixed slot on the audio thread.** The control side gives each track one of the 32 slots when the track is added and keeps it for the track's life. Reordering changes only where the header is drawn, and deleting frees the slot once the track's voices have faded out.
- Each slot holds that track's 16 voices and a bookmark into that track's own list of note events.
- The snapshot says which slot each track uses, and the audio thread only ever holds slot numbers, never references to snapshot data. That follows the plain-values rule in `CLAUDE.md`.
- Each block, every track with sound renders into its own buffer, set aside when the stream starts. Its volume and pan are applied, then it's added into the master buffer. The master volume and the safety stage come last.

**5. Snapshots match clips by ID, and each track keeps its own event list.** RFC-002 merged every clip's note events into one sorted list. With many tracks, one note edit would re-sort everything. Now each track has its own list, built from its clips, and a new snapshot rebuilds only the tracks whose clips changed. Unchanged clips, found by ID, keep sharing their notes through the same `Arc`. The fixed limit on events per block applies to each track.

**6. The UI keeps getting the whole project, until that's measured to be too slow.** After every change, including every step of a drag, Rust sends the UI the whole project view, every note included. With several tracks that grows, but it may still be fine. The stress check (manual check 7) will tell us. If drags lag, the fix is to send the view in two parts:
- the **outline:** tracks, mixer settings, clip positions and the transport, which is small and always sent in full;
- each clip's **notes**, sent only when that clip's notes have changed. In the core each clip's notes live behind an `Arc`, so checking whether they've changed is a quick comparison.

That becomes a ticket only if it's needed, as RFC-002 handled the same risk.

**7. Live notes are sent to a track.** The note you hear when placing it goes to the track of the clip you're editing. The engine's live note-on and note-off gain a track slot. Play it in's keyboard will play the selected track.

**8. The status carries a peak per track.** Each block's status holds a fixed-size array of 32 peaks, plus the master's peak and a count of master clips. It's plain values in the existing queue, so there's nothing new to free.

### Saving later

Saving is out of scope until Uta makes something worth keeping. These rules keep the model saveable in the meantime, without promising a file format:
- **Everything that matters lives in `uta-core`.** If it's needed to rebuild the song, it's in the project and changed by commands. View state (zoom, scroll, which tab is open) is deliberately outside it, and so will app settings such as the metronome be.
- **Everything has a permanent ID,** now including tracks and clips created after the project starts.
- **Anything outside the project is marked.** Nothing refers outside the project yet. Audio files and plugin state will be the first things that do. When they arrive, they should be separate kinds of content with their own IDs, not paths mixed into other data.
- **No format promise.** Until saving is designed, the project's shape can change without migrations. The one promise that already exists stays: old command lists still load, because tests and examples depend on them.
- **The likely direction** is saving the project as it stands, with a version number, as most DAWs do, rather than the list of commands that built it. It's noted here so nothing gets built that only works with one of the two. It isn't a decision.

### Next: drums (RFC-004, outlined)

- A second kind of track source: a synthesised drum machine with kick, snare, clap, closed and open hi-hat. Each sound has tune, decay and level. They're triggered by the General MIDI notes (36, 38, 39, 42, 46), and the closed hat cuts off the open one, as on an 808.
- The recipes come from the 808 and 909, with Mutable Instruments' Plaits and DaisySP (both MIT-licensed) as code to read. Noise is seeded the same way every time, so renders are exactly repeatable.
- For a drum track, the Notes tab shows five labelled rows instead of the keyboard. Its Sound tab shows the drum controls.
- Clips move only between tracks of the same kind, so a melody never ends up hidden and silent on a drum track.
- Drum tracks don't chase notes. Starting a drum hit halfway through sounds wrong.
- A new project gets a synth track and a drum track.
- Checked against this design: the track's source is already a list of kinds, each track has its own slot and voices, and the Notes and Sound tabs already switch with the selected track.

### Play it in (outlined in RFC-002)

Unchanged, except that the Mac keyboard plays the **selected track**, and recording puts a new clip on it at the play start. The **metronome** moves here too, with the count-in: it matters most when you're playing along. Research for it is done: a click on every beat, accented on the bar, made by the engine from the playhead, with its own switch and volume, and left out of renders unless asked for (Logic and Ableton leave it out of exports).

### Not in this project

Drums (RFC-004), the metronome (Play it in), the custom slider (a separate chore), sending the UI only what changed (unless measured to be needed), saving, renaming and colouring tracks, a separate mixer view, effects, sends and buses, automation, tempo changes or time signatures other than 4/4, audio clips, clips that repeat their contents, trimming a clip's left edge, and recording.

## What it looks like to you

You open Uta. There's one synth track with an empty 4-bar clip, the loop is on, and the piano roll below shows the clip. You draw a bassline and press Play, and it loops as it does today. You add a second track, drag out a 4-bar clip on it, and draw some chords. The bottom panel switches to that track, and on its Sound tab you give it a slow attack and a long release. You turn the chords down in their track header, pan them a little left, and watch the two meters move.

You select both clips and duplicate them three times, so the song is 16 bars. On bar 9 you change the bassline in the copy only. You switch the loop off, click bar 1 on the ruler and press Play. The song plays through, the timeline turns its pages as the playhead reaches the edge, and it stops after bar 16 and jumps back to bar 1. You solo the bass to check it, and ⌥-click solo on the chords to hear them alone. You click into the middle of a chord and press Play, and the chord sounds straight away rather than waiting for the next one.

You push both tracks and the master up until the clip light on the master comes on. What you hear gets harsh and distorted, but never louder than full scale. You click the light to reset it and pull the master back down. ⌘Z steps back through all of it, one drag at a time.

Nothing is saved when you close Uta yet. That comes once there's more worth saving.

## Alternatives considered

- **Share one pool of voices among all tracks.** It's less memory, and busy tracks could borrow voices from quiet ones. But a note on one track could then steal a voice from another, which is surprising and hard to test. Each voice would also need to look up its track's settings, and the audio thread would need a more complicated stealing rule. 32 × 16 separate voices is still only a small amount of memory, and only sounding voices cost CPU.
- **Keep one merged note event list for the whole song.** It's what the engine does today, with one bookmark. But every edit would rebuild and re-sort every track's events, and the events would need a track tag. Separate lists per track are simpler to rebuild, and each track's slot already needs its own voices.
- **Build the timeline from page elements (DOM).** A song has tens or hundreds of clips, not thousands, so the DOM would probably cope. But each clip draws a preview of its notes, the playhead animates over the lot, and the piano roll's canvas setup (layers, viewport, culling, the renderer interface) can be reused. The track headers *are* page elements, because there are at most 32 of them and they're full of controls that need focus and accessibility.
- **Trim the underlying clip when clips overlap (Ableton).** It keeps one clip playing at a time, which is tidy. But a move then quietly edits other clips, and undo has to restore them. Letting overlaps play (Logic) keeps each command about the clips you touched.
- **Solo wins over mute (Reaper).** It's useful when you want to hear a muted part briefly. But then the Mute button doesn't always mean silent, and there's no agreed standard to follow. We can add a preference later if it matters.
- **A lookahead limiter as a proper master effect now.** It sounds much cleaner than clipping when it acts. It's weighed in open question 1.
- **Send the UI only what changed, from the start.** It's likely to be needed eventually, and it's designed above. But it's a ticket of work against a problem we haven't measured yet, and Make a loop's full-project updates were fine with 3,000 notes. A generic patch of every change is the other way to do it. It loses to per-clip notes because nearly all of the project's data is notes, which already sit in per-clip `Arc`s.
- **A custom slider in this project.** The web view's built-in sliders don't take focus when clicked, so arrow keys only work after tabbing to them, and the track headers add many more sliders. But it's polish for the whole app, not song work, so it's a separate chore ticket (focus on click, arrow keys, ⌥-drag for fine steps, double-click to reset) that can be picked up any time.
- **Put tracks and clips in this RFC but leave the mixer until effects.** Volume, pan, mute and solo are already in the model, and a song you can't balance is hard to judge. The mixer view waits for effects, but the header controls can't.
- **Keep drums in this RFC.** It keeps everything that makes a song together, and several decisions (the new-project default, clip moves, drum rows) touch both. But it made one project of about 13 tickets. Splitting keeps each project reviewable and lets drums be designed and tested on their own.

## Risks & unknowns

- **CPU with many tracks.** 32 tracks each playing 16 voices is 512 voices. The timing report today covers 16. My guess is that a realistic song (8–12 tracks, a few voices each) is comfortably fine, and all 512 at once might not fit a 64-sample buffer. The timing report gains a many-tracks case, so we'll see where it ends up. If it's too slow, fewer voices per track or a lower track limit are both one-line changes.
- **Moving from mono to stereo processing.** The engine produces one mono signal and copies it to both speakers. Panning makes each track stereo. With the compensated pan law, a centred track should come out exactly as it does now, and the golden WAV shouldn't change. If it does, a human re-approves it.
- **Note chasing needs a fast lookup.** To chase, the audio thread has to find which notes are sounding at the new position. The snapshot will carry an index built for that, so the search is bounded. It's the same idea as the piano roll's note index (sorted by start, bounded by the longest note). Chased notes count towards each track's event limit per block.
- **The project is still big.** Without drums and the metronome, my guess is about 8 or 9 tickets, against Make a loop's 8. The ticket plan should put audible results early: several tracks playing from a command list before the timeline exists.
- **Two busy canvases at once.** The timeline and the piano roll both animate a playhead. Each only redraws its top layer per frame, so I expect it to be fine given the checkpoint numbers. But that's a guess until measured. The frame-time readout stays in the Develop menu.
- **Undoing a track delete brings back a lot at once.** The inverse carries the whole track. With thousands of notes that's one large command, but only on the control side and only once.
- **Focus and the Edit menu.** Copy, paste and duplicate now depend on which view you last clicked. If that turns out to be confusing, we could make the selection visible in both views and route by which view has a selection.

## How we'll verify it

**Automated, on every PR:**
- **Commands:** every new command undoes to the exact previous state and round-trips through serialisation. Random series of all commands, old and new, replay to the same project. A drag's worth of `SetClips` or `SetTrackMixer` undoes as one step. Undoing a track delete restores its clips and sound exactly. Format 2 command lists, including `demo-loop.json`, still load and render the same.
- **Note IDs:** adding a note whose ID is already used anywhere in the project is refused. Duplicating clips and tracks gives every note a new ID.
- **Mixing:** two tracks with known tones render at the expected levels. Volume changes a track's level by the expected dB. Pan follows the compensated law at centre, hard left, hard right and in between. Mute silences a track. Solo follows the rule, including mute winning over solo. Mixer changes glide without a jump above the click limit.
- **Headroom:** many loud tracks never produce a sample past full scale, and the clip count goes up. A quiet mix passes through the hard clip bit for bit unchanged.
- **Tracks and slots:** adding, deleting, reordering and duplicating tracks while playing causes no click and no stuck note. A deleted track's notes release. Reordering doesn't change the audio at all.
- **Snapshots:** unchanged clips share their notes with the previous snapshot (checked by pointer), matched by ID even after tracks are reordered. Editing one track rebuilds only that track's events.
- **Song playback:** with the loop off, playback stops at the song's end and returns to the play start. Stop returns to the play start. A jump while playing lands on the exact sample.
- **Note chasing:** starting playback, jumping and wrapping the loop in the middle of a note starts it at once, at the expected pitch, and it ends where it would have ended. A chased note starts its envelope from the attack, with no jump above the click limit. A note moved under the playhead by an edit doesn't start until its next pass.
- **Sound, as before:** blocks of 32, 128 and 1024 give the same audio with several tracks. A new golden WAV of a multi-track demo song, rendered from a committed command list. A human approves it in the PR.
- **Real-time safety:** a render that plays several tracks, chases notes, wraps the loop, and adds, removes and reorders tracks and clips mid-render runs under `assert_no_alloc` and RealtimeSanitizer.
- **Timing report:** adds a many-tracks case (for example 32 tracks, 8 voices each) at p50 and p99. Reported, not gated.
- **UI:** timeline and track header tests against the mocked back end. Creating, moving, resizing, duplicating and deleting clips, reordering tracks, the mixer controls, the loop region and ruler clicks all send the expected commands. The views draw what Rust sends. Unit tests for timeline hit-testing, snapping, and follow paging.

**Manual, by you:**
1. Build a song of three or four tracks and 16 or more bars, using duplicates. Play it through with the loop off. It stops at the end and returns to the start.
2. Balance it with volume and pan. Solo and mute tracks, including ⌥-click solo and a muted track that's also soloed.
3. Push the levels until the master's clip light comes on. It's never louder than full scale, and the light stays on until you click it.
4. Start playback in the middle of a long chord. It sounds straight away.
5. While it plays, move, resize, duplicate and delete clips, and add, delete, duplicate and reorder tracks. No clicks and no stuck notes.
6. Check the timeline follows the playhead page by page, stops following when you scroll, and resumes when you press Play.
7. Fill the song with stress notes across several tracks, then scroll and zoom both views while it plays, and drag a track's volume and a clip. It stays smooth, the frame readout stays near 16.7 ms, and drags don't lag. If they do, that's the signal to send the UI only what changed (shared model point 6).
8. Leave it playing for 10 minutes at buffer size 64. The dropout count stays at 0.

## Open questions

1. **Resolved: a hard clip on the master, with the clip light.** Will agreed with the recommendation. The question was: which safety stage on the master, a hard clip or a lookahead limiter?
   - **A hard clip** at full scale adds no delay, has no state and is trivial to prove. When it acts, you hear distortion, which is also a warning.
   - **A lookahead limiter** turns the level down smoothly just before a peak arrives, so it's much cleaner. The price is about 5 ms of extra delay on everything (Ableton's limiter offers 1.5 to 6 ms), a small delay buffer, and more code to test. Signalsmith's published design suits the audio thread: every step has a fixed cost, and it never overshoots.
   - **Recommendation: a hard clip with the clip light now,** and the limiter as the first effect once effects exist. The safety stage is there to protect ears and speakers when you make a mistake, not to sound good while you're making it.
2. **Resolved: chase with an ordinary "note on", always on for synth tracks.** Will's call: stick to the standard behaviour and skip special handling in our synth for now. The question was how a note that's already under way should start when you press Play in the middle of it. The options were:
   - don't chase (silence until the next note);
   - chase with a fresh attack, as DAWs do with plugins;
   - chase into the envelope, jumping the voice to where its envelope would be, which only works for our own sources.

   There's no setting for now. Drum tracks never chase (RFC-004).
3. **Resolved: the loop region is undoable.** Will agreed. The question was: should the loop region be undoable? It's project data here, like the loop length is today. Some DAWs treat the loop as a view setting that undo skips. Recommendation: undoable, as it is now. Changing it is deliberate, and undo bringing it back is more useful than surprising.
4. **Resolved: build neither, but reserve the fields.** Will agreed. The question was about clips that repeat their contents, and trimming a clip's left edge. You want repeating contents eventually, and both need the same addition to the model: a clip "content offset" and "content length" separate from its position and length on the timeline. Recommendation: build neither now. Leave room in the model by giving each clip those two fields, fixed at "no offset, same length as the clip". Adding the features later is then a UI and snapshot change, not a model change.
