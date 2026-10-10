//! The 808 clap, measured from its sound. See RFC-006, "How we'll verify
//! it".
//!
//! Every threshold here was calibrated once against the reference behaviour
//! (the research's 808 recipe: three bursts 10 ms apart, a fourth, and a
//! tail) and says why it is what it is.

mod common;

use common::*;
use uta_core::time::{TICKS_PER_QUARTER, Ticks};
use uta_core::{Command, DrumParam, DrumSound};
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

fn clap(param: DrumParam) -> (DrumSound, DrumParam) {
    (DrumSound::Clap, param)
}

/// One clap at the top of a bar, with these settings, at this velocity.
fn one_clap(params: &[(DrumSound, DrumParam)], velocity: u8, seconds: f64) -> Vec<f32> {
    one_hit(CLAP, params, velocity, seconds)
}

// The bursts.

/// The clap's first `seconds`, as the average of 16 hits: each sample is the
/// RMS of that sample across them. Each hit catches the kit's noise at a
/// different point, and noise through a narrow band-pass has so few
/// independent swings in a couple of milliseconds that one hit's level
/// wavers by several dB from the noise alone; averaging 16 steadies it to
/// about a dB, so the envelope underneath shows.
fn average_clap(params: &[(DrumSound, DrumParam)], velocity: u8, seconds: f64) -> Vec<f32> {
    const HITS: usize = 16;
    let hits = (0..HITS)
        .map(|i| hit(i as u128, CLAP, velocity, i as u64 * BEAT))
        .collect();
    // A hit every beat at 120 BPM: each has died away before the next.
    let samples = render_drums(&drum_project(120.0, 4, params, hits), 8.0, 128);
    let length = self::seconds(seconds);
    (0..length)
        .map(|n| {
            let power: f64 = (0..HITS)
                .map(|i| f64::from(samples[i * self::seconds(0.5) + n]).powi(2))
                .sum();
            (power / HITS as f64).sqrt() as f32
        })
        .collect()
}

/// Several people clapping, not quite together: three or four bursts about
/// 10 ms apart in the first 40 ms. Between bursts the level falls about 10
/// dB (measured), so each stands out by more than 6 dB from the 5 ms either
/// side of it, and the tail swells too smoothly to make a burst of its own.
/// Measured: four bursts, loudest about 1 ms after they start at 0, 10, 20
/// and 30 ms, at every Tone and Decay and every velocity from 64 up. 8 to 12
/// ms apart allows for what's left of the noise moving each one's loudest
/// point.
#[test]
fn the_clap_is_three_or_four_bursts_10_ms_apart() {
    let cases: [(&str, Vec<(DrumSound, DrumParam)>); 5] = [
        ("defaults", vec![]),
        ("Tone 700", vec![clap(DrumParam::Tone(700.0))]),
        ("Tone 2000", vec![clap(DrumParam::Tone(2000.0))]),
        ("Decay 0.05", vec![clap(DrumParam::DecaySeconds(0.05))]),
        ("Decay 0.4", vec![clap(DrumParam::DecaySeconds(0.4))]),
    ];
    for (what, params) in cases {
        for velocity in [64, 100, 127] {
            let samples = average_clap(&params, velocity, 0.1);
            let bursts: Vec<f64> = burst_peaks(&samples, RATE, 6.0)
                .into_iter()
                .filter(|&time| time < 0.04)
                .collect();
            let what = format!("{what}, velocity {velocity}: bursts at {bursts:?}");
            assert!((3..=4).contains(&bursts.len()), "{what}");
            assert!(bursts[0] < 0.004, "{what}");
            for pair in bursts.windows(2) {
                let gap = pair[1] - pair[0];
                assert!((0.008..=0.012).contains(&gap), "{what}");
            }
        }
    }
}

