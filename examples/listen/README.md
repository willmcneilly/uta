# Listening renders

Short command lists for judging each drum sound and the synth by ear, one folder per sound. Tests can show a sound has the right pitch and decay, but not that it's satisfying, so every sound ticket ends with these (RFC-006, "How we'll verify it", manual check 1, and RFC-008's manual check 1).

Render a folder to WAVs with one command, from the repository root (`target/` is ignored by git):

```bash
cargo run --release -p uta-cli -- render-all examples/listen/kick target/listen/kick
```

The other sounds are the same with their folder's name in place of `kick`.

Each list sets the master to 0 dB, so a full accent with its Level at 0 dB peaks at -12 dBFS, and an ordinary hit (velocity 100) about 8 dB under it. Each sound's default Level then balances it against the kick (see [The kit](#the-kit)), so a sound at its defaults may be louder or quieter than that. A sweep is one drum track per setting, each playing its own bar, so you hear the steps in order.

## Kick (808)

| Render | What it plays |
|---|---|
| `01-defaults` | Four on the floor at the defaults (Tune 49 Hz, Tone 0.2, Decay 0.3 s), with beat 1 of each bar accented. |
| `02-tune` | Tune 40, 45, 49, 55, 62, 70 and 80 Hz, two hits a bar. |
| `03-tone` | Tone 0, 0.2, 0.4, 0.6, 0.8 and 1. |
| `04-decay` | Decay 0.05, 0.1, 0.2, 0.3, 0.5 and 0.8 s, one hit a bar. |
| `05-level` | Level 0, -6, -12 and -24 dB. |
| `06-velocity` | A kick a beat, velocity 8 up to 127. |
| `07-repeats` | 8ths, 16ths, then 32nds, at the longest Decay so they pile up. |
| `08-flams` | A soft grace hit then an accent, 40, 20 and 10 ms apart. |
| `09-groove` | A 1-bar 808 pattern at 100 BPM, looped. Also the kick's golden WAV. |

What to listen for, against how you remember an 808:

- **Defaults:** a deep, round thump with a short, soft click, not a sine "boop". The first few milliseconds should feel punchy rather than sound like a pitch sweep, and the note should sag very slightly as it dies away.
- **Tune:** each step is clearly a different note, and the character stays the same: no step sounds thinner or quieter than its neighbours.
- **Tone:** 0 is all thump, with almost no click; 1 has a clear, bright click on top. Somewhere around 0.2 to 0.4 should sound like the classic kick.
- **Decay:** from a tight "tuck" at 0.05 s to a long boom at 0.8 s. The hardware tops out about there; a much longer "808 bass" is a later question.
- **Velocity:** the soft hits should sound softer *and* duller, like a lighter hit, not just quieter. 100 is an ordinary hit and 127 an accent: the accent should be clearly punchier, not only louder.
- **Repeats and flams:** no clicks anywhere, and the 32nds shouldn't sound like a machine gun: each hit should blend into the ring of the last, a little different every time. The flams should sound like one fat hit at 10 ms and two hits at 40 ms.

## Kick (909)

The kick's other model, in `kick909/`. Each list chooses it on the kick row.

| Render | What it plays |
|---|---|
| `01-defaults` | Four on the floor at the defaults (Tune 55 Hz, Sweep 0.4, Attack 0.5, Decay 0.5 s), with beat 1 of each bar accented. |
| `02-tune` | Tune 45, 50, 55, 62 and 70 Hz, two hits a bar. |
| `03-sweep` | Sweep 0, 0.2, 0.4, 0.6, 0.8 and 1. |
| `04-attack` | Attack 0, 0.25, 0.5, 0.75 and 1. |
| `05-decay` | Decay 0.1, 0.2, 0.35, 0.5, 0.8 and 1.5 s, one hit a bar. |
| `06-level` | Level 0, -6, -12 and -24 dB. |
| `07-velocity` | A kick a beat, velocity 8 up to 127. |
| `08-repeats` | 8ths, 16ths, then 32nds, at the longest Decay so they pile up. |
| `09-flams` | A soft grace hit then an accent, 40, 20 and 10 ms apart. |
| `10-groove` | A 1-bar house pattern at 125 BPM, looped. Also the 909 kick's golden WAV. |
| `11-808-and-909` | The 808 then the 909 at their defaults, a bar each, twice: an A/B. |

What to listen for, against how you remember a 909:

- **Defaults:** tighter and punchier than the 808: a fast downward "doom" at the start and a short click on top, with a firm body that sits in the low mids rather than a long sub boom.
- **Tune:** each step a different note, with the same character.
- **Sweep:** 0 has no drop at all, a plain thud; up to 0.5 the drop gets deeper, from up to 4.5 times the note; past 0.5 it gets slower, towards a laser-ish "pew" at 1. Somewhere around 0.3 to 0.5 should be the classic 909.
- **Attack:** 0 is a soft, rounded thump; 1 a hard, bright click on the front, like a beater on a tight head. The body shouldn't change level.
- **Decay:** from a short "tuck" at 0.1 s to a long boom at 1.5 s.
- **Velocity:** soft hits softer *and* rounder; accents clearly harder, with more click.
- **Repeats and flams:** no clicks anywhere, and no machine gun: each hit adds to the ring of the last, so the 32nds blur into a roll.
- **808 and 909:** the same kit row, two clearly different kicks, at about the same loudness.

