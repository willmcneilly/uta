//! The sync A/B (RFC-008, "How we'll verify it", manual check 2): a sync
//! sweep made two ways, for listening. One is the synced oscillator as
//! built; the other is the same oscillator run at eight times the sample
//! rate and brought back down through a steep low-pass filter, which leaves
//! almost no aliasing. If the two can be told apart by ear, the synced
//! oscillator is to be oversampled.
//!
//! Which is A and which is B is chosen at random on each run, and written
//! only to `sync-ab-key.txt` outside their folder, so the listener can't
//! come across it.
//!
//! ```text
//! cargo test -p uta-engine --release --lib sync_ab -- --ignored --nocapture
//! ```
//!
//! It writes `a.wav` and `b.wav` to `target/listen/sync-ab`, and the key to
//! `target/listen/sync-ab-key.txt`.

use std::f64::consts::PI;
use std::path::Path;

use super::Waveform;
use super::oscillator::{Oscillator, Synced};
use crate::EngineConfig;

const RATE: u32 = 48_000;
/// How many times faster the oversampled sweep runs.
const OVERSAMPLE: u32 = 8;
/// Both renders are cut above this, so they differ only below it.
const CUTOFF_HZ: f64 = 20_000.0;

/// Each note's sweep: oscillator 2 from oscillator 1's pitch up to just
/// under two octaves over `SWEEP_SECONDS` (exactly two octaves would sound
/// like a plain saw two octaves up), held there, then a gap.
const NOTES_HZ: [f64; 4] = [110.0, 220.0, 440.0, 880.0];
const SWEEP_SECONDS: f64 = 3.0;
const SWEEP_SEMITONES: f64 = 23.5;
const HOLD_SECONDS: f64 = 0.5;
const GAP_SECONDS: f64 = 0.4;
const LEVEL: f64 = 0.25;
const FADE_SECONDS: f64 = 0.01;

/// The sweep at `rate`: each note's synced saw, its pitch over oscillator
/// 1's rising smoothly by `SWEEP_SEMITONES`.
fn sweep(rate: f64) -> Vec<f64> {
    let note_samples = ((SWEEP_SECONDS + HOLD_SECONDS) * rate) as usize;
    let gap = (GAP_SECONDS * rate) as usize;
    let fade = FADE_SECONDS * rate;
    let mut out = Vec::new();
    for frequency in NOTES_HZ {
        let mut leader = Oscillator::default();
        let increment = frequency / rate;
        leader.set_increment(increment);
        let mut synced = Synced::default();
        synced.start_with(&leader, increment);
        for n in 0..note_samples {
            let t = n as f64 / rate;
            let semitones = SWEEP_SEMITONES * (t / SWEEP_SECONDS).min(1.0);
            synced.set_increment(increment * (semitones / 12.0).exp2());
            let step = synced.step(&leader);
            let value = f64::from(synced.value(Waveform::Saw, &step));
            synced.advance(&step);
            leader.advance();
            let n = n as f64;
            let envelope = (n / fade).min(1.0).min((note_samples as f64 - n) / fade);
            out.push(value * LEVEL * envelope);
        }
        out.extend(std::iter::repeat_n(0.0, gap));
    }
    out
}

/// A linear-phase low-pass at `cutoff_hz`, `taps` long (odd): a windowed
/// sinc, under a Blackman window, whose stop band is about 74 dB down.
fn low_pass(cutoff_hz: f64, rate: f64, taps: usize) -> Vec<f64> {
    let middle = (taps / 2) as f64;
    let fc = cutoff_hz / rate;
    let taps: Vec<f64> = (0..taps)
        .map(|n| {
            let x = n as f64 - middle;
            let sinc = if x == 0.0 {
                2.0 * fc
            } else {
                (2.0 * PI * fc * x).sin() / (PI * x)
            };
            let w = 2.0 * PI * n as f64 / (2.0 * middle);
            sinc * (0.42 - 0.5 * w.cos() + 0.08 * (2.0 * w).cos())
        })
        .collect();
    let sum: f64 = taps.iter().sum();
    taps.into_iter().map(|tap| tap / sum).collect()
}

/// `samples` filtered by `filter` and kept every `keep`th sample, lined up
/// so the filter adds no delay.
fn filter_and_keep(samples: &[f64], filter: &[f64], keep: usize) -> Vec<f32> {
    let middle = filter.len() / 2;
    (0..samples.len() / keep)
        .map(|out| {
            let centre = out * keep;
            let sum: f64 = filter
                .iter()
                .enumerate()
                .filter_map(|(k, tap)| {
                    (centre + k)
                        .checked_sub(middle)
                        .and_then(|i| samples.get(i))
                        .map(|s| s * tap)
                })
                .sum();
            sum as f32
        })
        .collect()
}

#[test]
#[ignore = "writes the sync A/B to listen to; run by hand"]
fn sync_ab() {
    let rate = f64::from(RATE);
    let as_built = filter_and_keep(&sweep(rate), &low_pass(CUTOFF_HZ, rate, 255), 1);
    let fast = f64::from(RATE * OVERSAMPLE);
    let oversampled = filter_and_keep(
        &sweep(fast),
        &low_pass(CUTOFF_HZ, fast, 2047),
        OVERSAMPLE as usize,
    );
    let length = as_built.len().min(oversampled.len());
    let (as_built, oversampled) = (&as_built[..length], &oversampled[..length]);

    let difference = as_built
        .iter()
        .zip(oversampled)
        .map(|(a, b)| f64::from(a - b).powi(2))
        .sum::<f64>();
    let level = as_built.iter().map(|a| f64::from(*a).powi(2)).sum::<f64>();
    println!(
        "The two differ by {:.1} dB against the sweep.",
        10.0 * (difference / level).log10()
    );

    let folder = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/listen/sync-ab");
    std::fs::create_dir_all(&folder).unwrap();
    let config = EngineConfig {
        sample_rate: RATE,
        channels: 1,
    };
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .subsec_nanos();
    let (a, b, key) = if nanos.is_multiple_of(2) {
        (as_built, oversampled, "A is as built; B is oversampled.")
    } else {
        (oversampled, as_built, "A is oversampled; B is as built.")
    };
    crate::offline::write_wav(&folder.join("a.wav"), config, a).unwrap();
    crate::offline::write_wav(&folder.join("b.wav"), config, b).unwrap();
    std::fs::write(folder.with_file_name("sync-ab-key.txt"), format!("{key}\n")).unwrap();
    println!("Wrote a.wav and b.wav to {}", folder.display());
}
