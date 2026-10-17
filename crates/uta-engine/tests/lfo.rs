//! The synth's LFO, measured from the waveform: Vibrato's wobble in the
//! pitch, Width's in the pulse's duty cycle, and the filter's LFO amount in
//! the brightness. See RFC-008, "How we'll verify it" (LFO), and
//! `CLAUDE.md`, "Proving audio code works".
//!
//! Most notes are started through the controller with the transport
//! stopped; the synced LFO plays a project, since its phase is the song's.

mod common;

use common::*;
use uta_core::SynthParam;
use uta_core::time::{NoteDivision, TICKS_PER_QUARTER, Ticks};
use uta_engine::offline::Renderer;
use uta_engine::{
    EngineConfig, LFO_SMOOTHING_SECONDS, LfoShape, NoteKey, Snapshot, SynthSettings, VOICE_LEVEL,
    Waveform,
};

const RATE: u32 = 48_000;
const BAR: Ticks = 4 * TICKS_PER_QUARTER;

fn config() -> EngineConfig {
    EngineConfig {
        sample_rate: RATE,
        channels: 1,
    }
}

fn renderer(settings: SynthSettings, block_size: usize) -> Renderer {
    let snapshot = with_synth(Snapshot::default().with_volume_db(0.0), settings);
    Renderer::new(config(), snapshot, block_size)
}

fn seconds(s: f64) -> usize {
    (s * f64::from(RATE)).round() as usize
}

/// A sine held at full level, so its pitch can be read from its zero
/// crossings.
fn sine() -> SynthSettings {
    SynthSettings {
        osc1_wave: Waveform::Sine,
        sustain: 1.0,
        ..SynthSettings::default()
    }
}

/// Holds one note for `hold` seconds and returns everything rendered.
fn held_note(settings: SynthSettings, pitch: u8, hold: f64) -> Vec<f32> {
    let mut renderer = renderer(settings, 128);
    renderer
        .controller
        .note_on(0, NoteKey(1), pitch, 127)
        .unwrap();
    renderer.render_seconds(hold);
    renderer.into_samples()
}

/// The pitch over time of a sine at `hz`, in semitones from `hz`, at the
/// time of each cycle's middle in seconds, from `from` seconds on.
fn semitones_over_time(samples: &[f32], hz: f64, from: f64) -> Vec<(f64, f64)> {
    pitch_over_time(samples, RATE)
        .into_iter()
        .filter(|&(time, _)| time >= from)
        .map(|(time, frequency)| (time, 12.0 * (frequency / hz).log2()))
        .collect()
}

/// The largest difference between `measured` and `expected` at each time.
fn worst_error(measured: &[(f64, f64)], expected: impl Fn(f64) -> f64) -> f64 {
    measured
        .iter()
        .map(|&(time, value)| (value - expected(time)).abs())
        .fold(0.0, f64::max)
}

/// The times `values` cross zero going up, interpolated between points.
fn rising_crossings(values: &[(f64, f64)]) -> Vec<f64> {
    values
        .windows(2)
        .filter(|w| w[0].1 < 0.0 && w[1].1 >= 0.0)
        .map(|w| w[0].0 + (w[1].0 - w[0].0) * w[0].1 / (w[0].1 - w[1].1))
        .collect()
}

/// Vibrato wobbles the pitch as a sine at the LFO's rate, by its depth
/// either way: the pitch measured cycle by cycle is the set wobble to within
/// a fiftieth of a semitone, and its own rate and depth, measured on their
/// own, are the set ones.
#[test]
fn vibratos_pitch_wobble_has_the_set_rate_and_depth() {
    for (rate_hz, depth) in [(5.0, 1.0), (2.0, 0.2), (7.5, 2.0)] {
        let settings = SynthSettings {
            lfo_rate_hz: rate_hz,
            lfo_vibrato_semitones: depth,
            lfo_retrigger: true,
            ..sine()
        };
        let samples = held_note(settings, 69, 2.0);
        let wobble = semitones_over_time(&samples, 440.0, 0.02);
        let (rate, depth) = (f64::from(rate_hz), f64::from(depth));
        // Retriggered, the note starts the LFO's cycle: at zero, rising.
        let error = worst_error(&wobble, |t| {
            depth * (std::f64::consts::TAU * rate * t).sin()
        });
        assert!(
            error < 0.02,
            "{rate} Hz, {depth} semitones: off by up to {error} semitones"
        );
        let crossings = rising_crossings(&wobble);
        let measured_rate =
            (crossings.len() - 1) as f64 / (crossings.last().unwrap() - crossings.first().unwrap());
        assert!(
            (measured_rate / rate - 1.0).abs() < 0.005,
            "{rate} Hz measured as {measured_rate} Hz"
        );
        let highest = wobble.iter().map(|&(_, s)| s).fold(f64::MIN, f64::max);
        let lowest = wobble.iter().map(|&(_, s)| s).fold(f64::MAX, f64::min);
        assert!(
            (highest - depth).abs() < 0.02 && (lowest + depth).abs() < 0.02,
            "{depth} semitones measured as {lowest} to {highest}"
        );
    }
}

