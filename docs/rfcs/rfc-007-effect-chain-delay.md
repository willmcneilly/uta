# RFC-007: Effect chain and Delay

**Status:** Accepted (2026-10-11) · **Author:** Will · **Notion:** https://app.notion.com/p/Effect-chain-and-Delay-3f63af969b6f81edb99fc507ee9794ba

## Summary

Give every track, and the master, a chain of up to eight **insert effects**, shown on the Sound tab after the synth or kit, and add the first effect: a tape-style **Delay**. You add effects from a **+** menu at the end of the chain, drag them to reorder, switch each one on and off, and set how much of it you hear with its own Mix. Every change can be undone. The Delay syncs to the tempo or runs in milliseconds, works in stereo or ping-pong, and sends its echoes round a loop that darkens, saturates and wobbles them like tape. Its echoes ring on after Stop. Effects get one RFC each, and this one is the first, so most of its weight is in the patterns the rest will reuse: how an effect is stored and undone, how its memory reaches the audio thread, how it rings out and goes to sleep, and how it's tested.

## Motivation

- **Sound decides whether Uta continues.** RFC-006 gave Uta a drum machine. A dry synth and a dry kit still sound like a demo. Space and movement are what make a loop sound like music, and a delay is the cheapest way to get both.
- **Six effects are planned:** EQ, Reverb, Delay, Compressor, Bitcrusher and Distortion, one RFC each, so none of them is too much at once. For the later ones to be written and built in parallel, the first has to settle every shared question cleanly, once.
- **Delay goes first because it asks the hardest shared questions.**
  - It needs a lot of memory set aside up front for its echoes, and how much depends on the sample rate.
  - It goes on sounding after its input stops, including after Stop.
  - It follows the tempo.

  A compressor or EQ would leave all three unanswered, and the reverb RFC would have had to reopen the patterns.
- **The model is ready for it.** Since RFC-002, a track has been "a source, then effects, then a mixer strip", with the effects always an empty list. This fills it in.

**When:** after RFC-006 (Drums) closes. It can run alongside RFC-008 (Synth features), which is being scoped in parallel; see "Shared with RFC-008" for where the two touch.

## Proposal

### The chain

- **Every track and the master has a chain:** an ordered list of up to eight effects. A track's sound goes source → chain → mixer strip (volume, pan, mute, solo). The master's goes sum of the tracks → master chain → master volume → the hard clip.
- **Each effect has:** a permanent ID (like a track's or a note's), its kind (just Delay for now), its own settings, an **on/off** switch and a **Mix**.
  - Mix blends the effect's output with what went into it. 0% is the sound unchanged, and 100% is only the effect.
  - The blend is "equal-power": at 50% you hear both at nearly full strength, without the dip in loudness a straight blend gives.
  - Every effect has this Mix, so a later effect doesn't have to build its own.
- **The order matters,** as in any DAW: a delay before a distortion distorts the echoes, and after it echoes the distortion.
- **The limit is eight per chain** (open question 5). DAWs with a mixer of fixed slots have a limit (Logic 15, Pro Tools and FL Studio 10), and Ableton's device chain has none. Uta needs some limit to keep the work per block bounded, and eight is more than a track usually needs. It's easy to raise later, and hard to lower once projects can be saved.

### Inserts only, with room for buses and groups later

These are **insert effects**: each one processes the whole sound of the track it's on. There are no **sends**, where several tracks feed a share of their sound to one shared effect (one reverb for the whole song, say). You want track groups in future, and sends may fit better designed alongside them. So this RFC designs neither. It only makes sure nothing it does gets in their way: a chain belongs to the place it sits, a track or the master, and doesn't assume it's a track's, so a return or a group can have one later.

### The Delay

A tape echo with a clean side. A recording head writes the sound onto a loop of tape, and a playback head a little further along plays it back, later. Some of what plays back is fed back to the recording head, so each echo makes another, quieter one. Tape makes those echoes darker each time round, softly squashes them when they get loud, and wobbles their pitch slightly. The Delay models those three things.

