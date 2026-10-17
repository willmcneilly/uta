# RFC-009: Hearing tools

**Status:** Accepted (2026-10-17) · **Author:** Will · **Notion:** https://app.notion.com/p/Hearing-tools-3fc3af969b6f818e8c37d5ed7d4c0fdc

## Summary

Give Claude better ways to judge a sound before you hear it, so its first attempt is closer to right. There are three parts:

- **`uta analyse`.** It measures any render with the same code the sound tests use, and draws images with the numbers printed on them.
- **Listening sessions.** Claude asks one question at a time, with the candidates, the measurements and its own prediction written down first. You answer in a local listening page (`scripts/listen`) instead of a folder of WAVs. Your verdicts are kept next to the numbers, so we learn which measurements predict what you hear, and the dependable ones become tests.
- **Demo songs.** There are lo-fi, house and techno songs, and the start of an ambient one, in the genres you want to make. Each is written in a readable file that compiles to commands, loads from the app's menu, and shows off every instrument and effect we've built.

This RFC comes before the Reverb RFC, which will add its own measurements to the same tools.

## Motivation

- **Claude tunes sounds half blind.** Today the measurements exist only inside the tests (`crates/uta-engine/tests/common`, about twenty of them: pitch, brightness, decay, clicks, loudness). A render made for listening can't be measured without writing a throwaway test. Python here has no numpy and can't read Uta's float WAVs, and agents rediscovered that in eight tickets (UTA-25 to UTA-56). The UTA-72 commit talks about "the spectrograms", but there's no spectrogram tool in the repo. So Claude's first try at a sound is a guess, and you're the measuring instrument.
- **Listening is a chore for you.** You get a folder of WAVs and instructions in the PR, and you flick between Claude and Finder to work out what you're comparing. The blind A/Bs (the sync test in UTA-72, the hats' oversampling) worked, but each was built by hand as an ignored test with a key file.
- **Your verdicts are lost.** They're recorded as prose in PRs and Notion. Nothing connects "too boxy" to the numbers of the render you heard, so the next sound starts from scratch.
- **Reverb is next, and it's the hardest to judge.** A reverb can hit its decay time and still sound metallic, grainy or fluttery. Reverb has well-studied measurements for exactly those qualities (see "Reverb, next"), but only if there's somewhere to put them.
- **No real song to try things in.** Every feature is tried from an empty project or a test loop. `examples/demo-song.json` is from UTA-19: four tracks, no drums, no effects, an old command format.

What it unlocks: better first attempts on Reverb and every effect after it, faster factory-sound work (UTA-82/83 can use `uta analyse` if it lands first), and the groundwork for the AI sound designer, which needs exactly this loop of brief → measure → verdict.

## Proposal

### 1. Shared measurements, in one place

The helpers in `tests/common` move into a new crate, **`uta-analysis`**, with no audio thread, no Tauri and no project code. It only reads samples. The sound tests and the CLI both call it, so the numbers Claude reads from a render are the numbers the tests check (open question 1). The tests keep passing unchanged, which is how we know the move broke nothing.

Its FFT is replaced with `rustfft`/`realfft` (MIT, widely used, released 2025). The home-made one only handles power-of-two lengths, and that matters once windows are sized in milliseconds. Each measurement is a few dozen lines on top. The Rust audio-feature libraries (dasp-rs, aus, estratto) are too young to depend on.

What it measures to begin with. The research shows **brightness and attack time** are the two qualities that timbre studies keep finding matter most, so they lead.

| Measurement | What it tells Claude | Have it? |
|---|---|---|
| Peak, RMS, crest factor | How loud, and how punchy or squashed | partly |
| Loudness (BS.1770: momentary, short-term, integrated) | How loud it *sounds*, for balancing and A/B matching | partly (K-weighting exists) |
| Brightness (spectral centroid), share above a frequency | Bright or dark | yes |
| Attack (10–90% rise) and decay (time to fall N dB) | Plucked or swelling, short or long | decay only |
| Pitch, and pitch over time | In tune; how a kick's pitch drops | yes |
| Noisiness (spectral flatness) | Tonal or noise-like | no |
| Inharmonicity | Metallic or bell-like (toms, cymbals, sync) | no |
| Clicks (largest step and where) | Clicks at note starts, ends and setting changes | yes |
| Stereo correlation and width | Mono-safe, how wide | one test's own helper |
| Aliasing (energy between the harmonics of a known pitch) | Digital harshness on high notes | in one test, by hand |