/// A project at `bpm` playing one A4 sine through four bars, with a synced
/// vibrato of a semitone at `division`, the voices sharing the LFO.
fn synced_project(bpm: f32, division: &str) -> uta_core::Project {
    project(
        bpm,
        4,
        &[
            SynthParam::Osc1Wave(uta_core::Waveform::Sine),
            SynthParam::Sustain(1.0),
            SynthParam::LfoSync(true),
            SynthParam::LfoDivision(NoteDivision::from_value(division).unwrap()),
            SynthParam::LfoVibratoSemitones(1.0),
        ],
        vec![note(0, 69, 0, 4 * BAR)],
    )
}

/// Plays `project` from `from` ticks for `hold` seconds, in blocks of
/// `block_size`.
fn play_from(project: &uta_core::Project, from: Ticks, hold: f64, block_size: usize) -> Vec<f32> {
    let mut renderer = Renderer::new(config(), Snapshot::from(project), block_size);
    renderer.controller.locate(from).unwrap();
    renderer.controller.play().unwrap();
    renderer.render_seconds(hold);
    renderer.into_samples()
}

/// A synced LFO's cycle is exactly its note length at the tempo, and its
/// phase is the song's: played from the middle of a beat in bar 3, the
/// wobble is where it would be had the song played from the top, with each
/// cycle starting on the beat (a quarter) or on the bar (a whole note).
#[test]
fn a_synced_lfos_cycle_is_the_note_length_and_lines_up_with_the_bar() {
    // Bar 3, beat 2 and a half.
    let from = 2 * BAR + TICKS_PER_QUARTER + TICKS_PER_QUARTER / 2;
    for (bpm, division, quarters) in [
        (100.0, "1/4", 1.0),
        (100.0, "1/1", 4.0),
        (137.0, "1/8d", 0.75),
    ] {
        let project = synced_project(bpm, division);
        // Short of the song's end at 137 BPM, where the loop goes round.
        let samples = play_from(&project, from, 2.5, 128);
        // From the note's first cycle: it starts at the song's phase, with
        // nothing for the smoothing to pull into line.
        let wobble = semitones_over_time(&samples, 440.0, 0.0);
        let beats_per_second = f64::from(bpm) / 60.0;
        let start = from as f64 / TICKS_PER_QUARTER as f64;
        let error = worst_error(&wobble, |t| {
            let song_quarters = start + t * beats_per_second;
            (std::f64::consts::TAU * song_quarters / quarters).sin()
        });
        assert!(
            error < 0.02,
            "{division} at {bpm} BPM: off the song's phase by up to {error} semitones"
        );
        // Each cycle is the note's length.
        let crossings = rising_crossings(&wobble);
        let cycle = quarters / beats_per_second;
        for pair in crossings.windows(2) {
            let length = pair[1] - pair[0];
            assert!(
                (length / cycle - 1.0).abs() < 0.002,
                "{division} at {bpm} BPM: a cycle of {length} s, not {cycle} s"
            );
        }
        // And starts on the beat or the bar: a whole number of note lengths
        // into the song.
        for &time in &crossings {
            let notes = (start + time * beats_per_second) / quarters;
            assert!(
                (notes - notes.round()).abs() < 0.002,
                "{division} at {bpm} BPM: a cycle starts {notes} notes into the song"
            );
        }
    }
}

