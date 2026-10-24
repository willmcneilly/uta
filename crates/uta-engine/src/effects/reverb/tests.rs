//! The Reverb's own tests, on impulse responses: a click through its real
//! process path, measured with `uta-analysis`'s reverb measurements
//! (RFC-010, "How we'll verify it"). Modulation is off throughout: it's
//! UTA-100's.

use super::*;
use uta_analysis::impulse::{self, OCTAVE_BANDS_HZ};

/// The rates every measurement must agree at.
const RATES: [f64; 4] = [44_100.0, 48_000.0, 96_000.0, 192_000.0];

/// The default settings, changed by `change`.
fn settings(change: impl FnOnce(&mut ReverbSettings)) -> ReverbSettings {
    let mut settings = ReverbSettings::default();
    change(&mut settings);
    settings
}

/// `seconds` of the Reverb's answer to a full-scale click on `click`'s
/// sides (left, right) at sample 0, in blocks of 128: its impulse response.
fn response_to(reverb: &mut Reverb, click: [f32; 2], rate: f64, seconds: f64) -> [Vec<f32>; 2] {
    let frames = (seconds * rate).round() as usize;
    let mut left = vec![0.0; frames];
    let mut right = vec![0.0; frames];
    left[0] = click[0];
    right[0] = click[1];
    for (left, right) in left.chunks_mut(128).zip(right.chunks_mut(128)) {
        reverb.process(left, right);
    }
    [left, right]
}

/// The impulse response of a Reverb with `settings`, for a click on both
/// sides.
fn impulse_response(settings: ReverbSettings, rate: f64, seconds: f64) -> [Vec<f32>; 2] {
    response_to(&mut Reverb::new(settings, rate), [1.0, 1.0], rate, seconds)
}

/// Each octave band's decay time, in seconds, from 250 Hz to 4 kHz: the
/// bands every rate has, and that a 2 s tail is long enough to measure.
fn band_decays(samples: &[f32], rate: f64) -> Vec<(f64, f64)> {
    impulse::band_decay_times(samples, rate as u32)
        .into_iter()
        .zip(OCTAVE_BANDS_HZ)
        .filter(|&(_, hz)| (250.0..=4000.0).contains(&hz))
        .map(|(decay, hz)| (hz, decay.expect("every band decays")))
        .collect()
}

/// The 1 kHz octave's decay time, in seconds.
fn mid_decay(samples: &[f32], rate: f64) -> f64 {
    impulse::band_decay_times(samples, rate as u32)[4].expect("the mid band decays")
}

/// The decay time Decay sets is the tail's: every resonance loses exactly
/// its share each lap, so the mid band falls 60 dB in Decay, from a small
/// room to a cathedral.
#[test]
fn the_mid_band_decays_in_decay_within_10_percent() {
    for decay in [0.3, 2.0, 10.0] {
        let response = impulse_response(
            settings(|s| s.decay_seconds = decay as f32),
            48_000.0,
            1.2 * decay + 0.5,
        );
        for side in &response {
            let measured = mid_decay(side, 48_000.0);
            assert!(
                (measured / decay - 1.0).abs() < 0.1,
                "Decay {decay} s: the mid band decays in {measured:.3} s"
            );
        }
    }
}

/// The first sound arrives exactly Predelay after the click, at every rate,
/// and nothing comes before it.
#[test]
fn the_first_sound_arrives_at_the_predelay_to_the_sample() {
    for rate in RATES {
        for predelay in [0.0, 0.0123, 0.25, 0.5] {
            let response = impulse_response(
                settings(|s| s.predelay_seconds = predelay),
                rate,
                predelay as f64 + 0.1,
            );
            let at = (f64::from(predelay) * rate).round() as usize;
            let [left, right] = &response;
            assert_eq!(
                impulse::onset(&[left, right]),
                Some(at),
                "Predelay {predelay} s at {rate} Hz"
            );
            assert!(
                left[..at].iter().chain(&right[..at]).all(|&s| s == 0.0),
                "nothing before Predelay {predelay} s at {rate} Hz"
            );
        }
    }
}