Room for more is part of the design: a measurement is a function plus an entry in the report. Reverb's will be the first added.

### 2. `uta analyse`

```
uta analyse <wav>                      # print the report
uta analyse <wav> --against <wav>      # the same, side by side, with the differences
uta analyse <wav> --png <dir>          # also draw the images
uta analyse <wav> --json               # the report as data, for sessions and scripts
uta analyse <wav> --pitch C3           # tell it the intended note (for tuning and aliasing)
```

The report is plain text, one line per measurement, with units. Claude reads it before deciding anything. `--against` is how Claude compares before and after a change. Later it's how a render is measured against a reference recording, with no change to the tool.

**The images are a sanity check, not the evidence.** The research is clear that today's models read spectrograms only roughly. One study found a model matched human experts at sorting everyday sounds, but only with careful image design. Another found models rarely beat chance at reading speech from them. So every image has its key numbers printed on it, uses fixed scales so two images compare at a glance, and has labelled axes. The judgement comes from the numbers. The images catch what numbers miss: a click in the wrong place, a line of ringing, aliasing folding back down.

The first set of images:
- **Spectrogram,** with a log-frequency axis and note names, and a fixed dB scale (0 to −90). There are two of them: a short window (about 3 ms) for drum hits and a long one (about 50 ms) for tones and tails.
- **Level over time in dB,** with the measured attack and decay marked.
- **Waveform close-ups** with each sample as a dot, at every place a click was found.
- **Harmonics on a grid,** when the pitch is known. Aliasing shows as bars between the lines.
- **A difference spectrogram,** with `--against`.

They're drawn as SVG and turned into PNG with `resvg` (MIT/Apache, maintained, released 2026-08), so the text is real text and the listening page can show the same picture (open question 2).

### 3. Listening sessions

A **listening session** is one question for you, prepared by Claude:

```
~/.uta/listening/2026-10-20-snare-decay/
  session.toml        the question, the candidates, Claude's prediction
  a.json  b.json …    each candidate's command list
  verdict.toml        your answer (written by the page)
```

`session.toml` holds:
- **The question,** in plain words: "Which snare decay sounds most like an 808 snare?"
- **The candidates,** each a command list, so anyone can render them again. The WAVs aren't committed.
- **The kind of question:**
  - *pick one*, a blind A/B or A/B/C;
  - *rate each*;
  - *listen and describe*: one render, "what's wrong with this?"
- **Claude's prediction, written before you listen:** which it expects you to pick and why, with the measurements it's relying on. This is the part that improves first attempts. Writing a prediction forces Claude to commit to what it thinks the numbers mean, and your verdict says whether it was right.

`scripts/listen <session>` starts a local page, the way Crit does, and Claude waits for it to finish:
- It renders any candidate that isn't rendered yet.
- **It's fair by default:**
  - The candidates are matched in loudness. Half a decibel louder already sounds "better".
  - They're lined up in time.
  - They're labelled A, B, C in a shuffled order that's kept off the page until you finish.
  - They loop, and switch instantly on the same beat. This is the trustworthy A/B setup from listening-test practice (webMUSHRA, ABX tools).
- You pick or rate, add a note in your own words, and press Finish. Then the page shows which was which, what Claude predicted, and the images.
- Several questions can queue in one sitting.
- It writes `verdict.toml`: your choice, your note, the gain it applied to each, and the time.

**The sessions and verdicts are kept privately, outside the repo** (open question 3), in `~/.uta/listening/`. It can be moved with `UTA_LISTENING_DIR`, for example into a private git repo of your own so it's backed up. Every worktree and session reads the same folder, so nothing is lost when a worktree is tidied. Two things then happen to them over time:
- **They feed the voicing guide.** When a few verdicts agree ("you always pick the snare with the darker tail"), that becomes a line in `docs/sound-design/`.
- **They become tests.** When a measurement keeps predicting your verdict (decay between 180 and 250 ms always reads as "808"), it becomes a sound test with that range. That's how a verdict stops being needed twice. The test and the voicing guide record only the range and that it came from listening, never your notes, so nothing private reaches the public repo.

