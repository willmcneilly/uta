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

The synth's oscillators, sub, noise and mixer, pulse width and sync (RFC-008), in `synth/`. Each list is one synth track per setting, each playing the same phrase in its own bar at 120 BPM: a held A minor chord for two beats, then four eighths of bass line. Settings a list doesn't name are at their defaults, so oscillator 2, the sub and the noise are off unless it turns them up.

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
| `10-width` | Oscillator 1 as a pulse at Width 50% (a square), 35%, 20%, 10% and 5%. |
| `11-sync` | Oscillator 2 alone, a saw synced to oscillator 1, at Pitch 0, +3, +7 and +10, then an octave up at +3, +7 and +11. |
| `12-sync-waves` | Oscillator 2 alone, synced an octave and a fifth up, as a sine, triangle, saw and pulse. |
| `13-sync-lead` | A lead line (cutoff 5 kHz, a little resonance) with oscillator 2 an octave and a fifth over oscillator 1: free, synced, then synced at +3 and +10 instead of +7. |

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
- **Width:** 50% is the hollow, woody square the pulse always was. Narrower is thinner and more nasal, towards a reedy buzz at 5%. It also gets quieter, about 7 dB by 5%, as a narrow pulse has less in it; that's how a pulse behaves on an analog synth too. Every step should stay centred and clean, with no new rumble or thump.
- **Sync:** every bar plays the same notes at the same pitch: only the tone changes. As oscillator 2's Pitch rises the sound gets brighter, more nasal and vocal, with a resonant "wah" that moves up; that's the sync sound. Listen on the high chord notes for anything gritty, glassy or out of tune whistling under the tone (aliasing). Some edge is the character of sync; a whine that doesn't belong to the note isn't.
- **Sync's waves:** the sine and triangle synced are softer and rounder than the saw and pulse, but still clearly a different tone from a plain sine or triangle. None should click or buzz on every cycle.
- **Sync lead:** the free bar beats and sounds like two notes, an octave and a fifth apart; synced, it's one note with a bright, tearing edge. +3 is darker and +10 brighter, each the same note.
