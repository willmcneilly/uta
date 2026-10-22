# RFC-010: Reverb

**Status:** Accepted (2026-10-22) · **Author:** Will · **Notion:** [RFC-10](https://app.notion.com/p/Reverb-4013af969b6f81d19df8cc791bd1087a)

## Summary

Uta gets a Reverb: an insert effect for any track's chain or the master, inspired by ValhallaRoom. Like Room it has two parts you balance: **early** energy, the dense first moments that tell you the size of a space, and a **late** tail that blooms and fades. Each part has its own size, stereo spread and slow random modulation. The tail's bass and treble can fade faster or slower than its middle. It's algorithmic: a network of delay lines that feed each other, built by hand in Uta to the audio thread rules. The analysis tools get the reverb measurements RFC-009 left to this RFC (decay time per band, echo density, colouration, stereo spread), so Claude can tune it with numbers before asking your ear. It comes with a menu of eleven factory sounds (a drum room, a vocal plate, a cathedral, gated drums, a slow bloom and more), each made from a brief with measured targets, as the synth's are. It's finished in the ambient song. Sends stay out, as their own RFC.

## Motivation

- **Every demo song asks for it.** The README of `examples/demos` lists Reverb as missing from all four songs. In ambient it's "the biggest gap by far: without it there's no space, and every note ends where the synth's release ends." Ambient was left unfinished on purpose, to be finished around this.
- **Sound decides whether Uta continues.** A dry mix sounds like a demo. The Delay gave Uta echoes, but not a room for the sounds to sit in.
- **The groundwork is done.** RFC-007 built the chain and its patterns with a reverb in mind: tracks are stereo from the chain onwards, an effect's memory is made off the audio thread, and tails, sleeping and fades are handled by the chain. RFC-009 built the measuring and listening tools, and left reverb's own measurements to this RFC because they need a reverb to check against.
- **Reverb is the hardest effect to judge.** A reverb can hit its decay time and still sound metallic, grainy or fluttery. That's why it comes after the hearing tools, and why its measurements are part of it.

## Proposal

### What inspires it

ValhallaRoom, Sean Costello's reverb, which you named as your favourite. It's an inspiration, not a target to copy: we take what's public about how it's built and what it's for, and go our own way from there. What's public about it, from Valhalla's blog and Costello's forum posts:
- **It's algorithmic only.** No convolution, no recordings of real rooms. Costello calls its design psychoacoustic: an idealised impression of a room, not a physical model.
- **It has an Early and a Late section,** each stereo in and stereo out, with a Depth slider balancing them.
- **Its early section has no allpass filters at all.** An allpass is the classic building block for smearing echoes. Costello avoids them because short chains of them "ring" on drum hits and vocals, which sounds metallic.
- **Its late section has "up to a few dozen" slow, randomised modulators,** which smear the resonances that make cheap reverbs sound metallic.
- **Each of its twelve modes is a different arrangement** of delay lines, filters and mixing, with its own character (Large Room, Large Chamber, Dark Space, and so on).
- **Its exact algorithms aren't public.** Costello says getting them to sound good took years of "eliminating stray resonances". We borrow its shape (an early and a late section, the controls that make it playable) and its goals (dense, smooth and never metallic), and build our own insides from published designs. The aim is a reverb you love using in Uta, judged on its own terms, not a copy of Room.

### The building blocks

In the order the sound passes through them.

**1. Predelay.** A short delay before anything else, so the dry hit is heard clearly before the space answers. It's a delay line like the Delay's tape, without feedback.

**2. Early energy: the diffuser.** This is what turns one hit into a dense burst of echoes in the first few tens of milliseconds.
- The sound is split into 8 copies. Each is delayed a slightly different, short time. Then they're shuffled, some have their polarity inverted (flipped upside down, like a channel's Ø button), and they're mixed together.
- That's one step. Doing it four times turns one click into thousands of echoes: each step multiplies the echoes by 8.
- This is the design from Geraint Luff's talk "Let's Write a Reverb" (Signalsmith Audio, ADC 2021). It has no allpasses, so it doesn't ring, which is Room's goal for its early section too.
- **Early Size** scales the delay times: from a small room's 10 ms up to a gated burst of up to a second.
- **Early Cross** sets how much the left and right sides swap as they pass through, which widens the image and thickens it.
- **Diffusion** sets how many steps and how much mixing: from separate, audible early echoes to a smooth burst.
- **Modulation** gently wobbles a few of the delays, slowly and at random.
- **The early output is taken from the last step's lanes before its mixing.** Adding up the outputs of a mixing step keeps only one lane's share of the echoes, because its plus and minus signs cancel everything else. Tapping the lanes before the last mix keeps all of them. The explainer page showed this; it's written down because it's easy to build the other way.