/// Guards the burst count: a single burst of noise, the clap's first 10 ms
/// alone, is one burst, not three, and the tail alone, with no bursts, is
/// none.
#[test]
fn one_burst_is_not_a_clap() {
    let samples = average_clap(&[], 100, 0.1);
    let mut first = samples.clone();
    first[seconds(0.009)..].fill(0.0);
    assert_eq!(burst_peaks(&first, RATE, 6.0).len(), 1);
    let tail = &samples[seconds(0.045)..];
    assert_eq!(burst_peaks(tail, RATE, 6.0), Vec::<f64>::new());
}

// Decay: the tail.

/// Decay sets how long the tail rings. Measured from 50 ms in, after the
/// bursts, to 40 dB under the tail's loudest point there, it comes out a
/// little short of the setting, as the tail has already begun to fall by
/// then: 0.08 s at 0.1 and 0.37 s at 0.4. So within 20% of it from 0.1 s up
/// (at 0.05 s, the first 50 ms are most of it), and doubling Decay doubles
/// it, within 20%.
#[test]
fn decay_lengthens_the_tail() {
    let measured: Vec<f64> = [0.05, 0.1, 0.2, 0.4]
        .into_iter()
        .map(|decay| {
            let samples = one_clap(&[clap(DrumParam::DecaySeconds(decay))], 100, 1.0);
            let tail = &samples[seconds(0.05)..];
            let measured = decay_seconds(tail, RATE, 40.0).expect("it dies away");
            let decay = f64::from(decay);
            if decay >= 0.1 {
                assert!(
                    (measured - decay).abs() / decay < 0.2,
                    "Decay {decay}: measured {measured:.3} s"
                );
            }
            measured
        })
        .collect();
    assert!(
        measured
            .windows(2)
            .all(|pair| (1.6..2.4).contains(&(pair[1] / pair[0]))),
        "{measured:?}"
    );
}

// Brightness.

/// Tone moves the band-pass, and the clap's spectral centroid follows it up
/// at every step. The centroid sits far above the band-pass itself, in the
/// kHz range: a band-pass's skirts fall slowly, and there are many more
/// hertz above 1 kHz than below. Measured: from 3.9 kHz at 700 Hz to 5.2 kHz
/// at 2 kHz, at least 5% a step.
#[test]
fn tone_brightens_the_clap() {
    let centroids: Vec<f64> = [700.0, 1000.0, 1400.0, 2000.0]
        .into_iter()
        .map(|tone| spectral_centroid(&one_clap(&[clap(DrumParam::Tone(tone))], 100, 1.0), RATE))
        .collect();
    assert!(
        centroids.windows(2).all(|pair| pair[1] > pair[0] * 1.05),
        "{centroids:?}"
    );
}

/// Velocity sets the strength of the bursts, and the tone follows: a harder
/// clap's band-pass sits higher, so a full accent is brighter than a soft
/// hit, not only louder. Measured: the energy above 2 kHz is 23% of it at
/// 127 against 17% at 64, and the centroid 4.4 kHz against 4.2 kHz (the
/// skirts, which don't move, weigh the centroid down).
#[test]
fn a_harder_hit_is_brighter_not_just_louder() {
    let soft = one_clap(&[], 64, 1.0);
    let hard = one_clap(&[], 127, 1.0);
    let (soft_share, hard_share) = (
        share_above(&soft, RATE, 2000.0),
        share_above(&hard, RATE, 2000.0),
    );
    assert!(
        hard_share > soft_share * 1.2,
        "127: {hard_share:.3}, 64: {soft_share:.3}"
    );
    let (soft_centroid, hard_centroid) = (
        spectral_centroid(&soft, RATE),
        spectral_centroid(&hard, RATE),
    );
    assert!(
        hard_centroid > soft_centroid * 1.03,
        "127: {hard_centroid:.0} Hz, 64: {soft_centroid:.0} Hz"
    );
}

