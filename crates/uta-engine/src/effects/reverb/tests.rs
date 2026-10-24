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
/// room to a cathedral. It's measured on the tail alone, at Depth 100%: at
/// a short Decay the early burst, a tenth as long, bends the fit (at 0.3 s
/// and Depth 50%, one side read 15% long).
#[test]
fn the_mid_band_decays_in_decay_within_10_percent() {
    for decay in [0.3, 2.0, 10.0] {
        let response = impulse_response(
            settings(|s| {
                s.decay_seconds = decay as f32;
                s.depth = 1.0;
            }),
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

/// At Depth 0% you hear only the early burst: nothing after it ends but
/// the High Cut's own ring, under −120 dB within a millisecond, though the
/// tail rings on inside. At 100% you hear only the tail:
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
    let rung = burst_ends + (0.001 * rate) as usize;
    assert!(
        left[rung..]
            .iter()
            .chain(&right[rung..])
            .all(|&s| s.abs() <= SILENCE),
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

/// At the longest Decay, with every setting at each of its ends, a
/// full-scale click never makes more than full scale, and once the tail has
/// built up it never grows. Each set of sizes and blends is tried with the
/// tone, the crosses and the drift at their defaults, and again at their
/// extremes: both Mults at their longest with the crossovers crossed, the
/// crosses at either end, the drift full on and fast, and the cuts at
/// their highest.
#[test]
fn at_30_seconds_and_every_extreme_the_output_never_grows_or_passes_full_scale() {
    use ReverbSettings as S;
    let rate = 48_000.0;
    let sizes = [
        (0.0, 0.0, 0.0, 0.002, 0.0, 0.0),
        (1.0, 1.0, 1.0, 1.0, 1.0, 0.5),
        (1.0, 0.0, 0.0, 0.002, 1.0, 0.0),
        (1.0, 1.0, 1.0, 0.002, 0.0, 0.0),
        (0.5, 1.0, 0.0, 1.0, 0.0, 0.5),
        (0.5, 0.0, 1.0, 0.002, 1.0, 0.0),
    ];
    for (n, (depth, diffusion, send, early, late, predelay)) in
        sizes.into_iter().cycle().take(2 * sizes.len()).enumerate()
    {
        let extremes = n >= sizes.len();
        let reverb = settings(|s| {
            s.decay_seconds = S::MAX_DECAY_SECONDS;
            s.depth = depth;
            s.diffusion = diffusion;
            s.early_send = send;
            s.early_size_seconds = early;
            s.late_size = late;
            s.predelay_seconds = predelay;
            if extremes {
                s.bass_mult = S::MAX_BASS_MULT;
                s.high_mult = S::MAX_HIGH_MULT;
                s.bass_xover_hz = S::MAX_BASS_XOVER_HZ;
                s.high_xover_hz = S::MIN_HIGH_XOVER_HZ;
                s.early_cross = (n % 2) as f32;
                s.late_cross = ((n + 1) % 2) as f32;
                s.early_mod_depth = 1.0;
                s.late_mod_depth = 1.0;
                s.early_mod_rate_hz = S::MAX_MOD_RATE_HZ;
                s.late_mod_rate_hz = S::MAX_MOD_RATE_HZ;
                s.low_cut_hz = S::MAX_LOW_CUT_HZ;
            }
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
        let length = |head: &Glide| head.value() as usize;
        assert_eq!(length(&reverb.predelay_head), samples(0.5), "{rate} Hz");
        for (step, heads) in ROOM.steps.iter().zip(reverb.lane_heads.iter()) {
            for (lane, head) in heads.iter().enumerate() {
                assert_eq!(
                    length(head),
                    samples(lane_seconds(step, lane, 1.0)),
                    "{rate} Hz"
                );
            }
        }
        for (&seconds, head) in ROOM.lines.iter().zip(reverb.line_heads.iter()) {
            assert_eq!(length(head), samples(seconds * 2.0), "{rate} Hz");
        }
    }
}

/// The diffuser's mix keeps the lanes' energy at every Diffusion and Early
/// Cross; at full both it's an even mix: a click in one lane comes out in
/// all eight at the same level. With no cross, a click in a left lane
/// stays in the left's lanes.
#[test]
fn the_mix_keeps_the_energy_and_at_full_spreads_it_evenly() {
    for diffusion in [0.0, 0.3, 0.8, 1.0] {
        for cross in [0.0, 0.5, 1.0] {
            let mut lanes = [0.3, -1.0, 0.5, 0.0, 0.25, 0.7, -0.2, 0.1];
            let before: f32 = lanes.iter().map(|x| x * x).sum();
            mix(&mut lanes, turn(diffusion), turn(cross));
            let after: f32 = lanes.iter().map(|x| x * x).sum();
            assert!(
                (after - before).abs() < 1e-5,
                "Diffusion {diffusion}, Early Cross {cross}"
            );
        }
    }
    let mut lanes = [1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0];
    mix(&mut lanes, turn(1.0), turn(1.0));
    for lane in lanes {
        assert!((lane.abs() - 8f32.sqrt().recip()).abs() < 1e-6, "{lanes:?}");
    }
    let mut lanes = [0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0];
    mix(&mut lanes, turn(1.0), turn(0.0));
    assert!(lanes[4..].iter().all(|&lane| lane == 0.0), "{lanes:?}");
    // The shuffle keeps each lane on its side.
    for (lane, &(to, _)) in ROOM.shuffle.iter().enumerate() {
        assert_eq!(
            lane < LANES / 2,
            to < LANES / 2,
            "lane {lane} moves to {to}"
        );
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

/// Each octave band's decay time at 48 kHz, in seconds, by its centre in
/// Hz.
fn decays(samples: &[f32]) -> impl Fn(f64) -> f64 {
    let decays = impulse::band_decay_times(samples, 48_000);
    move |hz| {
        let band = OCTAVE_BANDS_HZ.iter().position(|&b| b == hz).unwrap();
        decays[band].expect("the band decays")
    }
}

/// Bass Mult and High Mult set the lows' and highs' decay as a multiple of
/// the mids', which is Decay, within 15%: below Bass Xover, the octaves at
/// 63 and 125 Hz; above High Xover, the octave at 8 kHz. Every line's
/// shelves take its share of the band's loss for its own length, so every
/// resonance in a band fades at the band's rate. A Mult under 1 is tested
/// at a longer Decay, so its band's tail is long enough to measure: the
/// 63 Hz octave of a 1 s tail reads up to 12% long with no Mult at all.
#[test]
fn the_mults_set_the_lows_and_highs_decay_as_a_multiple_of_the_mids() {
    let rate = 48_000.0;
    for (bass_mult, high_mult, decay) in [
        (2.0, 1.0, 2.0),
        (1.0, 0.5, 2.0),
        (0.5, 1.0, 4.0),
        (1.0, 2.0, 2.0),
    ] {
        let reverb = settings(|s| {
            s.depth = 1.0;
            s.decay_seconds = decay;
            s.bass_mult = bass_mult;
            s.high_mult = high_mult;
            s.bass_xover_hz = 500.0;
            s.high_xover_hz = 2_000.0;
        });
        let response = impulse_response(reverb, rate, 9.0);
        for side in &response {
            let decays = decays(side);
            for (hz, mult) in [(62.5, bass_mult), (125.0, bass_mult), (8000.0, high_mult)] {
                let ratio = decays(hz) / f64::from(decay);
                assert!(
                    (ratio / f64::from(mult) - 1.0).abs() < 0.15,
                    "Bass Mult {bass_mult}, High Mult {high_mult}: {hz} Hz decays in {ratio:.2} times Decay"
                );
            }
        }
    }
}

/// The gain at `hz` of `process`, from a steady sine through it at 48 kHz.
fn gain_at(mut process: impl FnMut(f64) -> f64, hz: f64) -> f64 {
    let rate = 48_000.0;
    let samples = (rate * 0.2) as usize;
    let mut peak: f64 = 0.0;
    for n in 0..samples {
        let out = process((std::f64::consts::TAU * hz * n as f64 / rate).sin());
        if n > samples / 2 {
            peak = peak.max(out.abs());
        }
    }
    peak
}

/// However the Mults and crossovers are set, no line's tone keeps more of
/// any frequency than the band that lasts longest: its shelves go from one
/// level to the other without overshooting, and when both let their band
/// last longer the high shelf sits an octave above the low one at least,
/// so the two never add up to a gain of 1 or more, which would ring for
/// ever. Its longest line at the longest Decay is the closest to 1.
#[test]
fn no_line_keeps_more_than_its_longest_lasting_band_at_any_settings() {
    let rate = 48_000.0;
    use ReverbSettings as S;
    for (bass_mult, high_mult) in [
        (S::MAX_BASS_MULT, S::MAX_HIGH_MULT),
        (S::MIN_BASS_MULT, S::MAX_HIGH_MULT),
        (S::MAX_BASS_MULT, S::MIN_HIGH_MULT),
    ] {
        for (bass_xover, high_xover) in [
            (S::MAX_BASS_XOVER_HZ, S::MIN_HIGH_XOVER_HZ),
            (S::MIN_BASS_XOVER_HZ, S::MIN_HIGH_XOVER_HZ),
            (S::MAX_BASS_XOVER_HZ, S::MAX_HIGH_XOVER_HZ),
        ] {
            let reverb = Reverb::new(
                settings(|s| {
                    s.decay_seconds = S::MAX_DECAY_SECONDS;
                    s.late_size = 1.0;
                    s.bass_mult = bass_mult;
                    s.high_mult = high_mult;
                    s.bass_xover_hz = bass_xover;
                    s.high_xover_hz = high_xover;
                }),
                rate,
            );
            let longest = reverb.line_heads.len() - 1;
            let tone = reverb.tones[longest];
            let lap = reverb.line_heads[longest].value() / rate;
            let kept = |mult: f32| 10f64.powf(-3.0 * lap / (30.0 * f64::from(mult)));
            let most = kept(bass_mult.max(high_mult).max(1.0));
            for step in 0..=40 {
                let hz = 20.0 * 1000f64.powf(f64::from(step) / 40.0);
                let mut tone = tone;
                let gain = gain_at(|x| f64::from(tone.process(x as f32)), hz);
                assert!(
                    gain < 1.0 && gain <= most * 1.001,
                    "Mults {bass_mult} and {high_mult}, crossovers {bass_xover} and {high_xover}: \
                     it keeps {gain:.4} at {hz:.0} Hz, the longest-lasting band {most:.4}"
                );
            }
        }
    }
}

/// How much of `samples`' energy is in each octave band.
fn band_energies(samples: &[f32], rate: f64) -> Vec<f64> {
    impulse::octave_bands(samples, rate as u32)
        .iter()
        .map(|band| band.iter().map(|s| s * s).sum())
        .collect()
}

/// How much louder, in dB, `changed`'s impulse response is than
/// `reference`'s in each octave band, on the left.
fn band_changes(reference: ReverbSettings, changed: ReverbSettings) -> Vec<f64> {
    let rate = 48_000.0;
    let before = band_energies(&impulse_response(reference, rate, 3.0)[0], rate);
    let after = band_energies(&impulse_response(changed, rate, 3.0)[0], rate);
    before
        .iter()
        .zip(&after)
        .map(|(before, after)| 10.0 * (after / before).log10())
        .collect()
}

/// High Cut is a −12 dB an octave low-pass on the whole reverb: at 1 kHz,
/// the octave at 4 kHz is about 12 dB down, 8 kHz's 12 dB further, and
/// 250 Hz's untouched.
#[test]
fn high_cut_falls_12_db_an_octave() {
    let open = ReverbSettings::default();
    let changes = band_changes(open, settings(|s| s.high_cut_hz = 1_000.0));
    let at = |hz| changes[OCTAVE_BANDS_HZ.iter().position(|&b| b == hz).unwrap()];
    assert!(at(250.0).abs() < 0.5, "250 Hz changes {:.1} dB", at(250.0));
    assert!(
        (at(4000.0) + 24.0).abs() < 3.0,
        "4 kHz changes {:.1} dB",
        at(4000.0)
    );
    let slope = at(4000.0) - at(8000.0);
    assert!((slope - 12.0).abs() < 2.0, "{slope:.1} dB from 4 to 8 kHz");
}

/// Low Cut is a low shelf on the whole reverb, turning the lows down by
/// [`LOW_CUT_DB`]: at 500 Hz the octaves two and three below are about
/// that much down, and 4 kHz's untouched. At its lowest it's off: its
/// shelf passes everything exactly as it is.
#[test]
fn low_cut_is_a_low_shelf_off_at_its_lowest() {
    let open = ReverbSettings::default();
    let changes = band_changes(open, settings(|s| s.low_cut_hz = 500.0));
    let at = |hz| changes[OCTAVE_BANDS_HZ.iter().position(|&b| b == hz).unwrap()];
    for hz in [62.5, 125.0] {
        assert!(
            (at(hz) - LOW_CUT_DB).abs() < 3.0,
            "{hz} Hz changes {:.1} dB",
            at(hz)
        );
    }
    assert!(at(4000.0).abs() < 0.5, "4 kHz changes {:.1} dB", at(4000.0));

    let off = Reverb::new(open, 48_000.0).low_cut_shelf;
    assert_eq!((off.input, off.band, off.low), (1.0, 0.0, 0.0));
}

/// The left's and right's energy in `samples`.
fn side_energies([left, right]: &[Vec<f32>; 2]) -> [f64; 2] {
    [left, right].map(|side| side.iter().map(|&s| f64::from(s).powi(2)).sum())
}

/// With the crosses at 0, a sound only on the left stays on the left: the
/// early burst, and the tail, are two separate reverbs, one a side. At
/// full the two sides are about equal, within 1 dB, and unlike each other
/// (their correlation is near 0, the stereo spread at its widest); halfway,
/// the right has some of it.
#[test]
fn the_crosses_set_how_far_a_sound_on_the_left_spreads_to_the_right() {
    let rate = 48_000.0;
    for depth in [0.0, 1.0] {
        let at = |cross: f32| {
            let reverb = settings(|s| {
                s.depth = depth;
                s.early_cross = cross;
                s.late_cross = cross;
            });
            response_to(&mut Reverb::new(reverb, rate), [1.0, 0.0], rate, 3.0)
        };
        let [left, right] = side_energies(&at(0.0));
        assert!(left > 0.0);
        assert!(right < 1e-9 * left, "Depth {depth}: {right} on the right");

        let [left, right] = side_energies(&at(0.5));
        let db = 10.0 * (right / left).log10();
        assert!(
            (-12.0..-1.0).contains(&db),
            "Depth {depth}, halfway: {db:.1} dB"
        );

        let crossed = at(1.0);
        let [left, right] = side_energies(&crossed);
        let db = 10.0 * (right / left).log10();
        assert!(
            db.abs() < 1.0,
            "Depth {depth}: the right is {db:.1} dB from the left"
        );
        let [early, late] = impulse::early_and_late_correlation(&crossed[0], &crossed[1], 48_000);
        let correlation = if depth == 0.0 { early } else { late }.unwrap();
        assert!(
            correlation.abs() < 0.2,
            "Depth {depth}: correlation {correlation:.2}"
        );
    }
}

/// Depth's balance stays where it is whatever the Decay: in the mids, the
/// 1 kHz octave, which Decay sets, the tail's energy at Depth 100% against
/// the early burst's at 0% is within 1 dB of even, from 0.1 s to 30 s. The
/// Mults change the lows and highs on top: with the default High Mult of
/// 0.5, a short tail is darker than the burst, and a long one less so.
#[test]
fn changing_decay_does_not_change_the_balance_depth_sets() {
    let rate = 48_000.0;
    let mids = |response: [Vec<f32>; 2]| -> f64 {
        response
            .iter()
            .map(|side| band_energies(side, rate)[4])
            .sum()
    };
    for decay in [0.1, 0.3, 2.0, 10.0, 30.0] {
        let at = |depth| {
            let reverb = settings(|s| {
                s.decay_seconds = decay;
                s.depth = depth;
            });
            mids(impulse_response(reverb, rate, 2.5 * f64::from(decay) + 0.5))
        };
        let db = 10.0 * (at(1.0) / at(0.0)).log10();
        assert!(
            db.abs() < 1.0,
            "Decay {decay} s: the tail is {db:.1} dB from the burst"
        );
    }
}

/// The drift's steepest slope, in its whole range a cycle: each cycle it
/// moves to a new random place on a smoothstep curve, whose steepest slope
/// is 1.5 times its average, across a range of 2 (−1 to 1) at most.
const DRIFT_STEEPEST: f64 = 3.0;

/// Modulation full on, fast: both depths at full and both rates at their
/// fastest.
fn drifting(change: impl FnOnce(&mut ReverbSettings)) -> ReverbSettings {
    settings(|s| {
        s.early_mod_depth = 1.0;
        s.late_mod_depth = 1.0;
        s.early_mod_rate_hz = ReverbSettings::MAX_MOD_RATE_HZ;
        s.late_mod_rate_hz = ReverbSettings::MAX_MOD_RATE_HZ;
        s.early_size_seconds = 0.1;
        change(s);
    })
}

/// How fast its lines and lanes are read moving through them, at most, in
/// samples a sample, over `seconds` of a click and its tail, from the
/// second sample on.
fn fastest_drift(reverb: ReverbSettings, rate: f64, seconds: f64) -> f64 {
    let mut reverb = Reverb::new(reverb, rate);
    response_to(&mut reverb, [1.0, 1.0], rate, 1.0 / rate);
    reverb.fastest = 0.0;
    response_to(&mut reverb, [0.0, 0.0], rate, seconds);
    reverb.fastest
}

/// The drift moves where each line and lane is read no faster than its
/// depth and rate allow, so it bends the pitch no further: at most
/// [`DRIFT_STEEPEST`] × the rate × the line's share of it × the reach in
/// seconds, as a share of the pitch (at full Late Mod Depth and 5 Hz,
/// about 6%, a semitone; at 0.5 Hz, a tenth of that). Half the depth,
/// half the bend. It does move, at every rate.
#[test]
fn the_drift_bends_the_pitch_no_more_than_its_depth_and_rate_allow() {
    let fastest_line = ROOM.line_rates.iter().copied().fold(0.0, f64::max);
    let fastest_lane = ROOM.lane_rates.iter().copied().fold(0.0, f64::max);
    for rate in [48_000.0, 96_000.0] {
        for (depth, mod_rate) in [(1.0, 5.0), (0.5, 5.0), (1.0, 0.5)] {
            let late = drifting(|s| {
                s.early_mod_depth = 0.0;
                s.late_mod_depth = depth;
                s.late_mod_rate_hz = mod_rate;
            });
            let allowed =
                DRIFT_STEEPEST * f64::from(mod_rate * depth) * fastest_line * LATE_MOD_SECONDS;
            let fastest = fastest_drift(late, rate, 2.0);
            assert!(
                fastest <= allowed && fastest > 0.3 * allowed,
                "the lines at {rate} Hz, depth {depth}, {mod_rate} Hz: {fastest:.5}, allowed {allowed:.5}"
            );
            let early = drifting(|s| {
                s.late_mod_depth = 0.0;
                s.early_mod_depth = depth;
                s.early_mod_rate_hz = mod_rate;
            });
            let allowed =
                DRIFT_STEEPEST * f64::from(mod_rate * depth) * fastest_lane * EARLY_MOD_SECONDS;
            let fastest = fastest_drift(early, rate, 2.0);
            assert!(
                fastest <= allowed && fastest > 0.3 * allowed,
                "the lanes at {rate} Hz, depth {depth}, {mod_rate} Hz: {fastest:.5}, allowed {allowed:.5}"
            );
        }
    }
}

/// With the drift full on, two Reverbs given the same click give the same
/// samples: the drift comes from seeded noise, not from anything that
/// differs between runs.
#[test]
fn with_modulation_two_renders_are_identical() {
    let reverb = drifting(|_| {});
    let first = impulse_response(reverb, 48_000.0, 2.0);
    let again = impulse_response(reverb, 48_000.0, 2.0);
    assert_eq!(first, again);
    // And the drift changes the sound: it's on.
    assert_ne!(
        first,
        impulse_response(ReverbSettings::default(), 48_000.0, 2.0)
    );
}

/// Play starts the drift again from the same point: after a restart while
/// the tail rings, once the glide back is over, every line and lane drifts
/// exactly as a new Reverb's does. Meanwhile it glides there rather than
/// jumping: nothing is read moving faster than the drift itself moves.
#[test]
fn play_restarts_the_drift_from_the_same_point_gliding_while_it_rings() {
    let rate = 48_000.0;
    let reverb = drifting(|_| {});
    let mut played = Reverb::new(reverb, rate);
    response_to(&mut played, [1.0, 1.0], rate, 1.3);
    played.fastest = 0.0;
    played.restart();
    let mut new = Reverb::new(reverb, rate);
    // Woken by the same click a sample before, so both drifts move on from
    // here.
    response_to(&mut new, [1.0, 1.0], rate, 1.0 / rate);
    new.restart();
    let after = MOD_GLIDE_SECONDS + 0.1;
    response_to(&mut played, [0.0, 0.0], rate, after);
    response_to(&mut new, [0.0, 0.0], rate, after);
    for (played, new) in played
        .line_drifts
        .iter()
        .chain(&played.lane_drifts)
        .zip(new.line_drifts.iter().chain(&new.lane_drifts))
    {
        assert_eq!(played.value(), new.value());
    }
    let allowed = DRIFT_STEEPEST * 5.0 * 1.42 * LATE_MOD_SECONDS;
    assert!(played.fastest <= allowed, "{}", played.fastest);
}

/// The steepest step from one sample to the next in `samples`.
fn steepest(samples: &[f32]) -> f32 {
    samples
        .windows(2)
        .map(|pair| (pair[1] - pair[0]).abs())
        .fold(0.0, f32::max)
}

/// A setting's name, and how to set it `f` of the way from its default to
/// one of its ends.
type Turn = (&'static str, fn(&mut ReverbSettings, f32));

/// Every setting glides while the tail rings: turning any of them from its
/// default to either end, a step every block over a quarter of a second,
/// with a held A3 going in, never makes a step from one sample to the next
/// more than twice the steepest the Reverb makes of that tone, steady,
/// before or after. That's the sizes and Predelay too, which bend the pitch
/// of what's ringing as they glide, never by more than half again.
#[test]
fn turning_every_setting_while_the_tail_rings_does_not_click() {
    let rate = 48_000.0;
    let block = 128;
    let turns: Vec<Turn> = vec![
        ("Late Size up", |s, f| s.late_size = 0.5 + 0.5 * f),
        ("Late Size down", |s, f| s.late_size = 0.5 - 0.5 * f),
        ("Early Size up", |s, f| {
            s.early_size_seconds = 0.03 + 0.97 * f
        }),
        ("Early Size down", |s, f| {
            s.early_size_seconds = 0.03 - 0.028 * f
        }),
        ("Predelay", |s, f| s.predelay_seconds = 0.01 + 0.49 * f),
        ("Decay up", |s, f| s.decay_seconds = 2.0 + 28.0 * f),
        ("Decay down", |s, f| s.decay_seconds = 2.0 - 1.9 * f),
        ("Depth up", |s, f| s.depth = 0.5 + 0.5 * f),
        ("Depth down", |s, f| s.depth = 0.5 - 0.5 * f),
        ("High Cut", |s, f| s.high_cut_hz = 20_000.0 - 19_000.0 * f),
        ("Low Cut", |s, f| s.low_cut_hz = 20.0 + 980.0 * f),
        ("Early Cross", |s, f| s.early_cross = 1.0 - f),
        ("Late Cross", |s, f| s.late_cross = 1.0 - f),
        ("Diffusion", |s, f| s.diffusion = 0.8 - 0.8 * f),
        ("Early Send", |s, f| s.early_send = 0.9 - 0.9 * f),
        ("Early Mod Depth", |s, f| s.early_mod_depth = f),
        ("Late Mod Depth", |s, f| s.late_mod_depth = f),
        ("Early Mod Rate", |s, f| {
            s.early_mod_depth = 1.0;
            s.early_mod_rate_hz = 0.5 + 4.5 * f;
        }),
        ("Late Mod Rate", |s, f| {
            s.late_mod_depth = 1.0;
            s.late_mod_rate_hz = 0.5 + 4.5 * f;
        }),
        ("Bass Mult up", |s, f| s.bass_mult = 1.0 + 3.0 * f),
        ("Bass Mult down", |s, f| s.bass_mult = 1.0 - 0.75 * f),
        ("High Mult", |s, f| s.high_mult = 0.5 + 1.5 * f),
        ("Bass Xover", |s, f| s.bass_xover_hz = 500.0 + 1500.0 * f),
        ("High Xover", |s, f| s.high_xover_hz = 4_000.0 - 3_000.0 * f),
    ];
    let tone = |n: usize| (0.5 * (std::f64::consts::TAU * 220.0 * n as f64 / rate).sin()) as f32;
    for (name, turn) in turns {
        // Mod rates are turned with the drift on from the start.
        let mut start = ReverbSettings::default();
        turn(&mut start, 0.0);
        let mut reverb = Reverb::new(start, rate);
        let mut n = 0;
        let mut run = |reverb: &mut Reverb, seconds: f64| {
            let frames = (seconds * rate) as usize;
            let mut left: Vec<f32> = (n..n + frames).map(tone).collect();
            let mut right = left.clone();
            for (left, right) in left.chunks_mut(block).zip(right.chunks_mut(block)) {
                reverb.process(left, right);
            }
            n += frames;
            [left, right]
        };
        let ringing = run(&mut reverb, 1.5);
        let before =
            steepest(&ringing[0][rate as usize..]).max(steepest(&ringing[1][rate as usize..]));
        let steps = (0.25 * rate) as usize / block;
        let mut turned = [Vec::new(), Vec::new()];
        for step in 1..=steps {
            let mut settings = start;
            turn(&mut settings, step as f32 / steps as f32);
            reverb.set_settings(settings);
            let [left, right] = run(&mut reverb, block as f64 / rate);
            turned[0].extend(left);
            turned[1].extend(right);
        }
        let [left, right] = run(&mut reverb, 2.0);
        turned[0].extend(&left);
        turned[1].extend(&right);
        let after = steepest(&run(&mut reverb, 0.5)[0]);
        let limit = 2.0 * before.max(after);
        let jump = steepest(&turned[0]).max(steepest(&turned[1]));
        assert!(
            jump <= limit,
            "{name}: a step of {jump:.5}, twice the steady tone's steepest {limit:.5}"
        );
    }
}