/// With Retrigger on, a note starts its own LFO: one started later wobbles
/// exactly as one started first did, from its own start. Off, it joins the
/// LFO the voices share, so it wobbles in step with the first.
#[test]
fn retrigger_starts_each_notes_lfo_and_off_the_voices_share_one() {
    let later = seconds(0.33);
    let render = |retrigger: bool, at: usize| {
        let mut renderer = renderer(
            SynthSettings {
                lfo_vibrato_semitones: 1.0,
                lfo_retrigger: retrigger,
                ..sine()
            },
            128,
        );
        renderer.render(at);
        renderer.controller.note_on(0, NoteKey(1), 69, 127).unwrap();
        renderer.render_seconds(1.0);
        renderer.into_samples()
    };
    // Starting at 0.33 s isn't on a block's start; the note starts at the
    // next, which is where its own LFO starts.
    let at = later.div_ceil(128) * 128;
    let first = render(true, 0);
    let second = render(true, at);
    assert_eq!(
        &second[at..at + seconds(0.9)],
        &first[..seconds(0.9)],
        "retriggered, the later note sounds as the first did"
    );

    let first = render(false, 0);
    let second = render(false, at);
    let start = at as f64 / f64::from(RATE) + 0.02;
    let shared = semitones_over_time(&first, 440.0, start);
    // As far as the first note was rendered.
    let before_the_end = |measured: Vec<(f64, f64)>| {
        measured
            .into_iter()
            .filter(|&(time, _)| time < 0.98)
            .collect::<Vec<_>>()
    };
    let joined = before_the_end(semitones_over_time(&second, 440.0, start));
    let error = worst_error(&joined, |t| {
        let next = shared
            .partition_point(|&(time, _)| time < t)
            .min(shared.len() - 1);
        shared[next].1
    });
    // The two are measured a cycle of the tone apart at most, 2.3 ms, in
    // which a 5 Hz wobble of a semitone moves at most 0.07 semitones.
    assert!(
        error < 0.08,
        "off, the later note is off the shared LFO by {error} semitones"
    );
    // A retriggered later note, by contrast, is a third of a second, more
    // than one and a half cycles, out.
    let retriggered = before_the_end(semitones_over_time(&render(true, at), 440.0, start));
    let apart = worst_error(&retriggered, |t| {
        let next = shared
            .partition_point(|&(time, _)| time < t)
            .min(shared.len() - 1);
        shared[next].1
    });
    assert!(apart > 1.0, "{apart}");
}

/// With Retrigger and Sync off, the LFO the voices share starts again on
/// Play, and so do the sample-and-hold's levels: a note played on Play after
/// a while stopped, with a note drawing levels from the same voice's
/// generator meanwhile, sounds as one played on Play straight away.
#[test]
fn the_shared_lfo_restarts_on_play() {
    let render = |wait: usize| {
        let mut renderer = renderer(
            SynthSettings {
                lfo_vibrato_semitones: 1.0,
                lfo_shape: LfoShape::SampleAndHold,
                lfo_rate_hz: 7.0,
                ..sine()
            },
            128,
        );
        if wait > 0 {
            // In voice 0, which is free again by Play: its release is over.
            renderer.controller.note_on(0, NoteKey(2), 57, 127).unwrap();
            renderer.render(wait / 2);
            renderer.controller.note_off(0, NoteKey(2)).unwrap();
            renderer.render(wait - wait / 2);
        }
        renderer.controller.play().unwrap();
        renderer.controller.note_on(0, NoteKey(1), 69, 127).unwrap();
        renderer.render_seconds(1.0);
        renderer.into_samples()[wait..].to_vec()
    };
    let straight_away = render(0);
    assert!(peak(&straight_away) > 0.1);
    assert_eq!(render(128 * 300), straight_away);
}

/// The share of each cycle a pulse spends above zero, at the time of each
/// cycle's middle, from `from` seconds on: its width.
fn duty_over_time(samples: &[f32], from: f64) -> Vec<(f64, f64)> {
    let rises: Vec<usize> = samples
        .windows(2)
        .enumerate()
        .filter(|(_, w)| w[0] <= 0.0 && w[1] > 0.0)
        .map(|(i, _)| i + 1)
        .collect();
    rises
        .windows(2)
        .map(|w| {
            let cycle = &samples[w[0]..w[1]];
            let high = cycle.iter().filter(|&&s| s > 0.0).count();
            let middle = (w[0] + w[1]) as f64 / 2.0 / f64::from(RATE);
            (middle, high as f64 / cycle.len() as f64)
        })
        .filter(|&(time, _)| time >= from)
        .collect()
}

