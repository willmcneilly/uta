# RFC-002: Notes and a synth: making a loop

**Status:** Accepted (2026-09-27) · **Author:** Will · **Notion:** https://app.notion.com/p/Notes-and-a-synth-making-a-loop-3e83af969b6f81a0ba5df677f2e0a19c

## Summary

Uta moves from a test tone to making music. You draw notes in a piano roll, and a built-in synth plays them in a loop at the tempo you set. This RFC does two jobs. First, it settles the model all of Uta's composing work shares: how time is measured, what tracks and clips are, and how notes reach the audio thread on exactly the right sample. It does this so audio clips, plugins and generative tools can join later without a rewrite. Second, it details the first project, **Make a loop**: one track, one clip, a synth with a handful of knobs, and a piano roll. The two projects after it, **Make a song** and **Play it in**, are outlined so the design is checked against them. Each gets its own short RFC when its turn comes.

## Motivation

- **Milestone 0 proved the layers, but there's nothing to make yet.** You want to create music from scratch, starting by hand, with generative tools later. Notes and a synth are the shortest path to that.
- **It tests the biggest risk from RFC-001.** RFC-001 left web view performance to "find out when we build the timeline". A piano roll full of notes, with a moving playhead, is exactly that kind of busy view. If Tauri's web view can't keep up, we find out while there's little UI to redo.
- **Some decisions here are expensive to change later:** how time is stored, the shape of tracks and clips, and how notes are identified. Making them general now means audio clips, plugins and generative tools slot in later without rework.
- **It gives a project something worth saving.** Saving comes in Make a song, designed against real content, not guesses.

## Proposal

### Three projects, one design

