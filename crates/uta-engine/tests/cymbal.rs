//! The 808 cymbal, measured from its sound. See RFC-006, "How we'll verify
//! it".
//!
//! Every threshold here was calibrated once against the reference behaviour
//! (Werner's 808 cymbal and the research's recipe: the hats' metal through
//! band-passes at 3.44 and 7.1 kHz, three bands with their own envelopes and
//! clipping amplifiers, the low band longest and the high band a short
//! sizzle, and Tone mainly the high band's level) and says why it is what
//! it is.
//!
//! The metal runs freely, so each hit catches it somewhere else and comes
//! out a little different. Measurements that compare settings average
//! eight hits, so a difference is the setting's, not where the metal was.

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

fn cymbal(param: DrumParam) -> (DrumSound, DrumParam) {
    (DrumSound::Cymbal, param)
}

/// Its Tune is the closed hat's.
fn tune(hz: f32) -> (DrumSound, DrumParam) {
    (DrumSound::ClosedHat, DrumParam::TuneHz(hz))
}

/// Eight cymbals with these settings, two beats apart at 60 BPM and each a
/// few ticks later in its beat than the last, so each catches the metal
/// somewhere else: each hit's first `length` seconds.
fn eight_hits(params: &[(DrumSound, DrumParam)], velocity: u8, length: f64) -> Vec<Vec<f32>> {
    let starts: Vec<Ticks> = (0..8).map(|i| 2 * i * BEAT + (i * 37) % 100).collect();
    let hits = starts
        .iter()
        .enumerate()
        .map(|(i, &start)| hit(i as u128, CYMBAL, velocity, start))
        .collect();
    let samples = render_drums(&drum_project(60.0, 4, params, hits), 16.5, 128);
    starts
        .into_iter()
        .map(|start| {
            let at = (start as f64 / BEAT as f64 * f64::from(RATE)).round() as usize;
            samples[at..at + seconds(length)].to_vec()
        })
        .collect()
}

/// The spectral centroid of eight hits' first second, averaged.
fn centroid(params: &[(DrumSound, DrumParam)], velocity: u8) -> f64 {
    let hits = eight_hits(params, velocity, 1.0);
    hits.iter()
        .map(|hit| spectral_centroid(hit, RATE))
        .sum::<f64>()
        / hits.len() as f64
}

/// Whether every value is higher than the one before.
fn rising(values: &[f64]) -> bool {
    values.windows(2).all(|pair| pair[1] > pair[0])
}

// Brightness.

/// The cymbal is the kit's top end, with the hats: its spectral centroid
/// is in the kHz range, and Tone moves it up at every step, so turning Tone
/// down lowers it. Measured: 7.3 kHz at Tone 0 to 8.8 kHz at Tone 1. Its
/// low band, at 3.44 kHz and the longest, keeps it under the hats' (about
/// 9 to 10 kHz at their default Tone).
#[test]
fn the_cymbal_is_in_the_khz_range_and_tone_moves_it() {
    let centroids: Vec<f64> = [0.0, 0.25, 0.5, 0.75, 1.0]
        .into_iter()
        .map(|tone| centroid(&[cymbal(DrumParam::Tone(tone))], 100))
        .collect();
    assert!(rising(&centroids), "{centroids:?}");
    assert!(centroids[0] > 5000.0, "{centroids:?}");
    assert!(centroids[4] > centroids[0] * 1.15, "{centroids:?}");
}

/// Tune, the closed hat's, moves the whole metal up, and the cymbal's
/// brightness with it. The filters keep the same bands whatever the Tune,
/// so the centroid moves far less than the oscillators: measured, from
/// 7.3 kHz at Tune 102.65 Hz to 8.5 kHz at 410.6 Hz, two octaves up,
/// rising at every octave.
#[test]
fn raising_tune_raises_it() {
    let centroids: Vec<f64> = [102.65, 205.3, 410.6]
        .into_iter()
        .map(|hz| centroid(&[tune(hz)], 100))
        .collect();
    assert!(rising(&centroids), "{centroids:?}");
}