/// The LFO's Width moves oscillator 1's pulse width either side of its
/// Width, by the set amount, at the LFO's rate: the duty cycle measured
/// cycle by cycle follows it. At zero the width stays put.
#[test]
fn the_lfos_width_moves_the_pulse_width() {
    for (width, amount) in [(0.5, 0.4), (0.3, 0.2), (0.5, 0.0)] {
        let settings = SynthSettings {
            osc1_wave: Waveform::Pulse,
            osc1_width: width,
            lfo_width: amount,
            lfo_rate_hz: 1.0,
            lfo_retrigger: true,
            sustain: 1.0,
            ..SynthSettings::default()
        };
        let samples = held_note(settings, 45, 2.0);
        let duty = duty_over_time(&samples, 0.03);
        let (width, amount) = (f64::from(width), f64::from(amount));
        // A cycle at 110 Hz is 436 samples, so a duty cycle is read to
        // within about a 400th, and the width moves a little within it.
        let error = worst_error(&duty, |t| {
            width + amount * (std::f64::consts::TAU * t).sin()
        });
        assert!(
            error < 0.01,
            "width {width} ± {amount}: off by up to {error}"
        );
    }
}

/// The pulse's width is held to its limits however far the LFO would move
/// it: past 95% the pulse would lose an edge.
#[test]
fn the_lfo_never_moves_the_width_past_its_limits() {
    let settings = SynthSettings {
        osc1_wave: Waveform::Pulse,
        osc1_width: 0.8,
        lfo_width: 0.45,
        lfo_rate_hz: 1.0,
        lfo_retrigger: true,
        sustain: 1.0,
        ..SynthSettings::default()
    };
    let duty = duty_over_time(&held_note(settings, 45, 1.0), 0.03);
    let widest = duty.iter().map(|&(_, d)| d).fold(0.0, f64::max);
    assert!((widest - 0.95).abs() < 0.01, "the widest is {widest}");
}

/// The brightness of the 8,192 samples (171 ms) from `from` seconds, as
/// their spectral centroid in Hz under the Blackman-Harris window. Without a
/// window, the leakage of a cut-off stretch of wave would swamp the
/// filter's effect.
fn brightness(samples: &[f32], from: f64) -> f64 {
    let bins = spectrum(&samples[seconds(from)..seconds(from) + 8192]);
    let bin_hz = f64::from(RATE) / 8192.0;
    let total: f64 = bins.iter().sum();
    let weighted: f64 = bins
        .iter()
        .enumerate()
        .map(|(k, magnitude)| k as f64 * bin_hz * magnitude)
        .sum();
    weighted / total
}

/// The filter's LFO amount moves the cutoff, by octaves, the way its sign
/// says: with a square LFO at 1 Hz the first half of each second is the top
/// of its swing and the second the bottom, so with the amount up the first
/// half is brighter, down the second, and at zero they're the same.
#[test]
fn the_filter_lfo_amount_moves_the_cutoff() {
    let render = |amount: f32| {
        let settings = SynthSettings {
            cutoff_hz: 800.0,
            filter_lfo_octaves: amount,
            lfo_shape: LfoShape::Square,
            lfo_rate_hz: 1.0,
            lfo_retrigger: true,
            sustain: 1.0,
            ..SynthSettings::default()
        };
        let samples = held_note(settings, 45, 1.0);
        (brightness(&samples, 0.15), brightness(&samples, 0.65))
    };
    let (still_high, still_low) = render(0.0);
    assert!(
        (still_high / still_low - 1.0).abs() < 0.01,
        "at zero: {still_high} Hz then {still_low} Hz"
    );
    let (up_high, up_low) = render(3.0);
    assert!(
        up_high > 2.0 * still_high && up_low < 0.5 * still_low,
        "up: {up_high} Hz then {up_low} Hz, against {still_high} Hz"
    );
    let (down_high, down_low) = render(-3.0);
    assert!(
        down_high < 0.5 * still_high && down_low > 2.0 * still_low,
        "down: {down_high} Hz then {down_low} Hz, against {still_high} Hz"
    );
}