| Project | What you can do at the end | Detailed in |
|---|---|---|
| **1. Make a loop** | Set a tempo and loop length, draw notes in a piano roll, shape a synth's sound, and hear it loop, editing while it plays. | This RFC |
| **2. Make a song** | Several tracks, each with its own synth, volume, pan and mute. Place and move clips on a timeline, use a metronome, save and open projects. | Outline here, its own RFC later |
| **3. Play it in** | Play notes on the Mac keyboard (like GarageBand's Musical Typing) and record them into a clip. | Outline here, its own RFC later |

### The shared model

These are the decisions all three projects depend on, and the reason this RFC covers more than project 1.

**1. Time is musical, and turned into samples only for playback.**
- The project stores every position and length as a whole number of **ticks**: fixed fractions of a beat, with 960 ticks per quarter note. That's the resolution Logic uses and a common one in MIDI files. 960 divides evenly by 3 and 5, so triplets and quintuplets land exactly on a tick. A note "on beat 3 of bar 2, an eighth long" is stored like that, not as a number of seconds or samples.
- A **tempo map** says how fast beats go: a list of sections, each starting at a tick with a steady tempo. Project 1 only ever has one section, but the shape allows tempo changes later, and gradual tempo changes can be added as a second kind of section, as Ardour does. A time-signature map works the same way, fixed at 4/4 for now.
- The engine turns ticks into sample positions using the tempo map and the device's sample rate. Each position is worked out directly from the start of its tempo section, never by adding up small steps, so rounding happens once and nothing drifts. The audio thread only ever compares whole sample numbers.
- The engine's control side keeps the latest musical version of the project and rebuilds the sample-based snapshot whenever the project or the sample rate changes, for example when a device switch changes the rate.
- When the tempo changes during playback, the playhead keeps its place in the music: it's converted to the new tempo at the moment the snapshot swaps, just as a device switch already keeps the playback position.
- Why: changing the tempo doesn't rewrite a single note, and the project doesn't depend on the device's sample rate. Audio clips will later sit on the same musical timeline and be converted the same way.

**2. A project holds tracks, and tracks hold clips.**
- A **track** is a chain: a **source** that makes sound (for now, always the built-in synth), then **effects** (an empty list for now), then a **mixer strip** (volume, pan, mute). Tracks feed the master output, which keeps milestone 0's master volume.
- A **clip** is a container placed on a track: a start, a length and its content. For now the content is always **notes**. Later, audio becomes a second kind of content, and moving, trimming and drawing clips on the timeline work for both without caring what's inside.
- A **note** has a pitch (MIDI note number, 0 to 127), a velocity (1 to 127), a start relative to its clip, and a length.
- Notes that fall outside their clip's length are kept but not played. So shortening the loop and lengthening it again brings them back, even without undo.
- In project 1 the project always has exactly one track and one clip, created with the project. The data model still allows many; project 2 adds the commands and the UI to create them.

**3. Everything has a permanent ID, chosen before the command runs.**
- Tracks, clips and notes each get a permanent ID, like the project already has.
- A new note's ID is picked on the app side when the command is built, before the project applies it, and it travels inside the command. If the project picked IDs while applying, replaying the saved commands would produce different IDs, and later commands would point at nothing.
- The new commands (names indicative, the tickets settle the details):
  - `AddNotes` and `RemoveNotes`: each is the other's inverse, so undoing a paste or a delete is one step.
  - `SetNotes`: sets the full pitch, start, length and velocity of existing notes. Because it sets absolute values, the steps of one drag merge into one undo step through the gesture mechanism UTA-5 built for the volume slider: the undo entry keeps the inverse from the drag's first step and takes the latest step as its command. Pressing Esc mid-drag puts everything back where the drag started.
  - Grouping is by gesture (one press, drag and release), not by time. Grouping by a time window, as some editors do, would merge two quick separate drags into one undo step.
  - `SetTempo`, `SetLoopLength` and `SetSynthParam`, all absolute values, so their drags merge the same way.
- The command format number goes up once, to cover all the new commands together.
- **Generative tools later** are just code that writes these same commands. Undo, replay and saving then work for them without extra work.

**4. Live notes take a separate, direct route to the engine.**
- Hearing a note when you place it isn't a change to the project, so it doesn't go through commands or undo. The UI asks the engine directly to start and stop a note, the same way Play and Stop work today.
- Project 3's computer keyboard, and a MIDI keyboard later, use the same route. Recording then turns what was played into one `AddNotes` command.

**5. The engine plays notes on the exact sample.**
- The audio thread works in blocks (128 samples, about 2.7 ms, by default). A note rarely starts exactly at the start of a block. The snapshot carries each clip's note starts and ends as one list sorted by time, and the engine keeps a bookmark into it. Within each block it plays up to the next note event, handles it, and carries on, so every note starts on its exact sample and timing doesn't depend on the buffer size. CLAP, VST3, JUCE and nih-plug all split blocks this way.
- When the loop end falls inside a block, the engine plays up to the loop end, moves the bookmark back to the loop start and carries on in the same block. This is Ardour's approach.
- Notes are trimmed to the loop when the snapshot is built: a note that runs past the loop end stops at the loop end. This is what Tracktion's engine does, and it guarantees every note ends before the loop starts again. Notes stuck on at the loop point are a recurring bug in other DAWs (Ardour's tracker has several), which is why the tests below target it by name.
- There's a fixed limit on events per block, so the work per block stays bounded however dense the notes are. Anything over the limit is dropped and counted in the status, like dropouts.

**6. The synth is ours, small and fixed-size.**
- It's polyphonic with 16 voices, all created when the stream starts, so playing a note never allocates. 16 is Surge's default voice limit and what nih-plug's example synth uses.
- When all voices are busy, a new note takes over a free voice first, then the oldest voice that's already fading out, then the oldest held note. A taken-over voice fades out over a few milliseconds before restarting, so it doesn't click.
- Each voice remembers which note it's playing, by the note's permanent ID. That's stricter than Tracktion, which only goes by pitch. When a new snapshot arrives, any voice whose note has been deleted, re-pitched, or moved away from the playhead is released, so editing while playing never leaves a note stuck on. Stopping releases every voice.
- Each voice has a band-limited oscillator (the "PolyBLEP" technique, which removes most of the harsh aliasing tones cheaply), a state-variable filter (Andrew Simper's design, used widely because it stays smooth while its cutoff is moving), and an envelope worked out every sample, so note starts are exact.
- We write these ourselves, but we don't invent anything. Each of the three pieces is a standard, well-documented design with published reference code (Simper's filter paper includes the exact equations), and together they come to a few hundred lines. Written this way they don't allocate by construction, they're easy to run under the real-time checks, and the sound tests and golden WAVs prove them, since nobody reads the code line by line. See the alternatives for why not FunDSP.

