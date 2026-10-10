//! The 808 low and high toms, measured from their sound. See RFC-006, "How
//! we'll verify it".
//!
//! Every threshold here was calibrated once against the reference behaviour
//! (the research's 808 tom recipe, on the kick's resonator from Plaits'
//! model of the 808) and says why it is what it is.

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

/// One tom: its sound, its note, its Tune range and its default Tune.
#[derive(Clone, Copy)]
struct Tom {
    sound: DrumSound,
    pitch: u8,
    tunes: [f32; 2],
    default_tune: f32,
}

const LOW: Tom = Tom {
    sound: DrumSound::LowTom,
    pitch: LOW_TOM,
    tunes: [80.0, 100.0],
    default_tune: 90.0,
};

const HIGH: Tom = Tom {
    sound: DrumSound::HighTom,
    pitch: HIGH_TOM,
    tunes: [165.0, 220.0],
    default_tune: 185.0,
};

const TOMS: [Tom; 2] = [LOW, HIGH];

impl Tom {
    fn set(self, param: DrumParam) -> (DrumSound, DrumParam) {
        (self.sound, param)
    }

    /// One hit at the top of a bar, with these settings, at this velocity.
    fn one(self, params: &[(DrumSound, DrumParam)], velocity: u8, seconds: f64) -> Vec<f32> {
        one_hit(self.pitch, params, velocity, seconds)
    }