/// Velocity sets the strength of the hit, and the tone follows: a full
/// accent drives the amplifiers harder and moves the band-passes up, so it's
/// brighter than a soft hit, not only louder. The centroid doesn't depend
/// on level, so a louder copy of the soft hit would measure the same.
/// Measured: 8.4 kHz at 127 against 7.7 kHz at 64.
#[test]
fn a_harder_hit_is_brighter_not_just_louder() {
    let (soft, hard) = (centroid(&[], 64), centroid(&[], 127));
    assert!(hard > soft * 1.05, "127: {hard:.0} Hz, 64: {soft:.0} Hz");
}

/// Velocity 100 is an unaccented hit, and 127 a full accent, through the
/// kit's shared curve: an accent peaks at the kit's reference level, and the
/// unaccented hit 11 to 15 dB under it: 10.5 dB from the curve, and
/// measured 11.7 dB, as the softer hit also drives the amplifiers less hard.
#[test]
fn velocity_100_is_unaccented_and_127_a_full_accent() {
    let accent = peak(&one_hit(CYMBAL, &[], 127, 1.0));
    let unaccented = peak(&one_hit(CYMBAL, &[], 100, 1.0));
    let from_reference = 20.0 * (accent / REFERENCE_PEAK).log10();
    assert!(from_reference.abs() < 1.0, "{from_reference:.1} dB");
    let below = 20.0 * (accent / unaccented).log10();
    assert!((11.0..15.0).contains(&below), "{below:.1} dB");
}

// Decay, and the bands.

/// Decay is how long the low band, the longest, takes to die away by
/// 40 dB. The whole cymbal falls 40 dB from its peak a little sooner, as
/// its peak is all three bands at once: measured 0.90 to 0.97 of the
/// setting. So doubling Decay roughly doubles it: measured 1.84 times from
/// 0.35 to 0.7 s and 2.01 times from 0.6 to 1.2 s.
#[test]
fn doubling_decay_roughly_doubles_it() {
    let measure = |decay: f32| {
        let samples = one_hit(CYMBAL, &[cymbal(DrumParam::DecaySeconds(decay))], 127, 2.5);
        decay_seconds(&samples, RATE, 40.0).unwrap()
    };
    let decays: Vec<(f32, f64)> = [0.35, 0.6, 0.7, 0.8, 1.2]
        .into_iter()
        .map(|decay| (decay, measure(decay)))
        .collect();
    for (decay, measured) in &decays {
        let ratio = measured / f64::from(*decay);
        assert!((0.85..1.05).contains(&ratio), "{decay}: {measured:.3} s");
    }
    for (short, long) in [(0, 2), (1, 4)] {
        let ratio = decays[long].1 / decays[short].1;
        assert!((1.7..2.3).contains(&ratio), "{decays:?}");
    }
}

/// The low band outlasts the high band: the cymbal starts with its sizzle
/// and fades to its lower wash. At full Tone, above 9 kHz, where the high
/// band is, is 45% of its energy in the first 50 ms and 10% by 0.3 s. The
/// bands' own lengths are measured in the engine's unit tests.
#[test]
fn the_low_band_outlasts_the_high_band() {
    let hits = eight_hits(&[cymbal(DrumParam::Tone(1.0))], 127, 0.5);
    let share = |from: f64, to: f64| {
        hits.iter()
            .map(|hit| share_above(&hit[seconds(from)..seconds(to)], RATE, 9000.0))
            .sum::<f64>()
            / hits.len() as f64
    };
    let (start, later) = (share(0.0, 0.05), share(0.3, 0.4));
    assert!(later < start * 0.5, "{start:.3} then {later:.3}");
}

// No clicks.

