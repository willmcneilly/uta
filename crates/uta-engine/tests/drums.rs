//! The drum machine, measured from its sound: the 808 kick so far. See
//! RFC-006, "How we'll verify it".
//!
//! Every threshold here was calibrated once against the reference behaviour
//! (Werner's analysis of the 808 kick, and Plaits' model of it) and says
//! why it is what it is.

mod common;

use common::*;
use uta_core::time::{TICKS_PER_QUARTER, Ticks};
use uta_core::{Command, DrumParam, DrumSound, KIT, Note, Project};
use uta_engine::offline::Renderer;
use uta_engine::{EngineConfig, NoteKey, REFERENCE_PEAK, Snapshot};

const RATE: u32 = 48_000;
const BEAT: Ticks = TICKS_PER_QUARTER;

fn config() -> EngineConfig {
    EngineConfig {
        sample_rate: RATE,
        channels: 1,
    }
}

fn seconds(samples: f64) -> usize {
    (samples * f64::from(RATE)) as usize
}

/// Plays `project` from the top for `seconds`, in blocks of `block_size`.
fn render(project: &Project, seconds: f64, block_size: usize) -> Vec<f32> {
    let mut renderer = Renderer::new(config(), Snapshot::from(project), block_size);
    renderer.controller.play().unwrap();
    renderer.render_seconds(seconds);
    renderer.into_samples()
}

fn kick(param: DrumParam) -> (DrumSound, DrumParam) {
    (DrumSound::Kick, param)
}

/// One kick at the top of a bar at 60 BPM, so nothing else plays for 4 s,
/// with these settings, at this velocity, rendered for `seconds`.
fn one_kick(params: &[(DrumSound, DrumParam)], velocity: u8, seconds: f64) -> Vec<f32> {
    let project = drum_project(60.0, 1, params, vec![hit(0, KICK, velocity, 0)]);
    render(&project, seconds, 128)
}

/// The average pitch over `from..to` seconds.
fn pitch_between(samples: &[f32], from: f64, to: f64) -> f64 {
    let cycles: Vec<f64> = pitch_over_time(samples, RATE)
        .into_iter()
        .filter(|&(time, _)| (from..to).contains(&time))
        .map(|(_, hz)| hz)
        .collect();
    assert!(!cycles.is_empty(), "no cycles between {from} and {to} s");
    cycles.iter().sum::<f64>() / cycles.len() as f64
}

// Pitch.

/// The kick settles on its Tune, and starts higher: the attack shift, then
/// the pitch sigh. Measured, it settles 0.2% flat: Plaits' leakage diode
/// bends each cycle a little, flattening it. 1% is a sixth of a semitone,
/// too little to hear on a kick. The first whole cycle, just after the attack
/// shift, is more than 10% sharp: Werner measures the sigh at about 56 Hz
/// for 49 (14%) on a hard hit.
#[test]
fn the_kick_settles_on_its_tune_and_starts_higher() {
    for tune in [40.0, 49.0, 62.0, 80.0] {
        let samples = one_kick(
            &[
                kick(DrumParam::TuneHz(tune)),
                kick(DrumParam::DecaySeconds(0.8)),
            ],
            100,
            1.0,
        );
        let tune = f64::from(tune);
        let settled = pitch_between(&samples, 0.5, 0.9);
        assert!(
            (settled - tune).abs() / tune < 0.01,
            "Tune {tune}: settled at {settled:.2} Hz"
        );
        let first = pitch_over_time(&samples, RATE)[0].1;
        assert!(first > tune * 1.1, "Tune {tune}: started at {first:.2} Hz");
        // The sigh: still sharp 50 ms in, then falling towards Tune.
        let early = pitch_between(&samples, 0.03, 0.07);
        let later = pitch_between(&samples, 0.15, 0.25);
        assert!(
            early > later && later > settled,
            "{early} {later} {settled}"
        );
    }
}

#[test]
fn raising_tune_raises_the_pitch() {
    let pitches: Vec<f64> = [40.0, 45.0, 50.0, 60.0, 70.0, 80.0]
        .into_iter()
        .map(|tune| {
            let samples = one_kick(&[kick(DrumParam::TuneHz(tune))], 100, 0.6);
            pitch_between(&samples, 0.2, 0.5)
        })
        .collect();
    assert!(
        pitches.windows(2).all(|pair| pair[1] > pair[0] * 1.05),
        "{pitches:?}"
    );
}

// Decay.