    /// Its Tune range, and two points in it.
    fn tunes(self) -> [f32; 4] {
        let [low, high] = self.tunes;
        [
            low,
            low + (high - low) / 3.0,
            low + 2.0 * (high - low) / 3.0,
            high,
        ]
    }
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

/// `samples` through two one-pole low-passes at `cutoff_hz`: the body
/// without the skin's noise, which sits higher and would add zero crossings
/// of its own once the body has died down a little. A low-pass changes a
/// tone's phase, never its frequency.
fn without_skin(samples: &[f32], cutoff_hz: f64) -> Vec<f32> {
    let g = 1.0 - (-2.0 * std::f64::consts::PI * cutoff_hz / f64::from(RATE)).exp();
    let (mut a, mut b) = (0.0f64, 0.0f64);
    samples
        .iter()
        .map(|&s| {
            a += g * (f64::from(s) - a);
            b += g * (a - b);
            b as f32
        })
        .collect()
}

// Pitch.

/// Each tom ends on its Tune, and starts a little higher: the bend.
///
/// - **It ends on Tune:** measured from 150 to 250 ms, after the bend, with
///   the skin's noise filtered out (see [`without_skin`]). Measured within
///   0.25% of it. 1.5% is a quarter of a semitone.
/// - **It starts higher, slightly:** the first cycle of a full accent is
///   18 to 24% sharp (measured), against the SC-808's 25%: the recipe's
///   "less like a boing, and more like a tonk". So more than 8% and less
///   than 30%.
/// - **It falls back in about 100 ms:** from 100 to 150 ms it's within 2%
///   of Tune (measured, under 1.1% at any velocity).
#[test]
fn each_tom_ends_on_its_tune_and_starts_a_little_higher() {
    for tom in TOMS {
        for tune in tom.tunes() {
            let samples = tom.one(
                &[
                    tom.set(DrumParam::TuneHz(tune)),
                    tom.set(DrumParam::DecaySeconds(0.6)),
                ],
                127,
                0.5,
            );
            let tune = f64::from(tune);
            let what = format!("{:?} Tune {tune}", tom.sound);
            let ended = pitch_between(&without_skin(&samples, 1.5 * tune), 0.15, 0.25);
            assert!(
                (ended - tune).abs() / tune < 0.015,
                "{what}: ended at {ended:.2} Hz"
            );
            let first = pitch_over_time(&samples, RATE)[0].1 / tune;
            assert!(
                (1.08..1.3).contains(&first),
                "{what}: started {first:.3} times Tune"
            );
            let back = pitch_between(&samples, 0.1, 0.15);
            assert!(
                (back - tune).abs() / tune < 0.02,
                "{what}: {back:.2} Hz at 100 ms"
            );
        }
    }
}

/// Raising Tune raises the pitch at every step, by about as much as Tune.
#[test]
fn raising_tune_raises_the_pitch() {
    for tom in TOMS {
        let tunes = tom.tunes();
        let pitches: Vec<f64> = tunes
            .into_iter()
            .map(|tune| {
                let samples = tom.one(
                    &[
                        tom.set(DrumParam::TuneHz(tune)),
                        tom.set(DrumParam::DecaySeconds(0.6)),
                    ],
                    100,
                    0.4,
                );
                pitch_between(&without_skin(&samples, 1.5 * f64::from(tune)), 0.15, 0.25)
            })
            .collect();
        for (pair, tunes) in pitches.windows(2).zip(tunes.windows(2)) {
            let (raised, by) = (pair[1] / pair[0], f64::from(tunes[1] / tunes[0]));
            assert!(
                raised > 1.0 && (raised - by).abs() < 0.01,
                "{:?}: {pitches:?}",
                tom.sound
            );
        }
    }
}

// Decay.

/// Decay is the time to die away by 40 dB. Measured from the loudest point,
/// it comes out within 9% of the setting: up to 9% short on the low tom
/// (the loudest point is in the attack, a little over the ring), up to 5%
/// long on the high tom (the skin dies away slower than the body). So 12%.
/// Doubling it roughly doubles the measured decay: within 15% of twice.
#[test]
fn doubling_decay_roughly_doubles_it() {
    for tom in TOMS {
        let measured = |decay: f32| {
            let samples = tom.one(&[tom.set(DrumParam::DecaySeconds(decay))], 100, 2.0);
            let measured = decay_seconds(&samples, RATE, 40.0).expect("it dies away");
            let error = (measured - f64::from(decay)).abs() / f64::from(decay);
            assert!(
                error < 0.12,
                "{:?} Decay {decay}: measured {measured:.3} s",
                tom.sound
            );
            measured
        };
        for decay in [0.1, 0.15, 0.2, 0.3] {
            let ratio = measured(decay * 2.0) / measured(decay);
            assert!(
                (1.7..2.3).contains(&ratio),
                "{:?} Decay {decay} doubled: ratio {ratio:.2}",
                tom.sound
            );
        }
    }
}

/// The defaults are the research's: the low tom rings for about 200 ms and
/// the high tom for about 100 ms.
#[test]
fn the_low_tom_rings_longer_than_the_high_tom() {
    let low = decay_seconds(&LOW.one(&[], 100, 1.0), RATE, 40.0).unwrap();
    let high = decay_seconds(&HIGH.one(&[], 100, 1.0), RATE, 40.0).unwrap();
    assert!((low / high - 2.0).abs() < 0.3, "{low:.3} s and {high:.3} s");
}

// Brightness and velocity.

/// Velocity sets the strength of the hit, and the tone follows: a full
/// accent is brighter than a soft hit, not only louder, because its pulse
/// has sharper edges, it bends further, and it hits the skin's noise
/// harder. The centroid doesn't depend on level, so a louder copy of the
/// soft hit would measure the same. Measured: 495 Hz against 418 Hz on the
/// low tom, and 894 Hz against 654 Hz on the high tom, so at least 10%.
#[test]
fn a_harder_hit_is_brighter_not_just_louder() {
    for tom in TOMS {
        let soft = tom.one(&[], 64, 1.0);
        let hard = tom.one(&[], 127, 1.0);
        let (soft_centroid, hard_centroid) = (
            spectral_centroid(&soft, RATE),
            spectral_centroid(&hard, RATE),
        );
        assert!(
            hard_centroid > soft_centroid * 1.1,
            "{:?}: 127 {hard_centroid:.0} Hz, 64 {soft_centroid:.0} Hz",
            tom.sound
        );
        assert!(peak(&hard) > peak(&soft) * 4.0);
        // And the accent bends further.
        let first = |samples: &[f32]| pitch_over_time(samples, RATE)[0].1;
        assert!(first(&hard) > first(&soft) * 1.04, "{:?}", tom.sound);
    }
}

/// Velocity 100 is an unaccented hit, and 127 a full accent, through the
/// kit's shared curve: the accent peaks at the kit's reference level, and
/// the unaccented hit 9 to 12 dB under it (measured 10.9 dB: the 808's
/// accent range, 4 V to 14 V, is 11 dB of pulse).
#[test]
fn velocity_100_is_unaccented_and_127_a_full_accent() {
    for tom in TOMS {
        let accent = peak(&tom.one(&[], 127, 0.5));
        let unaccented = peak(&tom.one(&[], 100, 0.5));
        assert!(
            (accent - REFERENCE_PEAK).abs() < 0.01,
            "{:?}: {accent}",
            tom.sound
        );
        let below = 20.0 * (accent / unaccented).log10();
        assert!(
            (9.0..12.0).contains(&below),
            "{:?}: {below:.1} dB",
            tom.sound
        );
    }
}

// No clicks.

/// A click is a step the sound couldn't make otherwise. A tom's steepest
/// step is its attack, which the pulse's smoothed edges and the low-pass
/// after the resonator keep to a curve: measured, at most 8% of its peak
/// (the high tom at the top of its Tune). A hit that started at its peak,
/// or a restart cutting a ring off, would step by its whole level.
const HIT_STEP_LIMIT: f32 = 0.12;

/// The limit for anything that happens while a tom rings: the steepest step
/// a lone hit with the same settings makes, plus the ring's own steepest
/// step at `level` (a sine at the bend's top, 1.25 times Tune), with 10% to
/// spare.
fn ringing_limit(lone_hit_step: f32, tune_hz: f32, level: f32) -> f32 {
    (lone_hit_step + sine_max_step(f64::from(tune_hz) * 1.25, RATE) * level) * 1.1
}

#[test]
fn a_hit_does_not_click() {
    for tom in TOMS {
        let [low, high] = tom.tunes;
        let cases: [(&str, Vec<(DrumSound, DrumParam)>); 5] = [
            ("defaults", vec![]),
            ("lowest Tune", vec![tom.set(DrumParam::TuneHz(low))]),
            ("highest Tune", vec![tom.set(DrumParam::TuneHz(high))]),
            ("Decay 0.1", vec![tom.set(DrumParam::DecaySeconds(0.1))]),
            ("Level +6 dB", vec![tom.set(DrumParam::LevelDb(6.0))]),
        ];
        for (what, params) in cases {
            for velocity in [1, 64, 100, 127] {
                let samples = tom.one(&params, velocity, 0.5);
                let limit = peak(&samples) * HIT_STEP_LIMIT;
                let what = format!("{:?} {what}, velocity {velocity}", tom.sound);
                assert_no_click(&samples, limit, &what);
            }
        }
    }
}

/// Guards the click limits: a ring cut off part-way, as a tom that
/// restarted its resonator would, fails them.
#[test]
fn the_click_limits_catch_a_restart() {
    for tom in TOMS {
        let mut samples = tom.one(&[], 100, 0.5);
        let lone = max_jump(&samples).0;
        let ring = seconds(0.02)..seconds(0.04);
        let at = ring.start
            + samples[ring]
                .iter()
                .enumerate()
                .max_by(|a, b| a.1.abs().total_cmp(&b.1.abs()))
                .unwrap()
                .0;
        let level = peak(&samples);
        samples[at + 1..].fill(0.0);
        assert!(max_jump(&samples).0 > peak(&samples) * HIT_STEP_LIMIT);
        assert!(max_jump(&samples).0 > ringing_limit(lone, tom.default_tune, level));
    }
}

/// The steepest step of a lone hit with these settings.
fn lone_step(tom: Tom, params: &[(DrumSound, DrumParam)], velocity: u8) -> f32 {
    max_jump(&tom.one(params, velocity, 1.0)).0
}

/// Fast repeats of the same tom, at the longest Decay so they pile up:
/// 16ths, then 32nds, then a roll of 64ths at 120 BPM. Each hit adds to the
/// ring, so none clicks. (A roll of full accents piles up past full scale,
/// as an 808 would overload, so these are ordinary hits.)
#[test]
fn fast_repeats_do_not_click() {
    for tom in TOMS {
        let params = [tom.set(DrumParam::DecaySeconds(0.6))];
        let mut hits = Vec::new();
        for (bar, step) in [BEAT / 4, BEAT / 8, BEAT / 16].into_iter().enumerate() {
            for t in (0..4 * BEAT).step_by(step as usize) {
                hits.push(hit(
                    hits.len() as u128,
                    tom.pitch,
                    100,
                    bar as u64 * 4 * BEAT + t,
                ));
            }
        }
        let samples = render_drums(&drum_project(120.0, 3, &params, hits), 6.5, 128);
        let limit = ringing_limit(
            lone_step(tom, &params, 100),
            tom.default_tune,
            peak(&samples),
        );
        assert_no_click(&samples, limit, &format!("{:?} fast repeats", tom.sound));
        assert!(peak(&samples) < 0.9, "{:?}: {}", tom.sound, peak(&samples));
    }
}

/// Flams: a soft grace hit, then an accent, 40 ms down to 1 ms apart.
#[test]
fn flams_do_not_click() {
    for tom in TOMS {
        let mut hits = Vec::new();
        // 1 tick is about 0.52 ms at 120 BPM.
        for (i, gap) in [77, 38, 19, 10, 2].into_iter().enumerate() {
            let start = (i as u64 + 1) * 2 * BEAT;
            hits.push(hit(2 * i as u128, tom.pitch, 64, start - gap));
            hits.push(hit(2 * i as u128 + 1, tom.pitch, 127, start));
        }
        let samples = render_drums(&drum_project(120.0, 3, &[], hits), 6.0, 128);
        let limit = ringing_limit(lone_step(tom, &[], 127), tom.default_tune, peak(&samples));
        assert_no_click(&samples, limit, &format!("{:?} flams", tom.sound));
    }
}

/// Every control swept between its limits, faster than it can glide, while
/// a long tom rings. They all glide, so nothing clicks.
#[test]
fn turning_the_controls_while_it_rings_does_not_click() {
    for tom in TOMS {
        let [low, high] = tom.tunes;
        let sweeps: [(&str, [DrumParam; 2]); 3] = [
            ("Tune", [DrumParam::TuneHz(high), DrumParam::TuneHz(low)]),
            (
                "Decay",
                [DrumParam::DecaySeconds(0.1), DrumParam::DecaySeconds(0.6)],
            ),
            (
                "Level",
                [DrumParam::LevelDb(6.0), DrumParam::LevelDb(-60.0)],
            ),
        ];
        let base = [tom.set(DrumParam::DecaySeconds(0.6))];
        let step = lone_step(tom, &base, 127);
        for (what, [one, other]) in sweeps {
            // Hits every beat, so the turns land on hits as well as rings.
            let hits = (0..8)
                .map(|i| hit(i, tom.pitch, 127, i as u64 * BEAT))
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
                        sound: tom.sound,
                        param,
                    })
                    .unwrap();
                renderer.controller.set_project(&project).unwrap();
                renderer.render(seconds(0.09));
            }
            let samples = renderer.samples();
            // Level +6 dB doubles everything, and the top of Tune is the
            // highest ring.
            let limit = ringing_limit(step * 2.0, high, peak(samples));
            assert_no_click(samples, limit, &format!("{:?} {what}", tom.sound));
        }
    }
}

