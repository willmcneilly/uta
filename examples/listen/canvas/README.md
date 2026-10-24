# The canvas

Four short parts to tune sounds and effects against, so every listening verdict is on the same material and nobody writes test chords and lines by hand. A listening session names one part with `canvas = "<part>"`, and gives each candidate as commands added onto it: a sound for the part's track, or an effect on its chain or the master's (`scripts/listen --help` has the format, with an example of each). Nothing here is tied to one effect or sound.

Each part is a song file (the format is in [`examples/demos/README.md`](../../demos/README.md)) at 120 BPM with the loop off, the master at 0 dB, and one track named `Part`, so a candidate's commands name it as `$track:Part` on any part. Each plays once, from the top of bar 1, so the listening loop starts with the sound and ends half a second after the last candidate's tail is below −60 dB.

| Part | What it plays | What it's for | Dry, it's below −60 dB after |
|---|---|---|---|
| `snare` | One 909 snare hit at its defaults (velocity 110). | A short, bright transient (most of its energy around 5 kHz, down 40 dB in 0.23 s): a room's first reflections, a delay's echoes, and whether an effect smears or clicks on an attack. | 0.42 s |
| `chord` | A C major chord (C3 G3 C4 E4), struck and let go after a beat, as near a piano as the synth gets: a triangle with a quiet saw an octave up, through a low-pass that snaps open on the strike (brightness 591 Hz) and closes as it rings away (down 40 dB in 0.76 s). | A sound's attack and release on a chord, and what an effect does once a chord stops: the tail you hear on its own. | 1.07 s |
| `swell` | One sung-like A3 that fades in over 0.8 s, holds with a slight 5 Hz vibrato, and lets go: two saws through a band-pass near a vowel's first formant, with a breath of noise. | A slow attack and a held pitch that moves: how an effect blooms behind a voice, and whether modulation beats against vibrato. | 2.03 s |
| `pad` | A Cmaj7 (C3 G3 B3 E4) held for a bar on the factory Pad. | A sustained, moving sound: how an effect thickens and widens it, and whether its tail muddies a held chord. | 2.96 s |

The −60 dB times are measured from renders with `uta render --song` (each render rings out further than that, to silence). With nothing on it, the snare loops every 0.92 s: its tail, then the half-second pause. A session that wants a longer gap between hits sets its own `loop`, such as `loop = [0, 2]`.

Render one to hear it on its own:

```bash
cargo run --release -p uta-cli -- render target/listen/canvas-chord.wav --song examples/listen/canvas/chord.toml
```
