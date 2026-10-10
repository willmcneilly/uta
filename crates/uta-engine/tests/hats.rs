//! The 808 closed and open hats, measured from their sound. See RFC-006,
//! "How we'll verify it".
//!
//! Every threshold here was calibrated once against the reference behaviour
//! (the research's 808 recipe: six squares, a band-pass near 7.1 kHz, a
//! clipping amplifier and a high-pass, with a closed hit choking the open
//! hat) and says why it is what it is.
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
/// The closed hat's and open hat's notes, for short.
const CLOSED: u8 = CLOSED_HAT;
const OPEN: u8 = OPEN_HAT;

fn config() -> EngineConfig {
    EngineConfig {
        sample_rate: RATE,
        channels: 1,
    }
}

fn seconds(seconds: f64) -> usize {
    (seconds * f64::from(RATE)) as usize
}

fn closed(param: DrumParam) -> (DrumSound, DrumParam) {
    (DrumSound::ClosedHat, param)
}

fn open(param: DrumParam) -> (DrumSound, DrumParam) {
    (DrumSound::OpenHat, param)
}

/// Eight hits on `pitch` with these settings, a beat apart at 120 BPM and
/// each a few ticks later in its beat than the last, so each catches the
/// metal somewhere else: each hit's first 0.45 s.
fn eight_hits(pitch: u8, params: &[(DrumSound, DrumParam)], velocity: u8) -> Vec<Vec<f32>> {
    let starts: Vec<Ticks> = (0..8).map(|i| i * BEAT + (i * 37) % 100).collect();
    let hits = starts
        .iter()
        .enumerate()
        .map(|(i, &start)| hit(i as u128, pitch, velocity, start))
        .collect();
    let samples = render_drums(&drum_project(120.0, 3, params, hits), 4.5, 128);
    starts
        .into_iter()
        .map(|start| {
            let at = (start as f64 / BEAT as f64 * 0.5 * f64::from(RATE)).round() as usize;
            samples[at..at + seconds(0.45)].to_vec()
        })
        .collect()
}

/// The spectral centroid of eight hits, averaged.
fn centroid(pitch: u8, params: &[(DrumSound, DrumParam)], velocity: u8) -> f64 {
    let hits = eight_hits(pitch, params, velocity);
    hits.iter()
        .map(|hit| spectral_centroid(hit, RATE))
        .sum::<f64>()
        / hits.len() as f64
}

/// Whether every value is higher than the one before.
fn rising(values: &[f64]) -> bool {
    values.windows(2).all(|pair| pair[1] > pair[0])
}

// Brightness and pitch.

/// The hats are the kit's top end: their spectral centroid is in the kHz
/// range, and Tone moves it up at every step, on both hats, since they
/// share it. Measured on the closed hat: 6.9 kHz at Tone 4 kHz to 13.2 kHz
/// at 12 kHz. Even the darkest is well above anything else in the kit.
#[test]
fn the_hats_are_in_the_khz_range_and_tone_moves_them() {
    let tones = [4000.0, 5500.0, 7100.0, 9000.0, 12_000.0];
    for (what, pitch) in [("closed", CLOSED), ("open", OPEN)] {
        let centroids: Vec<f64> = tones
            .into_iter()
            .map(|tone| centroid(pitch, &[closed(DrumParam::Tone(tone))], 100))
            .collect();
        assert!(rising(&centroids), "{what}: {centroids:?}");
        assert!(centroids[0] > 5000.0, "{what}: {centroids:?}");
        assert!(centroids[4] > centroids[0] * 1.5, "{what}: {centroids:?}");
    }
}

/// Tune moves the whole metal up, and its brightness with it, on both hats.
/// The filters keep the same band whatever the Tune, so the centroid moves
/// far less than the oscillators: measured on the open hat, from 9.4 kHz at
/// Tune 102.65 Hz to 10.4 kHz at 410.6 Hz, two octaves up, rising at every
/// step; the closed hat's eight hits waver more, so it's checked an octave
/// at a time.
#[test]
fn raising_tune_raises_them() {
    let tunes = [102.65, 150.0, 205.3, 300.0, 410.6];
    let centroids: Vec<f64> = tunes
        .into_iter()
        .map(|tune| centroid(OPEN, &[closed(DrumParam::TuneHz(tune))], 100))
        .collect();
    assert!(rising(&centroids), "open: {centroids:?}");
    assert!(centroids[4] > centroids[0] * 1.05, "open: {centroids:?}");
    let centroids: Vec<f64> = [102.65, 205.3, 410.6]
        .into_iter()
        .map(|tune| centroid(CLOSED, &[closed(DrumParam::TuneHz(tune))], 100))
        .collect();
    assert!(rising(&centroids), "closed: {centroids:?}");
}

