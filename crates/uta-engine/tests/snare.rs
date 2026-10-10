//! The 909 snare, measured from its sound. See RFC-006, "How we'll verify
//! it".
//!
//! Every threshold here was calibrated once against the reference behaviour
//! (Plaits' model of the 909 snare, and the research's 909 recipe) and says
//! why it is what it is.

mod common;

use common::*;
use uta_core::time::{TICKS_PER_QUARTER, Ticks};
use uta_core::{Command, DrumParam, DrumSound, Project};
use uta_engine::offline::Renderer;
use uta_engine::{EngineConfig, REFERENCE_PEAK, Snapshot};

const RATE: u32 = 48_000;
const BEAT: Ticks = TICKS_PER_QUARTER;

fn config() -> EngineConfig {
    EngineConfig {
        sample_rate: RATE,
        channels: 1,
    }
}

fn seconds(seconds: f64) -> usize {
    (seconds * f64::from(RATE)) as usize
}

fn snare(param: DrumParam) -> (DrumSound, DrumParam) {
    (DrumSound::Snare, param)
}

/// One snare at the top of a bar, with these settings, at this velocity.
fn one_snare(params: &[(DrumSound, DrumParam)], velocity: u8, seconds: f64) -> Vec<f32> {
    one_hit(SNARE, params, velocity, seconds)
}

/// The shell alone: Snappy 0 has no wires at all.
const SHELL_ONLY: (DrumSound, DrumParam) = (DrumSound::Snare, DrumParam::Snappy(0.0));
/// The wires alone: Snappy 1 has no shell at all.
const WIRES_ONLY: (DrumSound, DrumParam) = (DrumSound::Snare, DrumParam::Snappy(1.0));

// Pitch.

/// The shell's strongest partial is its lower tone, at Tune. Measured after
/// the bump, it's about 1% sharp: the 909's coupling, where each
/// oscillator's wrap moves with the others', shortens its cycles a little.
/// 3% is half a semitone. Raising Tune raises it every time.
#[test]
fn the_shell_is_at_its_tune_and_raising_tune_raises_it() {
    let pitches: Vec<f64> = [140.0, 160.0, 180.0, 220.0, 260.0]
        .into_iter()
        .map(|tune| {
            let samples = one_snare(&[snare(DrumParam::TuneHz(tune)), SHELL_ONLY], 100, 0.5);
            let pitch = strongest_frequency(&samples[seconds(0.02)..], RATE, 100.0, 300.0);
            let tune = f64::from(tune);
            assert!(
                (pitch - tune).abs() / tune < 0.03,
                "Tune {tune}: the shell is at {pitch:.1} Hz"
            );
            pitch
        })
        .collect();
    assert!(
        pitches.windows(2).all(|pair| pair[1] > pair[0] * 1.1),
        "{pitches:?}"
    );
}

// Snappy.

/// Snappy sets how much of the snare is wires: their share of the energy
/// above 1 kHz, where the shell has next to nothing (its low-pass is at
/// three times Tune). Measured: none at Snappy 0, about a third at 0.5 and
/// 85% at 1, rising at every step.
#[test]
fn snappy_raises_the_wires_share() {
    let shares: Vec<f64> = [0.0, 0.25, 0.5, 0.75, 1.0]
        .into_iter()
        .map(|snappy| {
            let samples = one_snare(&[snare(DrumParam::Snappy(snappy))], 100, 1.0);
            share_above(&samples, RATE, 1000.0)
        })
        .collect();
    assert!(shares[0] < 0.01, "{shares:?}");
    assert!(shares[4] > 0.8, "{shares:?}");
    assert!(
        shares.windows(2).all(|pair| pair[1] > pair[0] * 1.05),
        "{shares:?}"
    );
}

// Tone: the wires' length.

