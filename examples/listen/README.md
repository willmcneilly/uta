# Listening renders

Short command lists for judging each drum sound by ear, one folder per sound. Tests can show a sound has the right pitch and decay, but not that it's satisfying, so every sound ticket ends with these (RFC-006, "How we'll verify it", manual check 1).

Render a folder to WAVs with one command, from the repository root (`target/` is ignored by git):

```bash
cargo run --release -p uta-cli -- render-all examples/listen/kick target/listen/kick
```

The other sounds are the same with their folder's name in place of `kick`.

Each list sets the master to 0 dB, so a full accent peaks at -6 dBFS and an ordinary hit (velocity 100) about 8 dB under it. A sweep is one drum track per setting, each playing its own bar, so you hear the steps in order.

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