/// Velocity sets the strength of the hit, and the tone follows: a full
/// accent drives the amplifier harder and moves the filters up, so it's
/// brighter than a soft hit, not only louder. The centroid doesn't depend
/// on level, so a louder copy of the soft hit would measure the same.
/// Measured: 10.5 kHz at 127 against 9.2 kHz at 64, on both hats.
#[test]
fn a_harder_hit_is_brighter_not_just_louder() {
    for (what, pitch) in [("closed", CLOSED), ("open", OPEN)] {
        let (soft, hard) = (centroid(pitch, &[], 64), centroid(pitch, &[], 127));
        assert!(
            hard > soft * 1.08,
            "{what}: 127: {hard:.0} Hz, 64: {soft:.0} Hz"
        );
    }
}

/// Velocity 100 is an unaccented hit, and 127 a full accent, through the
/// kit's shared curve: a closed hat's accent peaks at the kit's reference
/// level, and the unaccented hit 11 to 15 dB under it: 10.5 dB from the
/// curve, as the 808's 4 V to 14 V trigger is, and measured 12.8 dB, as the
/// softer hit also drives the amplifier less hard.
#[test]
fn velocity_100_is_unaccented_and_127_a_full_accent() {
    let accent = peak(&one_hit(CLOSED, &[], 127, 0.5));
    let unaccented = peak(&one_hit(CLOSED, &[], 100, 0.5));
    let from_reference = 20.0 * (accent / REFERENCE_PEAK).log10();
    assert!(from_reference.abs() < 1.0, "{from_reference:.1} dB");
    let below = 20.0 * (accent / unaccented).log10();
    assert!((11.0..15.0).contains(&below), "{below:.1} dB");
}

// Decay.

/// Decay is how long each hat takes to die away by 40 dB, and the hats
/// follow it: measured within 3% of the setting for the closed hat, and
/// within 8% for the open hat, whose longest ring is measured a little
/// short as it nears the floor of the 5 ms windows. So doubling Decay
/// roughly doubles it.
#[test]
fn decay_follows_the_control() {
    let cases = [
        (CLOSED, closed(DrumParam::DecaySeconds(0.02))),
        (CLOSED, closed(DrumParam::DecaySeconds(0.05))),
        (CLOSED, closed(DrumParam::DecaySeconds(0.1))),
        (CLOSED, closed(DrumParam::DecaySeconds(0.15))),
        (OPEN, open(DrumParam::DecaySeconds(0.09))),
        (OPEN, open(DrumParam::DecaySeconds(0.2))),
        (OPEN, open(DrumParam::DecaySeconds(0.35))),
        (OPEN, open(DrumParam::DecaySeconds(0.6))),
    ];
    for (pitch, (sound, param)) in cases {
        let samples = one_hit(pitch, &[(sound, param)], 127, 1.5);
        let measured = decay_seconds(&samples, RATE, 40.0).unwrap();
        let ratio = measured / f64::from(param.value());
        assert!(
            (0.9..1.1).contains(&ratio),
            "{sound:?} {param:?}: {measured:.3} s"
        );
    }
}

// The choke.

/// An open hat cut off by a closed hat an eighth note (125 ms) after it.
fn choked(params: &[(DrumSound, DrumParam)]) -> Vec<f32> {
    let hits = vec![hit(0, OPEN, 127, 0), hit(1, CLOSED, 127, BEAT / 4)];
    render_drums(&drum_project(120.0, 1, params, hits), 1.0, 128)
}

/// The level, as the RMS over 10 ms, around `at` seconds.
fn level(samples: &[f32], at: f64) -> f64 {
    rms(&samples[seconds(at - 0.005)..seconds(at + 0.005)])
}

/// The two hats are one circuit, so a closed hit cuts off a ringing open
/// hat: 60 ms after the closed hit, the two together are far quieter than
/// the open hat left ringing. Left ringing, the open hat has fallen about
/// 12 dB in those 60 ms; the closed hat itself has fallen about 50 dB, and
/// the choked open hat with it. Measured 26 dB quieter at the defaults,
/// and more with the open hat's longest Decay.
#[test]
fn a_closed_hat_cuts_off_a_ringing_open_hat() {
    for params in [vec![], vec![open(DrumParam::DecaySeconds(0.6))]] {
        let hits = vec![hit(0, OPEN, 127, 0)];
        let alone = render_drums(&drum_project(120.0, 1, &params, hits), 1.0, 128);
        let at = 0.125 + 0.06;
        let quieter = 20.0 * (level(&alone, at) / level(&choked(&params), at)).log10();
        assert!(quieter > 20.0, "{params:?}: {quieter:.1} dB");
    }
}