/// Velocity 100 is an unaccented hit, and 127 a full accent, through the
/// kit's shared curve: the accent peaks at the kit's reference level, give
/// or take the noise, and the unaccented hit 9 to 14 dB under it (10.5 dB
/// from the strength, and its band-pass sits lower, where it passes a little
/// less of the noise).
#[test]
fn velocity_100_is_unaccented_and_127_a_full_accent() {
    let accent = peak(&one_clap(&[], 127, 0.5));
    let unaccented = peak(&one_clap(&[], 100, 0.5));
    assert!((accent - REFERENCE_PEAK).abs() < 0.08, "{accent}");
    let below = 20.0 * (accent / unaccented).log10();
    assert!((9.0..14.0).contains(&below), "{below:.1} dB");
}

// No clicks.

/// A clap is noise through a band-pass, and noise is nothing but steps: at
/// its centre a band-passed noise can swing from one peak most of the way to
/// the other in a sample. So its steepest step is the noise's: measured, up
/// to half its peak (at Tone 2 kHz). This catches only steps bigger than the
/// noise's own: a burst whose edge isn't smoothed doesn't step further than
/// the noise does.
const HIT_STEP_LIMIT: f32 = 0.7;

#[test]
fn a_hit_does_not_click() {
    let cases: [(&str, Vec<(DrumSound, DrumParam)>); 5] = [
        ("defaults", vec![]),
        ("Tone 700", vec![clap(DrumParam::Tone(700.0))]),
        ("Tone 2000", vec![clap(DrumParam::Tone(2000.0))]),
        ("Decay 0.05", vec![clap(DrumParam::DecaySeconds(0.05))]),
        ("Level +6 dB", vec![clap(DrumParam::LevelDb(6.0))]),
    ];
    for (what, params) in cases {
        for velocity in [1, 64, 100, 127] {
            let samples = one_clap(&params, velocity, 0.5);
            let limit = peak(&samples) * HIT_STEP_LIMIT;
            assert_no_click(&samples, limit, &format!("{what}, velocity {velocity}"));
        }
    }
}

/// The limit for anything that happens while a clap rings: the steepest step
/// a lone accent with the same settings makes, with room for the noise to
/// meet steeper runs of itself over a bar of hits than in one: measured, up
/// to 1.25 times a lone hit's.
fn ringing_limit(lone_hit_step: f32) -> f32 {
    lone_hit_step * 2.0
}

/// The steepest step of a lone accent with these settings.
fn lone_step(params: &[(DrumSound, DrumParam)]) -> f32 {
    max_jump(&one_clap(params, 127, 1.0)).0
}

/// Fast repeats, at the longest Decay so the tails pile up: 16ths, then
/// 32nds, then a roll of 64ths at 120 BPM, faster than the bursts. A hit
/// charges the ringing clap and starts its own bursts, so none clicks.
#[test]
fn fast_repeats_do_not_click() {
    let params = [clap(DrumParam::DecaySeconds(0.4))];
    let mut hits = Vec::new();
    for (bar, step) in [BEAT / 4, BEAT / 8, BEAT / 16].into_iter().enumerate() {
        for t in (0..4 * BEAT).step_by(step as usize) {
            let velocity = if hits.len() % 2 == 0 { 127 } else { 100 };
            hits.push(hit(
                hits.len() as u128,
                CLAP,
                velocity,
                bar as u64 * 4 * BEAT + t,
            ));
        }
    }
    let samples = render_drums(&drum_project(120.0, 3, &params, hits), 6.5, 128);
    assert_no_click(&samples, ringing_limit(lone_step(&params)), "fast repeats");
    // The envelopes charge rather than add, so a roll can't pile up without
    // end. It does peak above a lone accent (0.5): the tail is held at full
    // charge under every burst, where a lone hit's first burst has none.
    // Measured 0.83, so it stays clear of full scale.
    assert!(peak(&samples) < 0.95, "{}", peak(&samples));
}

