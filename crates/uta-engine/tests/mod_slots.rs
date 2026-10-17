//! The synth's three mod slots, measured from the waveform: each
//! destination moves the thing it names, the way the amount's sign says,
//! by its full scale; each source drives it as it should; an empty slot or
//! an amount of zero changes nothing; changing a slot while notes sound
//! doesn't click; and the audio is the same twice and in any block size.
//! See RFC-008, "How we'll verify it" (Mod slots), and `CLAUDE.md`,
//! "Proving audio code works".

mod common;

use common::*;
use uta_core::SynthParam;
use uta_core::time::Ticks;
use uta_engine::offline::Renderer;
use uta_engine::{
    EngineConfig, LfoShape, ModDestination, ModSlot, ModSource, NoteKey, SYNTH_SMOOTHING_SECONDS,
    Snapshot, SynthSettings, VOICE_LEVEL, Waveform,
};

const RATE: u32 = 48_000;

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

/// A sine held at full level.
fn sine() -> SynthSettings {
    SynthSettings {
        osc1_wave: Waveform::Sine,
        sustain: 1.0,
        ..SynthSettings::default()
    }
}

/// `settings` with its first slot moving `destination` by `source` times
/// `amount`.
fn routed(
    settings: SynthSettings,
    source: ModSource,
    destination: ModDestination,
    amount: f32,
) -> SynthSettings {
    let mut settings = settings;
    settings.mods[0] = ModSlot {
        source,
        destination,
        amount,
    };
    settings
}

/// Holds one note at `pitch` and `velocity` for `hold` seconds and returns
/// everything rendered.
fn held_note(settings: SynthSettings, pitch: u8, velocity: u8, hold: f64) -> Vec<f32> {
    let mut renderer = renderer(settings, 128);
    renderer
        .controller
        .note_on(0, NoteKey(1), pitch, velocity)
        .unwrap();
    renderer.render_seconds(hold);
    renderer.into_samples()
}

/// The pitch of a held note, in semitones from `hz`, over the stretch from
/// 0.1 s to 0.4 s, once the attack is over.
fn semitones_from(samples: &[f32], hz: f64) -> f64 {
    let held = &samples[seconds(0.1)..seconds(0.4)];
    12.0 * (measure_frequency(held, RATE) / hz).log2()
}

/// The level of a held note, as the RMS of 0.1 s to 0.4 s.
fn held_level(samples: &[f32]) -> f64 {
    rms(&samples[seconds(0.1)..seconds(0.4)])
}

/// The brightness of the 8,192 samples from 0.15 s, as their spectral
/// centroid in Hz, weighted by magnitude as the LFO's tests weigh it.
fn brightness(samples: &[f32]) -> f64 {
    let bins = spectrum(&samples[seconds(0.15)..seconds(0.15) + 8192]);
    let bin_hz = f64::from(RATE) / 8192.0;
    let total: f64 = bins.iter().sum();
    let weighted: f64 = bins
        .iter()
        .enumerate()
        .map(|(k, magnitude)| k as f64 * bin_hz * magnitude)
        .sum();
    weighted / total
}

/// The share of a pulse's samples above zero from 0.1 s to 0.4 s: its
/// width.
fn duty(samples: &[f32]) -> f64 {
    let held = &samples[seconds(0.1)..seconds(0.4)];
    held.iter().filter(|&&s| s > 0.0).count() as f64 / held.len() as f64
}

/// A pitch destination moves its oscillator by 24 semitones at full
/// amount, up or down with the sign, and the other oscillator not at all.
/// With the velocity at its hardest the source is 1, so an amount of a
/// half is an octave.
#[test]
fn a_pitch_destination_moves_its_oscillator_by_the_amount() {
    let osc2_only = SynthSettings {
        osc1_level: 0.0,
        osc2_level: 1.0,
        osc2_wave: Waveform::Sine,
        ..sine()
    };
    for (settings, destination, other) in [
        (sine(), ModDestination::Osc1Pitch, ModDestination::Osc2Pitch),
        (
            osc2_only,
            ModDestination::Osc2Pitch,
            ModDestination::Osc1Pitch,
        ),
    ] {
        for (amount, expected) in [(0.5, 12.0), (-0.5, -12.0), (0.25, 6.0), (0.0, 0.0)] {
            let samples = held_note(
                routed(settings, ModSource::Velocity, destination, amount),
                69,
                127,
                0.5,
            );
            let moved = semitones_from(&samples, 440.0);
            assert!(
                (moved - expected).abs() < 0.02,
                "{destination:?} at {amount}: {moved} semitones, not {expected}"
            );
        }
        let samples = held_note(
            routed(settings, ModSource::Velocity, other, 0.5),
            69,
            127,
            0.5,
        );
        let moved = semitones_from(&samples, 440.0);
        assert!(moved.abs() < 0.02, "{other:?} moved the wrong oscillator");
    }
}