/// The level in each millisecond from `from` seconds, as a share of the
/// level in the millisecond before.
fn fade(samples: &[f32], from: f64, milliseconds: usize) -> Vec<f64> {
    let before = rms(&samples[seconds(from - 0.001)..seconds(from)]);
    (0..milliseconds)
        .map(|ms| {
            let at = from + ms as f64 / 1000.0;
            rms(&samples[seconds(at)..seconds(at + 0.001)]) / before
        })
        .collect()
}

/// The choke fades the open hat out over a few milliseconds rather than
/// cutting it mid-swing, so it doesn't click. With the closed hat silent
/// (Level -60 dB), the choke is all there is: the open hat keeps most of
/// its level for the first millisecond, then falls away with a time
/// constant of 2 ms. Measured in each millisecond: 0.98 of its level, then
/// 0.44, 0.37 and 0.21, under 0.1 from the fifth on (the metal wavers from
/// one millisecond to the next), and 0.01 by the tenth. A cut would leave
/// nothing in the first millisecond.
#[test]
fn the_choke_fades_rather_than_cuts() {
    let samples = choked(&[closed(DrumParam::LevelDb(-60.0))]);
    let fade = fade(&samples, 0.125, 10);
    assert!(fade[0] > 0.6, "{fade:.3?}");
    assert!(fade[4..].iter().all(|&level| level < 0.15), "{fade:.3?}");
    assert!(fade[9] < 0.03, "{fade:.3?}");
}

/// Guards the choke's check: an open hat cut off at the same moment fails
/// it.
#[test]
fn the_choke_check_catches_a_cut() {
    let mut samples = one_hit(OPEN, &[], 127, 1.0);
    samples[seconds(0.125)..].fill(0.0);
    assert!(fade(&samples, 0.125, 10)[0] < 0.6);
}

// No clicks.