## Snare (909)

| Render | What it plays |
|---|---|
| `01-defaults` | A snare a beat at the defaults (Tune 180 Hz, Tone 0.16 s, Snappy 0.5), with beat 1 of each bar accented. |
| `02-tune` | Tune 140, 160, 180, 200, 220, 240 and 260 Hz, two hits a bar. |
| `03-tone` | Tone 0.04, 0.08, 0.12, 0.16, 0.25 and 0.4 s: the wires' length. |
| `04-snappy` | Snappy 0, 0.25, 0.5, 0.75 and 1. |
| `05-level` | Level 0, -6, -12 and -24 dB. |
| `06-velocity` | A snare a beat, velocity 8 up to 127. |
| `07-repeats` | 8ths, 16ths, then 32nds, at the longest Tone so the wires pile up. |
| `08-flams` | A soft grace hit then an accent, 40, 20 and 10 ms apart. |
| `09-groove` | A 1-bar backbeat at 100 BPM, with ghost notes and a flam into beat 4, looped. Also the snare's golden WAV. |

What to listen for, against how you remember a 909:

- **Defaults:** a bright, snappy crack with a short tonal "body" under it, not a burst of hiss. The rattle should hold for a moment before it fades (the 909's hold), rather than fading from the first instant.
- **Tune:** the body moves in pitch and the rattle's colour moves with it; every step should still sound like the same drum.
- **Tone:** from a tight, dry snap at 0.04 s to a long, washy rattle at 0.4 s. The body shouldn't change.
- **Snappy:** 0 is the shell alone, a pitched "tok" like a tom with a short bump at the start; 1 is the wires alone; 0.5 should be the classic balance.
- **Velocity:** soft hits should sound like a lighter stick: quieter, with less rattle against the body. The accents should be noticeably crisper, not only louder.
- **Repeats and flams:** no clicks; the 32nds should blur into a roll, each hit a little different, not a machine gun.

## Clap (808)

| Render | What it plays |
|---|---|
| `01-defaults` | A clap a beat at the defaults (Tone 1 kHz, Decay 0.2 s), with beat 1 of each bar accented. |
| `02-tone` | Tone 700, 850, 1000, 1200, 1500 and 2000 Hz, two claps a bar. |
| `03-decay` | Decay 0.05, 0.1, 0.2, 0.3 and 0.4 s: the tail. |
| `04-level` | Level 0, -6, -12 and -24 dB. |
| `05-velocity` | A clap a beat, velocity 8 up to 127. |
| `06-repeats` | 8ths, 16ths, then 32nds, at the longest Decay. |
| `07-flams` | A soft grace clap then an accent, 40, 20 and 10 ms apart. |
| `08-groove` | Claps on 2 and 4, with a soft double before 4, at 100 BPM, looped. Also the clap's golden WAV. |
| `09-with-snare` | The snare on 2 and 4 for a bar, the clap for a bar, then both together on one kit's shared noise. |

What to listen for, against how you remember an 808:

- **Defaults:** several hands clapping almost together: a slightly smeared, "flammy" attack, then a short airy tail, like a clap in a small room. It shouldn't sound like a single burst of white noise, or like a whistle.
- **Tone:** 700 Hz is dark and hollow, 2 kHz bright and thin; somewhere around 1 kHz should be the classic clap.
- **Decay:** from a dry clap at 0.05 s to a roomy one at 0.4 s. The bursts at the start shouldn't change.
- **Velocity:** softer claps should be duller as well as quieter.
- **Repeats and flams:** no clicks. Fast repeats sound like applause rather than a buzz.
- **With the snare:** the third bar should sound like the two layered, with a slight hollow, "phasing" quality from the shared noise, as on a 909.

## Hats (808)

| Render | What it plays |
|---|---|
| `01-defaults` | Closed hats on 8ths at the defaults (Tune 205.3 Hz, Tone 7.1 kHz, closed Decay 0.05 s, open Decay 0.35 s), then open hats a beat apart, then 8ths with an open hat on each off-beat, cut off by the closed hat after it. |
| `02-tune` | Tune 102.65, 145, 205.3, 290 and 410.6 Hz: four closed hats and an open hat a bar. |
| `03-tone` | Tone 4, 5.5, 7.1, 9 and 12 kHz, the same pattern. |
| `04-closed-decay` | The closed hat's Decay 0.02, 0.05, 0.1 and 0.15 s, on 8ths. |
| `05-open-decay` | The open hat's Decay 0.09, 0.2, 0.35 and 0.6 s, two a bar. |
| `06-level` | Both Levels 0, -6, -12 and -24 dB. |
| `07-velocity` | A closed hat a beat, velocity 8 up to 127, then the same on the open hat. |
| `08-repeats` | Closed hats on 16ths, 32nds, then 64ths; then open hats on 16ths and 32nds at their longest Decay, so they pile up. |
| `09-flams` | A soft grace hit then an accent, 40, 20 and 10 ms apart, on the closed hat, then the open hat. |
| `10-choke` | An open hat left ringing at its longest Decay, then four cut off by a closed hat 240, 120, 60 and 30 ms after it. |
| `11-groove` | A 1-bar 808 hat pattern at 100 BPM, looped: 16ths with accents and ghost notes, and an open hat on the "and" of 2 and 4, cut off by the next closed hat. Also the hats' golden WAV. |
| `12-beat` | The groove with the kick and the clap, two bars at 110 BPM, looped. |

What to listen for, against how you remember an 808:

- **Defaults:** a crisp, metallic "tss" with a slightly clangy, ringing quality, not a burst of white noise. The closed hat is short and tight; the open hat rings, then the closed hat after it stops it dead, as on the 808.
- **Tune:** the metal's clang moves in pitch, lower and gongier at 102.65 Hz, thinner and higher at 410.6 Hz; it should still sound like the same hat.
- **Tone:** 4 kHz is darker, with more of the clang in it; 12 kHz is a thin, airy sizzle. Somewhere around 7 kHz should be the classic 808 hat.
- **Decay:** from a short tick at 0.02 s to a loose closed hat at 0.15 s, and on the open hat from a short "tsh" at 0.09 s to a long wash at 0.6 s.
- **Velocity:** softer hits should be duller and smoother as well as quieter; accents brighter and a little rougher, from the amplifier's clipping.
- **Repeats and flams:** no clicks; the 32nds and 64ths should shimmer, each hit a little different, not buzz like a machine gun.
- **Choke:** each closed hat should cut the open hat off cleanly, with no click, and the 30 ms one should sound like a single, slightly longer closed hat.

## Toms (808)

| Render | What it plays |
|---|---|
| `01-defaults` | The high tom a beat at its defaults (Tune 185 Hz, Decay 0.1 s), then the low tom (Tune 90 Hz, Decay 0.2 s), then a fill in 8ths down from the high tom to the low, with the first of each accented. |
| `02-low-tune` | The low tom's Tune 80, 85, 90, 95 and 100 Hz, two hits a bar. |
| `03-high-tune` | The high tom's Tune 165, 175, 185, 200 and 220 Hz, the same. |
| `04-low-decay` | The low tom's Decay 0.1, 0.2, 0.3, 0.45 and 0.6 s, one hit a bar. |
| `05-high-decay` | The high tom's Decay, the same. |
| `06-level` | Both Levels 0, -6, -12 and -24 dB: a high tom then a low tom a bar. |
| `07-velocity` | A high tom a beat, velocity 8 up to 127, then the same on the low tom. |
| `08-repeats` | 8ths, 16ths, then 32nds at the longest Decay, on the high tom, then the low tom. |
| `09-flams` | A soft grace hit then an accent, 40, 20 and 10 ms apart, on the high tom, then the low tom. |
| `10-groove` | A 1-bar tom pattern at 100 BPM, looped, with the low tom's Decay at 0.3 s. Also the toms' golden WAV. |
| `11-beat` | Kick, clap and closed hats for a bar, then a bar ending in a tom fill, at 110 BPM, looped. Every sound is at its default Level, with the toms 6 dB under the kick. |

What to listen for, against how you remember an 808:

- **Defaults:** a soft, round, pitched "tonk" with a short knock at the start, not a synth "boing" and not a sine beep. The pitch drops a little in the first tenth of a second, enough to give it a shape, not enough to hear as a sweep. Under it, a faint, low hiss that hangs on a moment after the note (the 808's "room"): you may only notice it when it isn't there.
- **Tune:** each step is a clearly different note with the same character. The low tom's range is narrow (80 to 100 Hz, the 808's own), so its steps are small.
- **Decay:** from a short, dead "tok" at 0.1 s to a long ring at 0.6 s, longer than the 808's toms, which have no Decay knob.
- **Velocity:** softer hits should be duller and bend less, as well as quieter; accents knock harder and have a little more of the hiss.
- **Repeats and flams:** no clicks. The 32nds should roll, each hit blending into the last ring, not stutter like a machine gun. A flam at 10 ms should sound like one fat hit.
- **The beat:** the fill should sit under the kick and clap like an 808's toms, not poke out as a different machine.

## Cymbal (808)

| Render | What it plays |
|---|---|
| `01-defaults` | An accent at the defaults (Tone 0.5, Decay 0.8 s), a bar later an ordinary hit, then a bar of quarter notes with a crash on the one. |
| `02-tune` | Tune 102.65, 145, 205.3, 290 and 410.6 Hz: the closed hat's Tune, which the cymbal shares, as the metal is one bank on the 808. Two hits a bar. |
| `03-tone` | Tone 0, 0.25, 0.5, 0.75 and 1, the same. |
| `04-decay` | Decay 0.35, 0.5, 0.8, 1.0 and 1.2 s, one hit a bar at 80 BPM. |
| `05-level` | Level 0, -6, -12 and -24 dB. |
| `06-velocity` | A cymbal every two beats, velocity 8 up to 127. |
| `07-repeats` | 8ths, 16ths, then 32nds, at the longest Decay so they pile up. |
| `08-flams` | A soft grace hit then an accent, 40, 20 and 10 ms apart. |
| `09-groove` | A 1-bar cymbal pattern at 100 BPM, looped: a crash, quarter notes, a soft off-beat and a ghost. Also the cymbal's golden WAV. |
| `10-beat` | Kick, clap, closed hats and an open hat, with the cymbal on the one of each bar and a last 8th, at 110 BPM, looped. Every sound is at its default Level, with the cymbal 11 dB under the kick. |

What to listen for, against how you remember an 808 cymbal:

- **Defaults:** a bright "tssshhh" with a sharp start: a short, fizzy sizzle on top that's gone in a fraction of a second, over a lower, metallic wash that rings on for about a second. It should sound like the same family as the hats (it's the same six oscillators), but bigger and longer, and more like a gong-ish metal plate than a real cymbal. If it's harsh, glassy or "digital", or if the wash sounds like a steady tone rather than a shimmer, say so.
- **Tune:** the whole metal moves, like the hats; the bands stay put, so it's more a change of colour than of pitch.
- **Tone:** from a dull, dark wash at 0 to a fizzy one at 1. Mostly the sizzle at the start changes; the tail changes less.
- **Decay:** the wash from a short crash at 0.35 s to a long ring at 1.2 s; the sizzle at the start stays short at every setting.
- **Velocity:** softer hits should be darker and smoother as well as quieter; accents brighter and a little rougher.
- **Repeats and flams:** no clicks. The 32nds should wash together into one roar, not buzz like a machine gun. A flam at 10 ms should sound like one thick hit.
- **The beat:** it should sit with the hats as one machine.