/// Each mixer level destination adds its amount to its level, within the
/// fader's range: from a level of a half, up a half is full and down a half
/// is silent.
#[test]
fn a_level_destination_moves_its_level() {
    let only = |level: usize| {
        let mut settings = SynthSettings {
            osc1_wave: Waveform::Sine,
            osc2_wave: Waveform::Sine,
            osc1_level: 0.0,
            sustain: 1.0,
            ..SynthSettings::default()
        };
        match level {
            0 => settings.osc1_level = 0.5,
            1 => settings.osc2_level = 0.5,
            2 => settings.sub_level = 0.5,
            _ => settings.noise_level = 0.5,
        }
        settings
    };
    let destinations = [
        ModDestination::Osc1Level,
        ModDestination::Osc2Level,
        ModDestination::SubLevel,
        ModDestination::NoiseLevel,
    ];
    for (level, destination) in destinations.into_iter().enumerate() {
        let level_at = |amount| {
            held_level(&held_note(
                routed(only(level), ModSource::Velocity, destination, amount),
                57,
                127,
                0.5,
            ))
        };
        let (still, up, down) = (level_at(0.0), level_at(0.5), level_at(-0.5));
        assert!(still > 0.01, "{destination:?} is heard at a half");
        assert!(
            (up / still - 2.0).abs() < 0.05,
            "{destination:?} up a half: {up} against {still}"
        );
        assert!(down < 1e-4, "{destination:?} down a half: {down}");
        let past = level_at(1.0);
        assert!(
            (past / up - 1.0).abs() < 0.01,
            "{destination:?} is kept to full level: {past} against {up}"
        );
    }
}

/// The width destination moves oscillator 1's pulse width by 90% at full
/// amount, within its limits.
#[test]
fn the_width_destination_moves_the_pulse_width() {
    let pulse = SynthSettings {
        osc1_wave: Waveform::Pulse,
        osc1_width: 0.5,
        ..sine()
    };
    for (amount, expected) in [(0.2, 0.68), (-0.2, 0.32), (0.0, 0.5), (1.0, 0.95)] {
        let samples = held_note(
            routed(
                pulse,
                ModSource::Velocity,
                ModDestination::Osc1Width,
                amount,
            ),
            45,
            127,
            0.5,
        );
        let width = duty(&samples);
        assert!(
            (width - expected).abs() < 0.01,
            "at {amount}: width {width}, not {expected}"
        );
    }
}

/// The cutoff destination moves the cutoff by ten octaves at full amount:
/// up is brighter, down darker, and at zero it's where the knob is.
#[test]
fn the_cutoff_destination_moves_the_cutoff() {
    let saw = SynthSettings {
        cutoff_hz: 800.0,
        sustain: 1.0,
        ..SynthSettings::default()
    };
    let at = |amount| {
        brightness(&held_note(
            routed(saw, ModSource::Velocity, ModDestination::Cutoff, amount),
            45,
            127,
            0.5,
        ))
    };
    let (still, up, down) = (at(0.0), at(0.3), at(-0.3));
    assert!(up > 2.0 * still, "up: {up} Hz against {still} Hz");
    assert!(down < 0.5 * still, "down: {down} Hz against {still} Hz");
}