/// The wires hold at their level before they decay, as the 909's do: from
/// just after the crack (the first 7 ms) to the end of the shortest hold, 40
/// ms, their level moves less than 3 dB. Then they fall: half of Tone after
/// the hold ends (40 ms at the shortest Tone, 70 ms at the longest), they're
/// about 20 dB down, so more than 12.
#[test]
fn the_wires_hold_before_they_decay() {
    for (tone, hold) in [(0.04f32, 0.04), (0.4, 0.07)] {
        let samples = one_snare(&[snare(DrumParam::Tone(tone)), WIRES_ONLY], 100, 0.5);
        let envelope = rms_envelope_db(&samples, RATE, 0.005);
        let level = |from: f64, to: f64| {
            envelope
                .iter()
                .filter(|(time, _)| (from..to).contains(time))
                .map(|&(_, db)| db)
                .collect::<Vec<_>>()
        };
        let held = level(0.015, 0.04);
        let (low, high) = held.iter().fold((f64::MAX, f64::MIN), |(lo, hi), &db| {
            (lo.min(db), hi.max(db))
        });
        assert!(
            high - low < 3.0,
            "Tone {tone}: held between {low:.1} and {high:.1} dB"
        );
        let after = hold + f64::from(tone) / 2.0;
        let later = level(after, after + 0.005)[0];
        assert!(
            later < low - 12.0,
            "Tone {tone}: still at {later:.1} dB after {after} s"
        );
    }
}

/// Tone sets the wires' length: the measured time for the wires to fall 40
/// dB from their loudest point grows with it at every step. It comes out
/// longer than Tone at the short end (the hold, 40 to 70 ms, comes first)
/// and about equal at the long end (the crack at the start is a few dB over
/// the hold, so 40 dB from it comes sooner): measured 72 ms at Tone 40 ms
/// and 400 ms at 400 ms, so within 20% of Tone plus the hold, less the
/// crack's head start.
#[test]
fn tone_lengthens_the_wires() {
    let measured: Vec<f64> = [0.04, 0.08, 0.16, 0.25, 0.4]
        .into_iter()
        .map(|tone| {
            let samples = one_snare(&[snare(DrumParam::Tone(tone)), WIRES_ONLY], 100, 1.5);
            let measured = decay_seconds(&samples, RATE, 40.0).expect("the wires die away");
            let tone = f64::from(tone);
            assert!(
                (tone * 0.9..tone + 0.07).contains(&measured),
                "Tone {tone}: measured {measured:.3} s"
            );
            measured
        })
        .collect();
    assert!(
        measured.windows(2).all(|pair| pair[1] > pair[0] * 1.2),
        "{measured:?}"
    );
}

// Brightness.

/// Tone moves the snare's brightness the right way: longer wires are more
/// of the sound, so its spectral centroid rises at every step. The shell is
/// a short low thump and the wires are 2 to 6 kHz, so the centroid is in
/// the kHz range throughout: measured from 4.7 kHz at Tone 40 ms to 5.1 kHz
/// at 400 ms.
#[test]
fn tone_brightens_the_snare() {
    let centroids: Vec<f64> = [0.04, 0.08, 0.16, 0.25, 0.4]
        .into_iter()
        .map(|tone| spectral_centroid(&one_snare(&[snare(DrumParam::Tone(tone))], 100, 1.0), RATE))
        .collect();
    assert!(
        centroids.windows(2).all(|pair| pair[1] > pair[0]),
        "{centroids:?}"
    );
    assert!(centroids[4] > centroids[0] * 1.05, "{centroids:?}");
}

/// Velocity sets the strength of the hit, and the tone follows: a full
/// accent is brighter than a soft hit, not only louder, because velocity
/// tilts the wires against the shell, as the 909's accent does. The
/// centroid doesn't depend on level, so a louder copy of the soft hit would
/// measure the same. Measured: the wires are 52% of the energy above 1 kHz
/// at 127 against 29% at 64, and the centroid is 5.2 kHz against 4.8 kHz.
#[test]
fn a_harder_hit_is_brighter_not_just_louder() {
    let soft = one_snare(&[], 64, 1.0);
    let hard = one_snare(&[], 127, 1.0);
    let (soft_share, hard_share) = (
        share_above(&soft, RATE, 1000.0),
        share_above(&hard, RATE, 1000.0),
    );
    assert!(
        hard_share > soft_share * 1.5,
        "127: {hard_share:.3}, 64: {soft_share:.3}"
    );
    let (soft_centroid, hard_centroid) = (
        spectral_centroid(&soft, RATE),
        spectral_centroid(&hard, RATE),
    );
    assert!(
        hard_centroid > soft_centroid * 1.05,
        "127: {hard_centroid:.0} Hz, 64: {soft_centroid:.0} Hz"
    );
}