## The kit

The whole kit at its defaults, in `kit/`, for judging its balance (RFC-006, manual check 5). Each sound's default Level comes from measuring its loudness: the K-weighted level, as a loudness meter measures it, over the 150 ms after a full-accent hit. Against the kick, the snare and clap sit 3 dB under it, the toms 6 dB, and the hats and cymbal 11 dB. Everything hitting at once at full accent peaks at -2.4 dBFS with every fader at 0 dB (-3.5 dBFS with the 909 kick).

The demo beat itself is `examples/demo-beat.json`, and it's also the kit's golden WAV:

```bash
cargo run --release -p uta-cli -- render target/listen/demo-beat.wav --commands examples/demo-beat.json
```

| Render | What it plays |
|---|---|
| `01-each-sound` | Each sound in turn, a beat apart, at 100 BPM: kick, snare, clap, closed hat, open hat, low tom, high tom, cymbal. Twice at velocity 100, then twice at 127. |
| `02-each-sound-909` | The same with the 909 kick. |
| `03-demo-beat-909` | The demo beat with the 909 kick. |

What to listen for:

- **Each sound:** nothing should jump out or disappear. The kick leads, the snare and clap sit just under it, the toms a step further back, and the hats and cymbal well back but clear. At velocity 100 the kick sits about 3 dB further forward than at 127, as an accent changes it less than the others.
- **The demo beat:** it should sound like a drum machine playing a beat, not a test. Listen for the hats against the cymbal: on UTA-51 they sounded much brighter when the cymbal played with them, when the cymbal was 12 dB louder against the kick than it is now.
- **The 909 kick:** about as loud as the 808's, so switching model doesn't change the balance.