| Control | Range | What it does |
|---|---|---|
| **Sync** | on / off | Whether the time follows the tempo |
| **Time** (synced) | 1/32 to 1/1, each straight, dotted or triplet | The echo's spacing in notes, from a menu |
| **Time** (free) | 1 ms to 5 s | The spacing in milliseconds, for slapback or effects off the grid |
| **Mode** | Stereo · Ping-pong | Stereo echoes each side on itself. Ping-pong bounces the echoes left, right, left |
| **Feedback** | 0 to 110% | How much of each echo comes round again. At 100% they barely fade; past it they grow until the saturation holds them |
| **Low cut** | 20 Hz to 2 kHz | Thins each repeat a little more than the last |
| **High cut** | 500 Hz to 20 kHz | Darkens each repeat a little more than the last |
| **Drive** | 0 to 100% | How hard the echoes hit the tape saturation: from clean to warm and crunchy |
| **Wobble** | 0 to 100% | Wow and flutter: the slow drift and fast waver in pitch of a worn tape machine |
| **Mix** | 0 to 100% | Shared by every effect (above) |
| **On / off** | | Shared by every effect (above) |

The defaults are a dotted eighth, 40% feedback and 30% Mix, with gentle cuts, a little Drive and a little Wobble. They're a starting point, tuned by ear in the Delay ticket.

How each part works:

- **The tape loop is memory set aside when you add the Delay:** 5 seconds per side, with a little extra for Wobble. At 48 kHz that's about 2 MB for each Delay, and at 192 kHz about 8 MB.
  - A synced time can't be longer than that. At a slow tempo a long division, such as a half note at 40 BPM (3 s) or a whole note at 60 BPM (4 s), fits, but a half note at 20 BPM (6 s) is held at 5 s.
  - Ableton's Delay goes up to 5 s too.
- **Changing the time repitches,** like tape: the playback head glides to its new place, so the echoes already on their way bend in pitch and then settle. Nothing jumps, so nothing clicks. A tempo change while synced does the same thing, and comes out as a gentle drift. This is Ableton's default "Repitch" behaviour, and Logic's Tape Delay follows tempo changes the same way. There's no Fade or Jump mode for now.
- **Reading between samples.** The playback head is almost never exactly on a stored sample, so the Delay estimates the sound between them. This is **interpolation**, as in a smooth line drawn through the dots of a chart.
  - The simplest kind draws straight lines between the dots. That dulls the sound a little, and in a delay the dulling compounds: the tenth echo has been dulled ten times.
  - Uta uses **cubic** interpolation, a smooth curve through four neighbouring samples, as Vital and FunDSP do. It costs a few more multiplications and keeps the echoes clear.
- **The feedback loop.** The echo coming round goes through the Low cut and High cut, then through the saturation, then back onto the tape.
  - Because the cuts are inside the loop, each repeat is filtered once more than the last, which is what makes tape echoes fade into darkness.
  - The saturation is always there. Drive only sets how hard the echoes hit it. That's what keeps 110% feedback in check: the echoes grow until they're squashed, and they turn into a sustained howl rather than an ever louder blast. Bitwig's Delay+ allows 122% the same way.
  - The low cut also stops any slow drift away from zero building up in the loop.
- **Ping-pong.** The left and right of the input are summed into the left side's tape. Each echo then crosses to the other side before it comes round again. A mono synth gets echoes that bounce from side to side.
- **Wobble.**
  - **Wow** is a slow sway of the playback head, about once a second. **Flutter** is a faster, smaller waver.
  - Both are deterministic, as the drums' noise is: they restart from the same point on every Play, so what you hear is what you render (open question 4).
  - With Wobble at 0 the Delay is exactly on time, which the tests rely on.

### Stopping, switching off, and tails

The sound an effect goes on making after its input stops is its **tail**. For a delay, that's the echoes.

- **Stop:** the echoes ring out naturally, as in Ableton. The track keeps processing until they've died away.
- **Switching an effect off:** its output fades into the unprocessed sound over about 10 ms, so there's no click, and then it does no work. Its echoes are gone, so switching it back on starts from silence. Off means off, as when you switch a device off in Ableton.
- **Adding, removing and reordering** each fade in the same way:
  - An effect you add fades in over about 10 ms.
  - An effect you remove fades out over about 10 ms before it's gone.
  - An effect you move fades out where it was and back in where it lands. It keeps its echoes, so a moving delay only dips for a moment.