/// Decay is the time to die away by 40 dB. Measured from the loudest point,
/// which is in the attack, it comes out a few percent short of the setting
/// (up to 12% at 50 ms, where the attack is a bigger share of it), so 15%.
/// Doubling it roughly doubles the measured decay: within 15% of twice.
#[test]
fn doubling_decay_roughly_doubles_it() {
    let measured = |decay: f32| {
        let samples = one_kick(&[kick(DrumParam::DecaySeconds(decay))], 100, 2.0);
        let measured = decay_seconds(&samples, RATE, 40.0).expect("it dies away");
        let error = (measured - f64::from(decay)).abs() / f64::from(decay);
        assert!(error < 0.15, "Decay {decay}: measured {measured:.3} s");
        measured
    };
    for decay in [0.05, 0.1, 0.2, 0.4] {
        let ratio = measured(decay * 2.0) / measured(decay);
        assert!(
            (1.7..2.3).contains(&ratio),
            "Decay {decay} doubled: ratio {ratio:.2}"
        );
    }
}

// Brightness.

/// A kick's energy is in its fundamental, 40 to 80 Hz, and the click lifts
/// the average a little. At its defaults it's around 160 Hz for an ordinary
/// hit and 190 Hz for a full accent: under 200 Hz either way. Tone moves it
/// up, from about 145 Hz at 0 to about 900 Hz at 1.
#[test]
fn the_kick_is_low_and_tone_brightens_it() {
    for velocity in [100, 127] {
        let centroid = spectral_centroid(&one_kick(&[], velocity, 1.0), RATE);
        assert!(
            centroid < 200.0,
            "velocity {velocity}: centroid {centroid:.0} Hz"
        );
    }
    let centroids: Vec<f64> = [0.0, 0.25, 0.5, 0.75, 1.0]
        .into_iter()
        .map(|tone| spectral_centroid(&one_kick(&[kick(DrumParam::Tone(tone))], 127, 1.0), RATE))
        .collect();
    assert!(
        centroids.windows(2).all(|pair| pair[1] > pair[0] * 1.1),
        "{centroids:?}"
    );
    assert!(centroids[4] > centroids[0] * 4.0, "{centroids:?}");
}

/// Velocity sets the strength of the hit, and the tone follows: a full
/// accent is brighter than a soft hit, not only louder. The centroid
/// doesn't depend on level, so a louder copy of the soft hit would measure
/// the same. Measured: about 190 Hz against 155 Hz, so at least 10% apart.
#[test]
fn a_harder_hit_is_brighter_not_just_louder() {
    let soft = one_kick(&[], 64, 1.0);
    let hard = one_kick(&[], 127, 1.0);
    let (soft_centroid, hard_centroid) = (
        spectral_centroid(&soft, RATE),
        spectral_centroid(&hard, RATE),
    );
    assert!(
        hard_centroid > soft_centroid * 1.1,
        "127: {hard_centroid:.0} Hz, 64: {soft_centroid:.0} Hz"
    );
    assert!(peak(&hard) > peak(&soft) * 4.0);
    // And the accent is punchier: its attack starts higher.
    let first = |samples: &[f32]| pitch_over_time(samples, RATE)[0].1;
    assert!(first(&hard) > first(&soft) * 1.05);
}

/// Velocity 100 is an unaccented hit, and 127 a full accent, through the
/// shared curve: the accent peaks at the kit's reference level, and the
/// unaccented hit 7 to 10 dB under it (the 808's accent range, 4 V to 14 V,
/// is 11 dB of pulse, and the resonator's ring doesn't grow quite as fast).
#[test]
fn velocity_100_is_unaccented_and_127_a_full_accent() {
    let accent = peak(&one_kick(&[], 127, 0.5));
    let unaccented = peak(&one_kick(&[], 100, 0.5));
    assert!((accent - REFERENCE_PEAK).abs() < 0.01, "{accent}");
    let below = 20.0 * (accent / unaccented).log10();
    assert!((7.0..10.0).contains(&below), "{below:.1} dB");
}

// No clicks.

/// A click is a step the sound couldn't make otherwise. A kick's steepest
/// step is its attack, which the edge smoothing and the Tone low-pass keep
/// to a curve: measured, at most a quarter of its peak (at the brightest
/// Tone). A hit that started at its peak, or a restart cutting a ring off,
/// would step by its whole level.
const HIT_STEP_LIMIT: f32 = 0.3;