/// A hit doesn't click: the cymbal rises over the 0.1 ms the kit smooths
/// every edge over, rather than starting at full level. Its first sample is
/// at most 0.12 of its peak, as the hats' is: measured, up to 0.105 at
/// every setting and velocity.
#[test]
fn a_hit_does_not_click() {
    let cases: [(&str, Vec<(DrumSound, DrumParam)>); 6] = [
        ("defaults", vec![]),
        ("Tone 1", vec![cymbal(DrumParam::Tone(1.0))]),
        ("Tone 0", vec![cymbal(DrumParam::Tone(0.0))]),
        ("Tune 410.6", vec![tune(410.6)]),
        ("Decay 0.35 s", vec![cymbal(DrumParam::DecaySeconds(0.35))]),
        ("Level +6 dB", vec![cymbal(DrumParam::LevelDb(6.0))]),
    ];
    for (what, params) in cases {
        for velocity in [1, 64, 100, 127] {
            let samples = one_hit(CYMBAL, &params, velocity, 0.5);
            let first = samples[first_sound(&samples).unwrap()].abs() / peak(&samples);
            assert!(first <= 0.12, "{what}, velocity {velocity}: {first:.3}");
        }
    }
}

/// The limit for anything that happens while the cymbal rings: the
/// steepest step a lone accent with the same settings makes, with room for
/// the metal to meet steeper runs of itself over a bar of hits than in one,
/// and for hits to pile up, as the hats' is.
fn ringing_limit(lone_hit_step: f32) -> f32 {
    lone_hit_step * 2.0
}

/// The steepest step of a lone accent with these settings.
fn lone_step(params: &[(DrumSound, DrumParam)]) -> f32 {
    max_jump(&one_hit(CYMBAL, params, 127, 1.5)).0
}

/// Fast repeats: 8ths, 16ths, then 32nds at 120 BPM, at the longest Decay
/// so they pile up. A hit charges the ringing cymbal rather than restarting
/// it, so none clicks.
#[test]
fn fast_repeats_do_not_click() {
    let params = [cymbal(DrumParam::DecaySeconds(1.2))];
    let mut hits = Vec::new();
    for (bar, step) in [BEAT / 2, BEAT / 4, BEAT / 8].into_iter().enumerate() {
        for t in (0..4 * BEAT).step_by(step as usize) {
            let velocity = if hits.len() % 2 == 0 { 127 } else { 100 };
            hits.push(hit(
                hits.len() as u128,
                CYMBAL,
                velocity,
                bar as u64 * 4 * BEAT + t,
            ));
        }
    }
    let samples = render_drums(&drum_project(120.0, 3, &params, hits), 7.5, 128);
    assert_no_click(&samples, ringing_limit(lone_step(&params)), "repeats");
    // The envelopes charge rather than add, so repeats don't pile up: an
    // accent peaks at 0.5, and the repeats measured up to 0.67.
    assert!(peak(&samples) < 0.85, "{}", peak(&samples));
}

/// Flams: a soft grace hit, then an accent, 40 ms down to 1 ms apart.
#[test]
fn flams_do_not_click() {
    let mut hits = Vec::new();
    // 1 tick is about 0.52 ms at 120 BPM.
    for (i, gap) in [77, 38, 19, 10, 2].into_iter().enumerate() {
        let start = (i as u64 + 1) * 4 * BEAT;
        hits.push(hit(2 * i as u128, CYMBAL, 64, start - gap));
        hits.push(hit(2 * i as u128 + 1, CYMBAL, 127, start));
    }
    let samples = render_drums(&drum_project(120.0, 6, &[], hits), 12.0, 128);
    assert_no_click(&samples, ringing_limit(lone_step(&[])), "flams");
}