**The way we work changes with it.** A sound ticket's "For Will to try" becomes "run `scripts/listen <session>`" instead of render instructions. The work-ticket skill says to analyse before asking, and to write the prediction first. The pr-reviewer checks that each listening step is a session. It's logged in the Process log.

### 4. The demo song

Four songs in the genres you want to make, **lo-fi, ambient, house and techno** (open question 4), each written in a **song file** a person can read and an agent can write in musical terms:

```toml
# examples/demos/lofi.toml (illustrative)
tempo = 82

[[track]]
name = "Drums"
kind = "drums"
pattern.verse = { kick = "x...x...x...x...", snare = "....x.......x...", hats = "x.x.x.x.x.x.x.x." }

[[track]]
name = "Bass"
kind = "synth"
sound = "Mono bass"
pattern.verse = "F2 . . . . . C2 . | D#2 . . . . . . ."

[[track]]
name = "Keys"
kind = "synth"
sound = "Warm pad"
effects = [{ delay = { time = "1/8 dotted", feedback = 0.35, mix = 0.2 } }]

[[section]]
name = "verse"
bars = 8
```

- **`uta song <file>` compiles it to a command list.** The `uta` command can then render it, and the app can load it. Because it compiles through the real commands, the song always builds with today's command format. There's no saved file to keep loading (saving is still deferred).
- **They load from the app's menu:** Develop → Load demo song → Lo-fi, Ambient, House or Techno, with a confirm, since it replaces the current project. Each reloads clean every time.
- **Lo-fi comes first, in this project.** It's around 80 BPM: a laid-back drum loop, a mellow keys part, a round bass and a Delay, which is all Uta can do today. It's about a minute long, with a few sections, so arrangement and the timeline get used too. It uses the factory sounds once UTA-82 lands.
- **House and techno come next, in this project.** They suit what Uta already does best: the 808 and 909 kicks, the claps and hats, a mono bass with glide, and the Delay. House is around 124 BPM, with an off-beat hat, a chord stab and a rolling bass. Techno is around 130 BPM, with a driving kick, a hypnotic synth line slowly moved by the LFO, and the filter opened over a section. Each is shorter than lo-fi, a 32-bar arrangement, since most of their interest is in the loop and how it builds.
- **Ambient is started here, and grows with Reverb.** Ambient without reverb is thin: it's mostly long pads, slow movement and space. So this project lays down its pads, its slow LFO movement and its long Delay, and the Reverb project fills in the space. It's the song Reverb gets tried in first.
- **They show what's missing, too.** Lo-fi usually leans on swing, saturation or a bitcrusher, vinyl noise and reverb. House wants swing and sidechain ducking. Techno wants distortion and automation over long builds. Uta has none of these yet. Writing the songs turns "what should we build next?" into a list of what they needed.
- **Each song is its own ticket,** after the ticket that builds the song file, so they can be written in parallel, and one that runs long doesn't hold up the rest.
- **They're kept up.** The PR template gets a "Demo song: added / not yet / doesn't apply" line, so each feature ticket decides.
- **CI renders both** and checks each builds, doesn't clip, and has no silent tracks. It's not a golden WAV, because the song is meant to change.
- **`uta analyse` runs on them too.** This is the "does it sound good in a mix" check, where a session is the "does this sound good on its own" check.

Writing it is also the first try at directing music with AI: you describe it, Claude writes the song file and renders it with the numbers, you listen and react (open question 4).

### Reverb, next

The Reverb RFC adds its measurements to `uta-analysis`, so it can name its checks in terms of tools that already exist. From the research:
- decay time per frequency band, from an impulse response;
- **echo density**, how quickly separate echoes thicken into a smooth wash (Abel & Huang). It's reported to agree strongly with listening tests;
- coloration, the metallic ringing measured from how evenly the reverb's resonances are spread (Heldmann & Schlecht, DAFx 2021);
- and the images reverb designers use: the impulse response, decay per band, and echo density over time.

They're left to that RFC because they need a reverb to check them against (open question 5).

### Not in this project

- **Reference recordings.** You have none to hand. `--against` and the session format take one as just another candidate, so they slot in later.
- **Learned similarity scores** (CLAP embeddings, ViSQOL, Fréchet Audio Distance). They're built for comparing sets of clips or codec damage, not "is this kick good", and need Python and model downloads.
- **The AI sound designer.** This RFC is its groundwork.
- **A listening view inside Uta,** where you turn knobs live. The Sound tab already does live tuning, and an in-app view brings the app's design and review rules with it.
- **More demo songs beyond lo-fi and ambient.** Lo-fi, ambient, house and techno are the four here, and the format allows `examples/demos/<genre>.toml` for more later.
- **Reverb's own measurements** (above).

