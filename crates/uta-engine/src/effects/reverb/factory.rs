//! The Reverb's factory sounds against their briefs: each one's impulse
//! response, through the real process path, measured with `uta-analysis`'s
//! reverb measurements as `uta analyse --impulse-response` takes them, and
//! checked against the targets its brief sets (RFC-010, "How we'll verify
//! it", Factory sounds). The briefs and targets are `uta-core`'s
//! (`reverb_factory.rs`).
//!
//! `cargo test -p uta-engine --release reverb::factory::print -- --ignored
//! --nocapture` prints every sound's measurements.

use super::tests::{sine_steepest, steepest};
use super::*;
use uta_analysis::impulse;
use uta_core::{REVERB_LOUDNESS_SPREAD_DB, ReverbFactorySound, ReverbTargets};

/// The rate the sounds are measured at. The Reverb's own tests check it
/// measures the same at every rate.
const RATE: f64 = 48_000.0;

/// What a factory sound's impulse response measures: each of
/// [`ReverbTargets`]' measurements.
#[derive(Debug)]
struct Measured {
    mid_decay_seconds: f64,
    low_decay_ratio: f64,
    high_decay_ratio: f64,
    onset_ms: f64,
    early_to_late_db: f64,
    dense_after_ms: Option<f64>,
    loudest_at_ms: f64,
    gone_by_ms: f64,
    /// Its whole impulse response, the mid, for the shares above and below
    /// a frequency, which each target names.
    mid: Vec<f32>,
    /// Only for a sound whose brief targets it: it takes a few seconds'
    /// processing of its own.
    drift_cents: Option<f64>,
    /// Its steady gain, absolute: the target, [`ReverbTargets::loudness_db`],
    /// is against the default's.
    steady_gain_db: f64,
}

impl Measured {
    fn share_above_percent(&self, hz: f64) -> f64 {
        100.0 * uta_analysis::share_above(&self.mid, RATE as u32, hz)
    }

    fn share_below_percent(&self, hz: f64) -> f64 {
        100.0 - self.share_above_percent(hz)
    }
}

/// How long to let `settings` ring to measure it: long enough for every
/// band to fall well past the 35 dB its decay is fitted to.
fn ring_seconds(settings: &ReverbSettings) -> f64 {
    let longest = f64::from(settings.decay_seconds * settings.bass_mult.max(1.0));
    f64::from(settings.predelay_seconds) + 0.8 * longest + 1.0
}

/// The impulse response of a Reverb with `settings`: its answer to a
/// full-scale click on both sides, in blocks of 128.
fn impulse_response(settings: ReverbSettings) -> [Vec<f32>; 2] {
    let frames = (ring_seconds(&settings) * RATE).round() as usize;
    let mut left = vec![0.0; frames];
    let mut right = vec![0.0; frames];
    left[0] = 1.0;
    right[0] = 1.0;
    let mut reverb = Reverb::new(settings, RATE);
    for (left, right) in left.chunks_mut(128).zip(right.chunks_mut(128)) {
        reverb.process(left, right);
    }
    [left, right]
}

/// How far the tail's pitch moves at the most, in cents: how fast any line
/// is read moving through it, from a quarter of a second into a click's
/// tail for the next second, or until it falls asleep, when its lines are
/// cleared.
fn drift_cents(settings: ReverbSettings) -> f64 {
    let mut reverb = Reverb::new(settings, RATE);
    let mut left = vec![0.0; 128];
    let mut right = vec![0.0; 128];
    left[0] = 1.0;
    right[0] = 1.0;
    reverb.process(&mut left, &mut right);
    let block = |seconds: f64| (seconds * RATE / 128.0) as usize;
    for n in 1..block(1.25) {
        if n == block(0.25) {
            reverb.fastest = 0.0;
        }
        left.fill(0.0);
        right.fill(0.0);
        reverb.process(&mut left, &mut right);
        assert!(reverb.is_ringing(), "the tail rings for the whole second");
    }
    1200.0 * (1.0 + reverb.fastest).log2()
}