- **Removing a track** with echoes still ringing fades them out quickly, as it does the track's notes today (open question 2).
- **Sleeping.** Once an effect's tail has died away and nothing is coming in, it stops working until something does. A delay whose echoes have been below −120 dB for a little longer than its delay time is asleep. A track with no notes and a sleeping chain costs nothing.
- **Rendering.** `uta render` keeps going after the song's end until every chain is asleep, up to a limit of 30 s, so a render includes the echoes. With feedback past 100% the echoes never stop on their own. The limit ends that render, and the CLI says so.
- **Starting mid-song** doesn't bring back echoes of notes before the start point, because nothing played them. RFC-003 already accepted this. Only `uta render` from the top is exact.

### The chain becomes stereo

Today a track's synth or kit makes one channel, and the pan stage is where it becomes left and right. A ping-pong delay needs separate left and right before the pan stage, and so will the reverb. So from the chain onwards, a track is stereo:

- The source's one channel goes into both sides, and the chain works on left and right.
- **Pan becomes a balance** on a stereo signal: it turns one side down rather than moving a single sound across. Ableton and Logic do the same on stereo tracks.
- **Nothing changes for a track with no effects.** The balance uses the same gains as today's pan law (at the centre each side is unchanged; panned hard, one side is 3 dB up and the other silent). With the same sound on both sides that's exactly today's result, so every existing golden WAV should stay the same, bit for bit.
- **One thing does change.** A ping-pong delay on a track panned hard left loses its right-hand echoes, because balance turns the right side off. That's how balance works in other DAWs too. If it bothers you in practice, a later RFC could add Ableton's "split stereo pan" option.

### Patterns for every effect after this one

This is the part later effect RFCs build on. Each is written so that adding an effect means adding a new kind, not changing the chain.

1. **In the project core.** An effect is its ID, its kind, its settings, on/off and Mix. A chain is addressed as "track *n*" or "the master". The commands:
   - **Add an effect** to a chain at a position, with a new permanent ID.
   - **Remove effects.**
   - **Move an effect** within its chain. Moving to another chain is later.
   - **Switch an effect on or off.**
   - **Set an effect's Mix.**
   - **Set a Delay setting.**

   Each later effect kind adds its own "set a setting" command, as the synth (`SetSynthParam`) and the drums (`SetDrumParam`) have. A knob drag is one undo step. Undoing a removal brings the effect back with its settings (not its echoes). Removing a track removes its chain with it, and undo restores both. The command format goes up by one, and older command lists still load.
2. **In the outline.** Each chain, and each effect's settings, go to the UI in the outline: a few dozen numbers per effect, as RFC-004 planned. The kinds of effect, their settings' ranges and their defaults come from Rust, as the synth's and kit's do, so the UI never hard-codes them.
3. **In the snapshot.** The snapshot carries each effect's settings as plain numbers: its ID, kind, on/off, Mix and settings. Like everything in the snapshot, they're replaced whole on every change.
4. **An effect's working memory is made off the audio thread.**
   - This memory is the effect's **state**: a delay's tape, a reverb's echo network, a compressor's sense of how loud it's been. It can't live in the snapshot, because a new snapshot arrives with every knob turn, and the echoes would be thrown away each time.
   - When a snapshot brings an effect the audio thread hasn't seen, the control side makes its state first, sized for the current sample rate. That's where setting memory aside is allowed. It sends the state across with that snapshot.
   - The audio thread keeps each chain's states in a fixed table, set aside when the engine starts, with room for the eight effects plus any still fading out. It finds a state by the effect's permanent ID.
   - When an effect has faded out after being removed, the audio thread sends its state back to the control side through a fixed-size queue, as it does with used snapshots, and the control side frees it.
   - The rule this adds to `CLAUDE.md`: the audio thread may own an effect's state that was handed to it, but never frees it. The state always goes back through a queue.
   - Firewheel, the most complete Rust audio engine, works the same way. Its pieces are built on the control side, sent to the audio thread, and returned to be freed.