/// The resonance destination moves the resonance: up, the filter rings at
/// the cutoff, so more of the sound is near it; down from the knob, less.
#[test]
fn the_resonance_destination_moves_the_resonance() {
    let saw = SynthSettings {
        cutoff_hz: 1000.0,
        resonance: 0.5,
        sustain: 1.0,
        ..SynthSettings::default()
    };
    let near_cutoff = |amount| {
        let samples = held_note(
            routed(saw, ModSource::Velocity, ModDestination::Resonance, amount),
            45,
            127,
            0.5,
        );
        let bins = spectrum(&samples[seconds(0.15)..seconds(0.15) + 8192]);
        let bin_hz = f64::from(RATE) / 8192.0;
        let band: f64 = bins
            .iter()
            .enumerate()
            .filter(|(k, _)| (800.0..1250.0).contains(&(*k as f64 * bin_hz)))
            .map(|(_, m)| m * m)
            .sum();
        let total: f64 = bins.iter().map(|m| m * m).sum();
        band / total
    };
    let (still, up, down) = (near_cutoff(0.0), near_cutoff(0.4), near_cutoff(-0.4));
    assert!(up > 1.5 * still, "up: {up} against {still}");
    assert!(down < still / 1.5, "down: {down} against {still}");
}

/// The rate of a held note's vibrato, in Hz, from when its pitch crosses
/// the note going up.
fn vibrato_rate(samples: &[f32]) -> f64 {
    let pitch: Vec<(f64, f64)> = pitch_over_time(samples, RATE)
        .into_iter()
        .filter(|&(time, _)| time > 0.05)
        .map(|(time, hz)| (time, 12.0 * (hz / 440.0).log2()))
        .collect();
    let crossings: Vec<f64> = pitch
        .windows(2)
        .filter(|w| w[0].1 < 0.0 && w[1].1 >= 0.0)
        .map(|w| w[0].0 + (w[1].0 - w[0].0) * w[0].1 / (w[0].1 - w[1].1))
        .collect();
    (crossings.len() - 1) as f64 / (crossings.last().unwrap() - crossings.first().unwrap())
}

/// The LFO rate destination moves the LFO's rate by four octaves at full
/// amount: a quarter is an octave, so a 2 Hz vibrato goes at 4 Hz, or 1 Hz
/// below zero. It moves a note's own LFO with Retrigger on, and the one the
/// notes share with it off. A synced LFO keeps to the tempo.
#[test]
fn the_lfo_rate_destination_moves_the_lfos_rate() {
    for retrigger in [true, false] {
        let vibrato = SynthSettings {
            lfo_rate_hz: 2.0,
            lfo_vibrato_semitones: 1.0,
            lfo_retrigger: retrigger,
            ..sine()
        };
        for (amount, expected) in [(0.25, 4.0), (-0.25, 1.0), (0.0, 2.0)] {
            let samples = held_note(
                routed(
                    vibrato,
                    ModSource::Velocity,
                    ModDestination::LfoRate,
                    amount,
                ),
                69,
                127,
                3.0,
            );
            let rate = vibrato_rate(&samples);
            assert!(
                (rate / expected - 1.0).abs() < 0.01,
                "retrigger {retrigger}, at {amount}: {rate} Hz, not {expected} Hz"
            );
        }
    }
    let synced = SynthSettings {
        lfo_sync: true,
        lfo_synced_seconds: 0.5,
        lfo_vibrato_semitones: 1.0,
        lfo_retrigger: true,
        ..sine()
    };
    let samples = held_note(
        routed(synced, ModSource::Velocity, ModDestination::LfoRate, 0.25),
        69,
        127,
        3.0,
    );
    let rate = vibrato_rate(&samples);
    assert!((rate / 2.0 - 1.0).abs() < 0.01, "synced: {rate} Hz");
}

/// The volume destination moves the voice's level by its amount times its
/// level, up to twice it (+6 dB) and down to silence.
#[test]
fn the_volume_destination_moves_the_level() {
    let at = |amount| {
        held_level(&held_note(
            routed(sine(), ModSource::Velocity, ModDestination::Volume, amount),
            57,
            127,
            0.5,
        ))
    };
    let still = at(0.0);
    for (amount, ratio) in [(0.5, 1.5), (-0.5, 0.5), (1.0, 2.0), (-1.0, 0.0)] {
        let level = at(amount);
        assert!(
            (level / still - ratio).abs() < 0.01,
            "at {amount}: {level} against {still}"
        );
    }
}

