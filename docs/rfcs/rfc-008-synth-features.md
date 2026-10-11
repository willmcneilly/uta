# RFC-008: Synth features

**Status:** Accepted (2026-10-11) · **Author:** Will · **Notion:** https://app.notion.com/p/Synth-features-3f63af969b6f81c89d37c1ec3aa42b31

## Summary

Turn the synth into a compact, Drift-sized instrument you can make real sounds with. Each synth track gets:
- two oscillators, a sub-oscillator and noise;
- pulse-width modulation, and hard sync of the second oscillator to the first;
- a filter with low-, band- and high-pass modes, its own envelope and key tracking;
- one LFO;
- three modulation slots;
- a Mono mode with legato and glide;
- a menu of factory sounds.

The default sound plays exactly as it does today. Each factory sound is built from a written brief with measurable targets, and the way briefs become settings is written down as we go: the groundwork for an AI sound designer later. The panel is redesigned from DESIGN.md's principles, sketched first. Analog character (drift, analog curves, a nonlinear filter, drive, unison and chorus) is a later synth RFC.

## Motivation

- **It's hard to do anything meaningful with the synth.** That's your verdict. It has one oscillator, one filter with no envelope of its own, one envelope and no modulation: seven settings in all. Most sounds people reach a synth for need more. A pluck needs a filter envelope. A bass is usually two oscillators or a sub, in mono with glide. A pad needs slow movement, and a lead needs vibrato or sync. None of them can be made today.
- **It's the second step in the sound-first stretch.** Drums (RFC-006) gave Uta rhythm. The synth is the other half of every loop. Until it can make the sounds you hear in your head, Uta stays a project rather than an instrument.
- **Features before character.** Your brainstorm (scratchpad, 2 Oct, "Synth is basic and sounds digital") laid out a ladder: features, then cheap character, then the deep end. Character work is judged by ear on a synth that can already make the sound, so features come first. The one character feature that's almost free, free-running oscillators, comes along.
- **Effects run alongside** (RFC-007: an effect chain and a delay on each track and the master, accepted the same day). The two touch different parts of the engine on purpose, so their projects can run side by side. "Where this meets RFC-007" lists the places they share.

## Proposal

### The instrument