5. **A change of sample rate** (switching output devices) rebuilds every state on the control side, in the same step that already re-prepares the synth and kit between streams. Echoes don't survive it, but the switch already fades out and back in.
6. **One shape for every effect on the audio thread.** Each effect provides the same five things:
   - getting ready for a sample rate;
   - processing a stereo block in place;
   - gliding to new settings without a click;
   - saying whether it's still ringing (for its tail and for sleeping);
   - clearing itself.

   The chain does the rest for every effect: on/off, Mix, the fades when you add, remove or move one, and sleeping.
7. **The same audio thread rules as everything else.** Nothing on the audio thread sets memory aside, frees it, waits or does I/O. Every loop is bounded by the block size and the eight-effect limit. Values decaying towards silence are flushed to zero, so a fading tail never slows the processor down (the same "denormal" problem the drums avoid).
8. **The test kit** in "How we'll verify it" is written once, against any effect. A new effect gets it by adding itself to the list.

### In the window

- **The Sound tab shows the whole chain, left to right.** The synth or kit panel comes first, then each effect as a panel, then a **+**.
  - Each effect panel has a title bar with its name, an on/off key, and a handle to drag it by.
  - The panel scrolls sideways when the chain is wider than the window.
  - Selecting a panel and pressing Delete removes it. ⌘Z brings it back.
- **The + menu** lists the effects that can be added: just Delay for now. A new effect goes at the end of the chain. The menu is greyed out at eight.
- **The Delay panel.** Following RFC-005's control rule:
  - Time is a **menu** when synced (eighteen choices) and a **knob** in ms. Sync is a toggle beside it.
  - Mode is a **segmented choice**.
  - Feedback, the cuts, Drive, Wobble and Mix are **knobs**.
  - Following "Draw the sound", the panel may draw something to help, such as the echoes as a row of fading ticks. That's settled in the design sandbox before the panel ticket, like the drum panel's layout was.
  - The shared panel frame (title bar, on/off, handle) is new, so it's marked provisional and logged in the Design decisions database.
- **The master gets a header row,** below the tracks, named Master, with the master volume and meter it has today. Clicking it selects it, and the Sound tab shows the master's chain, with no source panel before it. The Notes tab has nothing to show for the master, and says so.

### Shared with RFC-008 (Synth features)

The two RFCs should touch different parts of Uta, so their projects can run side by side. These are the places they meet:

- **A track's audio path inside the engine.** This RFC makes it stereo from the chain onwards. If the synth wants stereo voices (spreading detuned voices left and right), it changes how the source fills the stereo buffer, not the chain. Whichever project gets there first makes the track's working buffer stereo, and the other builds on it. This is the one real overlap, and the plans for both projects should name a single ticket for it.
- **The Sound tab.** The synth panel becomes the first panel in a row. If RFC-008 changes that panel's size or layout, the chain only needs it to stay a panel of its own.
- **The snapshot's track entry and the outline's track.** Both RFCs add fields there: the synth's settings for RFC-008, the chain for this one. They're different fields, so any clash is only textual.
- **The command format number.** Both raise it. Whichever merges second takes the next number. The same goes for later effect RFCs built in parallel.
- **A note-division type.** The Delay's synced times (1/8 dotted and so on) are the first in Uta. If the synth's LFO syncs to the tempo, it should use the same type.
- **The knob, fader, menu and segmented choice.** Neither RFC should need to change them. Either may want a new knob size.

### Not in this project

- Sends, returns, buses and track groups (see above).
- The other five effects, each its own RFC: EQ, Reverb, Compressor, Bitcrusher and Distortion. A limiter for the master is still planned as an effect, as RFC-003 said.
- More Delay features: Freeze, tape hiss, separate left and right times or a swing offset, Fade or Jump modes for time changes, ducking, a filter that moves with an LFO, and tap tempo.
- Presets, saving an effect's settings, and copying an effect to another track or chain.
- Automation.
- Effects on individual drum sounds, and a mixer view.
- Third-party plugins (AU, VST, CLAP).
- Bringing back echoes of notes before a mid-song start point.

## What it looks like to you

You select Synth 1 and open the Sound tab. The synth's panel is there as before, with a **+** after it. You click **+**, choose Delay, and a Delay panel appears beside the synth. You play a short stab, and it echoes on the dotted eighth, each echo softer and a little darker than the last.