/// Velocity 100 is an unaccented hit, and 127 a full accent, through the
/// kit's shared curve: the accent peaks at the kit's reference level, give
/// or take the noise (which is somewhere else in its run each time), and the
/// unaccented hit 8 to 12 dB under it (the 909's accent is about 11 dB, as
/// the 808's 4 V to 14 V is, and the wires tilt down a little more).
#[test]
fn velocity_100_is_unaccented_and_127_a_full_accent() {
    let accent = peak(&one_snare(&[], 127, 0.5));
    let unaccented = peak(&one_snare(&[], 100, 0.5));
    assert!((accent - REFERENCE_PEAK).abs() < 0.05, "{accent}");
    let below = 20.0 * (accent / unaccented).log10();
    assert!((8.0..12.0).contains(&below), "{below:.1} dB");
}

// No clicks.

/// A click is a step the sound couldn't make otherwise. The shell's
/// steepest step is a curve, smoothed by its low-pass: measured, at most 6%
/// of its peak. A shell that started at its peak, or a ring cut off, would
/// step by its whole level.
const SHELL_STEP_LIMIT: f32 = 0.1;
/// Noise is nothing but steps: at the top of its band it can swing from one
/// peak to the other in a sample or two. So with the wires in, the snare's
/// steepest step is the noise's: measured, up to 1.2 times its peak (on the
/// softest hits, where the crack's noise is most of the sound). This
/// catches only steps bigger than the noise's own; the shell's limit is the
/// one that catches a click in the drum.
const WIRES_STEP_LIMIT: f32 = 1.5;

#[test]
fn a_hit_does_not_click() {
    type Case = (&'static str, Vec<(DrumSound, DrumParam)>, f32);
    let cases: [Case; 7] = [
        ("shell", vec![SHELL_ONLY], SHELL_STEP_LIMIT),
        (
            "shell, Tune 260",
            vec![SHELL_ONLY, snare(DrumParam::TuneHz(260.0))],
            SHELL_STEP_LIMIT,
        ),
        (
            "shell, Tune 140",
            vec![SHELL_ONLY, snare(DrumParam::TuneHz(140.0))],
            SHELL_STEP_LIMIT,
        ),
        ("defaults", vec![], WIRES_STEP_LIMIT),
        ("wires", vec![WIRES_ONLY], WIRES_STEP_LIMIT),
        (
            "wires, Tune 260",
            vec![WIRES_ONLY, snare(DrumParam::TuneHz(260.0))],
            WIRES_STEP_LIMIT,
        ),
        (
            "Level +6 dB",
            vec![snare(DrumParam::LevelDb(6.0))],
            WIRES_STEP_LIMIT,
        ),
    ];
    for (what, params, limit) in cases {
        for velocity in [1, 64, 100, 127] {
            let samples = one_snare(&params, velocity, 0.5);
            let limit = peak(&samples) * limit;
            assert_no_click(&samples, limit, &format!("{what}, velocity {velocity}"));
        }
    }
}

/// The limit for anything that happens while a snare rings: the steepest
/// step a lone hit with the same settings makes, with room for a hit to land
/// anywhere in the shell's cycle (a lone hit always starts at the same
/// point): measured, never more than a lone hit's, so a quarter to spare.
/// With the wires in, the noise meets steeper runs of itself over a bar of
/// hits than in one: measured, up to 1.6 times a lone hit's.
fn ringing_limit(lone_hit_step: f32, wires: bool) -> f32 {
    lone_hit_step * if wires { 2.0 } else { 1.25 }
}

/// Guards the click limits: a shell cut off at its loudest, as a snare that
/// restarted would cut its ring, fails them.
#[test]
fn the_click_limits_catch_a_restart() {
    let mut samples = one_snare(&[SHELL_ONLY], 127, 0.5);
    let lone = max_jump(&samples).0;
    let ring = seconds(0.01)..seconds(0.03);
    let at = ring.start
        + samples[ring]
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.abs().total_cmp(&b.1.abs()))
            .unwrap()
            .0;
    samples[at + 1..].fill(0.0);
    assert!(max_jump(&samples).0 > peak(&samples) * SHELL_STEP_LIMIT);
    assert!(max_jump(&samples).0 > ringing_limit(lone, false));
}