/// An empty slot, or one whose amount is zero, changes nothing: the
/// render is the same, sample for sample, as with no slot in use, whatever
/// the source and destination.
#[test]
fn an_empty_slot_or_an_amount_of_zero_changes_nothing() {
    let settings = SynthSettings {
        osc1_wave: Waveform::Pulse,
        osc2_level: 0.4,
        sub_level: 0.3,
        noise_level: 0.1,
        cutoff_hz: 1500.0,
        resonance: 0.3,
        lfo_vibrato_semitones: 0.3,
        ..SynthSettings::default()
    };
    let render = |settings: SynthSettings| {
        let mut renderer = renderer(settings, 128);
        for (key, pitch) in [(1, 45), (2, 52), (3, 69)] {
            renderer
                .controller
                .note_on(0, NoteKey(key), pitch, 90)
                .unwrap();
            renderer.render(2000);
        }
        renderer.render_seconds(0.5);
        renderer.into_samples()
    };
    let reference = render(settings);
    assert!(peak(&reference) > 0.05);
    for source in ModSource::ALL {
        for destination in ModDestination::ALL {
            let zero = routed(settings, source, destination, 0.0);
            assert_eq!(
                render(zero),
                reference,
                "{source:?} to {destination:?} at 0"
            );
        }
        let off = routed(settings, source, ModDestination::Off, 1.0);
        assert_eq!(render(off), reference, "{source:?} to nothing");
    }
}

/// Each source drives the slot as it says, seen through oscillator 1's
/// pitch at full amount (24 semitones): the velocity from nothing to all of
/// it, the key from middle C, a fifth of the way per octave, the amp
/// envelope at its sustain, the filter envelope from its peak back to its
/// sustain, and the LFO either side.
#[test]
fn each_source_drives_the_slot() {
    let pitch_to_hz = |pitch: u8| 440.0 * 2f64.powf((f64::from(pitch) - 69.0) / 12.0);
    let pitch_by = |settings: SynthSettings, source, pitch: u8, velocity: u8| {
        let samples = held_note(
            routed(settings, source, ModDestination::Osc1Pitch, 1.0),
            pitch,
            velocity,
            0.5,
        );
        semitones_from(&samples, pitch_to_hz(pitch))
    };
    let near = |measured: f64, expected: f64, what: &str| {
        assert!(
            (measured - expected).abs() < 0.03,
            "{what}: {measured} semitones, not {expected}"
        );
    };
    near(
        pitch_by(sine(), ModSource::Velocity, 57, 127),
        24.0,
        "full velocity",
    );
    near(
        pitch_by(sine(), ModSource::Velocity, 57, 64),
        24.0 * 64.0 / 127.0,
        "half velocity",
    );
    near(pitch_by(sine(), ModSource::Key, 60, 127), 0.0, "middle C");
    near(
        pitch_by(sine(), ModSource::Key, 48, 127),
        -24.0 * 12.0 / 60.0,
        "an octave under middle C",
    );
    let half_sustain = SynthSettings {
        sustain: 0.5,
        decay_seconds: 0.01,
        ..sine()
    };
    near(
        pitch_by(half_sustain, ModSource::AmpEnv, 57, 127),
        12.0,
        "the amp envelope at a sustain of a half",
    );
    let filter_held = SynthSettings {
        filter_sustain: 0.25,
        filter_decay_seconds: 0.01,
        ..sine()
    };
    near(
        pitch_by(filter_held, ModSource::FilterEnv, 57, 127),
        6.0,
        "the filter envelope at a sustain of a quarter",
    );

    // The LFO: a square at 1 Hz, retriggered, so the first half of each
    // second is its top and the second its bottom.
    let square = SynthSettings {
        lfo_shape: LfoShape::Square,
        lfo_rate_hz: 1.0,
        lfo_retrigger: true,
        ..sine()
    };
    let samples = held_note(
        routed(square, ModSource::Lfo, ModDestination::Osc1Pitch, 0.25),
        57,
        127,
        1.0,
    );
    let at = |from: f64, to: f64| {
        let stretch = &samples[seconds(from)..seconds(to)];
        12.0 * (measure_frequency(stretch, RATE) / 220.0).log2()
    };
    near(at(0.1, 0.4), 6.0, "the LFO at its top");
    near(at(0.6, 0.9), -6.0, "the LFO at its bottom");
}

/// The click limit for four sines a little over 110 Hz while the slots move
/// their pitch up to two octaves and their level up to twice: the steepest
/// a sine two octaves up moves, plus the steepest its level moves gliding
/// over the synth's smoothing time, at twice the level.
fn click_limit() -> f32 {
    let smoothing = (SYNTH_SMOOTHING_SECONDS * f64::from(RATE)).round() as f32;
    let per_voice = sine_max_step(4.0 * 123.5, RATE) + 2.0 / smoothing;
    4.0 * VOICE_LEVEL * 2.0 * per_voice * 1.1
}