/// When the level of `samples`, in windows of `window_seconds`, is
/// loudest, and when it last is within `drop_db` of that, each in ms: the
/// middle of the loudest window, and the end of the last one within it.
fn loudest_and_gone(samples: &[f32], window_seconds: f64, drop_db: f64) -> (f64, f64) {
    let envelope = uta_analysis::rms_envelope_db(samples, RATE as u32, window_seconds);
    let (loudest_at, loudest) =
        envelope
            .iter()
            .copied()
            .fold((0.0, f64::NEG_INFINITY), |best, (time, db)| {
                if db > best.1 { (time, db) } else { best }
            });
    let gone = envelope
        .iter()
        .rev()
        .find(|&&(_, db)| db >= loudest - drop_db)
        .map_or(0.0, |&(time, _)| time + window_seconds / 2.0);
    (1000.0 * loudest_at, 1000.0 * gone)
}

fn measure(settings: ReverbSettings, targets: &ReverbTargets) -> Measured {
    let [left, right] = impulse_response(settings);
    let mid: Vec<f32> = left
        .iter()
        .zip(&right)
        .map(|(l, r)| 0.5 * (l + r))
        .collect();
    let rate = RATE as u32;
    let bands = impulse::band_decay_times(&mid, rate);
    let band = |i: usize| bands[i].unwrap_or(f64::NAN);
    let onset = impulse::onset(&[&left, &right]).expect("it sounds") as f64 / RATE;
    let density = impulse::echo_density(&mid, rate);
    let (loudest_at_ms, _) = loudest_and_gone(&mid, 0.02, 20.0);
    let (_, gone_by_ms) = loudest_and_gone(&mid, 0.01, 20.0);
    Measured {
        mid_decay_seconds: band(4),
        low_decay_ratio: band(1) / band(4),
        high_decay_ratio: band(6) / band(4),
        onset_ms: 1000.0 * onset,
        early_to_late_db: impulse::early_to_late_db(&mid, rate).unwrap_or(f64::NAN),
        dense_after_ms: impulse::dense_after(&density, onset).map(|s| 1000.0 * s),
        loudest_at_ms,
        gone_by_ms,
        drift_cents: targets.drift_cents.as_ref().map(|_| drift_cents(settings)),
        steady_gain_db: impulse::steady_gain_db(&[&left, &right], rate).expect("it sounds"),
        mid,
    }
}

/// Targets for nothing, to measure the default Reverb by.
const NO_TARGETS: ReverbTargets = ReverbTargets {
    mid_decay_seconds: None,
    low_decay_ratio: None,
    high_decay_ratio: None,
    onset_ms: None,
    early_to_late_db: None,
    dense_after_ms: None,
    loudest_at_ms: None,
    gone_by_ms: None,
    most_above: None,
    most_below: None,
    drift_cents: None,
    loudness_db: 0.0..=0.0,
};

/// The default Reverb's steady gain, which every sound's loudness is
/// against.
fn default_gain_db() -> f64 {
    measure(ReverbSettings::default(), &NO_TARGETS).steady_gain_db
}