/// The limit for anything that happens while a kick rings: the steepest
/// step a lone hit with the same settings makes, plus the ring's own
/// steepest step at `level` (a sine at the attack's top pitch, 2.7 times
/// Tune), with 10% to spare.
fn ringing_limit(lone_hit_step: f32, tune_hz: f64, level: f32) -> f32 {
    (lone_hit_step + sine_max_step(tune_hz * 2.7, RATE) * level) * 1.1
}

fn assert_no_click(samples: &[f32], limit: f32, what: &str) {
    let (jump, at) = max_jump(samples);
    assert!(
        jump <= limit,
        "{what}: jump of {jump} at sample {at}, limit {limit}"
    );
}

#[test]
fn a_hit_does_not_click() {
    let cases: [(&str, Vec<(DrumSound, DrumParam)>); 7] = [
        ("defaults", vec![]),
        ("Tone 0", vec![kick(DrumParam::Tone(0.0))]),
        ("Tone 1", vec![kick(DrumParam::Tone(1.0))]),
        ("Tune 40", vec![kick(DrumParam::TuneHz(40.0))]),
        (
            "Tune 80, Tone 1",
            vec![kick(DrumParam::TuneHz(80.0)), kick(DrumParam::Tone(1.0))],
        ),
        ("Decay 0.05", vec![kick(DrumParam::DecaySeconds(0.05))]),
        ("Level +6 dB", vec![kick(DrumParam::LevelDb(6.0))]),
    ];
    for (what, params) in cases {
        for velocity in [1, 64, 100, 127] {
            let samples = one_kick(&params, velocity, 0.5);
            let limit = peak(&samples) * HIT_STEP_LIMIT;
            assert_no_click(&samples, limit, &format!("{what}, velocity {velocity}"));
        }
    }
}

/// Guards the click limits: a ring cut off part-way, as a kick that
/// restarted its resonator would, fails them.
#[test]
fn the_click_limits_catch_a_restart() {
    let mut samples = one_kick(&[], 100, 0.5);
    let lone = max_jump(&samples).0;
    // Cut the ring off at its loudest point in the second cycle.
    let second_cycle = seconds(0.02)..seconds(0.04);
    let at = second_cycle.start
        + samples[second_cycle]
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.abs().total_cmp(&b.1.abs()))
            .unwrap()
            .0;
    let level = peak(&samples);
    samples[at + 1..].fill(0.0);
    assert!(max_jump(&samples).0 > peak(&samples) * HIT_STEP_LIMIT);
    assert!(max_jump(&samples).0 > ringing_limit(lone, 49.0, level));
}

/// The steepest step of a lone hit, and the peak, with these settings.
fn lone_hit(params: &[(DrumSound, DrumParam)], velocity: u8) -> (f32, f32) {
    let samples = one_kick(params, velocity, 1.0);
    (max_jump(&samples).0, peak(&samples))
}

/// Fast repeats of the same sound, at the longest Decay so they pile up:
/// 16ths, then 32nds, then a roll of 64ths at 120 BPM. Each hit adds to the
/// ring, so none clicks. (A roll of full accents piles up past full scale
/// at a master of 0 dB, as an 808 would overload, so these are ordinary
/// hits.)
#[test]
fn fast_repeats_do_not_click() {
    let params = [kick(DrumParam::DecaySeconds(0.8))];
    let mut hits = Vec::new();
    for (bar, step) in [BEAT / 4, BEAT / 8, BEAT / 16].into_iter().enumerate() {
        for t in (0..4 * BEAT).step_by(step as usize) {
            hits.push(hit(
                hits.len() as u128,
                KICK,
                100,
                bar as u64 * 4 * BEAT + t,
            ));
        }
    }
    let samples = render(&drum_project(120.0, 3, &params, hits), 6.5, 128);
    let (step, _) = lone_hit(&params, 100);
    let limit = ringing_limit(step, 49.0, peak(&samples));
    assert_no_click(&samples, limit, "fast repeats");
    // They pile up, but stay clear of full scale.
    assert!(peak(&samples) < 0.9, "{}", peak(&samples));
}