/// The steepest step of a lone accent with these settings.
fn lone_step(params: &[(DrumSound, DrumParam)]) -> f32 {
    max_jump(&one_snare(params, 127, 1.0)).0
}

/// Fast repeats, at the longest Tone so the wires pile up: 16ths, then
/// 32nds, then a roll of 64ths at 120 BPM. A hit charges the ringing snare
/// rather than restarting it, so none clicks.
#[test]
fn fast_repeats_do_not_click() {
    for (what, params, wires) in [
        ("shell", vec![SHELL_ONLY], false),
        ("defaults", vec![snare(DrumParam::Tone(0.4))], true),
    ] {
        let mut hits = Vec::new();
        for (bar, step) in [BEAT / 4, BEAT / 8, BEAT / 16].into_iter().enumerate() {
            for t in (0..4 * BEAT).step_by(step as usize) {
                let velocity = if hits.len() % 2 == 0 { 127 } else { 100 };
                hits.push(hit(
                    hits.len() as u128,
                    SNARE,
                    velocity,
                    bar as u64 * 4 * BEAT + t,
                ));
            }
        }
        let samples = render_drums(&drum_project(120.0, 3, &params, hits), 6.5, 128);
        assert_no_click(&samples, ringing_limit(lone_step(&params), wires), what);
        // A roll doesn't pile up: an accent peaks at 0.5, and over a bar of
        // hits the noise reaches higher runs of itself, measured up to 0.7.
        assert!(peak(&samples) < 0.85, "{what}: {}", peak(&samples));
    }
}

/// Flams: a soft grace hit, then an accent, 40 ms down to 1 ms apart.
#[test]
fn flams_do_not_click() {
    for (what, params, wires) in [
        ("shell", vec![SHELL_ONLY], false),
        ("defaults", vec![], true),
    ] {
        let mut hits = Vec::new();
        // 1 tick is about 0.52 ms at 120 BPM.
        for (i, gap) in [77, 38, 19, 10, 2].into_iter().enumerate() {
            let start = (i as u64 + 1) * 2 * BEAT;
            hits.push(hit(2 * i as u128, SNARE, 64, start - gap));
            hits.push(hit(2 * i as u128 + 1, SNARE, 127, start));
        }
        let samples = render_drums(&drum_project(120.0, 3, &params, hits), 6.0, 128);
        assert_no_click(&samples, ringing_limit(lone_step(&params), wires), what);
    }
}

/// Every control swept between its limits, faster than it can glide, while
/// accents ring. They all glide, so nothing clicks. The shell alone shows a
/// click best, so each control but Snappy is turned on the shell; Snappy
/// moves between all shell and all wires.
#[test]
fn turning_the_controls_while_it_rings_does_not_click() {
    let sweeps: [(&str, [DrumParam; 2]); 4] = [
        ("Tune", [DrumParam::TuneHz(260.0), DrumParam::TuneHz(140.0)]),
        ("Tone", [DrumParam::Tone(0.04), DrumParam::Tone(0.4)]),
        ("Snappy", [DrumParam::Snappy(1.0), DrumParam::Snappy(0.0)]),
        (
            "Level",
            [DrumParam::LevelDb(6.0), DrumParam::LevelDb(-60.0)],
        ),
    ];
    for (what, [one, other]) in sweeps {
        let base = if what == "Snappy" {
            vec![]
        } else {
            vec![SHELL_ONLY]
        };
        let hits = (0..8)
            .map(|i| hit(i, SNARE, 127, i as u64 * BEAT))
            .collect();
        let mut project = drum_project(120.0, 2, &base, hits);
        let mut renderer = Renderer::new(config(), Snapshot::from(&project), 128);
        renderer.controller.play().unwrap();
        renderer.render(seconds(0.05));
        for turn in 0..40 {
            let param = if turn % 2 == 0 { one } else { other };
            project
                .apply(&Command::SetDrumParam {
                    track: drum_track(),
                    sound: DrumSound::Snare,
                    param,
                })
                .unwrap();
            renderer.controller.set_project(&project).unwrap();
            renderer.render(seconds(0.09));
        }
        let samples = renderer.samples();
        // The steepest a lone hit steps anywhere in the sweep: at the top
        // of Tune for the shell, all wires for Snappy, and Level +6 dB
        // doubles it.
        let loudest = match what {
            "Tune" => lone_step(&[SHELL_ONLY, snare(DrumParam::TuneHz(260.0))]),
            "Snappy" => lone_step(&[WIRES_ONLY]),
            "Level" => 2.0 * lone_step(&base),
            _ => lone_step(&base),
        };
        assert_no_click(samples, ringing_limit(loudest, what == "Snappy"), what);
    }
}

