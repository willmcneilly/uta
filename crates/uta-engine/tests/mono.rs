//! Mono mode: one voice sounds, the latest note wins, letting go returns to
//! a note still held, Legato carries the envelopes on, Glide reaches the new
//! pitch in its set time, and a note after silence starts fresh. Then the
//! stuck notes, by name: chasing on Play, editing a note while it sounds,
//! deleting the held note and going round the loop each leave no note
//! sounding, with and without Legato. See RFC-008, "The instrument" (Voice),
//! "In the engine" (Mono mode), "How we'll verify it" and "Risks & unknowns"
//! (Mono mode meets the note bookkeeping).
//!
//! Everything is measured from the rendered waveform: pitch from its zero
//! crossings, level from its peaks.

mod common;

use std::ops::Range;

use common::*;
use uta_core::time::Ticks;
use uta_core::{Command, Note, Project, SynthParam, VoiceMode as CoreVoiceMode, Waveform as Wave};
use uta_engine::offline::Renderer;
use uta_engine::{
    EngineConfig, NoteKey, Snapshot, SynthSettings, VOICE_LEVEL, VoiceMode, Waveform, pitch_to_hz,
    velocity_to_gain,
};

const RATE: u32 = 48_000;
const BLOCK_SIZES: [usize; 3] = [32, 128, 1024];

/// A3, 220 Hz, and the E and A over it.
const A3: u8 = 57;
const E4: u8 = 64;
const A4: u8 = 69;

fn config() -> EngineConfig {
    EngineConfig {
        sample_rate: RATE,
        channels: 1,
    }
}

fn seconds(s: f64) -> usize {
    (s * f64::from(RATE)).round() as usize
}

fn renderer(settings: SynthSettings, block_size: usize) -> Renderer {
    let snapshot = with_synth(Snapshot::default().with_volume_db(0.0), settings);
    Renderer::new(config(), snapshot, block_size)
}

/// A sine in Mono, at full sustain, with this Legato and Glide.
fn mono(legato: bool, glide_seconds: f32) -> SynthSettings {
    SynthSettings {
        osc1_wave: Waveform::Sine,
        sustain: 1.0,
        voice_mode: VoiceMode::Mono,
        legato,
        glide_seconds,
        ..SynthSettings::default()
    }
}

/// Glide off: its shortest.
const NO_GLIDE: f32 = 0.001;

/// What happens to the live notes: at each time in seconds, a note on (key,
/// pitch, velocity) or off (key), in order.
enum At {
    On(f64, u128, u8, u8),
    Off(f64, u128),
}

/// Plays `events` on `settings` in blocks of `block_size`, then renders on
/// to `until` seconds. Each event lands on the block boundary at or before
/// its time.
fn play_live(settings: SynthSettings, events: &[At], until: f64, block_size: usize) -> Vec<f32> {
    let mut renderer = renderer(settings, block_size);
    for event in events {
        let at = match *event {
            At::On(at, ..) | At::Off(at, _) => seconds(at),
        };
        let rendered = renderer.samples().len();
        renderer.render((at / block_size * block_size).saturating_sub(rendered));
        match *event {
            At::On(_, key, pitch, velocity) => renderer
                .controller
                .note_on(0, NoteKey(key), pitch, velocity)
                .unwrap(),
            At::Off(_, key) => renderer.controller.note_off(0, NoteKey(key)).unwrap(),
        }
    }
    let rendered = renderer.samples().len();
    renderer.render(seconds(until) - rendered);
    renderer.into_samples()
}

/// The samples from `from` to `to` seconds.
fn between(samples: &[f32], from: f64, to: f64) -> Range<usize> {
    seconds(from)..seconds(to).min(samples.len())
}

fn assert_pitch(samples: &[f32], range: Range<usize>, pitch: u8, what: &str) {
    let expected = pitch_to_hz(pitch);
    let measured = measure_frequency(&samples[range.clone()], RATE);
    assert!(
        (measured / expected - 1.0).abs() < 0.001,
        "{what}: {measured:.2} Hz between samples {range:?}, not {expected:.2}"
    );
}