/// A hit doesn't click: the hats rise over the 0.1 ms the kit smooths every
/// edge over, rather than starting at full level. Their first sample is at
/// most 0.12 of their peak: measured, up to 0.08 at every setting and
/// velocity, and 0.22 to 0.40 with the smoothing taken out. (A step limit
/// like the other sounds' can't tell here: the hats are all top end, 5 to
/// 15 kHz, so their own samples swing nearly from one peak to the other.)
#[test]
fn a_hit_does_not_click() {
    type Case = (&'static str, u8, Vec<(DrumSound, DrumParam)>);
    let cases: [Case; 7] = [
        ("closed", CLOSED, vec![]),
        (
            "closed, Tone 12 kHz",
            CLOSED,
            vec![closed(DrumParam::Tone(12_000.0))],
        ),
        (
            "closed, Tone 4 kHz",
            CLOSED,
            vec![closed(DrumParam::Tone(4000.0))],
        ),
        (
            "closed, Tune 410.6",
            CLOSED,
            vec![closed(DrumParam::TuneHz(410.6))],
        ),
        (
            "closed, Decay 20 ms",
            CLOSED,
            vec![closed(DrumParam::DecaySeconds(0.02))],
        ),
        ("open", OPEN, vec![]),
        (
            "open, Level +6 dB",
            OPEN,
            vec![open(DrumParam::LevelDb(6.0))],
        ),
    ];
    for (what, pitch, params) in cases {
        for velocity in [1, 64, 100, 127] {
            let samples = one_hit(pitch, &params, velocity, 0.5);
            let first = samples[first_sound(&samples).unwrap()].abs() / peak(&samples);
            assert!(first <= 0.12, "{what}, velocity {velocity}: {first:.3}");
        }
    }
}

/// The limit for anything that happens while the hats ring: the steepest
/// step a lone accent with the same settings makes, with room for the
/// metal to meet steeper runs of itself over a bar of hits than in one, and
/// for hits to pile up: measured, up to 1.4 times a lone hit's. It catches
/// a ring cut off or restarted at its loudest only when that lands on a
/// steep step of its own, so the choke and the glides have checks of their
/// own.
fn ringing_limit(lone_hit_step: f32) -> f32 {
    lone_hit_step * 2.0
}

/// The steepest step of a lone accent on `pitch` with these settings.
fn lone_step(pitch: u8, params: &[(DrumSound, DrumParam)]) -> f32 {
    max_jump(&one_hit(pitch, params, 127, 1.0)).0
}

/// Fast repeats: 16ths, then 32nds, then a roll of 64ths at 120 BPM, on
/// each hat, the open hat at its longest Decay so it piles up. A hit
/// charges the ringing hats rather than restarting them, so none clicks.
#[test]
fn fast_repeats_do_not_click() {
    let params = [open(DrumParam::DecaySeconds(0.6))];
    for (what, pitch) in [("closed", CLOSED), ("open", OPEN)] {
        let mut hits = Vec::new();
        for (bar, step) in [BEAT / 4, BEAT / 8, BEAT / 16].into_iter().enumerate() {
            for t in (0..4 * BEAT).step_by(step as usize) {
                let velocity = if hits.len() % 2 == 0 { 127 } else { 100 };
                hits.push(hit(
                    hits.len() as u128,
                    pitch,
                    velocity,
                    bar as u64 * 4 * BEAT + t,
                ));
            }
        }
        let samples = render_drums(&drum_project(120.0, 3, &params, hits), 6.5, 128);
        let limit = ringing_limit(lone_step(pitch, &params));
        assert_no_click(&samples, limit, what);
        // The envelopes charge rather than add, so a roll doesn't pile up:
        // an accent peaks at about 0.5, and a roll measured up to 0.7.
        assert!(peak(&samples) < 0.85, "{what}: {}", peak(&samples));
    }
}

/// Flams: a soft grace hit, then an accent, 40 ms down to 1 ms apart, on
/// each hat, and the open hat flammed with the closed hat that chokes it.
#[test]
fn flams_do_not_click() {
    for (what, grace, accent) in [
        ("closed", CLOSED, CLOSED),
        ("open", OPEN, OPEN),
        ("open into closed", OPEN, CLOSED),
    ] {
        let mut hits = Vec::new();
        // 1 tick is about 0.52 ms at 120 BPM.
        for (i, gap) in [77, 38, 19, 10, 2].into_iter().enumerate() {
            let start = (i as u64 + 1) * 2 * BEAT;
            hits.push(hit(2 * i as u128, grace, 64, start - gap));
            hits.push(hit(2 * i as u128 + 1, accent, 127, start));
        }
        let samples = render_drums(&drum_project(120.0, 3, &[], hits), 6.0, 128);
        let limit = ringing_limit(lone_step(OPEN, &[]).max(lone_step(CLOSED, &[])));
        assert_no_click(&samples, limit, what);
    }
}

/// Every control swept between its limits, faster than it can glide, while
/// accents on both hats ring. They all glide, so nothing clicks.
#[test]
fn turning_the_controls_while_they_ring_does_not_click() {
    let sweeps: [(&str, [(DrumSound, DrumParam); 2]); 6] = [
        (
            "Tune",
            [
                closed(DrumParam::TuneHz(410.6)),
                closed(DrumParam::TuneHz(102.65)),
            ],
        ),
        (
            "Tone",
            [
                closed(DrumParam::Tone(12_000.0)),
                closed(DrumParam::Tone(4000.0)),
            ],
        ),
        (
            "closed Decay",
            [
                closed(DrumParam::DecaySeconds(0.15)),
                closed(DrumParam::DecaySeconds(0.02)),
            ],
        ),
        (
            "open Decay",
            [
                open(DrumParam::DecaySeconds(0.09)),
                open(DrumParam::DecaySeconds(0.6)),
            ],
        ),
        (
            "closed Level",
            [
                closed(DrumParam::LevelDb(6.0)),
                closed(DrumParam::LevelDb(-60.0)),
            ],
        ),
        (
            "open Level",
            [
                open(DrumParam::LevelDb(6.0)),
                open(DrumParam::LevelDb(-60.0)),
            ],
        ),
    ];
    for (what, [one, other]) in sweeps {
        // An open hat each beat, and a closed hat on each off-beat.
        let hits = (0..16)
            .map(|i| {
                let pitch = if i % 2 == 0 { OPEN } else { CLOSED };
                hit(i, pitch, 127, i as u64 * BEAT / 2)
            })
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
        // Tone, and Level +6 dB doubles it.
        let lone = |params: &[(DrumSound, DrumParam)]| {
            lone_step(OPEN, params).max(lone_step(CLOSED, params))
        };
        let loudest = match what {
            "Tone" => lone(&[closed(DrumParam::Tone(12_000.0))]),
            "Tune" => lone(&[closed(DrumParam::TuneHz(410.6))]),
            "closed Level" | "open Level" => 2.0 * lone(&[]),
            _ => lone(&[]),
        };
        assert_no_click(samples, ringing_limit(loudest), what);
    }
}

/// Every control glides in when it's turned while the hats ring, rather
/// than jumping: compared with the same render without the turn (the metal
/// runs the same in both), the turn makes little of its difference in the
/// first tenth of the 20 ms glide. A jump in Tone or Level hides inside the
/// hats' own steps, so the click limits can't see it, and this does. (A
/// jump in Decay would change how fast they fall, not their level, so it
/// couldn't click; it's checked anyway.) Tune can't be checked this way:
/// moving the metal even a little moves its top harmonics' phases a long
/// way within 2 ms, so the difference is soon all there is. Its glide is
/// checked on the metal itself, in the engine's unit tests.
#[test]
fn every_control_glides_in() {
    let glide = seconds(uta_engine::DRUM_SMOOTHING_SECONDS);
    // 16 ms in.
    let at = 6 * 128;
    for (what, pitch, param) in [
        ("Tone", OPEN, closed(DrumParam::Tone(12_000.0))),
        (
            "closed Decay",
            CLOSED,
            closed(DrumParam::DecaySeconds(0.02)),
        ),
        ("open Decay", OPEN, open(DrumParam::DecaySeconds(0.09))),
        ("closed Level", CLOSED, closed(DrumParam::LevelDb(6.0))),
        ("open Level", OPEN, open(DrumParam::LevelDb(6.0))),
    ] {
        let base = [closed(DrumParam::DecaySeconds(0.15))];
        let difference = turn_difference(pitch, &base, param, at, 0.3);
        assert_glides(&difference, glide, what);
    }
}

// The metal.

/// The metal runs freely between hits, so two hits on hats that have died
/// away, at different times, come out a little different, as on the 808.
#[test]
fn each_hit_catches_the_metal_somewhere_else() {
    let hits = vec![hit(0, OPEN, 127, 0), hit(1, OPEN, 127, 2 * BEAT)];
    let samples = render_drums(&drum_project(60.0, 1, &[], hits), 3.0, 128);
    let (first, second) = (
        &samples[..seconds(0.5)],
        &samples[seconds(2.0)..seconds(2.5)],
    );
    assert!(max_difference(first, second) > 0.05);
    // The same, though, in level and colour.
    let ratio = rms(first) / rms(second);
    assert!((0.85..1.15).contains(&ratio), "{ratio}");
    let ratio = spectral_centroid(first, RATE) / spectral_centroid(second, RATE);
    assert!((0.95..1.05).contains(&ratio), "{ratio}");
}

/// A beat with both hats: 16ths with accents and ghost notes, open hats
/// choked by the closed hat after them, a flam, and the kick and clap.
fn beat() -> Project {
    let mut pattern = Vec::new();
    for i in 0..16u64 {
        let pitch = if i == 6 || i == 14 { OPEN } else { CLOSED };
        let velocity = match i % 4 {
            0 => 127,
            2 => 100,
            _ => 60,
        };
        pattern.push((pitch, i * BEAT / 4, velocity));
    }
    pattern.extend([
        (CLOSED, 3 * BEAT - 19, 64),
        (KICK, 0, 127),
        (KICK, 2 * BEAT, 100),
        (CLAP, BEAT, 127),
        (CLAP, 3 * BEAT, 127),
    ]);
    let hits = pattern
        .into_iter()
        .enumerate()
        .map(|(i, (pitch, start, velocity))| hit(i as u128, pitch, velocity, start))
        .collect();
    drum_project(110.0, 1, &[open(DrumParam::DecaySeconds(0.6))], hits)
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
    renderer.render_seconds(3.0);
    let first_end = renderer.samples().len();
    renderer.controller.play().unwrap();
    renderer.render_seconds(2.0);
    let samples = renderer.samples();
    assert_eq!(&samples[first_end..], &samples[..first_end - seconds(3.0)]);
}

/// Once they have died away, the hats stop: a render of one open hat at
/// its longest Decay ends in exact silence. They stop 120 dB down, three
/// times the 40 dB of Decay, so 1.8 s after the hit.
#[test]
fn the_hats_die_away_to_silence() {
    let samples = one_hit(OPEN, &[open(DrumParam::DecaySeconds(0.6))], 127, 3.0);
    let last = samples.iter().rposition(|&s| s != 0.0).unwrap();
    assert!(
        last < seconds(2.1),
        "still sounding at {} s",
        last as f64 / 48_000.0
    );
}