/// What's wrong with `measured` against `targets`, each as a line: empty if
/// it meets them all.
fn misses(targets: &ReverbTargets, measured: &Measured, default_gain_db: f64) -> Vec<String> {
    let mut misses = Vec::new();
    let mut check = |name: &str, range: &Option<std::ops::RangeInclusive<f64>>, value: f64| {
        if let Some(range) = range
            && !range.contains(&value)
        {
            misses.push(format!("{name} {value:.3}, not {range:?}"));
        }
    };
    check(
        "mid decay (s)",
        &targets.mid_decay_seconds,
        measured.mid_decay_seconds,
    );
    check(
        "low decay ratio",
        &targets.low_decay_ratio,
        measured.low_decay_ratio,
    );
    check(
        "high decay ratio",
        &targets.high_decay_ratio,
        measured.high_decay_ratio,
    );
    check("onset (ms)", &targets.onset_ms, measured.onset_ms);
    check(
        "early to late (dB)",
        &targets.early_to_late_db,
        measured.early_to_late_db,
    );
    check(
        "dense after (ms)",
        &targets.dense_after_ms,
        measured.dense_after_ms.unwrap_or(f64::INFINITY),
    );
    check(
        "loudest at (ms)",
        &targets.loudest_at_ms,
        measured.loudest_at_ms,
    );
    check("gone by (ms)", &targets.gone_by_ms, measured.gone_by_ms);
    check(
        "drift (cents)",
        &targets.drift_cents,
        measured.drift_cents.unwrap_or(f64::NAN),
    );
    check(
        "loudness (dB)",
        &Some(targets.loudness_db.clone()),
        measured.steady_gain_db - default_gain_db,
    );
    if let Some((hz, most)) = targets.most_above {
        let share = measured.share_above_percent(hz);
        if share > most {
            misses.push(format!("{share:.1}% above {hz} Hz, not at most {most}%"));
        }
    }
    if let Some((hz, most)) = targets.most_below {
        let share = measured.share_below_percent(hz);
        if share > most {
            misses.push(format!("{share:.1}% below {hz} Hz, not at most {most}%"));
        }
    }
    misses
}