/// One sine voice's peak at full sustain and velocity 127, at 0 dB.
fn one_voice() -> f32 {
    VOICE_LEVEL * velocity_to_gain(127)
}

fn assert_level(samples: &[f32], range: Range<usize>, level: f32, what: &str) {
    let measured = peak(&samples[range.clone()]);
    assert!(
        (measured / level - 1.0).abs() < 0.01,
        "{what}: peak {measured} between samples {range:?}, not {level}"
    );
}

fn assert_silent(samples: &[f32], range: Range<usize>, what: &str) {
    let loudest = peak(&samples[range.clone()]);
    assert!(
        loudest == 0.0,
        "{what}: sounding at {loudest} between samples {range:?}"
    );
}

/// The steepest `voices` sine voices at up to `frequency_hz` move in a
/// sample at full velocity, while their level moves as fast as Mono moves
/// it (a 5 ms attack, take-over fade or velocity change), plus 10%.
fn click_limit(voices: usize, frequency_hz: f64) -> f32 {
    let per_voice = sine_max_step(frequency_hz, RATE) + 1.0 / seconds(0.005) as f32;
    voices as f32 * VOICE_LEVEL * per_voice * 1.1
}

/// Two notes overlap, then three start on one sample: in Mono, only one
/// note sounds at a time, at one voice's level, and it's the latest. In
/// Poly, the same notes sound together, louder.
#[test]
fn in_mono_one_voice_sounds_and_the_latest_note_wins() {
    let events = [
        At::On(0.0, 1, A3, 127),
        At::On(0.2, 2, E4, 127),
        At::Off(0.5, 1),
        At::Off(0.5, 2),
        At::On(0.8, 3, A3, 127),
        At::On(0.8, 4, A4, 127),
        At::On(0.8, 5, E4, 127),
    ];
    for legato in [false, true] {
        let what = format!("legato {legato}");
        let samples = play_live(mono(legato, NO_GLIDE), &events, 1.2, 128);
        assert_pitch(&samples, between(&samples, 0.25, 0.45), E4, &what);
        assert_level(&samples, between(&samples, 0.25, 0.45), one_voice(), &what);
        assert_pitch(&samples, between(&samples, 0.85, 1.15), E4, &what);
        assert_level(&samples, between(&samples, 0.85, 1.15), one_voice(), &what);
    }
    let poly = SynthSettings {
        voice_mode: VoiceMode::Poly,
        ..mono(false, NO_GLIDE)
    };
    let samples = play_live(poly, &events, 1.2, 128);
    assert!(peak(&samples[between(&samples, 0.25, 0.45)]) > 1.5 * one_voice());
    assert!(peak(&samples[between(&samples, 0.85, 1.15)]) > 2.0 * one_voice());
}

/// Letting go of the latest note goes back to the one still held, without
/// releasing; letting go of a note under the latest changes nothing; and
/// letting go of the last releases it.
#[test]
fn letting_go_returns_to_a_held_note() {
    for legato in [false, true] {
        let what = format!("legato {legato}");
        let samples = play_live(
            mono(legato, NO_GLIDE),
            &[
                At::On(0.0, 1, A3, 127),
                At::On(0.2, 2, E4, 127),
                At::Off(0.4, 2),
                // E again, then A let go from under it.
                At::On(0.7, 3, E4, 127),
                At::Off(0.8, 1),
                At::Off(1.0, 3),
            ],
            1.4,
            128,
        );
        assert_pitch(&samples, between(&samples, 0.25, 0.38), E4, &what);
        assert_pitch(&samples, between(&samples, 0.45, 0.68), A3, &what);
        assert_level(&samples, between(&samples, 0.45, 0.68), one_voice(), &what);
        assert_pitch(&samples, between(&samples, 0.82, 0.98), E4, &what);
        assert_level(&samples, between(&samples, 0.82, 0.98), one_voice(), &what);
        // The default release is 0.2 s.
        assert_silent(&samples, between(&samples, 1.21, 1.4), &what);
    }
}