## What it looks like to you

Claude is working on a sound. Before it asks you anything, it has measured its renders and looked at their pictures. When it does need your ear, the PR says: run `scripts/listen 2026-10-20-snare-decay`.

A page opens in your browser with one question at the top: "Which of these sounds most like an 808 snare?" There are three buttons, A, B and C, and a loop playing. You click between them and the sound switches on the beat, at the same loudness, so you're judging the sound and not the volume. You pick B and type "A is too ringy, C's tail is too short". You press Finish.

The page now shows what B was, what Claude predicted ("B, because its tail decays in 210 ms, close to the 808's"), and each candidate's spectrogram and level curve. You close the tab. Claude already has your answer, and carries on.

Weeks later, Claude predicts your picks more often than not, and some of what you said has become tests that guard the sounds.

In the app, you choose Develop → Load demo song → Lo-fi. A minute-long track loads at about 80 BPM: a laid-back beat, mellow keys, a round bass and a Delay, in sections. You turn the keys' filter while it plays and hear it in context. The ambient song is all slow pads and long echoes for now, and when Reverb arrives it's where you try it first.

## Alternatives considered

- **Pictures first, numbers second.** Spectrograms feel like "hearing", and Claude can look at PNGs. But the research says models read them only roughly, and the result depends on how the image is drawn. Numbers are exact and testable. Pictures stay, as a check.
- **Python and ffmpeg for the analysis.** It's quick to write with numpy. But Python here has only the standard library, the numbers would differ from the ones the tests check, and CI would need another toolchain. ffmpeg can draw a spectrogram, but not with our scales or printed numbers.
- **A Rust audio-features library** (dasp-rs, aus). They're immature, and one changed its licence from non-commercial between versions. Each measurement is small enough to own.
- **Keeping the helpers in `tests/common` and copying them into the CLI.** Two copies drift apart, and then the numbers Claude reads aren't the numbers the tests check.
- **The listening view inside Uta, or a claude.ai Artifact.** You chose the local page (option a).
- **webMUSHRA, the standard browser listening-test tool.** It's built for formal studies, with its own licence and a PHP or Python backend. It doesn't know about our command lists, predictions or verdict files. We borrow its rules: loudness matching, shuffling, sample-accurate looping.
- **The demo song as a hand-written command list.** That's thousands of lines of UUIDs that break each time the command format changes. A song file that compiles stays readable and always builds.
- **One RFC with Reverb.** You chose two, and this one is small enough to land while Synth features finishes.

## Risks & unknowns

- **Whether predictions improve.** It's a guess that writing predictions and logging verdicts will make Claude's first attempts better within a project or two. We'll know by counting how often the prediction matched. If it doesn't improve, the tools still save you the Finder shuffle, and the verdict log still documents your taste.
- **Moving the helpers could change test numbers slightly.** The FFT changes, and some windows get resized. Any test that moves gets looked at, not loosened. The move is done in two steps: first move them unchanged, then swap the FFT.
- **Loudness matching can hide a real difference,** when one candidate is *meant* to be louder (a velocity layer). The session can turn matching off for that question.
- **The song file is a small language,** and those grow. It covers notes, drum patterns, sections, sounds and effect settings, and nothing else. Anything else is written as raw commands inside the file.
- **resvg's size.** It brings a font library and a renderer. It's only in the CLI, not the app or the engine, so the app doesn't grow. If the build cost is too high, the fallback is to draw PNGs directly with a tiny built-in font.
- **Private verdicts aren't backed up by git.** `~/.uta/listening/` lives only on your Mac, and CI and the reviewer can't see it. So a sound test made from verdicts carries its range and a one-line reason in the test itself, and the reviewer checks that, not the verdicts. Point `UTA_LISTENING_DIR` at a private repo if you want a history.
- **Four songs is the biggest part of the project.** Each is its own ticket, so if the project runs long, the later songs can move to the next project without holding up the tools.
- **Your time.** Sessions only help if they're quick to answer. One question per session, and a few in a queue, keeps each sitting to a few minutes.

## How we'll verify it

