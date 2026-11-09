# Voicing the reverb

How to turn an idea for a space into the Reverb's settings: which settings give which qualities, and what tuning its modes taught about them. It's for whoever voices a mode or a factory sound next, a person or an agent (RFC-010). It starts with Room (UTA-101); Chamber and Space add what they teach.

The settings themselves, with their ranges and a sentence on how each sounds, are described in Rust, in `crates/uta-core/src/reverb.rs` (`ReverbParam::describe`). A mode's arrangement (its lines' lengths, its diffuser's steps) is in `crates/uta-engine/src/effects/reverb.rs`.

## The loop

1. **Measure the impulse response.** The Reverb's own tests use a one-sample click (`effects/reverb/tests.rs`), and that's what the limits below are for. `examples/listen/reverb/` has command lists for `uta analyse --impulse-response`, but their click is 2 ms of noise, whose own spectrum adds about 1.5 dB of colouration: compare those renders with each other, not with the limits.
2. **Fix what the numbers show** before anyone listens: a decay off its setting, a sparse stretch, a ring.
3. **Listen on the canvas** (`examples/listen/canvas/`): the chord for the tail on its own after a sound stops, the snare for the early burst and for rings, which a transient shows best. Ask one question at a time with `scripts/listen`.
4. **Keep the tests in step:** each mode's colouration test (`room_stays_under_colouration_s_limit`, `chamber_stays_under_colouration_s_limit`, `space_stays_under_colouration_s_limit`) and echo-density test (`room_s_echo_density_reaches_near_1_within_its_limit`, `chamber_s_echo_density_reaches_near_1_within_its_limit_and_stays_there`, `space_s_echo_density_reaches_near_1_within_its_slower_limit`), Space's darkness test (`space_has_little_above_11_khz`), and the golden WAVs (`examples/demo-reverb.json`, `examples/demo-reverb-chamber.json`, `examples/demo-reverb-space.json`).

## The limits

- **Colouration under 0.85 dB** (`impulse::COLOURATION_LIMIT_DB`), on a one-sample click, on the mid, with a Decay of 2 s or more. Set from listening: Room at 0.75 dB was picked as smooth, Room at 0.95 dB (with less Depth) was heard as the more metallic of the two, and a network made to ring, at 2.08 dB, was heard as ringing. A still tail has its own, looser guard, 1.2 dB (`STILL_COLOURATION_LIMIT_DB`), since the drift smears a ring more than it does Room: the ringing network reads 0.89 dB drifting.
- **Echo density near 1 within each mode's own limit** of the first sound (in the Reverb's tests, `ROOM_DENSE_WITHIN_SECONDS` and `CHAMBER_DENSE_WITHIN_SECONDS`): Room's is 40 ms, at Early Size 30 ms to 100 ms, any Late Size, and Depth from 50%. Chamber's is 20 ms at Early Size 30 ms (it reaches near 1 in 7 to 19 ms), and its echo density never falls below 0.5 in the first 300 ms (`CHAMBER_STAYS_DENSE`), where Room's falls to nothing. Space's is a slower 50 ms (`SPACE_DENSE_WITHIN_SECONDS`): it reaches near 1 in 9 to 35 ms, at Early Size 30 ms to 100 ms, any Late Size, and Depth from 50%.
- **Little above 11 kHz, for Space:** under 5% of its energy (`SPACE_MOST_ABOVE_11K`), at the defaults, with the highs lasting longest, and at Depth 0% and 100%. It measures about 3%, against Room's 33%.

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

Chamber is Room's network heard another way. Its lines are Room's; its tail is heard from a tap partway along each line (11% to 38% of its length) instead of from the line's end. What goes round the loop is the same, so the decay, the Mults and the colouration's causes are Room's; only when the tail is first heard changes. Its diffuser is Room's too.