/// The click limit for four sines a little over 110 Hz while the LFO swings
/// the filter and the pitch: as the synth tests' limit (the steepest a sine
/// moves, plus the steepest its level moves), with the level moving as fast
/// as an attack of the LFO's smoothing time.
fn lfo_click_limit() -> f32 {
    let smoothing = (LFO_SMOOTHING_SECONDS * f64::from(RATE)).round() as usize;
    // A whole tone of vibrato over 110 Hz is 123.5 Hz.
    let per_voice = sine_max_step(123.5, RATE) + 1.0 / smoothing as f32;
    4.0 * VOICE_LEVEL * per_voice * 1.1
}

/// Four sine notes under `settings` for a second at 110 Hz.
fn four_notes(settings: SynthSettings) -> Vec<f32> {
    let mut renderer = renderer(settings, 128);
    for key in 0..4 {
        renderer
            .controller
            .note_on(0, NoteKey(key), 45, 127)
            .unwrap();
        renderer.render(1000 + 77 * key as usize);
    }
    renderer.render_seconds(1.0);
    renderer.into_samples()
}

/// A stepped LFO, square or sample-and-hold, fast, swinging the filter
/// three octaves either way and the pitch a whole tone, doesn't click: each
/// of its steps is smoothed.
#[test]
fn square_and_sample_and_hold_stay_under_the_click_limit() {
    for shape in [LfoShape::Square, LfoShape::SampleAndHold, LfoShape::Saw] {
        for retrigger in [true, false] {
            let samples = four_notes(SynthSettings {
                cutoff_hz: 400.0,
                filter_lfo_octaves: 3.0,
                lfo_vibrato_semitones: 2.0,
                lfo_shape: shape,
                lfo_rate_hz: 8.0,
                lfo_retrigger: retrigger,
                ..sine()
            });
            assert!(peak(&samples) > 0.1);
            assert_no_click(
                &samples,
                lfo_click_limit(),
                &format!("{shape:?}, retrigger {retrigger}"),
            );
        }
    }
}

/// Guards the LFO click test: the same four notes with their level halved
/// at once fail it. An unsmoothed step in the filter's LFO would cut them by
/// more: from a cutoff of 3,200 Hz to 50 Hz, a 110 Hz tone falls by about
/// three quarters.
#[test]
fn the_lfo_click_limit_catches_a_level_step() {
    let mut samples = four_notes(SynthSettings {
        cutoff_hz: 400.0,
        ..sine()
    });
    let loudest = samples
        .iter()
        .enumerate()
        .max_by(|a, b| a.1.abs().total_cmp(&b.1.abs()))
        .unwrap()
        .0;
    for s in &mut samples[loudest + 1..] {
        *s *= 0.5;
    }
    assert!(max_jump(&samples).0 > 2.0 * lfo_click_limit());
}

/// Every new LFO control turned while four sine notes hold, faster than
/// they can glide: the shape, the rate, Sync and its note length, Retrigger,
/// Vibrato, Width and the filter's LFO amount, each on, changed and off.
/// Nothing clicks.
#[test]
fn no_clicks_when_the_lfo_changes_while_notes_sound() {
    let base = SynthSettings {
        cutoff_hz: 400.0,
        ..sine()
    };
    let mut renderer = renderer(base, 128);
    for key in 0..4 {
        renderer
            .controller
            .note_on(0, NoteKey(key), 45, 127)
            .unwrap();
    }
    renderer.render_seconds(0.1);
    // (shape, rate, sync, synced seconds, retrigger, vibrato, width, filter)
    let changes = [
        (LfoShape::Sine, 5.0, false, 0.5, false, 1.0, 0.2, 2.0),
        (LfoShape::Square, 20.0, false, 0.5, true, 2.0, 0.45, -3.0),
        (
            LfoShape::SampleAndHold,
            0.05,
            true,
            0.125,
            true,
            0.5,
            0.0,
            3.0,
        ),
        (LfoShape::Saw, 8.0, true, 1.5, false, 0.0, 0.3, -2.0),
        (LfoShape::Triangle, 2.0, false, 0.2, false, 2.0, 0.1, 0.0),
        (LfoShape::Square, 12.0, true, 0.05, true, 0.3, 0.45, 3.0),
        (LfoShape::Sine, 5.0, false, 0.5, false, 0.0, 0.0, 0.0),
    ];
    for _ in 0..3 {
        for &(shape, rate, sync, synced, retrigger, vibrato, width, filter) in &changes {
            renderer
                .controller
                .set_synth_settings(
                    0,
                    SynthSettings {
                        lfo_shape: shape,
                        lfo_rate_hz: rate,
                        lfo_sync: sync,
                        lfo_synced_seconds: synced,
                        lfo_retrigger: retrigger,
                        lfo_vibrato_semitones: vibrato,
                        lfo_width: width,
                        filter_lfo_octaves: filter,
                        ..base
                    },
                )
                .unwrap();
            renderer.render(2_000);
        }
    }
    renderer.render_seconds(0.1);
    assert_no_click(renderer.samples(), lfo_click_limit(), "the LFO turning");
}