## Synth

The synth's oscillators, sub, noise, mixer and filter, pulse width and sync, the LFO, Mono with Legato and Glide, and the mod slots (RFC-008), in `synth/`. Each list is one synth track per setting, each playing the same phrase in its own bar at 120 BPM: a held A minor chord for two beats, then four eighths of bass line. Settings a list doesn't name are at their defaults, so oscillator 2, the sub and the noise are off unless it turns them up.

| Render | What it plays |
|---|---|
| `01-defaults` | The phrase twice at the defaults: today's saw, as before this ticket. Everything else is compared with it. |
| `02-detune` | Oscillator 2, a second saw at full level, Pitch 0, 0.05, 0.1, 0.2 and 0.35 semitones (5 to 35 cents). |
| `03-intervals` | Oscillator 2 at 0.7: Pitch +7 (a fifth), +12 and -12, then Octave +1 and -1, each 10 cents sharp. |
| `04-osc2-wave` | Oscillator 2 at full level, 10 cents sharp, as a sine, triangle, saw and pulse, under oscillator 1's saw. |
| `05-octaves` | Oscillator 1 alone at Octave -2, -1, 0, 1 and 2. |
| `06-sub` | A bassline in eighths (cutoff 1.5 kHz): no sub, the sub an octave down at 0.5 and 1, two octaves down at 0.5 and 1, then the sub alone. |
| `07-noise` | The chord held for a bar as a soft pad (attack 0.2 s, cutoff 3 kHz), with Noise at 0, 0.05, 0.15, 0.3 and 0.6, then the noise alone. |
| `08-retrigger` | A pluck in sixteenths on one note, with oscillator 2 detuned and the sub: Osc retrigger on, off, on, off. |
| `09-mix` | Oscillator 1 alone, then with oscillator 2 detuned, the sub and some noise brought in one at a time, then alone again. |
| `10-filter-modes` | The phrase at cutoff 800 Hz, resonance 0.3: low-pass, band-pass and high-pass; then band-pass at 2 kHz with resonance 0.7, and high-pass at 200 Hz. |
| `11-plucks` | A bassline in sixteenths, cutoff 150 Hz, the filter envelope decaying to nothing in 0.2 s: Env amount 0, 3, 5 and 7 octaves; then at 5, a filter decay of 0.06 s and 0.6 s, and a filter sustain of 0.4. |
| `12-swells` | A held chord: cutoff 3 kHz with no envelope; the same with Env amount -4 over 1.2 s; then cutoff 300 Hz with Env amount +4, a 1 s filter attack and a sustain of 0.5. |
| `13-key` | A minor in eighths across four octaves, cutoff 400 Hz: Key 0, 0.5 and 1. |
| `14-resonant-pluck` | The sixteenths at cutoff 180 Hz, resonance 0.85, Env amount 4.5 over 0.15 s; then the same on a pulse with the sub and Key 0.5. |
| `15-width` | Oscillator 1 as a pulse at Width 50% (a square), 35%, 20%, 10% and 5%. |
| `16-sync` | Oscillator 2 alone, a saw synced to oscillator 1, at Pitch 0, +3, +7 and +10, then an octave up at +3, +7 and +11. |
| `17-sync-waves` | Oscillator 2 alone, synced an octave and a fifth up, as a sine, triangle, saw and pulse. |
| `18-sync-lead` | A lead line (cutoff 5 kHz, a little resonance) with oscillator 2 an octave and a fifth over oscillator 1: free, synced, then synced at +3 and +10 instead of +7. |
| `19-vibrato` | The phrase on a triangle: no vibrato, then Vibrato 0.15 semitones at 5 Hz, 0.3 at 5.5 Hz, 1 at 6 Hz and 2 at 2 Hz. |
| `20-lfo-shapes` | A chord held for the bar (cutoff 700 Hz, resonance 0.4) with the filter's LFO amount at 2.5 octaves, the LFO at 3 Hz: sine, triangle, saw, square and sample-and-hold. |
| `21-synced` | The same chord with the LFO synced at 120 BPM: a sine at 1/4, a saw at 1/8, a square at 1/8 dotted and at 1/4 triplet, and sample-and-hold at 1/16. |
| `22-pwm` | A chord held on a pulse (attack 0.15 s): still, then the LFO's Width at 0.3 at 0.5 Hz and 1.5 Hz, 0.45 at 4 Hz, and a string pad: 0.3 at 0.5 Hz with oscillator 2, a second pulse 10 cents sharp. |
| `23-lfo-retrigger` | Four notes entering an eighth apart and held to the bar's end: vibrato at 3 Hz with Retrigger off and on, then a sample-and-hold on the filter at 6 Hz, off and on. |
| `24-mono` | A plucky bass (detuned saws, the sub, a filter envelope) playing a line whose notes each overlap the next by a 16th: in Poly, in Mono, then in Mono with Legato. Glide is off. |
| `25-glide` | The same line in Mono with Legato, at Glide 1 ms (off), 30 ms, 80 ms, 0.2 s and 0.5 s. |
| `26-return` | A lead (a pulse and a detuned saw) holding A3 through the bar while E4, A4, C5 and E5 are stabbed over it and let go: in Poly, in Mono, then in Mono with Legato and a 60 ms Glide. |
| `27-mono-bassline` | Two bars of an acid-style line in 16ths (cutoff 250 Hz, resonance 0.65, a snappy filter envelope), where three notes run into the next: in Poly, then in Mono with Legato and a 70 ms Glide. |
| `28-velocity-cutoff` | Eighths played from soft (30) to hard (127) and back, a resonant pluck at cutoff 600 Hz: no slot, then Velocity to Cutoff at +30% and +50%, then at -30% from 4 kHz. |
| `29-sync-sweep` | A lead on oscillator 2 alone, synced, a note a beat: no slot, then the filter envelope (0.35 s decay) on oscillator 2's pitch at +25%, +50% and +100%, then +50% without Sync. |
| `30-tremolo` | A soft, detuned chord held for the bar: no slot, then the LFO on the volume at 30% and 60% at 5 Hz, at 100% as a square synced to 1/8, and at -100% as a saw synced to 1/16. |
| `31-more-routes` | One route each: Key to Cutoff +50% (an octave for each octave from middle C) on an A in four octaves; Velocity to Volume +50% on the soft-to-hard eighths; the LFO on oscillator 1's width at 30% (0.7 Hz) on a pulse chord; the amp envelope on the resonance +70% on the phrase; Velocity to LFO rate +50% under a 3 Hz vibrato on the eighths. |