/// On a note that decays to a low sustain, a second note overlapping the
/// first: with Legato its level carries on where it was; without, it
/// attacks again, from where it was, to full level. Neither clicks.
#[test]
fn with_legato_the_envelope_does_not_restart() {
    let decaying = |legato| SynthSettings {
        decay_seconds: 0.1,
        sustain: 0.2,
        ..mono(legato, NO_GLIDE)
    };
    let events = [
        At::On(0.0, 1, A3, 127),
        At::On(0.5, 2, E4, 127),
        At::Off(0.8, 2),
        At::Off(0.8, 1),
    ];
    let sustained = 0.2 * one_voice();
    for legato in [false, true] {
        let what = format!("legato {legato}");
        let samples = play_live(decaying(legato), &events, 1.1, 128);
        // The second note lands on the block boundary just before 0.5 s.
        assert_level(&samples, between(&samples, 0.4, 0.49), sustained, &what);
        // Just after the second note's 5 ms attack.
        let after = between(&samples, 0.505, 0.52);
        if legato {
            assert_level(&samples, after, sustained, &what);
        } else {
            assert!(
                peak(&samples[after]) > 0.8 * one_voice(),
                "{what}: didn't attack again"
            );
        }
        assert_no_click(&samples, click_limit(1, pitch_to_hz(E4)), &what);
    }
}

/// Glide slides the pitch in semitones at an even rate, and reaches the new
/// note exactly when its time is up: up an octave in 0.1 s, it's half way
/// (a tritone) at 0.05 s, and at the new pitch from 0.1 s on. With or
/// without Legato, and down as well as up.
#[test]
fn the_glide_reaches_the_new_pitch_in_the_set_time() {
    let glide = 0.1;
    for legato in [false, true] {
        for (from, to) in [(A3, A4), (A4, A3)] {
            let what = format!("legato {legato}, {from} to {to}");
            let samples = play_live(
                mono(legato, glide as f32),
                &[At::On(0.0, 1, from, 100), At::On(0.2, 2, to, 100)],
                0.5,
                128,
            );
            // The glide starts on the block boundary at or before 0.2 s.
            let start = (seconds(0.2) / 128 * 128) as f64 / f64::from(RATE);
            let cycles = pitch_over_time(&samples[seconds(0.1)..], RATE);
            let semitones = |hz: f64| 12.0 * (hz / 440.0).log2() + 69.0;
            for &(t, hz) in &cycles {
                // From the glide's start.
                let t = t + 0.1 - start;
                // A cycle across the glide's start or end is part of both.
                let period = 1.0 / hz;
                if t.abs() < period || (t - glide).abs() < period {
                    continue;
                }
                let expected = f64::from(from)
                    + (f64::from(to) - f64::from(from)) * (t / glide).clamp(0.0, 1.0);
                // A cycle's frequency is its average, a little off the
                // pitch at its middle while it glides.
                assert!(
                    (semitones(hz) - expected).abs() < 0.05,
                    "{what}: {hz:.2} Hz at {t:.4} s into the glide, not {expected:.2} semitones"
                );
            }
            assert_pitch(
                &samples,
                between(&samples, start + glide + 0.001, 0.5),
                to,
                &what,
            );
        }
    }
}