/// Every control swept between its limits, faster than it can glide, while
/// accents ring. They all glide, so nothing clicks.
#[test]
fn turning_the_controls_while_it_rings_does_not_click() {
    let sweeps: [(&str, [(DrumSound, DrumParam); 2]); 4] = [
        ("Tune", [tune(410.6), tune(102.65)]),
        (
            "Tone",
            [cymbal(DrumParam::Tone(1.0)), cymbal(DrumParam::Tone(0.0))],
        ),
        (
            "Decay",
            [
                cymbal(DrumParam::DecaySeconds(0.35)),
                cymbal(DrumParam::DecaySeconds(1.2)),
            ],
        ),
        (
            "Level",
            [
                cymbal(DrumParam::LevelDb(6.0)),
                cymbal(DrumParam::LevelDb(-60.0)),
            ],
        ),
    ];
    for (what, [one, other]) in sweeps {
        let hits = (0..8)
            .map(|i| hit(i, CYMBAL, 127, i as u64 * BEAT))
            .collect();
        let mut project = drum_project(120.0, 2, &[], hits);
        let mut renderer = Renderer::new(config(), Snapshot::from(&project), 128);
        renderer.controller.play().unwrap();
        renderer.render(seconds(0.05));
        for turn in 0..40 {
            let (sound, param) = if turn % 2 == 0 { one } else { other };
            project
                .apply(&Command::SetDrumParam {
                    track: drum_track(),
                    sound,
                    param,
                })
                .unwrap();
            renderer.controller.set_project(&project).unwrap();
            renderer.render(seconds(0.09));
        }
        let samples = renderer.samples();
        // The steepest a lone hit steps anywhere in the sweep: at the top of
        // Tone or Tune, and Level +6 dB doubles it.
        let loudest = match what {
            "Tone" => lone_step(&[cymbal(DrumParam::Tone(1.0))]),
            "Tune" => lone_step(&[tune(410.6)]),
            "Level" => 2.0 * lone_step(&[]),
            _ => lone_step(&[]),
        };
        assert_no_click(samples, ringing_limit(loudest), what);
    }
}

/// Every control glides in when it's turned while the cymbal rings, rather
/// than jumping: compared with the same render without the turn (the metal
/// runs the same in both), the turn makes little of its difference in the
/// first tenth of the 20 ms glide. A jump in Tone or Level hides inside the
/// cymbal's own steps, so the click limits can't see it, and this does.
/// Tune can't be checked this way (see the hats' test); its glide is
/// checked on the metal itself, in the engine's unit tests.
#[test]
fn every_control_glides_in() {
    let glide = seconds(uta_engine::DRUM_SMOOTHING_SECONDS);
    // 16 ms in.
    let at = 6 * 128;
    for (what, param) in [
        ("Tone", cymbal(DrumParam::Tone(1.0))),
        ("Decay", cymbal(DrumParam::DecaySeconds(0.35))),
        ("Level", cymbal(DrumParam::LevelDb(6.0))),
    ] {
        let difference = turn_difference(CYMBAL, &[], param, at, 0.3);
        assert_glides(&difference, glide, what);
    }
}

// The metal.

/// The metal runs freely between hits, so two hits on a cymbal that has
/// died away, at different times, come out a little different, as on the
/// 808, but the same in level and colour.
#[test]
fn each_hit_catches_the_metal_somewhere_else() {
    let hits = vec![hit(0, CYMBAL, 127, 0), hit(1, CYMBAL, 127, 3 * BEAT)];
    let samples = render_drums(&drum_project(60.0, 1, &[], hits), 4.0, 128);
    let (first, second) = (
        &samples[..seconds(0.5)],
        &samples[seconds(3.0)..seconds(3.5)],
    );
    assert!(max_difference(first, second) > 0.05);
    let ratio = rms(first) / rms(second);
    assert!((0.85..1.15).contains(&ratio), "{ratio}");
    let ratio = spectral_centroid(first, RATE) / spectral_centroid(second, RATE);
    assert!((0.95..1.05).contains(&ratio), "{ratio}");
}