What to listen for:

- **Defaults:** the same bright saw as before. If anything sounds different from the synth you know, say so: it shouldn't.
- **Detune:** 0 sounds like one saw, about 3 dB louder, as the two are in step and add up. From 0.05 a slow beat or swirl, getting faster and thicker; around 0.1 to 0.2 should be the classic fat, chorused saw, and 0.35 starts to sound out of tune.
- **Intervals:** +7 an organ-like fifth; ±12 an octave above or below, adding shimmer or weight. The octave ones should also beat slowly, from their 10 cents.
- **Oscillator 2's wave:** the sine and triangle add body under the saw without much brightness; the saw thickens it; the pulse adds a hollow, reedy colour.
- **Octaves:** each a whole octave apart, the same sound otherwise.
- **Sub:** an octave down adds weight you can feel without changing the bass's tone; two octaves down is a deeper rumble, which small speakers may barely play. The sub alone is a pure, round square.
- **Noise:** 0.05 is a breath of air on the pad, 0.15 to 0.3 a clear hiss, 0.6 more noise than tone. The hiss should be smooth and even, not crackly, and should fade in and out with each note.
- **Retrigger:** on, every pluck starts the same, tight and identical, like a machine; off, each pluck is a little different, livelier, while staying in time and in tune.
- **Mix:** bringing each source in changes the colour, not much the loudness: the mixer keeps it about as loud as one oscillator (within about 2 dB, as measured). Nothing should jump out.
- **Filter modes:** low-pass warm and rounded; band-pass thin and nasal, like a telephone; high-pass light and airy, with the bass line almost gone. The resonant band-pass should sing a little at 2 kHz, and the 200 Hz high-pass take only the weight away.
- **Plucks:** Env 0 is a dull thud. From 3 each note starts with a bright "pew" that closes down fast, getting brighter and more snappy at 5 and 7, which should sound like a classic synth pluck. The short decay is a tight click of brightness, the long one a slow "wah" on each note, and sustain 0.4 keeps each note brighter after the pluck.
- **Swells:** with Env -4 each chord starts dark and opens up to the plain sound over a second; with +4 and the slow attack it brightens slowly into focus, then settles. Neither should step or zip as it moves.
- **Key:** at 0 the low notes are full and the high ones dull and muffled; at 1 every octave sounds as bright as the others; 0.5 is in between.
- **Resonant pluck:** a squelchy, rubbery bass, each note with a ringing "yow" as the filter closes. No clicks or crackle at the start of the notes, even with the resonance this high.
- **Width:** 50% is the hollow, woody square the pulse always was. Narrower is thinner and more nasal, towards a reedy buzz at 5%. It also gets quieter, about 7 dB by 5%, as a narrow pulse has less in it; that's how a pulse behaves on an analog synth too. Every step should stay centred and clean, with no new rumble or thump.
- **Sync:** every bar plays the same notes at the same pitch: only the tone changes. As oscillator 2's Pitch rises the sound gets brighter, more nasal and vocal, with a resonant "wah" that moves up; that's the sync sound. Listen on the high chord notes for anything gritty, glassy or out of tune whistling under the tone (aliasing). Some edge is the character of sync; a whine that doesn't belong to the note isn't.
- **Sync's waves:** the sine and triangle synced are softer and rounder than the saw and pulse, but still clearly a different tone from a plain sine or triangle. None should click or buzz on every cycle.
- **Sync lead:** the free bar beats and sounds like two notes, an octave and a fifth apart; synced, it's one note with a bright, tearing edge. +3 is darker and +10 brighter, each the same note.
- **Vibrato:** 0.15 is a gentle, singer's vibrato you notice more as warmth than as wobble; 0.3 clearly sings; 1 is a wide, theatrical wobble, and 2 at 2 Hz a slow siren. The notes should stay centred on their pitch, no flatter or sharper on average, and the chord should wobble together.
- **LFO shapes:** the sine and triangle a smooth "wah-wah" back and forth (the triangle a little more even); the saw a repeated rising "wow" that drops back; the square a two-step flip between dark and bright, a trill of tone; sample-and-hold a random, bubbling, robot-like stepping, each note of the chord on its own path. The square and sample-and-hold steps should be crisp but never click or tick.
- **Synced:** every movement lands on the beat: the sine swells once a beat, the saw twice, the dotted square weaves against the beat, the triplet square gallops in threes, and the sample-and-hold steps on every sixteenth. Clap along: the steps should fall exactly on the grid.
- **PWM:** the still pulse is hollow and plain. With the Width moving it shimmers, a gentle chorus at 0.5 Hz and a faster, rolling movement at 1.5 Hz; 0.45 at 4 Hz is a strong, almost seasick warble. The string pad should sound like an old string machine: lush, wide and moving, with no beating that sounds out of tune.
- **LFO retrigger:** with vibrato, off the four notes wobble together as one; on, each starts its wobble as it enters, so they wobble against each other. With the sample-and-hold, off the notes step at the same moments; on, each steps in its own time from when it entered.
- **Mono:** in Poly each note's tail rings on under the next, so the overlaps smear into two-note chords. In Mono there's only ever one note: the line is clean and tight, and each note still plucks, as the envelopes start again from where they are. With Legato the line runs together into one phrase: only the first note plucks, and the rest change pitch under one long, closing filter. None of it should click as the notes change.
- **Glide:** at 1 ms the pitch steps, as in `24-mono`. From 30 ms each note slides into the next, a slur at first; by 0.2 s you hear every slide as a swoop, and at 0.5 s the line barely arrives at a note before it leaves, a slow, drunken lead. Every slide takes the same time whatever the interval, and should land exactly in tune.
- **Return:** in Poly the stabs sit on top of the held A as chords. In Mono each stab replaces the A, and letting go drops straight back to it, without a gap and without the A attacking again from silence. With Legato and Glide it slides up to each stab and back down, smoothly, like a trill played on one string.
- **Velocity to cutoff:** with no slot every eighth has the same tone, only louder or softer. With the slot up, soft notes are darker and hard ones brighter, the way a played synth bass responds; +50% is a wide range, from muffled to biting. At -30% it's the other way round: the hardest notes are the darkest.
- **Sync sweep:** with no slot each note is one steady, nasal tone. With the slot, each note starts with a bright tearing "yow" that sweeps down as the filter envelope closes, settling on the plain synced tone: subtle at +25%, the classic sync lead at +50%, a wild, screaming two-octave sweep at +100%. Every note stays the same pitch; only the tone sweeps. Without Sync the same slot is a pitch drop instead, a falling "pew".
- **Tremolo:** 30% at 5 Hz is a gentle, vintage amp tremolo; 60% a deep, throbbing one. The synced square at 100% chops the chord into eighths, silent between, like a gate, and should land on the beat; the saw at -100% swells into each sixteenth and cuts off. None of the steps should click, and the 100% gate shouldn't be louder overall than the plain chord by more than a little.
- **More routes:** with Key on the cutoff the As in every octave sound about equally bright, as the filter's own Key at full makes them, rather than the high ones dull. Velocity on the volume makes the hard eighths stand out much more against the soft ones. The width moving under the pulse chord is a slow shimmer, as the LFO's own Width gives. The resonance follows the amp envelope: each chord swells into a ringing, vocal "wah" as it attacks, and calms as it decays. The vibrato is faster on the hard notes and slower on the soft ones.
- **Mono bassline:** in Poly the long notes smear into the next. In Mono the three notes that run into the next slide up or down to it without re-plucking: the squelchy, rubbery slide of an acid bass. The notes with a gap before them pluck afresh and don't slide, so it should be clear by ear which notes slide and which don't.