/// How long the early burst lasts after Predelay, in samples, at `rate`:
/// the longest path through the diffuser, its last lane in every step.
fn burst_samples(settings: ReverbSettings, rate: f64) -> usize {
    ROOM.steps
        .iter()
        .map(|step| {
            let seconds = lane_seconds(step, LANES - 1, f64::from(settings.early_size_seconds));
            (seconds * rate).round() as usize
        })
        .sum()
}

/// At Depth 0% you hear only the early burst: nothing at all after it
/// ends, though the tail rings on inside. At 100% you hear only the tail:
/// nothing until the shortest line's first lap, though the early burst has
/// long since begun. Halfway, both are heard.
#[test]
fn depth_0_is_only_the_early_burst_and_100_only_the_tail() {
    let rate = 48_000.0;
    let at = |depth| settings(|s| s.depth = depth);
    let predelay = (f64::from(at(0.0).predelay_seconds) * rate).round() as usize;
    let burst_ends = predelay + burst_samples(at(0.0), rate) + 1;
    let first_lap = predelay + (ROOM.lines[0] * rate).round() as usize;
    assert!(
        burst_ends < first_lap + 1000,
        "the burst and the tail overlap"
    );

    let mut reverb = Reverb::new(at(0.0), rate);
    let [left, right] = response_to(&mut reverb, [1.0, 1.0], rate, 1.0);
    assert!(left[..burst_ends].iter().any(|&s| s != 0.0));
    assert!(
        left[burst_ends..]
            .iter()
            .chain(&right[burst_ends..])
            .all(|&s| s == 0.0),
        "Depth 0% has a late tail"
    );
    assert!(reverb.is_ringing(), "the tail rings on unheard");

    let [left, right] = impulse_response(at(1.0), rate, 1.0);
    assert!(
        left[..first_lap]
            .iter()
            .chain(&right[..first_lap])
            .all(|&s| s == 0.0),
        "Depth 100% has early sound of its own"
    );
    assert!(left[first_lap..].iter().any(|&s| s != 0.0));

    let [left, _] = impulse_response(at(0.5), rate, 1.0);
    assert!(left[..first_lap].iter().any(|&s| s != 0.0));
    assert!(left[burst_ends..].iter().any(|&s| s != 0.0));
}

/// The early output is the last step's lanes before their mix (RFC-010's
/// note). A click on the left reaches each of the left's four tapped lanes
/// by 4 × 8 × 8 paths, each with its own delay, so up to 1,024 echoes; had
/// it been tapped after the mix, the mix's signs would cancel all but two
/// of the eight lanes in the sum, halving them. Some land on the same
/// sample: tapped before the mix, 877 samples sound; tapped after it, which
/// was tried, 484.
#[test]
fn the_early_burst_is_tapped_before_the_last_mix() {
    let rate = 48_000.0;
    let reverb = settings(|s| {
        s.depth = 0.0;
        s.diffusion = 1.0;
        s.early_size_seconds = 0.2;
        s.predelay_seconds = 0.0;
    });
    let [left, _] = response_to(&mut Reverb::new(reverb, rate), [1.0, 0.0], rate, 0.3);
    let echoes = left.iter().filter(|&&s| s != 0.0).count();
    assert!(echoes > 700, "{echoes} echoes");
}

/// `samples` through a 20 kHz low-pass (two Butterworth filters, 24 dB an
/// octave): what anyone hears of them, at any rate.
fn audible(samples: &[f32], rate: f64) -> Vec<f32> {
    let gain = prewarp(20_000.0, rate);
    let mut filters = [Svf::default(); 2];
    samples
        .iter()
        .map(|&s| {
            let once = filters[0].process(f64::from(s), gain, TONE_Q).low_pass;
            filters[1].process(once, gain, TONE_Q).low_pass as f32
        })
        .collect()
}