You switch Mode to Ping-pong, and the echoes bounce left and right, though the synth itself is mono. You pull High cut down and the repeats fade into murk. You turn Wobble up, and the echoes start to sway like an old tape machine. You turn Time while it plays, and the echoes bend in pitch and settle, with no click. You push Feedback past 100% and Drive up, and the echoes swell into a dub howl that never blows up. You bring Feedback back down and it subsides.

You press Stop and the echoes ring out to silence. You switch the Delay off mid-echo and it's gone in a moment, cleanly. You add a second Delay on the 1/4, drag it before the first, delete it, and ⌘Z brings it back. You click **Master** below the tracks, the Sound tab shows the master's chain, and you put a Delay there too.

You render the song, and the file ends after the last echo has died away.

## Alternatives considered

- **Keeping the effect's working memory in the snapshot.** It's the simplest, because everything would arrive and leave the way settings do. But a new snapshot arrives with every edit, and the echoes would cut out every time you turned a knob or moved a note.
- **Setting aside effect memory for every track up front, when the engine starts.** No memory would be passed between threads. But a Delay's tape is about 8 MB at 192 kHz, and there are 33 chains of eight: gigabytes for effects nobody added. Firewheel and nih-plug both make an effect's memory when it's added, off the audio thread, as proposed here.
- **Locking while the chain changes,** as Ardour does: the audio thread outputs silence if it can't get the lock. That breaks the audio thread rules and drops out on edits.
- **Linear interpolation** (straight lines between samples). It's the cheapest, and it's JUCE's default. But it dulls each echo, and through the feedback loop the dulling compounds. Cubic costs a few multiplications more.
- **Allpass ("Thiran") interpolation.** It doesn't dull the sound, but it has a memory of its own, so it clicks when the delay time moves, and the Delay's time moves all the time with Wobble and repitching. JUCE's and Faust's documentation both warn about this.
- **Windowed sinc interpolation,** as Surge uses. It's the most accurate, at several times the cost. Cubic is clean enough for a tape echo, whose cuts and wobble are anyway its character. It's a later upgrade if a listening test ever says otherwise.
- **Crossfading to the new time instead of repitching (Ableton's "Fade").** It has no pitch bend, but sweeping the time sounds grainy. During a tempo change it would crossfade on every block, unless it's held back to one crossfade per lap of the tape, as Surge does. Repitch is what tape does and handles tempo changes for free. Fade can be added as a mode later.
- **A clean digital delay first, tape later.** It's simpler to tune, but cold-sounding at high feedback. You chose the tape character, and the loop's cuts and saturation are what make a delay musical at all.
- **A chain that stays mono until the pan stage.** It would leave the mixer as it is, but ping-pong wouldn't be possible, and the reverb RFC would have to make the chain stereo anyway.
- **A Rust library for the delay.** FunDSP is good reference code, but it sets memory aside when the sample rate changes, and its way of building sound out of connected nodes overlaps with our snapshots (RFC-002 turned it down for the synth for the same reason). dasp hasn't had a release since 2020, and synfx-dsp is GPL. A delay line, cubic read, filters and saturation come to about 150 lines by hand, which is what the projects researched do.
- **Sends instead of, or as well as, inserts.** Sends are how most mixes share one reverb. You'd rather design them alongside track groups, so they're kept for later.
- **An Effects tab of its own, or a mixer view.** A separate tab splits one signal path across two places. A mixer view is a new view as well as new effects. The Sound tab showing the whole chain is Ableton's Device View, which you know.

## Risks & unknowns

- **"Sounds like tape" is your judgement.** The tests can show the echoes are on time, darken and stay bounded, not that they're beautiful. The Delay ticket ends with listening renders compared against Ableton's Echo, and the plan allows a tuning pass. The Wobble and Drive figures (rates, depths, saturation curve) are guesses until then.
- **Runaway with feedback past 100%.** It's bounded by the saturation, but with the cuts set wide it can sit loud for a long time. The tests check it settles below full scale. If it's still too easy to make something painful, the top of the range comes down.
- **Effects on the master hit harder than you might expect.** The master's chain comes before the master volume, as in Ableton. The master volume defaults to −12 dB, so the chain sees the mix about 12 dB hotter than you hear it. The Delay's saturation will bite sooner there. That's how other DAWs behave too, but it's worth knowing when you tune Drive.
- **The fades on add, remove and move are a guess at 10 ms.** Short enough not to notice, long enough not to click: a common figure, but not a measured one. A moved delay's echoes dip for about 20 ms; I expect that to be inaudible, but that's a guess until you try it.
- **Memory.** Each Delay sets aside about 2 MB at 48 kHz (8 MB at 192 kHz). Even a Delay on every chain at 192 kHz is about 260 MB. Making the memory takes a moment on the control side, which only matters if you add many at once.
- **CPU.** A Delay is a handful of multiplications per sample per side, cheap next to 16 synth voices. The timing report adds a case with a Delay on every track and the master, to measure it rather than guess.
- **Sleeping must never cut off a quiet tail you can hear.** The −120 dB threshold is far below hearing. The tests check that a long, quiet tail plays out before the Delay sleeps.
- **The stereo change touches every track's audio path.** If the existing golden WAVs change at all, that's a bug in the change, not a new golden to approve.
- **Coordination with RFC-008** (above). If both projects change a track's audio path at the same time, they'll clash. The plans should name one ticket for it.

## How we'll verify it

**Automated, on every PR.** The effect test kit, written once and run for every kind of effect:
- **Off and Mix 0 change nothing.** With the effect switched off, or at Mix 0, the output matches the same render with no effect, bit for bit.
- **No clicks** from adding, removing, moving, or switching an effect on and off while it plays, all under the existing jump limit.
- **The same audio at every block size:** blocks of 32, 128 and 1024 give the same audio.
- **Real-time safety:** a render that adds, removes, moves and switches effects, swaps snapshots and changes the sample rate runs under `assert_no_alloc` and RealtimeSanitizer. Every effect state handed to the audio thread comes back to the control side.
- **Tails:** after Stop, the track keeps sounding until the tail has died away, then it sleeps, and the status shows it as silent.

The Delay's own tests, with Wobble at 0:
- **On time.** A single click comes back at exactly the right sample: in ms at 44.1, 48 and 96 kHz, and synced at two tempos.
- **Feedback.** With the cuts wide open and no Drive, each echo is Feedback times the one before, within tolerance.
- **The loop's cuts.** With the High cut on, each echo is less bright than the one before (measured as the spectral centroid), and the Low cut makes each one thinner.
- **Ping-pong** alternates echoes left and right from a mono input.
- **Repitch.** Changing the time while it plays never jumps by more than the click limit, and the echo arrives at the new time once the glide is over.
- **Bounded runaway.** At 110% feedback with full Drive, the output settles below full scale and stays there.
- **Wobble is repeatable.** Rendering twice gives the same file, and it moves the echo's timing by about the amount set.
- **Rendering includes the tail.** `uta render` runs past the song's end until the echoes stop, and stops at the 30 s limit when they never do.

Also:
- **A golden WAV** of a demo with a Delay on a synth track and one on the master, from a committed command list (`examples/demo-delay.json`). You approve it in the PR.
- **The existing golden WAVs don't change.** The stereo chain mustn't alter a track with no effects.
- **Timing report:** a new case with a Delay on every track and on the master, at p50 and p99. It's reported, not gated.
- **Project core:** each new command undoes exactly and round-trips through saving. A knob drag is one undo step. A chain of eight refuses a ninth. Removing a track and undoing brings its chain back. Older command lists still load and render the same.
- **UI,** against the mocked back end: the chain draws from the outline, **+** adds a Delay, dragging a panel sends a move, Delete removes the selected panel, the on/off key sends its command, and selecting Master shows the master's chain.

**Manual, by you:**
1. **Listen to the Delay renders** in its PR: a stab through each Mode, a High cut sweep, Drive and Wobble at a few settings, and feedback at 100% and 110%. Compare against Ableton's Echo on the same sound.
2. **Turn every knob while it plays,** especially Time. It should bend, never click.
3. **Make a loop with a delay on the synth and the hats,** then one on the master. It should sound like a record, not a demo.
4. **Press Stop mid-echo,** then switch a Delay off mid-echo. Stop rings out; off is gone at once, cleanly.
5. **Add, reorder and delete** effects while it plays, and undo each.
6. **Change the tempo** with a synced Delay running. The echoes should drift to the new time, not jump.

## Open questions

1. **Which synced divisions?** Recommendation: 1/32, 1/16, 1/8, 1/4, 1/2 and 1/1, each straight, dotted or triplet: eighteen choices in a menu. A time longer than 5 s at a slow tempo is held at 5 s. **Resolved: as recommended (Will, 2026-10-11).**
2. **Removing a track while its echoes ring:** fade them out, or let them ring? Recommendation: fade them out quickly, as removing a track does to its notes today. You removed it, so it should go. **Resolved: fade them out (Will, 2026-10-11).**
3. **The fades on add, remove and move:** about 10 ms? Recommendation: 10 ms, with the tests holding them under the click limit. Adjust by ear if a move's dip is audible. **Resolved: 10 ms (Will, 2026-10-11).**
4. **Wobble restarts on every Play?** Recommendation: yes, as the drums' free-running sounds do (RFC-006), so what you hear is what you render. **Resolved: restart on every Play (Will, 2026-10-11).**
5. **How many effects per chain?** How other DAWs do it:
   - **A mixer with a fixed number of insert slots:** Logic has 15 per channel strip, Pro Tools 10 (A to J), FL Studio 10 per mixer track (with a Patcher in a slot for more).
   - **A device chain with no fixed limit:** Ableton, and as far as I could find Bitwig too (its manual doesn't state a limit). You're limited only by CPU, and the chain scrolls.

   Uta's chain looks like Ableton's, but the audio thread needs a limit, because its table of effect states is set aside when the engine starts and its work per block has to be bounded. Going unlimited would mean sending a bigger table from the control side when a chain outgrows its own, as Firewheel does. That's possible later without changing anything else. Recommendation: start at eight and raise it the day you hit it. A limit only moves safely one way: raising it is one number and its tests, but lowering it would stop saved projects with more effects from loading. A lower limit also halves the worst case every test and timing report has to cover. **Resolved: eight, raised when it's actually reached (Will, 2026-10-11).**
6. **Mix as equal-power?** Recommendation: equal-power, so 50% doesn't dip in loudness. At 0% it's still exactly the unprocessed sound, which the tests check. **Resolved: equal-power (Will, 2026-10-11).**

## New terms

- **Insert effect:** an effect that processes the whole sound of the track or master it's on, in order with any others on that chain.
- **Effect chain:** the ordered list of insert effects on a track or the master, between its source and its mixer strip.
- **Send / return:** a way for several tracks to feed a share of their sound into one shared effect. Not in Uta yet.
- **Dry / wet (Mix):** the unprocessed sound (dry) and the effect's output (wet), and the control that blends them.
- **Bypass (on/off):** switching an effect off, so the sound passes through it unchanged and it does no work.
- **Tail:** the sound an effect goes on making after its input stops, such as a delay's echoes or a reverb's decay.
- **Effect state:** the working memory an effect keeps between blocks, such as a delay's stored sound, made off the audio thread and handed to it.
- **Delay line:** memory that stores the last few seconds of sound so it can be played back later; the tape loop of a digital delay.
- **Feedback:** sending some of an effect's output back into its input, so each echo makes another.
- **Interpolation:** estimating the sound between two stored samples, needed when a delay's time falls between them.
- **Repitch:** the pitch bend you hear when a delay's time changes while echoes are on their way, as on a tape machine.
- **Wow and flutter:** the slow (wow) and fast (flutter) wavering in pitch of a tape machine, recreated as the Delay's Wobble.
- **Saturation:** the soft squashing of loud sound that tape and analogue circuits do, which adds warmth and keeps runaway feedback in check.
- **Ping-pong:** a delay whose echoes alternate between the left and right speakers.
- **Tempo sync:** setting a time in note lengths (a dotted eighth) rather than milliseconds, so it follows the tempo.
- **Balance:** the stereo version of pan, which turns one side of a stereo sound down instead of moving a single sound across.
- **Sleeping:** an effect stopping work once its tail has died away and no sound is coming in, until some does.