**3. The late tail: a feedback delay network.** This is what keeps sounding.
- **What it is.** 16 delay lines, each a different length (tens to a couple of hundred milliseconds). Whatever comes out of each line is mixed into all of them and goes round again. Each lap, the sound loses a little, so the tail fades.
- **Why it doesn't sound metallic.** A single delay line fed back on itself rings at one pitch, like blowing across a bottle. With 16 lines whose lengths share no common rhythm (their lengths are chosen so no two divide evenly into each other), the resonances crowd together so densely that you hear a wash instead of a pitch. The research's rules of thumb, which the tests check: enough total delay for the longest Decay, at least 8 lines, and every resonance fading at the same rate.
- **Every resonance fading at the same rate** is the trick that matters most (Jot's design, 1991, which Costello lists among the best papers on reverb). A longer line loses a bit more per lap than a short one, in proportion, so no single resonance outlives the rest and sings on its own.
- **Decay** is the time the tail takes to fall by 60 dB, the standard measure of a room's reverb time ("RT60"). Each line's loss per lap is worked out from it exactly.
- **Bass Mult and High Mult** make the low and high frequencies fade faster or slower than the middle. Each line has two tone filters, one at **Bass Xover** and one at **High Xover**, which set how much more or less the lows and highs lose per lap. A High Mult of 0.5 makes the highs fade in half the time, the way air and soft furnishings swallow treble in a real room.
- **Late Size** scales every line's length together: a bigger room has its reflections further apart.
- **Late Cross** sets how much the left and right halves of the network feed each other. At 0 they're two separate reverbs and the stereo image of what goes in is kept. At full, everything spreads across both sides.
- **Modulation:** some of the lines' lengths drift slowly and at random, each at its own rate. That smears the resonances further, which is the "lush" quality. Around 0.5 Hz it just smooths the tail; above 1 Hz it starts to sound like chorus, as Room's notes say. The drifting uses the Delay's cubic interpolation, and it restarts from the same point on every Play, like the Delay's Wobble, so what you hear is what you render.

**4. Putting them together.**
- **Early Send** feeds the early burst into the late network. At 0 they run side by side. Turned up, the tail starts denser, or, with a long Early Size, swells in slowly.
- **Depth** blends the two outputs: 0% is only early, 100% is only late. Like Room, the levels are balanced so turning Decay doesn't change the blend.
- **High Cut** (a −12 dB per octave low-pass) and **Low Cut** (a low shelf, off at its lowest setting) shape the whole reverb, as Room's do.
- **Mix** is the chain's own, shared by every effect: equal-power, the same curve Room uses.

**5. Modes.** A mode is a different arrangement of the same blocks: how many lines and how long, how many diffuser steps, how the modulation moves, where the outputs are taken, and how dark it is built to be. That's how Room's twelve differ. This project builds the machinery for modes and three of them (open question 1):
- **Room:** a natural, medium to large room. Starts a little sparse, builds to a dense tail. Good on drums and keys. In the spirit of Room's Large Room.
- **Chamber:** dense from the very first moment, smooth and even. Good on snares and vocals. In the spirit of Large Chamber and Dense Room.
- **Space:** huge and dark, with a slow, sparse onset and deep, slow modulation. Nothing much above about 11 kHz. For ambient pads. In the spirit of Dark Space and LV-426.

Uta's modes get their own names. We don't use Valhalla's.

### The controls

Room's set, apart from its newest control, Space (open question 3). The ranges are Uta's: Room stores its ranges as hidden 0 to 1 values, so its exact limits aren't public.

| Control | Range | What it does |
|---|---|---|
| **Mode** | Room · Chamber · Space | The arrangement (above) |
| **Predelay** | 0 to 500 ms | The gap before the space answers |
| **Decay** | 0.1 to 30 s | How long the tail takes to fall 60 dB (open question 2) |
| **Depth** | 0 to 100% | Early only, to late only |
| **High Cut** | 1 to 20 kHz | Darkens the whole reverb |
| **Low Cut** | off, 20 Hz to 1 kHz | Thins the whole reverb |
| **Early Size** | 2 ms to 1 s | The early burst's length |
| **Early Cross** | 0 to 100% | How much left and right swap in it |
| **Diffusion** | 0 to 100% | Separate early echoes, to a smooth burst |
| **Early Send** | 0 to 100% | How much of the early burst feeds the tail |
| **Early Mod Rate / Depth** | 0.05 to 5 Hz / 0 to 100% | Its slow random wobble |
| **Late Size** | 0 to 100% | The room's size |
| **Late Cross** | 0 to 100% | How much the tail's sides feed each other |
| **Late Mod Rate / Depth** | 0.05 to 5 Hz / 0 to 100% | The tail's slow random drift |
| **Bass Mult** | 0.25 to 4× | The lows' decay, as a multiple of Decay |
| **Bass Xover** | 100 Hz to 2 kHz | Where the lows start |
| **High Mult** | 0.1 to 2× | The highs' decay, as a multiple of Decay |
| **High Xover** | 1 to 16 kHz | Where the highs start |
| **Mix**, **On / off** | | Shared by every effect (RFC-007) |

The defaults are Room mode, a 2 s Decay, Depth at 50%, a High Mult of 0.5 and Mix at 25%. They're a starting point, tuned by ear.

### Turning knobs while it rings

- **Most settings glide,** as the Delay's do, so nothing clicks.
- **The sizes glide too.** Changing Late Size or Early Size while the tail rings moves every delay length, which bends the pitch of what's ringing, as the Delay's Time does. Done slowly, over about a quarter of a second, it should sound like the room swelling, not a swoop. That's a guess until you try it (Risks).
- **Changing mode** is a different arrangement, so the old tail fades out over about 50 ms and the new mode starts from silence. You'd normally choose a mode before playing, not during.

### Factory sounds

The Reverb gets a menu of factory sounds, the classic reverbs you'd reach for, made the way RFC-008 made the synth's:
- **Each starts from a brief:** what it should sound like and where the sound comes from, with **measurable targets** taken from the reverb measurements below. Reverb suits this even better than the synth did, because a reverb's character is mostly things we can measure: how long each band lasts, how the early and late parts are balanced and when each arrives, and how quickly the echoes thicken.
- **Choosing one is one command** that sets every Reverb setting at once, including its mode, and one undo puts them all back, as `SetSynthSettings` does for the synth. If the mode changes, the old tail fades out as it does when you change mode by hand.
- **They don't change Mix.** Mix is how much reverb this track gets, which is yours, so trying presets keeps it where you set it. Room calls this Mix Lock. Each brief suggests a Mix, shown in the menu.
- **They're balanced by measurement,** so at the same Mix no preset is much louder than another, as the kit's and the synth's sounds were.
- **The voicing guide grows a reverb page,** `docs/sound-design/reverb.md`, beside the synth's: which settings give which qualities (tight or roomy, bright or dark, smooth or grainy, still or moving), learned from making them.
- **In the panel,** a preset icon opens the menu, folding in the scratchpad note "Presets need a preset icon that opens a menu, not a select". It's the same control the synth panel should use, so it's settled in the design sandbox once for both.

The first drafts, from the explainer page (open question 6):

| Factory sound | The brief, in short | Mode | Measured targets (first guesses) |
|---|---|---|---|
| **Drum room** | A kit in the live room it was played in: thick and real, barely any tail | Room | Mostly early energy; early burst over by 20 ms; mid decay 0.5 s |
| **Vocal plate** | A steel plate: no walls so no early reflections, dense at once, bright, thin in the bass | Chamber | No separate early burst; echo density near 1 within 10 ms; mid decay 1.8 s; highs 0.9×, lows 0.7× |
| **Echo chamber** | A hard, tiled room, with the lows and the very top filtered out (the "Abbey Road trick", a rule of thumb with no first-hand source) | Chamber | Dense, even burst; mid decay 1.9 s; little energy below 500 Hz or above 10 kHz |
| **Concert hall** | A big, lush digital hall with slow movement in the tail | Room | Predelay 30 ms; mid decay 2.8 s; lows 1.3×, highs 0.55× |
| **Cathedral** | Stone, enormous, the tail darkening as it rings | Space | First sound after 60 ms; mid decay 7 s; highs 0.4× |
| **Gated drums** | The 80s burst that stops dead, with no tail | Chamber | All early energy; nothing left 20 dB down after 300 ms |
| **Ambient wash** | Long, dark and drifting, every note melting into the next: the ambient song's | Space | Mid decay 18 s; highs 0.35×; little above 6 kHz |
| **Dark room** | A cheap, dark, slightly wobbly space, for lo-fi | Room | Mid decay 1.3 s; little above 3.5 kHz; audible pitch movement in the tail |
| **Bloom** | Each note swells in rather than starting at once, so chords grow out of nothing | Space | The tail at its loudest 0.3 to 0.5 s after the note; mid decay 6 s |
| **Endless** | The longest, darkest tail there is: a pad chord still hanging under the next | Space | Mid decay 30 s; highs 0.3×; little above 5 kHz |
| **Distant** | A part placed far behind the mix: late, dull and mostly space | Room | First sound after 110 ms; mostly tail; little above 3.2 kHz or below 300 Hz |

The vocal plate is an approximation on the Chamber mode. A real plate has its own character, which a Plate mode (Dattorro's design, under Alternatives) would give it later. A spring reverb, shimmer, reverse reverb and freeze aren't in the list, because this design can't make them.

### Fitting the chain

Nothing in RFC-007's patterns changes. The Reverb is a new kind of effect:
- **In the core:** a new `EffectKind` and its settings, and a "set a Reverb setting" command, as the Delay has. The command format goes up by one.
- **Its state** (the predelay, diffuser and network's delay lines, and the modulators' positions) is made on the control side when it's added, sized for the sample rate and for the largest sizes, and handed over with the snapshot. That's a few MB at 192 kHz, like a Delay.
- **It does the five things every effect does:** gets ready for a sample rate, processes a stereo block, glides to new settings, says whether it's still ringing, and clears itself. Plus restarting its modulators on Play.
- **Its tail and sleeping** use the chain's rules: Stop lets it ring out, off fades it and clears it, and it sleeps once the tail is below −120 dB. `uta render` runs on until it sleeps, up to the 30 s limit.
- **Delay lengths are worked out in seconds, then in samples** for the current sample rate. So the Reverb sounds the same at 44.1, 48 and 96 kHz, though its samples aren't identical.
- **The test kit** from RFC-007 runs on it by adding it to the list.

### In the window

- **+ lists Reverb** after Delay.
- **The Reverb panel has a core view and an advanced view.**
  - **Core,** what you see when you add it: the preset menu, Mode, Predelay, Decay, Size, Depth, High Cut, Low Cut and Mix. Enough to choose a space and fit it to the track. Size here is Late Size, the one size that matters most.
  - **Advanced** opens the rest, in two groups as Room arranges them: Early (Size, Cross, Diffusion, Send, Mod Rate and Depth) and Late (Cross, Mod Rate and Depth, and the Bass and High Mults and Xovers).
  - Whether the panel opens core or advanced is remembered per panel, not saved in the project. How the switch looks and where the advanced controls go (a wider panel, or a second page) is settled in the design sandbox and logged as provisional.
  - Mode is a segmented choice and the rest are knobs, by RFC-005's control rule.
- **The panel may draw the tail,** following "Draw the sound": the decay's shape across low, mid and high, which changes as you turn Decay and the Mults. That's settled in the design sandbox before the panel ticket, as the Delay's panel was. The core view is about as wide as the Delay's panel; the advanced view is the design question. Anything `DESIGN.md` doesn't cover is marked provisional and logged.

### Measurements, as RFC-009 promised

RFC-009 left reverb's measurements to this RFC. Each is a function in `uta-analysis` and a line in `uta analyse`'s report. They're measured from the reverb's **impulse response**: the sound of a single click through it at Mix 100%, which holds everything about how a reverb behaves. `examples/listen/reverb` gets the command lists that render those clicks.

| Measurement | What it catches | How |
|---|---|---|
| **Decay time per band** | A Decay or Mult that's wrong; a boomy or fizzy tail | The standard: sum the energy backwards from the end, then fit how fast it falls from −5 to −35 dB, per octave |
| **Echo density** over time | Grainy, fluttery tails; a sparse onset | Abel and Huang's measure: how close each short window is to smooth noise. It's reported to match listening tests well |
| **Colouration** | Metallic ringing | How bumpy the tail's spectrum is once smoothed: the resonances that stick out. Our own proxy (Risks) |
| **Stereo spread** | A narrow or lopsided space | How alike the left and right are, early and late |
| **Onset** | Predelay wrong | When the first sound arrives |

And three pictures, each with its numbers printed on and a line saying what to look for (folding in the scratchpad note "Explain what to look for in each analysis image"):
- **The impulse response** itself, as a waveform and a spectrogram.
- **Decay by frequency** (an "energy decay relief"): a spectrogram of how the tail falls. A Bass Mult that's working shows the lows lasting longer. A metallic ring shows as a line that outlives the rest.
- **Echo density over time:** a line rising to 1 as the tail turns into a smooth wash.

### Listening to it

- **Reverb's sessions use `scripts/listen`.** Folding in the scratchpad note "Listening loop includes the ring-out silence": a reverb's ring-out is the point, but a session's loop shouldn't wait through silence below hearing. The loop ends when every candidate's tail is below −60 dB.
- **A fixed canvas of sounds** to judge it on, folding in "A consistent canvas of parts to tune sounds against": a snare hit, a short piano-like chord, a vocal-like swell and a pad, rendered through each candidate. The same canvas every time, so verdicts compare.
- **No outside reference.** You haven't used ValhallaRoom for years (open question 4), so the sessions compare our own candidates against each other: two versions of a mode, or a mode before and after a tuning pass. If you ever want an outside reference, Ableton's Reverb on the same canvas is one render away, since sessions take a recording as just another candidate.

### The ambient song is finished in this project

As RFC-009 planned, ambient is where Reverb is tried first. It gets a long Space reverb on the Pad and Strings and one on the Bell after its Delay, so the echoes bloom. Each Pad and Bell track has its own reverb, since there are no sends yet. The song is then finished around it: its sections reworked so the space has time to bloom and fade. The other three songs each get a reverb where their notes ask for one: the lo-fi keys and snare, the house claps and the stab's tail in the break, and the techno dub chord and clap.

### Not in this project

- **Sends and returns.** You'll tackle them as their own RFC. Until then, each track that needs a reverb has its own.
- **Room's other nine modes,** and any mode beyond the three. Each is a later ticket, tuned by ear.
- **Space,** Room 2.0's control that feeds the early output back into itself (open question 3).
- **Convolution reverb.** You chose to see how far algorithmic goes.
- **Freeze,** shimmer (octave-up), a gated reverb with its own gate, ducking, and Predelay synced to the tempo. Room has none of these. A long Early Size gives Room's style of gated burst.
- **Saving your own reverb presets,** which needs saving, and remembering which factory sound a reverb started from (as with the synth).
- **A Plate mode,** a spring, and any factory sound that needs a mode this project doesn't build.
- **Automation.**

## What it looks like to you

You open the ambient song. It's dry and thin, as it is today. You select the Pad, open the Sound tab, click **+** and choose Reverb. A panel appears with the core controls and a picture of the tail. You set Space and pull Decay to 8 s. The chords stop ending: each one blooms, hangs, and fades into the next. You open the advanced view and turn High Mult down, and the tail darkens as it fades, while the picture shows the treble falling away first. You turn up Late Mod Depth and the tail starts to shimmer and move.

You add a Reverb to the snare in the lo-fi song, choose Chamber, set a short Decay and a little Predelay. The snare sits in a small room instead of in your face. You turn Depth to 0 and hear only the early burst, a tight slap of a space. You turn Early Size up past 100 ms and it becomes a gated, eighties burst.

On the vocal-like keys you open the preset menu and step through it: Drum room, Vocal plate, Echo chamber, Cathedral. Each changes the whole space at once, but the amount of reverb stays where you set it, so you're comparing spaces, not levels. You settle on Vocal plate and nudge its Predelay.

You press Stop and the tail rings out. You turn Late Size while it rings and the room swells, without a click. You render, and the file ends after the last tail has died away.

In a listening session, you hear the snare and pad through two versions of the Space mode, A and B, not knowing which is the tuned one. You pick the one you'd use and say why.

## Alternatives considered

- **Freeverb** (8 comb filters and 4 allpasses per side), the classic free reverb, with an MIT Rust port. It's cheap and well tuned, but comb banks colour the sound, and its allpasses ring. Schroeder himself found that design coloured.
- **Dattorro's plate (1997).** Lush, compact and widely ported under MIT and BSD licences (Mutable Instruments' Clouds, jpcima/fverb). But it has one damping control for everything, so no Bass and High Mult. Its delay lengths are fixed "magic numbers" specified at 29,761 Hz. And it's a plate, not a room. It would make a good fourth mode later.
- **Gardner's nested allpass rooms (1992).** A hand-built structure for each room size, so a Size knob can't move smoothly between them, and it's built on the allpasses Room avoids.
- **FunDSP's `reverb_stereo`.** A 32-line network under MIT/Apache. But its decay time is approximate (one loss for every line, not one per line's length), its settings are fixed when it's built, so turning a knob would rebuild it and cut the tail, and it brings FunDSP's way of connecting sounds, which RFC-002 and RFC-007 turned down. It's useful to check our numbers against.
- **Porting CloudSeed** (MIT, C++). Lush, with per-line shelving filters and early reflections. But it sets memory aside when its settings change, its character is a special-effects reverb rather than a room, and porting its code means taking on its design without understanding it. It's reference reading, with credit if we take anything from it.
- **Signalsmith's example code.** The talk's design is the one proposed here, but the example code has no licence, so we write ours from the talk, not from the code.
- **GPL reverbs** (Dragonfly, synfx-dsp's Dattorro, Surge's reverbs). Uta can't use GPL code.
- **Convolution.** Real rooms, recorded. It needs impulse-response files, loading and managing samples, and saving, which is still deferred. You chose algorithmic first.
- **All twelve of Room's modes.** Each is a separate design to tune by ear, and Costello added them over years. Three cover what the songs need.
- **One mode with no modes machinery.** Simpler, but the modes are where much of Room's range comes from, and adding the machinery later would reopen the settings and the command format.