/// The Reverb sounds the same at every rate, though its samples aren't
/// identical: each band's decay, and its echo density over its first half
/// second, agree with 48 kHz's.
///
/// Echo density is measured over what anyone hears, below 20 kHz. Measured
/// on the raw samples it would differ by rate even for the same echoes: an
/// echo is one sample at any rate, so at 192 kHz the same echoes leave four
/// times the empty samples between them, and read as sparse.
#[test]
fn decay_per_band_and_echo_density_match_at_every_rate() {
    let reverb = ReverbSettings::default();
    let measure = |rate: f64| {
        let [left, _] = impulse_response(reverb, rate, 3.0);
        let profile = impulse::echo_density(&audible(&left, rate), rate as u32);
        // Every 10 ms over the first half second.
        let density: Vec<f64> = (1..50)
            .map(|n| {
                let time = f64::from(n) * 0.01;
                profile.iter().find(|&&(at, _)| at >= time).unwrap().1
            })
            .collect();
        (band_decays(&left, rate), density)
    };
    let (decays, density) = measure(48_000.0);
    for rate in RATES {
        let (other_decays, other_density) = measure(rate);
        for (&(hz, decay), &(_, other)) in decays.iter().zip(&other_decays) {
            assert!(
                (other / decay - 1.0).abs() < 0.05,
                "{hz} Hz decays in {other:.3} s at {rate} Hz, {decay:.3} s at 48 kHz"
            );
        }
        // A 20 ms window of noise reads 1 give or take about 0.06, so two
        // renders' profiles differ by about that on average even when
        // they're equally dense.
        let difference = density
            .iter()
            .zip(&other_density)
            .map(|(a, b)| (a - b).abs())
            .sum::<f64>()
            / density.len() as f64;
        assert!(
            difference < 0.08,
            "echo density at {rate} Hz is {difference:.3} from 48 kHz's on average"
        );
    }
}

/// At the longest Decay, with every setting the engine plays at each of its
/// ends, a full-scale click never makes more than full scale, and once the
/// tail has built up it never grows.
#[test]
fn at_30_seconds_and_every_extreme_the_output_never_grows_or_passes_full_scale() {
    let rate = 48_000.0;
    for (depth, diffusion, send, early, late, predelay) in [
        (0.0, 0.0, 0.0, 0.002, 0.0, 0.0),
        (1.0, 1.0, 1.0, 1.0, 1.0, 0.5),
        (1.0, 0.0, 0.0, 0.002, 1.0, 0.0),
        (1.0, 1.0, 1.0, 0.002, 0.0, 0.0),
        (0.5, 1.0, 0.0, 1.0, 0.0, 0.5),
        (0.5, 0.0, 1.0, 0.002, 1.0, 0.0),
    ] {
        let reverb = settings(|s| {
            s.decay_seconds = ReverbSettings::MAX_DECAY_SECONDS;
            s.depth = depth;
            s.diffusion = diffusion;
            s.early_send = send;
            s.early_size_seconds = early;
            s.late_size = late;
            s.predelay_seconds = predelay;
        });
        let case = format!("{reverb:?}");
        let response = impulse_response(reverb, rate, 6.0);
        for side in &response {
            assert!(
                side.iter().all(|s| s.abs() <= 1.0),
                "{case} passes full scale"
            );
            // Half a second at a time, from 2 s on: a 30 s tail falls 1 dB
            // in each.
            let window = (0.5 * rate) as usize;
            let levels: Vec<f64> = side[(2.0 * rate) as usize..]
                .chunks_exact(window)
                .map(|chunk| chunk.iter().map(|&s| f64::from(s).powi(2)).sum::<f64>())
                .collect();
            // At Depth 0% it's silent once the early burst is over.
            for pair in levels.windows(2) {
                assert!(
                    pair[1] < pair[0] || pair == [0.0, 0.0],
                    "{case} grows: {levels:?}"
                );
            }
        }
    }
}

/// The lines are set aside for the longest sizes: at the largest Late Size,
/// Early Size and Predelay, at every rate, every delay is the length its
/// seconds give, none held short by its line.
#[test]
fn every_line_holds_the_longest_sizes_at_every_rate() {
    let largest = settings(|s| {
        s.late_size = 1.0;
        s.early_size_seconds = ReverbSettings::MAX_EARLY_SIZE_SECONDS;
        s.predelay_seconds = ReverbSettings::MAX_PREDELAY_SECONDS;
    });
    for rate in RATES {
        let reverb = Reverb::new(largest, rate);
        let samples = |seconds: f64| (seconds * rate).round() as usize;
        assert_eq!(reverb.predelay_samples, samples(0.5), "{rate} Hz");
        for (step, delays) in ROOM.steps.iter().zip(reverb.lane_samples.iter()) {
            for (lane, &delay) in delays.iter().enumerate() {
                assert_eq!(delay, samples(lane_seconds(step, lane, 1.0)), "{rate} Hz");
            }
        }
        for (&seconds, &delay) in ROOM.lines.iter().zip(reverb.line_samples.iter()) {
            assert_eq!(delay, samples(seconds * 2.0), "{rate} Hz");
        }
    }
}