// The noise.

/// The wires' noise runs freely between hits, so two hits on a snare that
/// has died away, at different times, come out a little different, as on
/// the hardware. The shell starts from the same point each time, so it's
/// only the wires that differ.
#[test]
fn each_hit_catches_the_noise_somewhere_else() {
    let hits = vec![hit(0, SNARE, 127, 0), hit(1, SNARE, 127, 2 * BEAT)];
    let samples = render_drums(&drum_project(60.0, 1, &[], hits), 3.0, 128);
    let (first, second) = (
        &samples[..seconds(0.5)],
        &samples[seconds(2.0)..seconds(2.5)],
    );
    assert!(max_difference(first, second) > 0.05);
    // The same, though, in level and colour.
    let ratio = rms(first) / rms(second);
    assert!((0.9..1.1).contains(&ratio), "{ratio}");
    let hits = vec![hit(0, SNARE, 127, 0), hit(1, SNARE, 127, 2 * BEAT)];
    let shell = render_drums(&drum_project(60.0, 1, &[SHELL_ONLY], hits), 3.0, 128);
    assert_eq!(&shell[..seconds(0.5)], &shell[seconds(2.0)..seconds(2.5)]);
}

/// A beat with the snare and clap: accents, ghost notes, a flam, and both
/// together on the same noise.
fn beat() -> Project {
    let pattern = [
        (SNARE, 0, 127),
        (SNARE, 3 * BEAT / 4, 45),
        (CLAP, BEAT, 127),
        (SNARE, BEAT, 100),
        (SNARE, BEAT + BEAT / 2, 60),
        (SNARE, 2 * BEAT - 19, 64),
        (SNARE, 2 * BEAT, 127),
        (CLAP, 3 * BEAT - BEAT / 4, 90),
        (CLAP, 3 * BEAT, 127),
        (SNARE, 3 * BEAT + BEAT / 4, 40),
        (KICK, 0, 127),
        (KICK, 2 * BEAT, 100),
    ];
    let hits = pattern
        .into_iter()
        .enumerate()
        .map(|(i, (pitch, start, velocity))| hit(i as u128, pitch, velocity, start))
        .collect();
    drum_project(110.0, 1, &[snare(DrumParam::Tone(0.25))], hits)
}

// Determinism.

/// Rendering twice gives identical audio, and so do blocks of 32, 128 and
/// 1024: every hit lands on its exact sample, and the noise and the sounds
/// run a sample at a time.
#[test]
fn renders_are_identical_twice_and_at_any_block_size() {
    let project = beat();
    let reference = render_drums(&project, 5.0, 128);
    assert!(peak(&reference) > 0.3);
    assert_eq!(
        render_drums(&project, 5.0, 128),
        reference,
        "rendered twice"
    );
    for block_size in [32, 1024, 1000] {
        assert_eq!(
            render_drums(&project, 5.0, block_size),
            reference,
            "blocks of {block_size}"
        );
    }
}

/// Playing from the top again, once the kit has gone quiet, sounds exactly
/// as the first time did: the noise restarts on Play.
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

/// Once it has died away, a snare stops: a render of one hit ends in exact
/// silence, well within the time the kit says it rings for.
#[test]
fn a_snare_dies_away_to_silence() {
    let params = [snare(DrumParam::Tone(0.4)), snare(DrumParam::Snappy(0.0))];
    let samples = one_snare(&params, 127, 3.0);
    let last = samples.iter().rposition(|&s| s != 0.0).unwrap();
    assert!(
        last < seconds(2.0),
        "still sounding at {} s",
        last as f64 / 48_000.0
    );
}