/// A beat with the cymbal: a crash on the one, kick and closed hats, the
/// cymbal again softly, a flam, and the open hat, which shares its metal.
fn beat() -> Project {
    let mut pattern = vec![
        (CYMBAL, 0, 127),
        (KICK, 0, 127),
        (KICK, 2 * BEAT, 100),
        (CLAP, BEAT, 127),
        (CLAP, 3 * BEAT, 127),
        (OPEN_HAT, 2 * BEAT + BEAT / 2, 100),
        (CYMBAL, 3 * BEAT - 19, 64),
        (CYMBAL, 3 * BEAT, 110),
    ];
    for i in 0..8u64 {
        pattern.push((CLOSED_HAT, i * BEAT / 2, if i % 2 == 0 { 100 } else { 70 }));
    }
    let hits = pattern
        .into_iter()
        .enumerate()
        .map(|(i, (pitch, start, velocity))| hit(i as u128, pitch, velocity, start))
        .collect();
    drum_project(110.0, 1, &[], hits)
}

// Determinism.

/// Rendering twice gives identical audio, and so do blocks of 32, 128 and
/// 1024: every hit lands on its exact sample, and the metal and the sounds
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
/// as the first time did: the metal restarts on Play.
#[test]
fn playing_again_sounds_the_same() {
    let project = beat();
    let mut renderer = Renderer::new(config(), Snapshot::from(&project), 128);
    renderer.controller.play().unwrap();
    renderer.render_seconds(2.0);
    renderer.controller.stop().unwrap();
    renderer.render_seconds(4.0);
    let first_end = renderer.samples().len();
    renderer.controller.play().unwrap();
    renderer.render_seconds(2.0);
    let samples = renderer.samples();
    assert_eq!(&samples[first_end..], &samples[..first_end - seconds(4.0)]);
}

/// Once it has died away, the cymbal stops: a render of one cymbal at its
/// longest Decay ends in exact silence, at every Tune and Tone. It stops
/// 120 dB down, three times the 40 dB of Decay, so about 3.6 s after the
/// hit, and the filters after a little longer: measured, under 4 s. The
/// metal runs on underneath, so this checks the cymbal stops on its
/// envelopes, not on the metal falling quiet, which it never does.
#[test]
fn the_cymbal_dies_away_to_silence() {
    for hz in [102.65, 205.3, 410.6] {
        for tone in [0.0, 0.5, 1.0] {
            let params = [
                cymbal(DrumParam::DecaySeconds(1.2)),
                cymbal(DrumParam::Tone(tone)),
                tune(hz),
            ];
            // Two bars at 60 BPM, so the loop doesn't hit it again.
            let hits = vec![hit(0, CYMBAL, 127, 0)];
            let samples = render_drums(&drum_project(60.0, 2, &params, hits), 6.0, 128);
            let last = samples.iter().rposition(|&s| s != 0.0).unwrap();
            assert!(
                last < seconds(4.2),
                "Tune {hz}, Tone {tone}: still sounding at {} s",
                last as f64 / 48_000.0
            );
        }
    }
}

/// Play restarts the metal even while only the cymbal rings, crossfading
/// so its ring doesn't jump: Stop and Play again straight away, with a
/// cymbal at its longest Decay still ringing, and once that ring has died
/// away the beat sounds exactly as a render from the top does. The
/// crossfade itself is checked in the engine's unit tests: this render
/// would pass without it.
#[test]
fn playing_again_while_it_rings_restarts_the_metal() {
    let hits = vec![hit(0, CYMBAL, 127, 0), hit(1, CYMBAL, 127, 8 * BEAT)];
    let project = drum_project(120.0, 3, &[cymbal(DrumParam::DecaySeconds(1.2))], hits);
    let reference = render_drums(&project, 6.0, 128);
    let mut renderer = Renderer::new(config(), Snapshot::from(&project), 128);
    renderer.controller.play().unwrap();
    // Stop 0.1 s in, as the cymbal rings, and play again.
    renderer.render(seconds(0.1));
    renderer.controller.stop().unwrap();
    renderer.controller.play().unwrap();
    renderer.render_seconds(6.0);
    let again = &renderer.samples()[seconds(0.1)..];
    // From the second cymbal on, 4 s in, the first has long gone.
    let from = seconds(4.0);
    assert_eq!(&again[from..seconds(6.0)], &reference[from..seconds(6.0)]);
    assert!(peak(&again[from..seconds(6.0)]) > 0.3);
}