/// Every factory sound's impulse response meets the targets its brief
/// sets: its decay per band, its onset, its early and late balance, its
/// density, its top and bottom, its drift and its loudness.
#[test]
fn every_reverb_factory_sound_meets_its_briefs_targets() {
    let default = default_gain_db();
    let failures: Vec<String> = ReverbFactorySound::all()
        .iter()
        .flat_map(|sound| {
            let measured = measure(sound.settings, &sound.targets);
            misses(&sound.targets, &measured, default)
                .into_iter()
                .map(move |miss| format!("{}: {miss}", sound.name))
        })
        .collect();
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// At the same Mix, stepping through the menu changes the space, not the
/// amount of reverb: from the quietest to the loudest on a steady sound,
/// the factory sounds are within [`REVERB_LOUDNESS_SPREAD_DB`].
#[test]
fn the_reverb_factory_sounds_loudness_is_within_its_spread() {
    let gains: Vec<(&str, f64)> = ReverbFactorySound::all()
        .iter()
        .map(|sound| {
            (
                sound.name,
                measure(sound.settings, &sound.targets).steady_gain_db,
            )
        })
        .collect();
    let quietest = gains.iter().map(|g| g.1).fold(f64::INFINITY, f64::min);
    let loudest = gains.iter().map(|g| g.1).fold(f64::NEG_INFINITY, f64::max);
    assert!(
        loudest - quietest <= REVERB_LOUDNESS_SPREAD_DB,
        "a spread of {:.2} dB: {gains:?}",
        loudest - quietest
    );
}

/// The check catches a sound that misses: the Drum room with a cathedral's
/// Decay and the Endless with a bright top miss their briefs.
#[test]
fn the_brief_check_catches_a_sound_that_misses() {
    let default = default_gain_db();
    let room = ReverbFactorySound::find("drum_room").unwrap();
    let long = ReverbSettings {
        decay_seconds: 7.0,
        ..room.settings
    };
    let missed = misses(&room.targets, &measure(long, &room.targets), default);
    assert!(
        missed.iter().any(|miss| miss.starts_with("mid decay")),
        "{missed:?}"
    );
    let endless = ReverbFactorySound::find("endless").unwrap();
    let bright = ReverbSettings {
        high_cut_hz: ReverbSettings::MAX_HIGH_CUT_HZ,
        high_mult: 1.0,
        ..endless.settings
    };
    let missed = misses(
        &endless.targets,
        &measure(bright, &endless.targets),
        default,
    );
    assert!(
        missed.iter().any(|miss| miss.contains("above")),
        "{missed:?}"
    );
}

/// Prints every factory sound's measurements, for tuning them.
#[test]
#[ignore]
fn print() {
    let default = default_gain_db();
    println!("default steady gain {default:.2} dB");
    for sound in ReverbFactorySound::all() {
        let m = measure(sound.settings, &sound.targets);
        let t = &sound.targets;
        let above = t
            .most_above
            .map(|(hz, _)| format!(" above {hz}: {:.1}%", m.share_above_percent(hz)))
            .unwrap_or_default();
        let below = t
            .most_below
            .map(|(hz, _)| format!(" below {hz}: {:.1}%", m.share_below_percent(hz)))
            .unwrap_or_default();
        println!(
            "{:13} mid {:6.2}s low× {:.2} high× {:.2} onset {:6.1}ms E/L {:6.1}dB dense {:>6} loudest {:5.0}ms gone {:6.0}ms drift {:5.1}c loud {:+.2}dB{above}{below}",
            sound.name,
            m.mid_decay_seconds,
            m.low_decay_ratio,
            m.high_decay_ratio,
            m.onset_ms,
            m.early_to_late_db,
            m.dense_after_ms
                .map_or("-".to_string(), |ms| format!("{ms:.1}ms")),
            m.loudest_at_ms,
            m.gone_by_ms,
            m.drift_cents.unwrap_or(f64::NAN),
            m.steady_gain_db - default,
        );
        for miss in misses(t, &m, default) {
            println!("    MISS {miss}");
        }
    }
}

/// Choosing one factory sound after another while a held A3 rings through
/// it, every sound from every other, never clicks, mode changes included:
/// no step from one sample to the next is more than twice the steepest a
/// sine as loud as what comes out could make at 330 Hz, the A3 bent up by
/// half again, as far as a gliding size or Predelay bends it (the limit the
/// Reverb's own turning test uses). A change of mode fades the old tail out
/// and the new one in, so it never jumps either.
#[test]
fn choosing_one_factory_sound_after_another_while_it_rings_does_not_click() {
    let block = 128;
    let tone = |n: usize| (0.5 * (std::f64::consts::TAU * 220.0 * n as f64 / RATE).sin()) as f32;
    let sounds = ReverbFactorySound::all();
    let mut worst = (0.0f32, "", "");
    for from in &sounds {
        for to in sounds.iter().filter(|to| to.key != from.key) {
            let mut reverb = Reverb::new(from.settings, RATE);
            let mut n = 0;
            let mut run = |reverb: &mut Reverb, seconds: f64| {
                let frames = (seconds * RATE) as usize;
                let mut left: Vec<f32> = (n..n + frames).map(tone).collect();
                let mut right = left.clone();
                for (left, right) in left.chunks_mut(block).zip(right.chunks_mut(block)) {
                    reverb.process(left, right);
                }
                n += frames;
                [left, right]
            };
            let [before_left, before_right] = run(&mut reverb, 0.5);
            reverb.set_settings(to.settings);
            let [left, right] = run(&mut reverb, 0.5);
            // The last block before the switch, and everything after it.
            let tail = before_left.len() - block;
            let left: Vec<f32> = before_left[tail..].iter().chain(&left).copied().collect();
            let right: Vec<f32> = before_right[tail..].iter().chain(&right).copied().collect();
            let peak = left
                .iter()
                .chain(&right)
                .fold(0.0f32, |peak, s| peak.max(s.abs()));
            let limit = 2.0 * peak * sine_steepest(330.0, RATE);
            let jump = steepest(&left).max(steepest(&right));
            assert!(
                jump <= limit,
                "{} to {}: a step of {jump:.5}, the limit {limit:.5}",
                from.name,
                to.name
            );
            if jump / limit > worst.0 {
                worst = (jump / limit, from.name, to.name);
            }
        }
    }
    // It tests something: some switch comes within reach of the limit.
    assert!(
        worst.0 > 0.1,
        "the steepest step was {worst:?} of the limit"
    );
}
