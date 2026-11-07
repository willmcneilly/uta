# Voicing the reverb

How to turn an idea for a space into the Reverb's settings: which settings give which qualities, and what tuning its modes taught about them. It's for whoever voices a mode or a factory sound next, a person or an agent (RFC-010). It starts with Room (UTA-101); Chamber and Space add what they teach.

The settings themselves, with their ranges and a sentence on how each sounds, are described in Rust, in `crates/uta-core/src/reverb.rs` (`ReverbParam::describe`). A mode's arrangement (its lines' lengths, its diffuser's steps) is in `crates/uta-engine/src/effects/reverb.rs`.

## The loop

1. **Measure the impulse response.** The Reverb's own tests use a one-sample click (`effects/reverb/tests.rs`), and that's what the limits below are for. `examples/listen/reverb/` has command lists for `uta analyse --impulse-response`, but their click is 2 ms of noise, whose own spectrum adds about 1.5 dB of colouration: compare those renders with each other, not with the limits.
2. **Fix what the numbers show** before anyone listens: a decay off its setting, a sparse stretch, a ring.
3. **Listen on the canvas** (`examples/listen/canvas/`): the chord for the tail on its own after a sound stops, the snare for the early burst and for rings, which a transient shows best. Ask one question at a time with `scripts/listen`.
4. **Keep the tests in step:** each mode's colouration test (`room_stays_under_colouration_s_limit`, `chamber_stays_under_colouration_s_limit`) and echo-density test (`room_s_echo_density_reaches_near_1_within_its_limit`, `chamber_s_echo_density_reaches_near_1_within_its_limit_and_stays_there`), and the golden WAVs (`examples/demo-reverb.json`, `examples/demo-reverb-chamber.json`).

## The limits

- **Colouration under 0.85 dB** (`impulse::COLOURATION_LIMIT_DB`), on a one-sample click, on the mid, with a Decay of 2 s or more. Set from listening: Room at 0.75 dB was picked as smooth, Room at 0.95 dB (with less Depth) was heard as the more metallic of the two, and a network made to ring, at 2.08 dB, was heard as ringing. A still tail has its own, looser guard, 1.2 dB (`STILL_COLOURATION_LIMIT_DB`), since the drift smears a ring more than it does Room: the ringing network reads 0.89 dB drifting.
- **Echo density near 1 within each mode's own limit** of the first sound (in the Reverb's tests, `ROOM_DENSE_WITHIN_SECONDS` and `CHAMBER_DENSE_WITHIN_SECONDS`): Room's is 40 ms, at Early Size 30 ms to 100 ms, any Late Size, and Depth from 50%. Chamber's is 20 ms at Early Size 30 ms (it reaches near 1 in 4 to 15 ms), and its echo density never falls below 0.5 in the first 300 ms (`CHAMBER_STAYS_DENSE`), where Room's falls to nothing.

## Which settings give which qualities

| Quality | Turn | Notes |
|---|---|---|
| **Smooth ↔ metallic** | Late Mod Depth, then Late Size | The tail's drift is what keeps Room under the limit: still, it reads 1.02 dB; drifting at 50% and 0.5 Hz, 0.75 dB. A slower drift smears less (0.90 dB at 0.2 Hz), and a smaller room rings more (0.97 dB at Late Size 0). |
| **Close ↔ far** | Depth | Depth 50% gives the early burst and the tail the same energy, so the first 80 ms hold 5 dB more than the rest: close and boxy. 65% brings that to 2 dB, and the tail sits up behind the sound. |
| **A burst then a tail ↔ one wash** | Early Size against Late Size | The tail's first lap comes the shortest line's length after the Predelay (42 ms at Late Size 50%, 85 ms at 100%). An early burst that ends before that leaves a sparse stretch between them: with a 10 ms burst, echo density takes 70 to 100 ms to reach near 1, against 7 ms with the default 30 ms. |
| **Dark ↔ bright tail** | High Mult, then High Cut | High Mult darkens as the tail fades, as a room does; High Cut darkens all of it at once. |
| **Narrow ↔ wide** | Late Cross, Early Cross | With the same sound on both sides, both read wide at any cross; a cross at 0 keeps a sound on its side. Late Cross 0 is the most coloured of the tone settings (0.81 dB), as each side is a reverb of 8 lines. |