/// A note that follows silence, or one let go and still ringing in its
/// release, starts at its own pitch, without gliding, and so do notes that
/// start on the same sample.
#[test]
fn a_note_after_silence_does_not_glide() {
    let glide = 0.5;
    for legato in [false, true] {
        let what = format!("legato {legato}");
        let settings = SynthSettings {
            release_seconds: 0.3,
            ..mono(legato, glide)
        };
        let samples = play_live(
            settings,
            &[
                At::On(0.0, 1, A3, 127),
                At::Off(0.2, 1),
                // In A's release.
                At::On(0.3, 2, E4, 127),
                At::Off(0.5, 2),
                // After silence.
                At::On(1.0, 3, A4, 127),
                // Two at once, after silence.
                At::Off(1.2, 3),
                At::On(1.8, 4, E4, 127),
                At::On(1.8, 5, A3, 127),
            ],
            2.0,
            128,
        );
        // After the 5 ms fade of A's release.
        assert_pitch(&samples, between(&samples, 0.31, 0.35), E4, &what);
        assert_silent(&samples, between(&samples, 0.9, 1.0), &what);
        assert_pitch(&samples, between(&samples, 1.0, 1.04), A4, &what);
        assert_pitch(&samples, between(&samples, 1.8, 1.84), A3, &what);
    }
}

/// A bassline in Mono that changes velocity, retriggers, glides, goes back
/// to held notes and switches between Poly and Mono, Legato on and off and
/// Glide's time while its notes sound. Every event lands on a multiple of
/// 1024 frames.
fn session(block_size: usize) -> Vec<f32> {
    let base = SynthSettings {
        osc1_wave: Waveform::Saw,
        osc2_level: 0.5,
        osc2_pitch_semitones: 0.1,
        osc_retrigger: false,
        cutoff_hz: 1500.0,
        filter_env_octaves: 2.0,
        ..mono(false, 0.04)
    };
    let mut renderer = renderer(base, block_size);
    let changes = [
        mono(false, 0.04),
        SynthSettings {
            legato: true,
            ..base
        },
        SynthSettings {
            glide_seconds: 0.2,
            ..base
        },
        SynthSettings {
            voice_mode: VoiceMode::Poly,
            ..base
        },
        base,
    ];
    let pitches = [36, 43, 48, 41, 39, 46, 36, 51];
    for (round, settings) in changes.into_iter().enumerate() {
        for (i, &pitch) in pitches.iter().enumerate() {
            let key = NoteKey((round * 100 + i) as u128);
            renderer
                .controller
                .note_on(0, key, pitch, 40 + 10 * i as u8)
                .unwrap();
            renderer.render(1024 * (1 + i % 3));
            if i % 2 == 1 {
                renderer
                    .controller
                    .note_off(0, NoteKey((round * 100 + i - 1) as u128))
                    .unwrap();
            }
            if i == 4 {
                renderer.controller.set_synth_settings(0, settings).unwrap();
            }
            renderer.render(1024);
        }
        for i in 0..pitches.len() {
            renderer
                .controller
                .note_off(0, NoteKey((round * 100 + i) as u128))
                .unwrap();
        }
        renderer.render(1024 * 4);
    }
    renderer.render(1024 * 12);
    renderer.into_samples()
}

/// Rendering twice gives identical audio in Mono, glides and all, and so do
/// blocks of 32, 128 and 1024.
#[test]
fn block_size_does_not_change_mono() {
    let reference = session(1024);
    assert!(peak(&reference) > 0.05, "the session made no sound");
    assert_eq!(session(1024), reference, "rendered twice");
    for block_size in [32, 128] {
        let difference = max_difference(&session(block_size), &reference);
        assert!(
            difference < 1e-6,
            "blocks of {block_size} differ from 1024 by {difference}"
        );
    }
}