/// Every control glides in when it's turned while the tom rings, rather
/// than jumping: compared with the same render without the turn, the turn
/// makes little of its difference in the first tenth of the 20 ms glide.
/// The skin's noise is in both renders, so a jump in Level, which could
/// hide inside the noise's steps, still shows here.
#[test]
fn every_control_glides_in() {
    let glide = seconds(uta_engine::DRUM_SMOOTHING_SECONDS);
    // 32 ms in, while it rings.
    let at = 12 * 128;
    for tom in TOMS {
        let base = [tom.set(DrumParam::DecaySeconds(0.6))];
        for (what, param) in [
            ("Tune", DrumParam::TuneHz(tom.tunes[1])),
            ("Decay", DrumParam::DecaySeconds(0.1)),
            ("Level", DrumParam::LevelDb(6.0)),
        ] {
            let difference = turn_difference(tom.pitch, &base, tom.set(param), at, 0.3);
            assert_glides(&difference, glide, &format!("{:?} {what}", tom.sound));
        }
    }
}

// Determinism.

/// A tom fill over a beat: both toms, accents and soft hits, a roll, a flam,
/// and the kick and snare under them.
fn beat() -> Project {
    let pattern = [
        (KICK, 0, 127),
        (SNARE, BEAT, 100),
        (KICK, 2 * BEAT, 100),
        (HIGH_TOM, 2 * BEAT + BEAT / 4, 90),
        (HIGH_TOM, 2 * BEAT + BEAT / 2, 127),
        (HIGH_TOM, 2 * BEAT + 3 * BEAT / 4, 70),
        (LOW_TOM, 3 * BEAT - 19, 64),
        (LOW_TOM, 3 * BEAT, 127),
        (LOW_TOM, 3 * BEAT + BEAT / 4, 100),
        (LOW_TOM, 3 * BEAT + BEAT / 2, 110),
        (HIGH_TOM, 3 * BEAT + 3 * BEAT / 4, 127),
    ];
    let hits = pattern
        .into_iter()
        .enumerate()
        .map(|(i, (pitch, start, velocity))| hit(i as u128, pitch, velocity, start))
        .collect();
    drum_project(110.0, 1, &[LOW.set(DrumParam::DecaySeconds(0.4))], hits)
}