/// Flams: a soft grace hit, then an accent, 40 ms down to 1 ms apart.
#[test]
fn flams_do_not_click() {
    let mut hits = Vec::new();
    for (i, gap) in [77, 38, 19, 10, 2].into_iter().enumerate() {
        let start = (i as u64 + 1) * 2 * BEAT;
        hits.push(hit(2 * i as u128, CLAP, 64, start - gap));
        hits.push(hit(2 * i as u128 + 1, CLAP, 127, start));
    }
    let samples = render_drums(&drum_project(120.0, 3, &[], hits), 6.0, 128);
    assert_no_click(&samples, ringing_limit(lone_step(&[])), "flams");
}

/// Every control swept between its limits, faster than it can glide, while
/// accents ring. They all glide, so nothing clicks.
#[test]
fn turning_the_controls_while_it_rings_does_not_click() {
    let sweeps: [(&str, [DrumParam; 2]); 3] = [
        ("Tone", [DrumParam::Tone(2000.0), DrumParam::Tone(700.0)]),
        (
            "Decay",
            [DrumParam::DecaySeconds(0.05), DrumParam::DecaySeconds(0.4)],
        ),
        (
            "Level",
            [DrumParam::LevelDb(6.0), DrumParam::LevelDb(-60.0)],
        ),
    ];
    for (what, [one, other]) in sweeps {
        let hits = (0..8).map(|i| hit(i, CLAP, 127, i as u64 * BEAT)).collect();
        let mut project = drum_project(120.0, 2, &[], hits);
        let mut renderer = Renderer::new(config(), Snapshot::from(&project), 128);
        renderer.controller.play().unwrap();
        renderer.render(seconds(0.05));
        for turn in 0..40 {
            let param = if turn % 2 == 0 { one } else { other };
            project
                .apply(&Command::SetDrumParam {
                    track: drum_track(),
                    sound: DrumSound::Clap,
                    param,
                })
                .unwrap();
            renderer.controller.set_project(&project).unwrap();
            renderer.render(seconds(0.09));
        }
        let samples = renderer.samples();
        // The steepest a lone hit steps anywhere in the sweep: at the top
        // of Tone, and Level +6 dB doubles it.
        let loudest = match what {
            "Tone" => lone_step(&[clap(DrumParam::Tone(2000.0))]),
            "Level" => 2.0 * lone_step(&[]),
            _ => lone_step(&[clap(DrumParam::DecaySeconds(0.4))]),
        };
        assert_no_click(samples, ringing_limit(loudest), what);
    }
}

// The kit's noise.

/// The snare and the clap take their noise from the kit's one noise source,
/// as on the 909: it runs a sample at a time whatever is playing, so hit
/// together, they sound exactly as each does alone, added. Neither takes
/// noise from the other, and both hear the same noise at the same moment
/// (the 909's "phasing" when they play together).
#[test]
fn the_snare_and_clap_together_are_each_alone_added() {
    let render = |pitches: &[u8]| {
        let hits = pitches
            .iter()
            .enumerate()
            .map(|(i, &pitch)| hit(i as u128, pitch, 127, BEAT))
            .collect();
        render_drums(&drum_project(120.0, 1, &[], hits), 1.5, 128)
    };
    let (snare, clap, both) = (render(&[SNARE]), render(&[CLAP]), render(&[SNARE, CLAP]));
    let added: Vec<f32> = snare.iter().zip(&clap).map(|(a, b)| a + b).collect();
    assert!(peak(&both) > 0.5);
    assert!(max_difference(&both, &added) < 1e-6);
}

/// Once it has died away, a clap stops: a render of one hit ends in exact
/// silence, well within the time the kit says it rings for.
#[test]
fn a_clap_dies_away_to_silence() {
    let samples = one_clap(&[clap(DrumParam::DecaySeconds(0.4))], 127, 3.0);
    let last = samples.iter().rposition(|&s| s != 0.0).unwrap();
    assert!(
        last < seconds(1.5),
        "still sounding at {} s",
        last as f64 / 48_000.0
    );
}