/// Mono's notes following on (new velocities, attacking again from where
/// they are, gliding, returning to held notes), a note after a release
/// fading it out, and each of the Voice mode, Legato and Glide turned while
/// notes sound: nothing clicks.
#[test]
fn no_clicks_when_mono_notes_follow_on_or_its_settings_change() {
    for block_size in BLOCK_SIZES {
        let mut renderer = renderer(mono(false, 0.03), block_size);
        let mut key = 0;
        let mut on = |renderer: &mut Renderer, pitch: u8, velocity: u8| {
            key += 1;
            renderer
                .controller
                .note_on(0, NoteKey(key), pitch, velocity)
                .unwrap();
            key
        };
        let turns = [
            mono(true, 0.03),
            mono(true, 0.3),
            SynthSettings {
                voice_mode: VoiceMode::Poly,
                ..mono(true, 0.3)
            },
            mono(false, NO_GLIDE),
            mono(true, NO_GLIDE),
        ];
        for settings in turns {
            let low = on(&mut renderer, A3, 127);
            renderer.render(2_000);
            let high = on(&mut renderer, A4, 30);
            renderer.render(1_500);
            let middle = on(&mut renderer, E4, 127);
            renderer.render(700);
            renderer.controller.note_off(0, NoteKey(middle)).unwrap();
            renderer.render(900);
            renderer.controller.set_synth_settings(0, settings).unwrap();
            renderer.render(1_100);
            renderer.controller.note_off(0, NoteKey(high)).unwrap();
            renderer.render(1_300);
            renderer.controller.note_off(0, NoteKey(low)).unwrap();
            renderer.render(400);
            // In the release.
            let next = on(&mut renderer, E4, 90);
            renderer.render(3_000);
            renderer.controller.note_off(0, NoteKey(next)).unwrap();
            renderer.render(400);
        }
        renderer.render(seconds(0.5));
        // A Voice mode switch releases a note in one voice while the next
        // starts in another.
        let samples = renderer.samples();
        assert_no_click(
            samples,
            click_limit(2, pitch_to_hz(A4)),
            &format!("blocks of {block_size}"),
        );
        assert_eq!(peak(&samples[samples.len() - seconds(0.1)..]), 0.0);
    }
}

// The stuck notes, through the sequencer, as the app plays them.

const BEAT: Ticks = 960;
/// A beat at 120 BPM, in samples.
const BEAT_SAMPLES: usize = 24_000;
const BAR_SAMPLES: usize = 4 * BEAT_SAMPLES;
/// A sine at full sustain released over 5 ms, as in `editing.rs`, in Mono
/// with a short glide.
const RELEASE_SAMPLES: usize = 240;

fn sequenced(legato: bool) -> [SynthParam; 6] {
    [
        SynthParam::Osc1Wave(Wave::Sine),
        SynthParam::Sustain(1.0),
        SynthParam::ReleaseSeconds(0.005),
        SynthParam::VoiceMode(CoreVoiceMode::Mono),
        SynthParam::Legato(legato),
        SynthParam::GlideSeconds(0.02),
    ]
}

/// The sample `beats` into the bar.
fn beats(beats: f64) -> usize {
    (beats * BEAT_SAMPLES as f64).round() as usize
}

fn ticks(beats: f64) -> Ticks {
    (beats * BEAT as f64).round() as Ticks
}

/// A note from `start` for `length` beats.
fn beat_note(index: u128, pitch: u8, start: f64, length: f64) -> Note {
    note(index, pitch, ticks(start), ticks(length))
}

/// `at`, rounded down to a whole number of blocks, so an edit sent after
/// rendering that far lands on it exactly.
fn block_start(at: usize, block_size: usize) -> usize {
    at / block_size * block_size
}

/// Where the sound is after silence: no note sounds from `ended` (the
/// sample a note ended on, released and faded out from there) to `next`.
fn assert_silent_after(samples: &[f32], ended: usize, next: usize, what: &str) {
    assert_silent(samples, ended + RELEASE_SAMPLES + 1..next, what);
}