/// Flams: a soft grace hit, then an accent, 40 ms down to 1 ms apart.
#[test]
fn flams_do_not_click() {
    let mut hits = Vec::new();
    // 1 tick is about 0.52 ms at 120 BPM.
    for (i, gap) in [77, 38, 19, 10, 2].into_iter().enumerate() {
        let start = (i as u64 + 1) * 2 * BEAT;
        hits.push(hit(2 * i as u128, KICK, 64, start - gap));
        hits.push(hit(2 * i as u128 + 1, KICK, 127, start));
    }
    let samples = render(&drum_project(120.0, 3, &[], hits), 6.0, 128);
    let (step, _) = lone_hit(&[], 127);
    let limit = ringing_limit(step, 49.0, peak(&samples));
    assert_no_click(&samples, limit, "flams");
}

/// Every control swept between its limits, faster than it can glide, while
/// a long kick rings. They all glide, so nothing clicks.
#[test]
fn turning_the_controls_while_it_rings_does_not_click() {
    let sweeps: [(&str, [DrumParam; 2]); 4] = [
        ("Tune", [DrumParam::TuneHz(80.0), DrumParam::TuneHz(40.0)]),
        ("Tone", [DrumParam::Tone(1.0), DrumParam::Tone(0.0)]),
        (
            "Decay",
            [DrumParam::DecaySeconds(0.05), DrumParam::DecaySeconds(0.8)],
        ),
        (
            "Level",
            [DrumParam::LevelDb(6.0), DrumParam::LevelDb(-60.0)],
        ),
    ];
    let base = [
        kick(DrumParam::DecaySeconds(0.8)),
        kick(DrumParam::Tone(1.0)),
    ];
    let (step, _) = lone_hit(&base, 127);
    for (what, [one, other]) in sweeps {
        // Kicks every beat, so the turns land on hits as well as rings.
        let hits = (0..8).map(|i| hit(i, KICK, 127, i as u64 * BEAT)).collect();
        let mut project = drum_project(120.0, 2, &base, hits);
        let mut renderer = Renderer::new(config(), Snapshot::from(&project), 128);
        renderer.controller.play().unwrap();
        renderer.render(seconds(0.05));
        for turn in 0..40 {
            let param = if turn % 2 == 0 { one } else { other };
            project
                .apply(&Command::SetDrumParam {
                    track: drum_track(),
                    sound: DrumSound::Kick,
                    param,
                })
                .unwrap();
            renderer.controller.set_project(&project).unwrap();
            renderer.render(seconds(0.09));
        }
        let samples = renderer.samples();
        // Level +6 dB doubles everything, and Tune 80 is the highest ring.
        let limit = ringing_limit(step * 2.0, 80.0, peak(samples));
        assert_no_click(samples, limit, what);
    }
}

// Determinism.

/// A beat with everything in it: accents, soft hits, a repeat and a flam.
fn beat() -> Project {
    let pattern = [
        (0, 127),
        (3 * BEAT / 4, 90),
        (BEAT + BEAT / 2, 100),
        (2 * BEAT - 19, 64),
        (2 * BEAT, 127),
        (2 * BEAT + BEAT / 4, 100),
        (3 * BEAT + BEAT / 4, 40),
        (3 * BEAT + BEAT / 2, 110),
    ];
    let hits = pattern
        .into_iter()
        .enumerate()
        .map(|(i, (start, velocity))| hit(i as u128, KICK, velocity, start))
        .collect();
    drum_project(110.0, 1, &[kick(DrumParam::DecaySeconds(0.5))], hits)
}

/// Rendering twice gives identical audio, and so do blocks of 32, 128 and
/// 1024: every hit lands on its exact sample, and the kick runs a sample at
/// a time.
#[test]
fn renders_are_identical_twice_and_at_any_block_size() {
    let project = beat();
    let reference = render(&project, 5.0, 128);
    assert!(peak(&reference) > 0.3);
    assert_eq!(render(&project, 5.0, 128), reference, "rendered twice");
    for block_size in [32, 1024, 1000] {
        assert_eq!(
            render(&project, 5.0, block_size),
            reference,
            "blocks of {block_size}"
        );
    }
}

/// Playing from the top again, once the kit has gone quiet, sounds exactly
/// as the first time did: the kit's free-running parts restart on Play.
#[test]
fn playing_again_sounds_the_same() {
    let project = beat();
    let mut renderer = Renderer::new(config(), Snapshot::from(&project), 128);
    renderer.controller.play().unwrap();
    renderer.render_seconds(2.0);
    renderer.controller.stop().unwrap();
    renderer.render_seconds(3.0);
    let first_end = renderer.samples().len();
    renderer.controller.play().unwrap();
    renderer.render_seconds(2.0);
    let samples = renderer.samples();
    assert_eq!(&samples[first_end..], &samples[..first_end - seconds(3.0)]);
}