## Factory sounds

The synth's eight factory sounds (RFC-008), in `factory/`, each as the Sound menu chooses it, playing a 2-bar loop at 120 BPM of the part it's for. The lists are written from the sounds' settings by `tests/factory.rs` (`UTA_GOLDEN=1` rewrites them), so they always play the sounds as they are. Each sound's brief, the targets it was measured against and what making them taught are in [the voicing guide](../../docs/sound-design/synth.md).

| Render | What it plays |
|---|---|
| `01-bass` | A synth-pop bassline around C2: roots, octaves and fifths in 16ths and 8ths. |
| `02-glide-bass` | A line around C2 whose overlapping notes slide and whose fresh notes pluck. Also a golden WAV. |
| `03-pluck` | 8th-note arpeggios over Cmaj7, then Am7, around middle C. |
| `04-pad` | Cmaj7, then Am7, a bar each. |
| `05-strings` | The Pad's chords, so the two compare. Also a golden WAV. |
| `06-lead` | A tune around G4: a held note for the vibrato, overlapping notes that glide, fresh notes after rests. |
| `07-sync-lead` | A line around C4 with a gap before most notes, so each tears in afresh. Also a golden WAV. |
| `08-bell` | A sparse tune around C5 with room to ring. |

What to listen for, against each brief:

- **Bass:** round and solid, a Juno or Moog bass: warm rather than buzzy, a little bite at the start of each note that settles while it's held. It should be felt, and still be a clear note on small speakers.
- **Glide bass:** an SH-101 or 303 line: the overlapping notes slide in about 80 ms without re-attacking, rubbery and squelchy; the notes after a gap snap open afresh. The resonance should squelch without whistling.
- **Pluck:** a round, short Juno pluck: each note bright for its first tenth of a second and closing fast, never harsh, gone within about half a second. It should sit under a chord rather than poke out.
- **Pad:** warm and slow: the chord swells in over about half a second, shimmers from the two detuned saws, opens a little after the start and drifts gently, with a faint breath of noise, and fades over about a second. Soft enough to sit behind everything.
- **PWM strings:** a string machine: brighter and thinner than the Pad, a bowed start of about a quarter of a second, and a slow breathing in the tone, as if several players were on each note.
- **Lead:** a singing Minimoog-style lead: present and clear, with a gentle vibrato on the held notes and short slides between overlapping ones. It should cut through without being shrill.
- **Sync lead:** each fresh note starts with a bright, tearing "yow" that sweeps down over about a third of a second into a steady, nasal tone, at the note's pitch throughout.
- **Bell:** glassy, like an FM tine or a music box: a brief inharmonic shimmer at the strike that fades fast, leaving a pure tone ringing for a couple of seconds. Clear, not clangy.
- **All of them:** about equally loud. The pads play 4-note chords here and measure 2 to 3 dB louder than the single-note sounds; say if they sound it.