/// Two overlapping notes, A3 for 3 beats and E4 inside it from beat 1 to 2,
/// and the other way round, chased from beat 1.5: E4 started later, so it
/// wins, though chasing finds it first. When E4 ends, A3 plays if it's
/// still held, and when the last ends, nothing sounds until the loop comes
/// round.
#[test]
fn chasing_on_play_leaves_no_note_stuck_in_mono() {
    struct Case {
        what: &'static str,
        notes: [Note; 2],
        /// The note sounding from beat 2, if any, and when the last ends.
        after: Option<u8>,
        last_end: f64,
    }
    let cases = [
        Case {
            what: "E4 inside A3",
            notes: [beat_note(0, A3, 0.0, 3.0), beat_note(1, E4, 1.0, 1.0)],
            after: Some(A3),
            last_end: 3.0,
        },
        Case {
            what: "A3 ends under E4",
            notes: [beat_note(0, A3, 0.0, 2.0), beat_note(1, E4, 1.0, 2.0)],
            after: Some(E4),
            last_end: 3.0,
        },
    ];
    for legato in [false, true] {
        for block_size in BLOCK_SIZES {
            for case in &cases {
                let what = format!("{}, legato {legato}, blocks of {block_size}", case.what);
                let project = project(120.0, 1, &sequenced(legato), case.notes.to_vec());
                let mut renderer = Renderer::new(config(), Snapshot::from(&project), block_size);
                renderer.controller.locate(ticks(1.5)).unwrap();
                renderer.controller.play().unwrap();
                // From beat 1.5 to the end of the bar, then a beat of the
                // next pass.
                renderer.render(beats(3.5));
                let samples = renderer.into_samples();
                let at = |b: f64| beats(b - 1.5);
                assert_pitch(&samples, at(1.55)..at(1.95), E4, &what);
                if let Some(pitch) = case.after {
                    assert_pitch(&samples, at(2.1)..at(2.9), pitch, &what);
                }
                assert_silent_after(&samples, at(case.last_end), at(4.0), &what);
                // The next pass starts A3 afresh.
                assert_pitch(&samples, at(4.1)..at(4.9), A3, &what);
            }
        }
    }
}

/// Applies `command` to the project and sends the engine the result, as the
/// app does.
fn edit(renderer: &mut Renderer, project: &mut Project, command: Command) {
    project.apply(&command).unwrap();
    renderer.controller.set_project(project).unwrap();
}

/// A3 for 3 beats from the bar's start, with E4 on top from beat 1 to 2.5,
/// edited at beat 1.5 by `command`, given the clip and the two notes. Plays
/// to the end of the bar.
fn edited(
    legato: bool,
    block_size: usize,
    command: impl Fn(uta_core::ClipId, Note, Note) -> Command,
) -> (Vec<f32>, usize) {
    let low = beat_note(0, A3, 0.0, 3.0);
    let high = beat_note(1, E4, 1.0, 1.5);
    let mut project = project(120.0, 1, &sequenced(legato), vec![low, high]);
    let clip = project.tracks()[0].clips()[0].id();
    let mut renderer = Renderer::new(config(), Snapshot::from(&project), block_size);
    renderer.controller.play().unwrap();
    let edit_at = block_start(beats(1.5), block_size);
    renderer.render(edit_at);
    edit(&mut renderer, &mut project, command(clip, low, high));
    renderer.render(BAR_SAMPLES - edit_at);
    (renderer.into_samples(), edit_at)
}

/// Re-pitching E4 (the note Mono plays) as it sounds releases it, and Mono
/// goes back to A3, which ends on beat 3; shortening A3 under it to before
/// the playhead lets it go, so when E4 ends nothing sounds. Moving E4 later
/// lets it go too, and it plays again where it was moved to. Nothing sounds
/// once the last note ends.
#[test]
fn editing_a_note_while_it_sounds_leaves_no_note_stuck_in_mono() {
    for legato in [false, true] {
        for block_size in BLOCK_SIZES {
            let what = |case| format!("{case}, legato {legato}, blocks of {block_size}");

            let (samples, edit_at) =
                edited(legato, block_size, |clip, _, high| Command::SetNotes {
                    clip,
                    notes: vec![Note { pitch: 65, ..high }],
                });
            let what_now = what("E4 re-pitched");
            assert_pitch(&samples, edit_at + 2_000..beats(2.9), A3, &what_now);
            assert_silent_after(&samples, beats(3.0), BAR_SAMPLES, &what_now);

            let (samples, edit_at) = edited(legato, block_size, |clip, low, _| Command::SetNotes {
                clip,
                notes: vec![Note {
                    length: ticks(1.0),
                    ..low
                }],
            });
            let what_now = what("A3 shortened under E4");
            assert_pitch(&samples, edit_at..beats(2.45), E4, &what_now);
            assert_silent_after(&samples, beats(2.5), BAR_SAMPLES, &what_now);

            let (samples, edit_at) =
                edited(legato, block_size, |clip, _, high| Command::SetNotes {
                    clip,
                    notes: vec![Note {
                        start: ticks(3.5),
                        length: ticks(0.25),
                        ..high
                    }],
                });
            let what_now = what("E4 moved later");
            assert_pitch(&samples, edit_at + 2_000..beats(2.9), A3, &what_now);
            assert_silent_after(&samples, beats(3.0), beats(3.5), &what_now);
            assert_pitch(&samples, beats(3.5) + 100..beats(3.74), E4, &what_now);
            assert_silent_after(&samples, beats(3.75), BAR_SAMPLES, &what_now);
        }
    }
}