**7. Snapshots share what hasn't changed.**
- Each clip's notes sit in a shared, reference-counted container (Rust's `Arc`). Building a new snapshot after a change copies only the pointers of clips that didn't change, not their notes.
- A reference-counted value is freed by whichever thread lets go of the last reference. So the audio thread must never be the last holder. That holds as long as it only ever swaps whole snapshots and sends the old one back through the existing queue for the control side to drop, which is what it does today.
- **A new rule for the audio thread:** anything it keeps outside the snapshot, such as voices and the bookmark, holds only plain values like IDs and positions, never its own reference to shared data. The real-time tests check it: many snapshot swaps with shared data, run under `assert_no_alloc`, which catches frees as well as allocations.

**8. The piano roll and, later, the timeline are separate views.**
- The **piano roll** edits the notes in one clip. The **timeline** (project 2) arranges clips on tracks and draws them the same way whatever they contain.
- Both are drawn on canvas with our own pointer handling, as RFC-001 planned. Tauri's file drop disables the page's own drag and drop, but clips and notes never needed it.
- The piano roll uses three stacked canvases, the standard way to keep a canvas view fast:
  - the grid, redrawn only on scroll or zoom;
  - the notes, redrawn when the notes, the selection or the view change;
  - a top layer for the playhead, the selection box and drag previews, redrawn every frame.
- It only draws what's in view, and works out what's under the pointer itself, without page elements.
- The playhead moves smoothly between the engine's position updates by working out where it should be from the elapsed time.
- The drawing code sits behind one interface, so it can be swapped for WebGL if Canvas 2D isn't fast enough in Tauri. Signal, an open-source web MIDI editor, moved its piano roll to WebGL, and it still keeps a Canvas 2D fallback.

### Project 1: Make a loop

**The window**
- **Transport bar:** Play, Stop, tempo (20 to 300 BPM), loop length (1 to 16 bars) and the position in bars and beats. The output device, buffer size, dropout count and master volume from milestone 0 stay.
- **Piano roll**, the main area: a keyboard down the left for all 128 notes, bars and beats across the top, notes as rectangles, and the playhead moving across while playing. Scroll and zoom in both directions.
- **Synth panel:** waveform (sine, triangle, saw, square), filter cutoff and resonance, and an envelope (attack, decay, sustain, release). Every knob change is undoable, and one drag is one undo step.

**Piano roll edits**
- Click to draw a note, drag to set its length, and delete it with a click, Backspace or Delete.
- Drag notes to move them, and drag their ends to resize them. Everything snaps to a grid you choose (1/4, 1/8, 1/16, 1/32, or off). Esc during a drag cancels it.
- A velocity lane under the notes: drag a note's bar to change how hard it's played. Notes are also shaded by velocity.
- Box-select, Shift-click to add to the selection, move a selection together, ⌘C, ⌘V and ⌘D (duplicate). Copy and paste go through the Edit menu, next to Undo and Redo.
- A note sounds briefly when you place it, or when a drag moves it to a new pitch, even while stopped.
- Every edit works while the loop plays, and you hear it on the next pass.

**Engine and tools**
- The engine gains the transport's musical position and loop, the note sequencer, the synth, and the track chain feeding the master.
- `uta render` gains a `--commands <file>` option: it replays a list of commands (the same JSON the command format already defines) to build a project, then renders it. Tests use this to render exact musical situations, and it's the first real use of replay.

**Not in project 1:** more than one track or clip, the timeline, saving, a metronome, time signatures other than 4/4, tempo changes within the song, effects, automation, audio clips and live keyboard input.

### Project 2 outline: Make a song

- Add and remove tracks, each with its own synth settings, volume, pan and mute.
- Create, move, resize, duplicate and delete clips on a canvas timeline. Double-click a clip to open it in the piano roll.
- A metronome.
- **Save and open.** A notes-only project is self-contained, with no external files, which keeps this small. It needs its own RFC for the file format, the "write to a temporary file, then swap it in" safety RFC-001 describes, and whether autosave and the crash-recovery log come with it.
- Checked against this design: tracks and clips already exist in the model, the timeline draws clips without caring about their content, and saving writes the project that commands already build.

### Project 3 outline: Play it in

- Play the synth from the Mac keyboard, with keys for octave and velocity.
- Record what you play into a clip, snapped to the grid if you choose, as one undoable step.
- Known web view quirks to design around: keys are read by their physical position, so the layout works on any keyboard language; held keys auto-repeat and the repeats must be ignored; and on macOS a key released while ⌘ is held never reports its release, so held notes are released when ⌘ goes down or the window loses focus.
- A MIDI keyboard can join later through the same live route. The Mac's web view has no Web MIDI, so it would be read on the Rust side.
- Checked against this design: live notes already have their direct route, and recording ends in an ordinary `AddNotes` command.