## Risks & unknowns

- **"A reverb you love" is a high bar.** Room took Costello years of "eliminating stray resonances", and we're going our own way without its insides. The tests can show the decay times are right, the tail is dense and the stereo is wide. They can't show it's beautiful. The plan should allow a tuning pass per mode after the first listening session.
- **Metallic ringing.** The commonest way reverbs fail. The design avoids its known causes (prime-like lengths, equal decay, many lines, no allpasses, modulation), and the colouration measurement looks for it. If a ring survives, the fixes are known: move a line's length, or modulate it more.
- **The colouration measurement is our own proxy.** The research's validated measure (Heldmann and Schlecht, 2021) needs the network's maths, not just its sound. Ours, the bumpiness of the tail's smoothed spectrum, is a guess at the same thing. It's checked against your verdicts before any test relies on its limits.
- **Modulation can sound like chorus or seasickness.** Too much and the pitch wavers. The tests measure how far the pitch moves, and the defaults are set by ear.
- **Size changes while ringing are a guess.** Gliding should sound like the room swelling, but it might swoop. If it does, the fallback is a short crossfade to the new size, as Ableton's delay "Fade" mode does.
- **Long tails cost CPU for longer.** A 30 s Decay rings for about a minute before it sleeps. The cost per block is small (a 16-line network is a few hundred sums per sample, around 1 to 3% of a core at 48 kHz, by the research's estimate). The timing report measures it rather than guessing.
- **Long tails and the render limit.** A 30 s Decay is still ringing at −60 dB when `uta render`'s 30 s limit stops it. That's quiet, but not silent. The CLI says so.
- **Memory.** A few MB per Reverb at 192 kHz. A Reverb on every chain at 192 kHz is under 200 MB.
- **Eleven factory sounds is eleven things to tune by ear.** Each starts measured against its brief, so the first versions should be close, but each still wants your listening and a tuning pass. If that runs long, the factory sounds can land in two batches, the ones the demo songs use first.
- **The panel is big.** 19 knobs is more than any panel Uta has, which is why it has a core and an advanced view. Which controls belong in core is a guess until you've used it; the list is easy to change.
- **Sample-rate independence.** Rounding delay lengths to samples differs per rate, so renders at 44.1 and 48 kHz differ slightly. The tests check that their decay times and density match, not their samples. The golden WAV is at 48 kHz only.

## How we'll verify it

**Automated, on every PR.** The RFC-007 effect test kit, with the Reverb added to its list: off and Mix 0 change nothing, no clicks on add, remove, move or switch, the same audio at block sizes 32, 128 and 1024, `assert_no_alloc` and RealtimeSanitizer, and tails and sleeping.

The Reverb's own tests, on impulse responses, with modulation off unless the test is about modulation:
- **Decay is right.** The measured decay time in the mid band is within 10% of Decay, at 0.3, 2 and 10 s, in every mode.
- **The Mults are right.** With Bass Mult at 2, the low octaves' decay is about twice the mid's. With High Mult at 0.5, the high octaves' is about half. Within 15%.
- **It's the same at every sample rate.** Decay per band and echo density match within tolerance at 44.1, 48, 96 and 192 kHz.
- **Dense, not grainy.** Echo density reaches near 1 (a smooth wash) within a limit set per mode: fast for Chamber, slower for Space.
- **Not metallic.** The colouration measure stays under its limit, once its limit has been set against your verdicts.
- **Predelay is exact.** The first sound arrives at the Predelay time, to the sample.
- **Depth.** At 0% there's no late tail after the early burst ends. At 100% there's nothing from the early section on its own.
- **Cross.** With Late Cross at 0, a sound only on the left stays mostly on the left. At full, the two sides are about equal.
- **Modulation is repeatable** and bounded: rendering twice gives the same file, and the pitch moves by no more than the depth allows.
- **Bounded.** At the longest Decay and every setting at its extreme, the output never grows and never passes full scale.
- **Gliding sizes don't click.** Sweeping Late Size and Early Size while a tail rings stays under the jump limit.
- **The measurements themselves** are tested against synthetic signals with known answers: an exponential decay of a known time, white noise (density 1) and a sparse click train (density near 0).

Also:
- **A golden WAV** of a short phrase through each mode, from a committed command list. You approve it in the PR.
- **Timing report:** a case with a Reverb on every track and the master, and the finished ambient song with its reverbs, at p50 and p99. Reported, not gated, but a song's worth of reverbs coming anywhere near the block deadline is treated as a bug to fix in this project, not a limit to live with.
- **Factory sounds:** each meets the measured targets in its brief, and at the same Mix their loudness sits within an agreed range of each other.
- **Project core:** the new commands undo exactly and round-trip through saving; a knob drag is one undo step; choosing a factory sound is one, changes every Reverb setting except Mix, and its undo puts them all back; older command lists still load.
- **UI,** against the mocked back end: + adds a Reverb, its controls send their command, the preset menu lists the factory sounds and choosing one sends its command, and the panel draws from the outline.
- **Demo songs:** CI renders all four, with no clipping and no silent track. `uta analyse` is run on each with its reverb, and the numbers go in the PR.

**Manual, by you:**
1. **Listening sessions** on the fixed canvas: each mode at its defaults, then before and after its tuning pass. The first question each time is "which would you use, and what's wrong with the other?"
2. **The ambient song,** finished: does it bloom and hang the way you wanted?
3. **Turn every knob while it rings,** especially both sizes and the mod rates. Nothing should click. The sizes should swell, not swoop.
4. **A snare through Chamber, short Decay,** and then through a long Early Size: a small room, then a gated burst.
5. **Stop mid-tail, then switch the Reverb off mid-tail.** Stop rings out; off is gone at once, cleanly.
6. **Play each factory sound** against its brief, on the snare, keys and a pad. Does it sound like its name? Step through the menu while a loop plays: the space should change, the amount of reverb shouldn't.

## Open questions

1. **How many modes in this project?** Each mode is tuned by ear, so each is real work. Recommendation: three (Room, Chamber, Space), because between them they cover the four songs: Room for lo-fi keys, Chamber for the claps and snares, Space for ambient. The first mode's ticket builds the machinery, and the other two follow as their own tickets, so they can be tuned in parallel. The alternative is one mode, Room, with the rest later, which is quicker but leaves ambient without its dark, huge space. **Resolved: three modes (Will, 2026-10-21).**
2. **How long can Decay be?** Room goes from 0.1 to 100 s. Past about 30 s a tail barely fades within a song and acts more like a freeze. It also outlasts `uta render`'s 30 s limit. Recommendation: 0.1 to 30 s, raised later if you miss the top. The ranges are the core's, so raising it is one number and its tests. **Resolved: 0.1 to 30 s (Will, 2026-10-21).**
3. **Include Space** (Room 2.0's feedback around the early section, which turns it into a reverb of its own)? Recommendation: not in this project. It came eleven years after Room's first release, and it's a feedback loop that needs its own tuning. Better to get the early and late sections right first. **Resolved: not in this project (Will, 2026-10-21).**
4. **Do you have ValhallaRoom?** If so, it's the reference for the blind sessions: you render the fixed canvas through it once, in Ableton, at settings matched to each of our modes, and the sessions put the two side by side. Recommendation: yes, if you have it. If not, the sessions compare our candidates against each other, and against Ableton's Reverb if you'd like a reference. **Resolved: no, not for years, and Room is an inspiration rather than something to match. The sessions compare our own candidates (Will, 2026-10-21).**
5. **A reverb on every track that needs one, until sends arrive?** In the songs, that's up to about five reverbs. Recommendation: yes. It's what this scope means, and it costs some CPU but nothing else. The sends RFC can then move each song to one shared reverb. **Resolved: yes. If Uta can't run five reverbs, that's a performance problem to fix, not a reason to limit them (Will, 2026-10-21).**
6. **Which factory sounds, and how many?** Recommendation: the eight in "Factory sounds": Drum room, Vocal plate, Echo chamber, Concert hall, Cathedral, Gated drums, Ambient wash and Dark room. Between them they cover the four demo songs (the dark room for lo-fi, the drum room, chamber and gated drums for house and techno, the wash and cathedral for ambient) and the reverbs you'd most often reach for. The spring and the "can't do" sounds stay out. Swap any for one you'd use more. **Resolved: the eight, plus three ambient ones, Bloom, Endless and Distant, eleven in all (Will, 2026-10-22).** They were chosen from five ambient candidates heard in the explainer page: **Bloom** (each note swells in, its tail peaking about 0.4 s later), **Endless** (a 30 s, dark, drifting tail, as near a freeze as this design gets), **Distant** (long predelay, mostly tail, both ends rolled off: a part far behind the mix), **Ensemble** (fast, deep modulation that choruses the tail) and **Canyon** (separate echoes first, melting into a wash). Ensemble overlaps Ambient wash with more modulation, and Canyon is closer as a Delay before a Reverb than as a preset, so those two stay out.
7. **Should a factory sound leave Mix alone?** Recommendation: yes, as Room's Mix Lock does, with each brief's suggested Mix shown in the menu. Mix is how much reverb this track wants, and that doesn't change when you try a different space. The alternative is that each preset sets its own Mix, which makes gated drums sound right at once but makes comparing presets a comparison of levels. **Resolved: leave Mix alone (Will, 2026-10-21).**

## New terms

- **Reverb:** the sound of a space: thousands of reflections arriving after a sound, blending into a tail that fades.
- **Impulse response:** the sound of a single click through a reverb (or a room), which captures everything about how it behaves.
- **Decay time (RT60):** how long a reverb's tail takes to fall by 60 dB; the standard measure of how long a space rings.
- **Predelay:** the gap between a sound and its reverb starting.
- **Early reflections (early energy):** the first, dense part of a reverb, which tells you the size and shape of a space.
- **Late reverb:** the smooth tail that follows the early part and fades away.
- **Diffusion:** how quickly separate echoes are smeared into a smooth burst.
- **Feedback delay network:** several delay lines of different lengths whose outputs are mixed and fed back into all of them, used to make a reverb's tail.
- **Colouration:** a tone or ring a reverb adds that the space shouldn't have, heard as metallic or boxy.
- **Crossover:** the frequency where one band (bass, mids, treble) hands over to the next.
- **Reverb mode:** one arrangement of a reverb's building blocks, with its own character (Room, Chamber, Space).

Changed, not new: **Factory sound** is in the Glossary as the synth's. Its entry widens to "a built-in starting point for an instrument's or effect's settings, chosen from a menu".