/// Deleting E4 as Mono plays it goes back to A3; deleting A3 from under it
/// lets it go, so when E4 ends nothing sounds.
#[test]
fn deleting_the_held_note_leaves_no_note_stuck_in_mono() {
    for legato in [false, true] {
        for block_size in BLOCK_SIZES {
            let what = |case| format!("{case}, legato {legato}, blocks of {block_size}");
            let remove = |which: fn(Note, Note) -> Note| {
                move |clip: uta_core::ClipId, low: Note, high: Note| Command::RemoveNotes {
                    clip,
                    notes: vec![which(low, high).id],
                }
            };

            let (samples, edit_at) = edited(legato, block_size, remove(|_, high| high));
            let what_now = what("E4 deleted");
            assert_pitch(&samples, edit_at + 2_000..beats(2.9), A3, &what_now);
            assert_silent_after(&samples, beats(3.0), BAR_SAMPLES, &what_now);

            let (samples, edit_at) = edited(legato, block_size, remove(|low, _| low));
            let what_now = what("A3 deleted under E4");
            assert_pitch(&samples, edit_at..beats(2.45), E4, &what_now);
            assert_silent_after(&samples, beats(2.5), BAR_SAMPLES, &what_now);
        }
    }
}

/// A 1-bar loop with a note in the first beat, then E4 from beat 2 and A3
/// from beat 3 crossing the loop's end. Going round the loop lets both go:
/// nothing sounds from the end of the loop to the first note of the next
/// pass, and each pass plays the same.
#[test]
fn going_round_the_loop_leaves_no_note_stuck_in_mono() {
    let notes = vec![
        beat_note(0, A4, 0.5, 0.5),
        beat_note(1, E4, 2.0, 1.5),
        beat_note(2, A3, 3.0, 2.0),
    ];
    for legato in [false, true] {
        for block_size in BLOCK_SIZES {
            let what = format!("legato {legato}, blocks of {block_size}");
            let project = project(120.0, 1, &sequenced(legato), notes.clone());
            let mut renderer = Renderer::new(config(), Snapshot::from(&project), block_size);
            renderer.controller.play().unwrap();
            let passes = 3;
            renderer.render(passes * BAR_SAMPLES);
            let samples = renderer.into_samples();
            for pass in 0..passes {
                let bar = pass * BAR_SAMPLES;
                let what = format!("{what}, pass {pass}");
                if pass > 0 {
                    assert_silent_after(&samples, bar, bar + beats(0.5), &what);
                }
                assert_pitch(&samples, bar + beats(0.55)..bar + beats(0.95), A4, &what);
                assert_silent_after(&samples, bar + beats(1.0), bar + beats(2.0), &what);
                assert_pitch(&samples, bar + beats(2.05)..bar + beats(2.95), E4, &what);
                assert_pitch(&samples, bar + beats(3.05)..bar + beats(3.45), A3, &what);
                // E4 ends: still A3, held.
                assert_pitch(&samples, bar + beats(3.55)..bar + beats(3.95), A3, &what);
            }
        }
    }
}