## Delay

The Delay's tape loop (RFC-007), in `delay/`. Each list is one synth track per setting, each with its own Delay and playing in its own two bars at 120 BPM: a plucked A minor stab, then the echoes. The track is at -6 dB and the master at 0 dB. Settings a list doesn't name are at the Delay's defaults: synced to a dotted eighth, 40% Feedback, Low cut 100 Hz, High cut 8 kHz, Drive 20% and Wobble 10%.

| Render | What it plays |
|---|---|
| `01-defaults` | A two-bar phrase, twice, through the Delay at its defaults and a new Delay's Mix of 30%. |
| `02-modes` | The stab through Stereo, then Ping-pong, at 50% Feedback and Mix 50%. |
| `03-high-cut` | High cut 20, 8, 4, 2, 1 and 0.5 kHz, at 60% Feedback, with no Drive, no Wobble and the Low cut wide open. |
| `04-low-cut` | Low cut 20, 100, 300, 800 and 2000 Hz, the same, with the High cut wide open. |
| `05-drive` | Drive 0, 25, 50, 75 and 100% on an accented stab, at 60% Feedback, with no Wobble. |
| `06-wobble` | Wobble 0, 25, 50 and 100% on a held chord, echoing on the quarter at 50% Feedback. |
| `07-feedback-100` | One stab at 100% Feedback. It barely fades, so the render runs to its 30 s limit. |
| `08-feedback-110` | One stab at 110%. The echoes swell, then the loop's level control holds them. Also to the 30 s limit. |

A Delay on a synth track and one on the master is `examples/demo-delay.json`, which is also the Delay's golden WAV:

```bash
cargo run --release -p uta-cli -- render target/listen/demo-delay.wav --commands examples/demo-delay.json
```

What to listen for, against Ableton's Echo on the same sound:

- **Defaults:** echoes on the dotted eighth, weaving between the stabs, each softer and a little darker than the last, like tape rather than a clean digital copy. They should sit behind the sound, not on top of it.
- **Modes:** Stereo echoes in the middle, where the stab is. Ping-pong bounces them left, right, left, though the synth is mono, and they should sound about as loud as Stereo's.
- **High cut:** at 20 kHz the echoes stay bright; at each step down they fade into murk sooner, until at 500 Hz the second echo is already a dull thud.
- **Low cut:** each step thins the echoes, until at 2 kHz they're a tinny ghost of the stab. 100 Hz, the default, should barely change the sound, only keep the echoes off the bass.
- **Drive:** 0 is clean. A little should round the echoes off and warm them; at full they're squashed and crunchy, and quieter than the stab, since the tape can't take them. Nothing should crackle or sound digital.
- **Wobble:** at 0 the echoes are steady copies of the chord. A quarter should make them feel analogue without sounding out of tune; at full they sway and waver like a worn tape. The rates, how far they sway and how much is wow or flutter are first guesses, to be tuned in UTA-77.
- **Feedback 100%:** the echoes go on and on, getting darker and thinner each time round (the cuts are still in the loop), but not quieter for a long while.
- **Feedback 110%:** they swell for a few seconds, then hold about as loud as the first echoes, still separate echoes of the stab, slowly getting darker and rounder: never an ever louder blast, and never a wall of thuds. If it's still painful, the top of the range should come down (RFC-007 risk).
- **No clicks anywhere,** and no zipper noise as echoes go round.