**Automated:**
- **Each measurement is checked on a synthetic signal with a known answer.** A 440 Hz sine reads 440 Hz. A sine at a known level reads the BS.1770 loudness the standard gives. An exponential decay of a known rate reads that decay. White noise reads as flat, and a sine as tonal. A square wave's partials read as harmonic, and a sum of unrelated partials reads as inharmonic. Stereo with L = R reads correlation 1.
- **The moved helpers keep every existing sound test passing** with its current limits. The golden WAVs don't change, because nothing in the engine does.
- **`uta analyse` is tested on WAVs it renders itself:** the report's fields, `--json`'s shape, and `--against` showing a difference only where there is one. The images are checked for size and that they render, not pixel for pixel.
- **The listening page is tested in vitest against its own server:**
  - the loudness matching sets the gains BS.1770 asks for;
  - the A/B/C order is shuffled, and isn't in anything the page receives before Finish;
  - Finish writes `verdict.toml` with every field.
- **The song file has tests for compiling** (a pattern becomes the right notes, a section repeats, an unknown sound is an error with its line number). CI renders the demo song and checks it doesn't clip and has no silent tracks.

**For you to try:**
- Run one real session from a sound ticket, end to end, and say whether it was quicker than a folder of WAVs and whether anything was confusing.
- Do a session where two candidates are identical, and check you can't tell which is which. It checks that the matching and switching are fair.
- Load the lo-fi song from the menu, play it, and change a few things while it plays. Say whether it feels like the genre, and what it's missing.

## Open questions

1. **A new crate for the measurements, or a module in `uta-engine`?** A new `uta-analysis` crate keeps the engine free of the FFT and drawing libraries, and keeps the analysis free of anything real-time. A module is simpler but puts offline-only code next to the audio thread. Recommendation: the new crate. **Resolved: the new crate (Will, 2026-10-17).**
2. **Images through SVG and `resvg`, or drawn straight to PNG?** SVG gives real text and the same picture in the listening page, at the cost of a bigger CLI build. Recommendation: SVG and resvg, with the plain-PNG fallback if the build cost bites. **Resolved: SVG and resvg (Will, 2026-10-17).**
3. **Keep sessions and verdicts in the repo?** In `docs/listening/`, they're the record of your taste, they survive worktrees, and tests can be made from them. They're only small text files, since WAVs are rendered on demand. The alternative is outside the repo, private, in the session scratchpad. Recommendation: in the repo. The repo is public, so your notes would be public. **Resolved: kept private, outside the repo, in `~/.uta/listening/` (movable with `UTA_LISTENING_DIR`). Tests and the voicing guide take only the measured ranges (Will, 2026-10-17).**
4. **What should the demo song be?** Its genre, feel and tempo are yours to direct. Recommendation: something close to what you'd want to make in Uta, so it's useful for trying features, not a tech demo. Tell me a style and a reference track, and Claude drafts the first version as part of this project. **Resolved: lo-fi, ambient, house and techno. Lo-fi, house and techno are written in full here, and ambient is started here and finished alongside Reverb (Will, 2026-10-17).**
5. **Reverb's measurements here or in the Reverb RFC?** Building them here means they're ready the day Reverb starts. But they can only be checked against synthetic decays until there's a reverb. Recommendation: the Reverb RFC, with this RFC making sure adding them is just "a function and a line in the report". **Resolved: the Reverb RFC (Will, 2026-10-17).**

## New terms

- **Listening session:** one question for Will about a sound, with the candidates, Claude's prediction and Will's verdict kept together.
- **Blind A/B:** comparing two or more versions without knowing which is which, matched in loudness, so you judge only the sound.
- **Loudness matching:** setting each candidate's gain so they sound equally loud (by the BS.1770 measure), since the louder one usually sounds better.
- **Spectrogram:** a picture of a sound over time, with time across, pitch up the side and loudness as brightness.
- **Spectral flatness:** a number from 0 to 1 saying how noise-like a sound is (1) or how tonal (0).
- **Inharmonicity:** how far a sound's partials sit from whole-number multiples of its pitch; what makes it sound metallic or bell-like.
- **Song file:** a readable text description of a song (tracks, patterns, sections, sounds) that compiles to Uta's commands.
- **Echo density:** how quickly a reverb's separate echoes thicken into a smooth wash (defined here, measured in the Reverb RFC).