/// Turning each new control while notes sound doesn't click: four sines
/// held while the first slot's source, destination and amount change every
/// tenth of a second, through every destination, and the other two slots
/// sweep their amounts.
#[test]
fn turning_the_slots_while_notes_sound_stays_under_the_click_limit() {
    let base = SynthSettings {
        cutoff_hz: 3000.0,
        lfo_rate_hz: 3.0,
        ..sine()
    };
    let mut renderer = renderer(base, 128);
    for key in 0..4 {
        renderer
            .controller
            .note_on(0, NoteKey(key), 45, 100)
            .unwrap();
        renderer.render(1000 + 77 * key as usize);
    }
    let mut settings = base;
    for (turn, destination) in ModDestination::ALL.into_iter().cycle().take(24).enumerate() {
        settings.mods[0] = ModSlot {
            source: ModSource::ALL[turn % ModSource::ALL.len()],
            destination,
            amount: if turn % 2 == 0 { 0.5 } else { -0.4 },
        };
        settings.mods[1] = ModSlot {
            source: ModSource::Lfo,
            destination: ModDestination::Volume,
            amount: 0.5 * (turn % 3) as f32 / 2.0,
        };
        settings.mods[2] = ModSlot {
            source: ModSource::Key,
            destination: ModDestination::Cutoff,
            amount: -0.2 * (turn % 4) as f32 / 3.0,
        };
        renderer.controller.set_synth_settings(0, settings).unwrap();
        renderer.render_seconds(0.1);
    }
    let samples = renderer.into_samples();
    assert!(peak(&samples) > 0.05, "the notes made no sound");
    assert_no_click(&samples, click_limit(), "the mod slots turning");
}

/// The loop playing in Poly and then in Mono with every slot in use and
/// changing, going round the loop twice.
fn mod_session(block_size: usize) -> Vec<f32> {
    let notes = (0..8)
        .map(|i| note(i, 45 + 5 * i as u8, i as Ticks * 600, 1_500))
        .collect();
    let project = project(
        120.0,
        1,
        &[
            SynthParam::Osc1Wave(uta_core::Waveform::Pulse),
            SynthParam::Osc2Level(0.5),
            SynthParam::CutoffHz(900.0),
            SynthParam::FilterEnvOctaves(2.0),
            SynthParam::LfoShape(uta_core::LfoShape::SampleAndHold),
            SynthParam::Mod1Source(uta_core::ModSource::FilterEnv),
            SynthParam::Mod1Destination(uta_core::ModDestination::Osc2Pitch),
            SynthParam::Mod1Amount(0.5),
            SynthParam::Mod2Source(uta_core::ModSource::Velocity),
            SynthParam::Mod2Destination(uta_core::ModDestination::Cutoff),
            SynthParam::Mod2Amount(0.3),
            SynthParam::Mod3Source(uta_core::ModSource::Lfo),
            SynthParam::Mod3Destination(uta_core::ModDestination::Volume),
            SynthParam::Mod3Amount(0.5),
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
    for (turn, mono) in [false, true, true].into_iter().enumerate() {
        settings.voice_mode = if mono {
            uta_engine::VoiceMode::Mono
        } else {
            uta_engine::VoiceMode::Poly
        };
        settings.mods[0].destination = ModDestination::ALL[3 + turn];
        settings.mods[1].amount = -0.3;
        settings.mods[2].source = ModSource::ALL[turn];
        settings.lfo_retrigger = turn != 1;
        renderer.controller.set_synth_settings(0, settings).unwrap();
        renderer.render(1024 * 30);
    }
    renderer.into_samples()
}

/// Rendering twice gives identical audio, and so do blocks of 32, 128 and
/// 1024, with every slot in use, in Poly and Mono.
#[test]
fn the_mod_slots_render_the_same_twice_and_in_any_block_size() {
    let reference = mod_session(1024);
    assert!(peak(&reference) > 0.05, "the session made no sound");
    assert_eq!(mod_session(1024), reference, "rendered twice");
    for block_size in [32, 128] {
        let difference = max_difference(&mod_session(block_size), &reference);
        assert!(
            difference < 1e-6,
            "blocks of {block_size} differ from 1024 by {difference}"
        );
    }
}