/// While its three amounts are zero the LFO moves nothing, whatever its
/// shape, rate, Sync or Retrigger: the audio is the default synth's, sample
/// for sample.
#[test]
fn the_lfo_moves_nothing_while_its_amounts_are_zero() {
    let render = |settings| {
        let mut renderer = renderer(settings, 128);
        for (key, pitch) in [(1, 45), (2, 52), (3, 57)] {
            renderer
                .controller
                .note_on(0, NoteKey(key), pitch, 100)
                .unwrap();
            renderer.render(2_345);
        }
        renderer.render_seconds(0.5);
        renderer.into_samples()
    };
    let default = render(SynthSettings::default());
    for shape in LfoShape::ALL {
        let moved = render(SynthSettings {
            lfo_shape: shape,
            lfo_rate_hz: 17.0,
            lfo_sync: shape == LfoShape::Saw,
            lfo_synced_seconds: 0.1,
            lfo_retrigger: true,
            ..SynthSettings::default()
        });
        assert_eq!(moved, default, "{shape:?}");
    }
}

/// The loop playing with every LFO destination on and a synced
/// sample-and-hold the voices share, going round the loop twice, with the
/// shape, Retrigger and Sync changed along the way.
fn lfo_session(block_size: usize) -> Vec<f32> {
    let notes = (0..8)
        .map(|i| note(i, 45 + 5 * i as u8, i as Ticks * 600, 1_500))
        .collect();
    let project = project(
        120.0,
        1,
        &[
            SynthParam::Osc1Wave(uta_core::Waveform::Pulse),
            SynthParam::Osc1Width(0.3),
            SynthParam::CutoffHz(900.0),
            SynthParam::LfoShape(uta_core::LfoShape::SampleAndHold),
            SynthParam::LfoSync(true),
            SynthParam::LfoDivision(NoteDivision::from_value("1/16t").unwrap()),
            SynthParam::LfoVibratoSemitones(0.7),
            SynthParam::LfoWidth(0.3),
            SynthParam::FilterLfoOctaves(2.5),
        ],
        notes,
    );
    let mut renderer = Renderer::new(config(), Snapshot::from(&project), block_size);
    renderer.controller.locate(700).unwrap();
    renderer.controller.play().unwrap();
    renderer.render(1024 * 40);
    let mut settings = match renderer.controller.snapshot().tracks()[0].sound {
        uta_engine::TrackSound::Synth(settings) => settings,
        uta_engine::TrackSound::Drums(_) => unreachable!("a synth track"),
    };
    for (shape, retrigger, sync) in [
        (LfoShape::Square, true, true),
        (LfoShape::Triangle, false, false),
        (LfoShape::SampleAndHold, true, false),
    ] {
        settings.lfo_shape = shape;
        settings.lfo_retrigger = retrigger;
        settings.lfo_sync = sync;
        renderer.controller.set_synth_settings(0, settings).unwrap();
        renderer.render(1024 * 30);
    }
    renderer.into_samples()
}

/// Rendering twice gives identical audio, and so do blocks of 32, 128 and
/// 1024, with the synced LFO's phase taken from the song and the
/// sample-and-hold's levels from each voice's seed.
#[test]
fn the_lfo_renders_the_same_twice_and_in_any_block_size() {
    let reference = lfo_session(1024);
    assert!(peak(&reference) > 0.05, "the session made no sound");
    assert_eq!(lfo_session(1024), reference, "rendered twice");
    for block_size in [32, 128] {
        let difference = max_difference(&lfo_session(block_size), &reference);
        assert!(
            difference < 1e-6,
            "blocks of {block_size} differ from 1024 by {difference}"
        );
    }
}