## What the measurements can't see

- **Colouration reads higher on a short tail,** with less of it to average: Room reads 1.0 dB at a 1 s Decay and 1.5 dB at 0.5 s, all else the same. Its limit only holds from 2 s, and a short room's ring still needs an ear.
- **It's blind to a buzz.** A network whose lines are all whole multiples of 10 ms echoes every 10 ms, a 100 Hz buzz, but its spectrum is as smooth as Room's (1.01 dB). The rule that no two lines share a rhythm is what keeps that out, not the measure.
- **The drift lowers it more than it lowers a ring.** With the drift on, even a network made to ring reads under 1 dB (six distinct lengths, 1.23 dB still, read 0.84 drifting). That's why the test checks each case still as well, against its own guard.

## What needed an ear

- **Room's defaults.** The numbers said the burst dominated and the tail was a little coloured. Only listening said that mattered: on the chord, the defaults were heard as metallic, and Depth 65% with the drift on was softer. That's where the colouration limit came from.
- **Whether a ring is a fault.** A ring can be a character rather than a fault (a plate is one someone might want), so the question that set the limit asked only which rang, not which was better. Room shouldn't ring.
- **Still to judge:** Room at a small Late Size and at short Decays, which read over the limit; and the dip between the end of the burst and the tail's first lap, about 40 to 53 ms after a click at the defaults, which the chord and the snare hide.

## Chamber

Chamber is Room's network heard another way. Its lines are Room's; its tail is heard from a tap partway along each line (11% to 38% of its length) instead of from the line's end. What goes round the loop is the same, so the decay, the Mults and the colouration's causes are Room's; only when the tail is first heard changes. Its diffuser's steps are spread further apart than Room's, so a long burst holds level.

| Quality | What gives it | Notes |
|---|---|---|
| **Dense from the first moment** | The taps | Room's tail is first heard a lap after the sound goes in (42 ms at Late Size 50%), after a 30 ms burst has ended: a sparse stretch whose echo density falls to nothing at 40 to 53 ms. Chamber's first tap is heard 5 ms after the sound goes in, so the tail starts inside the burst, and its echo density stays at 0.88 or more at the defaults. It falls 20 dB from its peak in 209 ms against Room's 15 ms: one even wash, not a slap then a tail. |
| **Smooth** | The taps alone, and Room's lines | Hearing each line at its tap *and* its end makes a comb for each line: 0.91 dB with Late Cross at 0, over the limit. At its tap alone Chamber reads 0.70 dB at the defaults, under Room's 0.75. Shorter lines close the gap too, but colouration follows the network's total length: lines from 7 to 140 ms read 1.22 dB. |
| **A gated burst** | Depth 0%, Early Size 100 ms or more, and the diffuser's steps | With no tail heard, the burst stops dead: at Early Size 250 ms it ends about 240 ms after the hit, then nothing. On Room's diffuser, whose steps' shares are 8:4:2:1, a long burst swells in: it starts 11 dB under its peak and takes 75 ms to reach it, because the steps' delays overlap and their sums bunch in the middle. Chamber's steps are each a third of the one before (27:9:3:1), adding up more evenly, like a number's digits: it starts 6 dB under and holds within 2 dB until it stops. A quarter (64:16:4:1) started flatter still, but read 0.83 dB, near the limit. |

### What the measurements can't see

- **Whether "dense at once" sounds like a plate.** A plate is also dense at once. Whether Chamber sounds like a room or a sheet of steel is for the ear.
- **How abrupt a gated burst should be.** The numbers say Chamber's burst holds and stops; whether that's the eighties sound is for the ear, and on the snare at Mix 30% the dry hit covers much of the difference.

### What needed an ear

- **That the gap matters.** On the canvas's snare at a short Decay, Will picked Chamber over Room, heard as too much of a slap back: the sparse stretch between Room's burst and its tail, which the numbers had shown but couldn't say was audible.
- **Whether it's plate-like.** Not raised: Will's note on the snare was only about Room's slap back.