Modelled on Ableton's Drift. It has nearly every control on the front panel, with a small number of flexible routings tucked away, which its designer gives as the reason it's quick to get a good sound from. The changes from Drift: a sub-oscillator (as on the Juno), hard sync (which Drift doesn't have), and a filter mode instead of Drift's separate high-pass.

A word on **modulation** first, since it runs through all of this. A setting such as the cutoff has the value its knob is set to. Modulation moves it away from that value while a note plays, driven by a **source**: an envelope (a shape that runs once per note), an LFO (a slow repeating wave), how hard the note was played (velocity), or which note it is (key). How far the source moves it is the **amount**. It works like a CSS animation layered on top of a property's base value: the knob sets the base, and modulation is the animation.

| Section | Controls | What it's for |
|---|---|---|
| **Oscillator 1** | Wave (sine, triangle, saw, pulse) · Octave (−2 to +2) · **Width** (pulse only) | The main tone. Width narrows the pulse from square (50%) towards a thin, nasal buzz (5%). Moving it (pulse-width modulation, PWM) gives the slow, chorus-like movement of string pads. |
| **Oscillator 2** | Wave · Octave · **Pitch** (±12 semitones, fine near zero) · **Sync** | A second tone. A small Pitch offset makes the two beat against each other for a thicker sound. A larger one makes an interval. Sync restarts oscillator 2 each time oscillator 1 starts a cycle, so it always sounds at oscillator 1's pitch. Sweeping its Pitch then changes its tone instead, which is the classic tearing "sync lead". |
| **Sub** | Octave (−1 or −2) | A square wave one or two octaves under oscillator 1, as on the Juno. Weight under a bass or a lead, almost free to make. |
| **Noise** | (level only) | White noise: breath on a pad, a click on a pluck, the hiss in a snare-like sound. |
| **Mixer** | A level for Osc 1, Osc 2, Sub, Noise | How much of each goes to the filter. These are faders, since they're compared side by side (RFC-005 part 7's rule). |
| **Filter** | Mode (low-pass, band-pass, high-pass) · Cutoff · Resonance · **Env amount** (±) · **Key** · **LFO amount** | Env amount is how far the filter envelope opens (or, below zero, closes) the filter on each note. Key makes the cutoff follow the note, so high notes aren't duller than low ones. The three modes come from the same filter, which works out all three already. |
| **Filter envelope** | Attack · Decay · Sustain · Release | Its own ADSR, separate from the volume's. Also a source for the mod slots. |
| **Amp envelope** | Attack · Decay · Sustain · Release | Today's envelope. Also a source for the mod slots. |
| **LFO** | Shape (sine, triangle, saw, square, sample-and-hold) · Rate (Hz, or a note division when synced, as RFC-007's Delay has) · **Sync** · **Retrigger** · **Vibrato** · **Width** | A slow wave for movement. Sync locks its rate to the tempo and its phase to the bar. Retrigger restarts it on each note, so every note moves the same way; off, all voices share one LFO running in time with the song. Vibrato wobbles both oscillators' pitch; Width moves oscillator 1's pulse width. The filter's LFO amount is on the filter. Sample-and-hold jumps to a new random level every cycle, smoothed over a few milliseconds so it doesn't click. |
| **Voice** | **Poly / Mono** · **Legato** · **Glide** · **Osc retrigger** | Mono plays one note at a time, with the latest note winning, and returns to a note still held when you let go. When a new note overlaps one already held, Glide slides the pitch to it over the set time, and with Legato on the envelopes carry on instead of starting again (a smooth, connected line). A note after silence always starts fresh. Osc retrigger off means the oscillators keep running between notes (free-running), so each note starts somewhere slightly different in the wave, as on an analog synth. |
| **Mod** | Three slots, each **Source → Destination**, with an **Amount** (±) | For routings the fixed knobs don't cover, such as velocity to cutoff, filter envelope to oscillator 2's pitch (the sync sweep), or LFO to volume (tremolo). Sources: filter envelope, amp envelope, LFO, velocity, key. Destinations: oscillator 1 and 2 pitch, each mixer level, width, cutoff, resonance, LFO rate, volume. |
| **Sound** | A menu of about 8 factory sounds | See "Factory sounds" below. |

That's about 40 controls, against 7 today, for a sense of scale. The drum panel has about 27.

**The default stays as it is.** A new synth track plays exactly today's sound: oscillator 1 a saw, every other level and amount at zero, low-pass, Poly, the oscillators restarting on each note. So existing projects and command lists sound the same, and the existing golden WAVs don't change. Today's Square wave becomes Pulse; at a Width of 50% it is today's square.

### In the engine

How each piece is made. All of them follow the audio thread rules, and none needs a library.

- **Sync, Width and the sub reuse today's oscillator.** Today's oscillator already smooths each jump in its wave with a small correction (PolyBLEP), placed at the exact point between two samples where the jump falls. That correction is what keeps aliasing, the false tones digital audio makes out of sharp edges, mostly out.
  - **Width:** a pulse is two of those jumps, one at the start of the cycle and one at the width. Built as one saw minus another shifted by the width, so it never drifts off centre whatever the width. Width is kept between about 5% and 95%, because two jumps closer than a sample apart can't both be corrected.
  - **Sync:** the jump comes where oscillator 1 restarts oscillator 2. The oscillator already knows where between two samples oscillator 1 wraps, so oscillator 2 gets the same correction there, sized by how far it jumped. Triangle and sine under sync also get the corner correction the triangle already uses (PolyBLAMP), since a sync restart bends their slope as well.
  - **Sub:** it flips each time oscillator 1 wraps (every second wrap for −2 octaves), so it gets its correction at a point the oscillator has already found. It's the software version of the Juno's divider circuit.
- **The filter's settings become per voice.** Today all 16 voices share one set of filter settings, worked out once per sample. With an envelope, key tracking and modulation, every voice's cutoff moves on its own. So each voice works out its own, about every 16 samples, and blends smoothly in between. That's the usual approach (Surge does it, and mystran recommends it on KVR). When nothing moves the cutoff per voice, the voices keep sharing one set as today, which also keeps the default sound identical.
- **Noise and sample-and-hold use a seeded random generator,** one per voice. Each starts from a fixed seed and restarts on Play, like the drums' noise, so a render is the same every time and what you hear from bar 1 is what you render.
- **Free-running oscillators and the LFO's shared phase also restart on Play,** as RFC-006 decided for the drums. A synced LFO takes its phase from the song's position, so it lines up with the bars however playback started.
- **Mono mode keeps a list of held notes,** up to 16, as plain numbers (key, pitch, velocity) in a fixed-size array. It plays the latest, and falls back to the one before when that's let go. Glide is a ramp on the voice's pitch. The notes the sequencer starts still go through the same bookkeeping as today (chasing, editing while playing, the loop), so no note can stick.
- **Everything glides.** Knob changes glide as they do today, and stepped shapes (the LFO's square and sample-and-hold) are smoothed by a few milliseconds.
- **The synth's output stays mono.** Unison and stereo spread are what would make it stereo, and they're in the later RFC. See "Where this meets RFC-007".

### In the project

- **The synth's settings grow,** from 7 to about 40, plus the three mod slots. Each is still set by `SetSynthParam`, and a drag is still one undo step.
- **Choosing a factory sound is one command** that sets every setting at once, `SetSynthSettings`, and its undo puts them all back.
- **The command format goes up.** A new track carries its full synth settings, so older command lists need converting: missing settings take today's defaults, and Square becomes Pulse at 50%. Format 4 lists still load and render the same.
- **Rust describes the synth's settings to the UI,** as it already does for the drums. Each setting comes with its label, unit, range, scale, default and a **description of how it sounds** (below). The UI builds the panel from that list instead of naming 40 fields by hand, and never works out a range or a default itself.

### Factory sounds, and turning an idea into settings

About 8 starting points covering the main jobs: a bass, a mono bass with glide, a pluck, a pad, a PWM string pad, a lead, a sync lead and a bell-like or noise sound. They live in Rust with the code, and the menu's list comes from Rust.

You want an AI sound designer for each instrument and effect one day. That's a separate idea, and it's in the scratchpad. What this RFC does is build the factory sounds the way a designer would, so the knowledge it needs gets written down along the way:

1. **Every setting describes how it sounds,** not just what it is. For example: "Env amount: how far the filter opens on each note. High with a short decay is a pluck, low with a long attack is a swell." These live in Rust next to the ranges, so the panel, the docs and a designer all read the same words.
2. **Every factory sound starts from a brief:** a sentence or two of what it should sound like, with a reference ("a round, short Juno pluck that sits under a chord"). Each brief also has **measurable targets**: how bright it is (the spectral centroid the drum tests use), how fast it starts, how long it takes to fade, how loud it is. The brief and targets are kept with the sound.
3. **The sounds are made with a fixed loop:** brief → settings → render → measure against the targets → you listen → adjust. It's done by hand here. It's the loop an AI designer would run, so we find out whether a brief and a few measurements are enough to land a sound.
4. **A voicing guide is written as we go:** which settings give which qualities (bright or dark, plucked or swelling, thin or fat, moving or still), learned from making the factory sounds. It goes in the repo, at `docs/sound-design/synth.md`, next to the code (open question 2).

The factory sounds are balanced by measurement, as UTA-57 balanced the kit, and the last ticket tunes them by ear.

### The panel

D-22 (knobs in a row under the filter and envelope drawings) was made for 7 settings and won't hold 40. The panel is redesigned from DESIGN.md's principles:
- **Laid out like the signal flows,** left to right: Oscillators → Mixer → Filter → Envelopes → LFO → Voice, with the Sound menu at the start.
- **Draw the sound** where a drawing shows what a control does: the filter's curve (and how far the envelope moves it), both envelopes' shapes, the LFO's wave, and a cycle of each oscillator's wave at its width.
- **Controls by RFC-005 part 7's rule:** the mixer levels are faders, the choices (waves, filter mode, Poly/Mono, LFO shape) are segmented choices, the switches (Sync, Retrigger, Legato) are toggles, mod slot sources and destinations are menus, and the rest are knobs.
- **Uncluttered:** the three mod slots sit behind a Mod section you open, as in Drift, since they're used less than the knobs. An empty slot shows nothing but its "+".

As with the drum panel, two or three layouts are sketched in the design sandbox before the panel ticket, and the chosen one is checked in the real app. New decisions are marked provisional and logged.

### Where this meets RFC-007

RFC-007 (Effect chain and Delay, accepted 2026-10-11) runs alongside this one, and its "Shared with RFC-008" section lists the same meeting points from its side. Here they are from this side:

1. **A track's audio path.** RFC-007 makes a track stereo from its effect chain onwards, and turns pan into a balance. This RFC keeps the synth itself mono and changes nothing after it: the synth fills the track's stereo buffer the way RFC-007 defines. Stereo voices (unison spread, the later RFC) would change how the synth fills that buffer, not the chain.
2. **The track's snapshot and outline entry.** This RFC changes the synth's settings there; RFC-007 adds the chain next to them. Different fields: any clash is only textual.
3. **The command format.** Both bump it. Whichever merges second takes the next number and converts from the first.
4. **The Sound tab.** RFC-007 makes it a row: the synth panel first, then each effect as a panel, then a **+**, scrolling sideways when it's wider than the window. The redesigned synth panel stays one panel in that row, and its sketches are done with a Delay panel beside it.
5. **Note divisions and the tempo.** RFC-007's Delay brings Uta's first note-division type (1/8 dotted and so on) and the tempo to an effect. The LFO's synced rate uses the same type and the same menu, and whichever project gets there first brings the tempo and song position to the track.
6. **The knob and other controls.** Neither RFC should need to change them. Either may want another knob size, which goes into the shared knob, not a second kind of knob.
7. **The timing baseline.** Both add load and re-record it.

### Not in this project

- **Analog character** (the later synth RFC): drift, analog envelope curves, a nonlinear or 24 dB filter, drive, unison, stereo spread and chorus.
- A second filter, filter types beyond the three modes, and a separate high-pass.
- A second LFO, an LFO fade-in, and a cycling envelope.
- Pitch bend, the mod wheel and aftertouch, which need MIDI input.
- Saving your own sounds, which needs saving. Remembering which factory sound a track started from.
- Drag-to-modulate, and showing modulation on the knobs it moves.
- Wavetables, FM, and Drift's extra wave shapes.
- The AI sound designer itself (in the scratchpad).

## What it looks like to you

You click the synth clip and open the Sound tab. The panel reads left to right like a synth: oscillators, a small mixer, the filter with its curve, two envelopes drawn as shapes, the LFO's wave, and the voice settings. It's playing the same saw it always did.

You open the Sound menu and pick "Pluck". The loop now plays short, round plucks that open and close with each note. You turn up the filter envelope's Decay, and the plucks bloom into longer notes. You watch the filter's curve move as you turn Env amount.

You start from the default again. You bring in oscillator 2, set its Pitch a few cents sharp, and the saw thickens and beats slowly. You add the Sub an octave down and a touch of noise. You switch to Mono, turn up Glide, and draw a bassline with overlapping notes; it slides between them, and with Legato on the slides don't re-attack. You let go of a note while holding another, and it drops back to the held one.

You switch oscillator 1 to Pulse and narrow its Width. You set the LFO slow and turn up its Width amount, and the sound breathes like a string machine. You turn on Sync on oscillator 2, open the Mod section, route the filter envelope to oscillator 2's pitch, and the attack tears in like a classic sync lead. You sync the LFO to 1/8 and route it to volume, and the pad pulses in time with the beat, lined up with the bars from wherever you press Play.

You turn knobs while it loops and nothing clicks. You render the song twice and get the same file, noise and all. Every factory sound sounds like its name, and none is much louder than another.

## Alternatives considered

- **A Juno-like synth:** one oscillator with saw, pulse, sub and noise mixed, one LFO, one envelope shared by filter and amp. It's very musical and has the fewest controls. But it can't detune two oscillators, can't do sync, and one shared envelope rules out a pluck that also swells. It solves "hard to do anything" only halfway.
- **An Analog-like synth:** two oscillators, two filters, two amps, two LFOs, fixed routings. More range, but nearly twice the controls, and the second filter and amp are rarely what a sound is missing. It works against "uncluttered".
- **A Moog-like mono synth** with three oscillators. Great basses and leads, but chords and pads matter as much in a loop. Mono mode on a poly synth gets most of it.
- **Three full oscillators instead of two plus a sub.** A third oscillator is mostly used for a third detuned copy, which unison does better later. The sub covers the other common use (weight underneath) for one control and almost no CPU.
- **Fixed routings only** (Juno-style depth knobs, no slots). Smallest, but velocity to cutoff, envelope to pitch for sync, and tremolo would each need their own knob. Three slots cover the long tail with three rows.
- **A full mod matrix or drag-to-modulate** (Wavetable, Vital, Serum). The most flexible, and the hardest to make uncluttered: it needs a way to show modulation on every knob, and RFC-005 notes that can't use orange or blue. Drift's three slots are its designer's deliberate limit.
- **Two LFOs.** Useful for a slow filter sweep plus fast vibrato at once. But the filter envelope and the mod slots cover many of those cases, and the second LFO is an easy addition later if the factory sounds keep wanting one.
- **One LFO for the whole synth** instead of one per voice. Simpler, and it's what the Juno had. But per-voice with Retrigger gives both behaviours: on, each note gets its own; off, they share one phase, which is the Juno's.
- **minBLEP or oversampling for sync and pulse width.** Better at removing aliasing on paper. But minBLEP needs delay buffers and has DC build-up that u-he moved away from in Diva, and oversampling the whole synth multiplies its cost. PolyBLEP already works here; the sync sweep is A/B'd by ear (verification below) to check it holds up.
- **Working out each voice's filter settings every sample.** The most exact, but a `tan` per voice per sample is the most expensive part of the filter, and a cutoff moved by an envelope changes slowly enough that every 16 samples, smoothly blended, can't be heard. A lookup table (as Vital does) is the fallback if the timing report says it's still too slow.
- **A DSP library** (fundsp, MIT). It has PolyBLEP oscillators and Simper filters, but not sync or per-voice modulation in the shape we need, and some of its parts allocate. The rest of the synth is ours for the reasons RFC-002 gave; this keeps it that way. Vital and Surge are GPL, so they're read for ideas only.
- **Factory sounds as command-list files** instead of Rust data. Files would need loading from disk and a place to live, which waits for saving. As data in Rust they're versioned with the settings they set, and tests can check each one against its brief.
- **Keep naming each setting in the UI by hand,** as today. At 40 settings that's 40 places for the UI and Rust to disagree about a range or a default. The drums already describe their settings from Rust.
- **Unison now.** It's the biggest single "fat" control. But it's mostly about stereo width, it changes how the synth fills the track's stereo path that RFC-007 builds, and it belongs with chorus in the character RFC.

## Risks & unknowns

- **"Meaningful" is judged by ear.** Tests show the filter envelope moves the cutoff, not that the pluck is satisfying. So there's a listening checkpoint after the voice (oscillators, filter envelope, mono and glide), before modulation and the factory sounds, and the factory sounds end with a tuning pass.
- **CPU per voice goes up.** Two oscillators, a sub, noise, an LFO, a second envelope and per-voice filter settings: my guess is two to three times today's cost per voice. Today's worst case (32 tracks of 8 voices) uses about a tenth of a block's time, so there's room, but it's a guess until the timing report measures a full voice. The fallbacks are the shared filter settings when nothing moves them, and a lookup table for the filter.
- **Levels.** Two oscillators and a sub at full level are louder than one. The mixer's levels are scaled so that a typical mix is close to today's level, and the factory sounds are balanced by measurement. Measured rather than assumed.
- **Mono mode meets the note bookkeeping.** Chasing notes on Play, editing notes while they sound, and the loop point all assume one note per voice. Mono changes which note a voice is playing while it plays. Stuck notes are the classic bug here, so the tests target them by name.
- **Sync aliasing at extreme settings.** PolyBLEP is "mostly" band-limited, and a sync sweep to a high pitch is its hardest case. The A/B listening test decides whether it's good enough. If it isn't, the fix (oversampling just the synced oscillator) is contained.
- **The panel is the densest yet.** About 40 controls must fit the Sound tab's height, and stay narrow enough that the first effect in RFC-007's row is still in view on a normal window. That's why it's sketched first. The likely answer is two rows or smaller knobs (the drum panel's 24px), and that's a guess.
- **The brief-and-measure loop may not be enough to land a sound.** That's fine: finding out is part of the point. Whatever needed your ear rather than a measurement goes in the voicing guide too.
- **RFC-007 running at the same time.** The meeting points above are known. The Sound tab's row and the track's stereo path are the two that need the plans to name one ticket each.

## How we'll verify it

**Automated, on every PR:**
- **The default is unchanged:** the existing golden WAVs pass without being regenerated, and format 4 command lists render the same.
- **Each feature, measured,** with the sound-test tools the drums use (pitch from the waveform, spectral centroid, level, click limit):
  - **Oscillator 2 and the sub:** the measured pitches match their Octave and Pitch settings; the sub is one or two octaves under oscillator 1.
  - **Width:** at 50% it matches today's square; at 25% every fourth harmonic is missing, as a pulse that width should; no DC offset at any width.
  - **Sync:** the output's pitch follows oscillator 1 whatever oscillator 2's Pitch is, and the measured aliasing in a sweep stays at or below an agreed figure, so it can't quietly regress.
  - **Noise:** its spectrum is flat and its level matches the mixer.
  - **Filter:** each mode passes and cuts where it should; Env amount moves the measured brightness over the note's life in the right direction, and zero does nothing; Key makes higher notes as bright as lower ones.
  - **LFO:** vibrato's measured pitch wobble has the set rate and depth; a synced LFO's cycle is exactly the note length at the tempo and lines up with the bar; sample-and-hold and square stay under the click limit.
  - **Mod slots:** each destination moves the thing it names in the right direction, with the amount's sign; amount zero does nothing.
  - **Mono, legato, glide:** one voice sounds; latest note wins; letting go returns to a held note; with Legato the envelope doesn't restart; the glide reaches the new pitch in the set time; a note after silence doesn't glide. Chasing, editing while playing, and the loop leave no note stuck.
  - **Free-running:** with Osc retrigger off, two identical notes in a row differ, yet a render twice is identical.
- **Determinism:** every render twice gives identical files, and blocks of 32, 128 and 1024 give the same audio.
- **Factory sounds:** each one meets the measurable targets in its brief, and their loudness sits within an agreed range of each other.
- **Golden WAVs** for a few new renders from committed command lists: a sync lead sweep, a PWM pad, a mono bass with glide. You approve every golden file in the PR.
- **Real-time safety:** a render with every feature on, mono and poly, every knob turned while playing and factory sounds switched mid-loop, runs under `assert_no_alloc` and RealtimeSanitizer.
- **Timing report:** adds 16 voices with everything on, and 32 tracks of 8 full voices. Reported at p50 and p99, not gated, and the baseline re-recorded.
- **Project core:** the new commands undo exactly and round-trip; a drag is one undo step; choosing a factory sound is one; out-of-range values are refused; format 4 command lists convert.
- **UI**, against the mocked back end: the panel is built from the settings Rust describes; each control sends the right command; the Sound menu lists Rust's sounds and sends one command; the drawings follow a drag.

**Manual, by you:**
1. **Listen at the checkpoint** after the voice tickets: renders of two detuned oscillators, the sub, noise, filter envelope plucks and a mono glide bassline.
2. **The sync A/B:** a sync sweep made two ways, labelled A and B. Say if you can hear a difference.
3. **Build three sounds yourself** from the default: a bass, a pad and a lead. It should feel quick, and the panel should make sense without the docs.
4. **Turn every kind of control while the loop plays.** Nothing clicks.
5. **Play each factory sound** against its brief. Does it sound like what it says?
6. **Check the panel at a normal window size,** with a Delay beside it if RFC-007's chain has landed.

## Open questions

1. **The scope.** **Resolved: Drift-like; two oscillators plus sub and noise, with sync on oscillator 2; one LFO plus the filter envelope as a mod source; fixed routings plus three mod slots; Mono with legato and glide; about 8 factory sounds; free-running oscillators in, drift and unison out; filter modes in; per-voice LFO; a Drift-like panel, sketched first (Will, 2026-10-11).**
2. **Where does the voicing guide live?** In the repo, it sits next to the code and the settings' descriptions, where an agent making sounds would look. In Notion, it sits with the research. Recommendation: the repo (`docs/sound-design/synth.md`), because its reader is whoever makes sounds next, person or agent. **Resolved: the repo, `docs/sound-design/synth.md` (Will, 2026-10-11).**
3. **Show each setting's description in the panel?** It's written anyway, and a hover hint would teach the synth. But there's no tooltip in DESIGN.md yet, and "Uncluttered" says detail appears only when useful. Recommendation: not in this project. Revisit with the AI sound designer, which will need a way to explain itself. **Resolved: not in this project (Will, 2026-10-11).**
4. **One project or two?** About 12 tickets, the size of Drums. Recommendation: one project, with the listening checkpoint after the voice (oscillators, filter, mono and glide) and before modulation, factory sounds and the panel. If you'd rather close a project at that checkpoint, it's the natural split. **Resolved: one project, with the listening checkpoint after the voice (Will, 2026-10-11).**

## New terms

- **Modulation:** a source moving a setting away from its knob's value while a note plays, by an amount.
- **Modulation source:** what drives modulation: an envelope, an LFO, velocity or key.
- **Mod slot:** one free routing of a source to a destination, with an amount; the synth has three.
- **LFO (low-frequency oscillator):** a slow repeating wave, below hearing, used to move a setting rather than to be heard.
- **Sample-and-hold:** an LFO shape that jumps to a new random level each cycle and holds it.
- **Tempo sync:** an LFO's rate set as a note length (1/8, a bar) and its phase locked to the song, rather than in Hz.
- **Retrigger:** restarting something (an LFO, the oscillators) at the start of every note, rather than letting it run.
- **Filter envelope:** an envelope that moves the filter's cutoff over the life of a note, separately from the volume.
- **Key tracking:** the cutoff following the note's pitch, so high notes aren't duller than low ones.
- **Filter mode:** which part of the sound the filter keeps: the lows (low-pass), a band around the cutoff (band-pass) or the highs (high-pass).
- **Pulse width (PWM):** how much of each cycle a pulse wave spends high; moving it (pulse-width modulation) gives a pad slow movement.
- **Hard sync:** restarting one oscillator each time another starts its cycle, so its pitch is locked and sweeping it changes its tone instead.
- **Sub-oscillator:** a square wave an octave or two under the main oscillator, derived from it, for weight.
- **Detune:** setting two oscillators slightly apart in pitch so they beat against each other and sound thicker.
- **White noise:** random signal with equal energy at every frequency, heard as hiss.
- **Seeded randomness:** random numbers that come out the same every time from the same starting value (the seed), so renders repeat exactly.
- **Mono mode:** playing one note at a time, with the latest note winning.
- **Legato:** in mono, a new note that overlaps the last changes pitch without restarting the envelopes.
- **Glide (portamento):** sliding the pitch from one note to the next over a set time.
- **PolyBLAMP:** PolyBLEP's companion for a corner rather than a jump, used where the wave's slope changes suddenly.
- **Control rate:** working something out every few samples instead of every sample, and blending in between, where the ear can't tell the difference.
- **Factory sound:** a built-in starting point for the synth's settings, chosen from a menu.
- **Sound brief:** a short description of what a sound should be, with measurable targets, that a factory sound is made from.
- **Voicing guide:** the written record of which settings give which qualities of sound, learned from making the factory sounds.