// The engine.

/// Drum notes are one-shots: the end of a note does nothing, so a note a
/// sixteenth long sounds the same as one a bar long.
#[test]
fn drum_notes_are_one_shots() {
    let short = hit(0, KICK, 100, 0);
    let long = Note {
        length: 4 * BEAT,
        ..short
    };
    let render = |note| render(&drum_project(120.0, 2, &[], vec![note]), 2.0, 128);
    assert_eq!(render(short), render(long));
}

/// Drum tracks don't chase: starting playback in the middle of a kick note
/// doesn't play it late. The next hit plays on time.
#[test]
fn drum_tracks_do_not_chase() {
    let long = Note {
        length: 2 * BEAT,
        ..hit(0, KICK, 127, 0)
    };
    let next = hit(1, KICK, 127, 2 * BEAT);
    let project = drum_project(120.0, 1, &[], vec![long, next]);
    let mut renderer = Renderer::new(config(), Snapshot::from(&project), 128);
    renderer.controller.locate(BEAT).unwrap();
    renderer.controller.play().unwrap();
    renderer.render_seconds(1.0);
    let samples = renderer.samples();
    // Beat 2 to beat 3 is half a second at 120 BPM.
    let half = seconds(0.5);
    assert_eq!(peak(&samples[..half]), 0.0, "the long note was chased");
    assert_eq!(first_sound(samples), Some(half));
}

/// Every row of the kit plays a sound: a hit on each, alone, is heard.
/// Each sound has its own tests.
#[test]
fn every_row_plays_a_sound() {
    for row in KIT {
        let hits = vec![hit(0, row.pitch, 127, 0)];
        let samples = render(&drum_project(120.0, 1, &[], hits), 1.0, 128);
        assert!(peak(&samples) > 0.1, "{}: {}", row.name, peak(&samples));
    }
}

/// A live note on a drum track's slot hits its kit, whether or not the
/// transport is playing, as clicking a row's label will.
#[test]
fn a_live_note_hits_the_kit() {
    let project = drum_project(120.0, 1, &[], vec![]);
    let mut renderer = Renderer::new(config(), Snapshot::from(&project), 128);
    let slot = renderer.controller.slot(drum_track()).unwrap();
    renderer
        .controller
        .note_on(slot, NoteKey(1), KICK, 127)
        .unwrap();
    renderer.render_seconds(0.5);
    // The note is never released, and it doesn't need to be.
    let samples = renderer.samples();
    assert!((peak(samples) - REFERENCE_PEAK).abs() < 0.01);
    assert_eq!(samples, &one_kick(&[], 127, 0.5)[..]);
}

/// Every one of the 32 slots can play a kit: a project of drum tracks, each
/// hitting the kick at once, plays every one.
#[test]
fn every_slot_can_play_a_kit() {
    let mut project = drum_project(120.0, 1, &[], vec![hit(0, KICK, 100, 0)]);
    project
        .apply(&Command::RemoveTracks {
            tracks: vec![project.tracks()[0].id()],
        })
        .unwrap();
    let mut copies = Vec::new();
    for n in 1..uta_core::Project::MAX_TRACKS {
        let track = project.tracks()[0].copy(
            uta_core::TrackId::from_uuid(uuid::Uuid::from_u128(9000 + n as u128)),
            format!("Drums {}", n + 1),
            || uta_core::ClipId::from_uuid(uuid::Uuid::from_u128(10_000 + n as u128)),
            || uta_core::NoteId::from_uuid(uuid::Uuid::from_u128(11_000 + n as u128)),
        );
        copies.push(uta_core::PlacedTrack { index: n, track });
    }
    project
        .apply(&Command::AddTracks { tracks: copies })
        .unwrap();
    assert_eq!(project.tracks().len(), 32);
    let mut renderer = Renderer::new(config(), Snapshot::from(&project), 128);
    renderer.controller.play().unwrap();
    renderer.render_seconds(0.1);
    let status = renderer.take_status();
    assert!(
        status.track_peaks.iter().all(|&peak| peak > 0.1),
        "{:?}",
        status.track_peaks
    );
}