/// The diffuser's mix keeps the lanes' energy at every Diffusion, and at
/// full it's an even mix: a click in one lane comes out in all eight at
/// the same level.
#[test]
fn the_mix_keeps_the_energy_and_at_full_spreads_it_evenly() {
    for diffusion in [0.0f64, 0.3, 0.8, 1.0] {
        let angle = diffusion * FRAC_PI_4;
        let butterfly = (angle.cos() as f32, angle.sin() as f32);
        let mut lanes = [0.3, -1.0, 0.5, 0.0, 0.25, 0.7, -0.2, 0.1];
        let before: f32 = lanes.iter().map(|x| x * x).sum();
        mix(&mut lanes, butterfly);
        let after: f32 = lanes.iter().map(|x| x * x).sum();
        assert!((after - before).abs() < 1e-5, "Diffusion {diffusion}");
    }
    let mut lanes = [1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0];
    let even = (FRAC_PI_4.cos() as f32, FRAC_PI_4.sin() as f32);
    mix(&mut lanes, even);
    for lane in lanes {
        assert!((lane.abs() - 8f32.sqrt().recip()).abs() < 1e-6, "{lanes:?}");
    }
}

/// An arrangement built dark puts out less above its top than one that
/// isn't: Space's will be (RFC-010, "Modes").
#[test]
fn a_dark_arrangement_is_darker() {
    static DARK: Arrangement = Arrangement {
        top_hz: Some(2_000.0),
        ..ROOM
    };
    let rate = 48_000.0;
    let reverb = ReverbSettings::default();
    let bright = response_to(&mut Reverb::new(reverb, rate), [1.0, 1.0], rate, 1.0);
    let dark = response_to(
        &mut Reverb::arranged(&DARK, reverb, rate),
        [1.0, 1.0],
        rate,
        1.0,
    );
    let energy = |samples: &[f32], band: usize| -> f64 {
        impulse::octave_bands(samples, rate as u32)[band]
            .iter()
            .map(|s| s * s)
            .sum()
    };
    // 8 kHz loses most; 250 Hz keeps nearly everything.
    assert!(energy(&dark[0], 7) < 0.01 * energy(&bright[0], 7));
    assert!(energy(&dark[0], 2) > 0.9 * energy(&bright[0], 2));
}

/// It rings while its tail sounds, then sleeps with every line clear, and
/// asleep, silence in gives silence out. A new Reverb is asleep.
#[test]
fn it_rings_until_the_tail_dies_away_then_sleeps() {
    let rate = 48_000.0;
    let mut reverb = Reverb::new(settings(|s| s.decay_seconds = 0.5), rate);
    assert!(!reverb.is_ringing(), "a new Reverb is asleep");
    let [left, _] = response_to(&mut reverb, [1.0, 1.0], rate, 0.1);
    assert!(reverb.is_ringing());
    assert!(left.iter().any(|&s| s != 0.0));
    let mut seconds = 0.1;
    while reverb.is_ringing() {
        assert!(seconds < 10.0, "it never sleeps");
        response_to(&mut reverb, [0.0, 0.0], rate, 0.1);
        seconds += 0.1;
    }
    // At a 0.5 s Decay, falling 120 dB takes 1 s, then everything in its longest
    // path has to pass through.
    assert!(seconds > 1.0, "it slept after {seconds} s");
    let [left, right] = response_to(&mut reverb, [0.0, 0.0], rate, 1.0);
    assert!(left.iter().chain(&right).all(|&s| s == 0.0));
    assert!(!reverb.is_ringing());
}