## What it looks like to you

You open Uta and see an empty piano roll with a loop of 4 bars at 120 BPM. You click in a few notes for a bassline, drag one longer, and hear each note as you place it. You press Play and the loop starts, with the playhead sweeping across the notes and back to the start without a gap. While it plays, you move a note up an octave and hear it change on the next pass. You box-select the first bar, press ⌘D to repeat it, and turn a few notes' velocity down so they sit back. You switch the waveform to saw, sweep the filter cutoff down and hear the sound darken, lengthen the release, and the notes start to ring into each other. You set the tempo to 90 and the loop slows down with every note still on the beat. ⌘Z steps back through each of those changes one at a time: a whole drag undoes as one step.

Nothing clicks, no note gets stuck on, and the piano roll stays smooth with the playhead moving, even with a lot of notes.

It isn't saved yet. Closing Uta loses the loop until Make a song adds saving.

## Alternatives considered

- **Store time in samples or seconds.** It's simpler for the engine, and the audio thread uses samples anyway. But every tempo change would have to rewrite every note, and a project would depend on the sample rate it was made at. Musical time is what every major DAW stores.
- **Store time as fractional beats (floating point).** VST3 and Ableton's scripting interface describe positions this way, and there's plenty of precision. But grid positions like a third of a beat can't be stored exactly, two notes that should line up can end up a hair apart, and it's ambiguous whether a note right on a boundary is inside or outside it. Whole-number ticks line up exactly and replay identically.
- **One RFC detailing all three projects.** It keeps everything in one place, but projects 2 and 3 would be decided before we've learned anything from project 1, and the RFC would be hard to review. This RFC details the shared design and project 1, and outlines the rest.
- **A step sequencer instead of a piano roll.** It's simpler to build: a grid of on/off steps. But it can't do notes of varied lengths well, and you asked for a piano roll. A step sequencer is a good candidate for a generative tool later, writing into the same clips.
- **Build the synth from FunDSP.** FunDSP (MIT or Apache, actively maintained) is a good Rust library of oscillators, filters and envelopes, and RFC-001 named it as a source of building blocks. It loses for this synth on three counts. Its envelopes update about every 2 ms by default, which blurs exact note starts unless configured otherwise. It brings its own "swap in a new sound network" machinery, which would sit on top of our snapshot swap and duplicate it. And its recent versions renamed parts of its interface, so we'd be tracking changes. A middle path would be to use FunDSP's oscillator and filter on their own, without its network machinery, and write only the envelope. It saves little, because each piece is only tens of lines (most of the synth's code is voice handling, which is ours either way), and it still ties us to its interface changes. FunDSP stays a candidate for effects later, and as a reference to test our sound against.
- **Other ways to share data with the audio thread.** `arc-swap` is a popular crate for swapping shared data, but its readers can end up freeing old data on the audio thread; one Rust audio project hit exactly that and moved to the queue approach we already use. `basedrop` and `rtgc` defer frees to a background collector, which we'd only need if the audio thread held shared data outside a snapshot. The rule in the proposal means it doesn't.
- **Host an existing synth plugin (such as Surge XT, via CLAP) instead of building one.** You'd get a far better synth. But plugin hosting is a project of its own, and it adds another program's code to the audio thread before we've got our own notes playing. The track's source slot is where plugins go later.
- **Store raw MIDI messages instead of notes.** MIDI files store note-on and note-off messages separately. Editing is much easier with a note as one object with a start and length. Converting to MIDI for export is easy later.
- **Build the piano roll from page elements (DOM) or WebGL.** Page elements are the simplest to write, but slow down with thousands of notes. WebGL is the fastest, but much more work for a view that's mostly rectangles. Canvas 2D is what RFC-001 planned. If it falls short, WebGL is the next step.

## Risks & unknowns

- **Web view performance.** This is the test RFC-001 put off. Tauri apps on macOS are still capped at 60 frames a second on 120 Hz screens, as of February 2026, and there's one report of animation being smoother in Safari than in a Tauri window, with no cause found. I found no benchmarks of Canvas 2D drawing thousands of rectangles in the Mac's web view. If the piano roll stutters with the playhead moving, we'll know in project 1. The fallback is WebGL behind the same drawing interface, then Electron or native, as RFC-001 says.
- **Project 1 is bigger than Foundations.** A synth, a piano roll with four sets of edits, and the transport. My guess is 9 to 12 tickets. The ticket plan should put audible results early: notes playing from a command list before the piano roll exists.
- **Editing while playing is where the subtle bugs are.** Stuck notes, notes double-triggering at the loop point, or a click when a note is cut short. The tests below target each of these by name.
- **The synth could sound harsh, or dull.** Simple digital waveforms "alias", adding unwanted high tones, especially on high notes. PolyBLEP removes most of it cheaply, but it's an approximation, and it can sound slightly dull at the top end. The tests measure aliasing. If it's a problem, running the oscillators at a higher internal rate ("oversampling") fixes it at some CPU cost.
- **The full project goes to the UI after every change.** Today the whole project view is sent after each command. With hundreds of notes that's still small (my guess: fine up to a few thousand notes). If drags feel slow, we send only what changed.
- **Copy and paste in the web view.** ⌘C and ⌘V are handled by the native Edit menu, which also has to hand them to the piano roll. The volume slider's Undo and Redo already go through that menu, but copy and paste haven't been tried.
- **No saving until project 2.** Anything made in project 1 is lost when the app closes. That's acceptable if project 1 is about proving the loop, but see the open questions.

## How we'll verify it

**Automated, on every PR:**
- **Timing:** render a single note at a known tick at several tempos. Its first sample lands on the expected sample, at block sizes 32, 128 and 1024.
- **The loop point:** a note at the end of the loop plays once per pass, never twice and never cut off early. A note that crosses the loop end is released at the loop end. Many loops in a row show no drift: the start of pass 1,000 lands on the exact expected sample.
- **No stuck notes:** deleting, moving or re-pitching a note while it sounds releases it within a few milliseconds. Stop releases everything. The output is silent afterwards.
- **No clicks:** starting notes, releasing them, stealing voices, looping and editing produce no sample-to-sample jump above a set limit.
- **Pitch and level:** each of a range of MIDI notes measures at its expected frequency. Velocity changes the level by the expected amount.
- **Synth:** the envelope's attack, decay, sustain and release take the times they're set to. The filter cuts the expected amount above the cutoff. A high saw note shows aliasing below a set level.
- **Block-size independence** and a **golden WAV** of a short demo loop, rendered from a command list.
- **Real-time safety:** a render that plays notes, loops, steals voices and swaps snapshots with edits runs under RealtimeSanitizer and `assert_no_alloc`.
- **Commands:** every new command undoes to the exact previous state, survives being saved and loaded, and random series of them replay to the same project. A drag's steps merge into one undo step.
- **Time conversion:** ticks to samples and back is exact at every tempo and sample rate tested, over an hour of positions.
- **Tempo and rate changes:** changing the tempo during playback keeps the playhead at the same bar and beat, with no click and no skipped or doubled note. Moving the engine to a different sample rate (as a device switch does) keeps the loop in time.
- **UI:** piano roll tests against the mocked back end: clicking, dragging, selecting and pasting send the expected commands, and the view draws what Rust sends back.

**Manual, by you:**
1. Draw a bassline, press Play, and check it loops without a gap or a double note at the loop point.
2. While it plays, move, resize, delete and re-pitch notes. No clicks, no stuck notes.
3. Sweep each synth knob while it plays. It changes smoothly, and ⌘Z undoes each sweep as one step.
4. Change the tempo and the loop length while it plays.
5. Fill the loop with a lot of notes (a "stress" button can add a few thousand), then scroll and zoom while it plays. It should feel smooth.
6. Leave the loop playing for 10 minutes at buffer size 64. The dropout count stays at 0.

## Open questions

1. **Resolved: saving waits for project 2.** Will's call. Recommendation was: wait. Saving deserves its own design, and project 1's job is proving the loop. If losing loops on close is too annoying, a stopgap is to save the command list to a file and load it back with the same replay `uta render` uses.
2. **Resolved: 16 voices.** Recommendation was: 16, Surge's default. That's plenty for chords with long releases, and when they run out the oldest fading note is taken over first. It's a constant, easy to change.
3. **Resolved: a velocity lane under the notes.** Recommendation was: a lane, as most DAWs do. It shows every velocity at a glance.
4. **Resolved: snapping on by default, at 1/16.** Recommendation was: on, at 1/16, with ⌘ held while dragging to turn it off for that drag.