/// A slot changing kind of track while it sounds. With every slot taken, a
/// track added in place of a removed one takes its slot.
///
/// Synth to drums: a held sine's track is replaced by a drum track, which
/// hits at once. The sine fades out quickly under the kick, as a synth note
/// does when its slot is handed on, and nothing clicks.
#[test]
fn a_synth_slot_handed_to_a_drum_track_does_not_click() {
    let project = with_every_slot_taken(drum_project(120.0, 1, &[], vec![]));
    let synth = project.tracks()[0].id();
    let mut renderer = Renderer::new(config(), Snapshot::from(&project), 128);
    let slot = renderer.controller.slot(synth).unwrap();
    let mut sine = with_every_slot_taken(drum_project(120.0, 1, &[], vec![]));
    sine = replace_track(&sine, synth, empty_sine_track(60));
    let sine_id = sine.tracks()[0].id();
    renderer.controller.set_project(&sine).unwrap();
    // The sine takes the synth's slot, and holds A2.
    assert_eq!(renderer.controller.slot(sine_id), Some(slot));
    renderer
        .controller
        .note_on(slot, NoteKey(1), 45, 127)
        .unwrap();
    // A quarter cycle past a whole number of them, so the handover lands at
    // the sine's peak, where cutting it off would step by its whole level.
    renderer.render(seconds(0.3) + seconds(0.25 / 110.0));
    let handover = renderer.samples().len();
    let held = peak(&renderer.samples()[seconds(0.2)..]);
    assert!(held > 0.1, "the sine isn't sounding: {held}");

    let drums = replace_track(&sine, sine_id, empty_drum_track(61));
    renderer.controller.set_project(&drums).unwrap();
    let drums_id = drums.tracks()[0].id();
    assert_eq!(renderer.controller.slot(drums_id), Some(slot));
    renderer
        .controller
        .note_on(slot, NoteKey(2), KICK, 127)
        .unwrap();
    renderer.render_seconds(1.0);

    let samples = renderer.samples();
    let after = &samples[handover..];
    // The sine is gone after its 5 ms fade: from then on it's exactly a
    // lone kick.
    let alone = one_kick(&[], 127, 1.0);
    let faded = seconds(0.01);
    assert_eq!(&after[faded..], &alone[faded..after.len()]);
    assert!(peak(&after[..faded]) > 0.1);
    let (step, _) = lone_hit(&[], 127);
    // The sine's own steepest step, and its fade's, are on top of the
    // kick's: 0.25 is the synth's level at full velocity.
    let sine_step = (sine_max_step(110.0, RATE) + 1.0 / 240.0) * 0.25;
    let limit = ringing_limit(step + sine_step, 49.0, peak(samples));
    assert_no_click(samples, limit, "synth to drums");
}

/// Drums to synth: a ringing kick's track is replaced by a synth track,
/// which plays a note at once. The kick rings on through the new track's
/// gains, as a removed track's sound does, and nothing clicks.
#[test]
fn a_drum_slot_handed_to_a_synth_track_does_not_click() {
    let params = [kick(DrumParam::DecaySeconds(0.8))];
    let project =
        with_every_slot_taken(drum_project(120.0, 1, &params, vec![hit(0, KICK, 127, 0)]));
    let mut renderer = Renderer::new(config(), Snapshot::from(&project), 128);
    let slot = renderer.controller.slot(drum_track()).unwrap();
    renderer.controller.play().unwrap();
    renderer.render_seconds(0.1);

    let synth = replace_track(&project, drum_track(), empty_sine_track(62));
    renderer.controller.set_project(&synth).unwrap();
    let synth_id = synth.tracks()[1].id();
    assert_eq!(renderer.controller.slot(synth_id), Some(slot));
    renderer
        .controller
        .note_on(slot, NoteKey(3), 45, 127)
        .unwrap();
    renderer.render_seconds(1.0);

    let samples = renderer.samples();
    // The kick carried on ringing: a lone kick is still well above
    // silence 0.2 s in.
    let alone = one_kick(&params, 127, 1.1);
    assert!(peak(&alone[seconds(0.2)..seconds(0.25)]) > 0.1);
    let (step, _) = lone_hit(&params, 127);
    let sine_step = (sine_max_step(110.0, RATE) + 1.0 / 240.0) * 0.25;
    let limit = ringing_limit(step + sine_step, 49.0, peak(samples));
    assert_no_click(samples, limit, "drums to synth");
    // Both sound together after the handover.
    let after = &samples[seconds(0.15)..seconds(0.2)];
    let kick_only = &alone[seconds(0.15)..seconds(0.2)];
    assert!(
        max_difference(after, kick_only) > 0.05,
        "the synth is silent"
    );
    assert!(peak(after) > 0.1);
}