| Quality | What gives it | Notes |
|---|---|---|
| **Dense from the first moment** | The taps | Room's tail is first heard a lap after the sound goes in (42 ms at Late Size 50%), after a 30 ms burst has ended: a sparse stretch whose echo density falls to nothing at 40 to 53 ms. Chamber's first tap is heard 5 ms after the sound goes in, so the tail starts inside the burst, and its echo density stays at 0.88 or more at the defaults. It falls 20 dB from its peak in 195 ms against Room's 15 ms: one even wash, not a slap then a tail. |
| **Smooth** | The taps alone, and Room's lines | Hearing each line at its tap *and* its end makes a comb for each line: 0.91 dB with Late Cross at 0, over the limit. At its tap alone Chamber reads 0.70 dB at the defaults, under Room's 0.75. Shorter lines close the gap too, but colouration follows the network's total length: lines from 7 to 140 ms read 1.22 dB. |
| **A gated burst** | Depth 0%, Early Size 100 ms or more | With no tail heard, the burst stops dead: at Early Size 250 ms it ends about 240 ms after the hit, then nothing. It swells in over its first 75 ms or so, starting 11 dB under its peak, because the diffuser's steps' delays (shares 8:4:2:1) overlap and their sums bunch in the middle. |

### What the measurements can't see

- **Whether "dense at once" sounds like a plate.** A plate is also dense at once. Whether Chamber sounds like a room or a sheet of steel is for the ear.
- **What makes a gated burst.** A diffuser whose steps were each a third of the one before (27:9:3:1) made a long burst start 6 dB under its peak instead of 11 and hold until it stopped, nearer the textbook shape, with no more colouration. On the snare, Will preferred the burst that swells in.

### What needed an ear

- **That the gap matters.** On the canvas's snare at a short Decay, Will picked Chamber over Room, heard as too much of a slap back: the sparse stretch between Room's burst and its tail, which the numbers had shown but couldn't say was audible.
- **The gated burst's shape.** The numbers' gated burst (a hit at full level, a flat hold) lost to the one that swells in, heard on the snare at Mix 30%. Measure a gated burst's shape, but let the ear pick it.
- **Whether it's plate-like.** Not raised: Will's note on the snare was only about Room's slap back.


## Space

Space is a bigger, darker network than Room, drifting further and more slowly. Its lines run from 81 ms to 285 ms, nearly twice Room's; its tail is heard from taps, as Chamber's is; a low-pass at 8 kHz darkens everything it puts out; and its drift reaches 8 ms where Room's reaches 3 ms, at a quarter of Room's rates. Its diffuser is Room's.

| Quality | What gives it | Notes |
|---|---|---|
| **Huge, slow to build** | Long lines, and taps spread across them | Heard from their ends, lines this long leave a silent hole of about 50 ms between the burst and the tail's first lap, worse than Room's slap back. Taps from 28 ms to 148 ms filled it but peaked at 110 ms and then fell, a second slap. Spread from 28 ms to 208 ms, the tail holds within about 5 dB of its loudest from 40 ms to 200 ms (Room peaks at 60 ms): a tail that builds, then lasts. Its echo density reaches near 1 in 9 to 35 ms, slower than Room's 7 to 33 ms. |
| **Dark** | The 8 kHz low-pass | High Mult and High Cut darken the tail; the low-pass also takes the top off the burst. Space leaves about 3% of its energy above 11 kHz, against Room's 33%. Because the low-pass makes a click rise over a sample, Space's first sound can land a sample after the Predelay at high sample rates. |
| **Smooth** | Long lines | Colouration follows the network's total length, so the longest lines are the least coloured of the three modes: 0.47 to 0.77 dB drifting and 0.56 to 0.81 dB still, at any Late Size and Late Mod Depth. |
| **Deep, slow drift** | `late_mod_seconds` and the line rates | The drift's reach is now part of a mode's arrangement. Space's reaches 8/3 as far as Room's, on rates a quarter as fast (0.155 to 0.345 of Late Mod Rate), so the pitch bends 2/3 as far as Room's: about 7 cents at full Late Mod Depth and 0.5 Hz. |

### What the measurements can't see

- **When a drift wavers.** The bend in cents says how far the pitch moves, not whether it's heard as a waver. At three eighths of Room's rates (about 11 cents at full depth), Will heard full depth waver behind the swell's voice and kept 50%. At a quarter (about 7 cents), he couldn't tell the two apart. Measure the bend, but ask the ear.
- **Whether dark is muffled.** Space's darkness is in its brief, and the pad is already dark (948 Hz brightness). Only listening says whether the two together are woolly.

### What needed an ear

- **The pad.** At the same 8 s Decay and Mix 35%, Will picked Space over Room on the pad as smoother. Not woolly, and no wobble.
- **How deep the drift goes.** Full Late Mod Depth wavered under the swell, so Space keeps Room's default of 50%. Slowing the rates from 3/8 to 1/4 of Room's made no clear difference: on the swell at full depth, Will found the two hard to tell apart. The slower rates were kept for their smaller bend.