/// Rendering twice gives identical audio, and so do blocks of 32, 128 and
/// 1024: every hit lands on its exact sample, and the toms run a sample at a
/// time.
#[test]
fn renders_are_identical_twice_and_at_any_block_size() {
    let project = beat();
    let reference = render_drums(&project, 4.0, 128);
    assert!(peak(&reference) > 0.3);
    assert_eq!(
        render_drums(&project, 4.0, 128),
        reference,
        "rendered twice"
    );
    for block_size in [32, 1024, 1000] {
        assert_eq!(
            render_drums(&project, 4.0, block_size),
            reference,
            "blocks of {block_size}"
        );
    }
}

/// Playing from the top again, once the kit has gone quiet, sounds exactly
/// as the first time did: the skin's noise restarts on Play.
#[test]
fn playing_again_sounds_the_same() {
    let project = beat();
    let mut renderer = Renderer::new(config(), Snapshot::from(&project), 128);
    renderer.controller.play().unwrap();
    renderer.render_seconds(2.5);
    renderer.controller.stop().unwrap();
    renderer.render_seconds(3.0);
    let first_end = renderer.samples().len();
    renderer.controller.play().unwrap();
    renderer.render_seconds(2.5);
    let samples = renderer.samples();
    assert_eq!(&samples[first_end..], &samples[..first_end - seconds(3.0)]);
}

/// Once it has died away far below hearing, a tom stops: a render of one
/// hit at the longest Decay ends in exact silence. Its skin, the slowest
/// part, is about 140 dB down after 3.5 times its decay, 1.3 times the
/// body's: 2.7 s. And its last sound was far below hearing, so it doesn't
/// click as it stops.
#[test]
fn a_tom_dies_away_to_silence() {
    for tom in TOMS {
        let samples = tom.one(&[tom.set(DrumParam::DecaySeconds(0.6))], 127, 4.0);
        let last = samples.iter().rposition(|&s| s != 0.0).unwrap();
        assert!(
            last < seconds(3.5),
            "{:?} still sounding at {} s",
            tom.sound,
            last as f64 / 48_000.0
        );
        let tail = peak(&samples[last - seconds(0.1)..=last]);
        assert!(tail < 1e-6, "{:?}: last 0.1 s peaked at {tail}", tom.sound);
    }
}
