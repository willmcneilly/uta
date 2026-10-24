//! Measurements of rendered audio, taken from the waveform: pitch, level,
//! loudness, clicks, brightness, noisiness, inharmonicity, aliasing,
//! stereo, attack and decay, and, in [`impulse`], a reverb's impulse
//! response. The sound tests check
//! Uta's renders with them, and `uta analyse` prints them as a [`report`],
//! so the numbers read from a render are the numbers the tests check.
//!
//! This crate only reads samples. It has no project, no engine, no Tauri and
//! no audio thread. Every measurement that depends on time or frequency
//! takes the sample rate, and its FFTs (from `realfft`) take any length, so
//! windows are sized in milliseconds rather than in powers of two.
//!
//! # Adding a measurement
//!
//! A measurement is a function here and one entry in
//! [`report::MEASUREMENTS`]:
//!
//! 1. Write the function, taking samples (and the sample rate, if it needs
//!    it), with a doc comment saying what it measures and how.
//! 2. Test it on a synthetic signal whose answer is known, such as a sine
//!    of a known frequency or a decay of a known rate.
//! 3. Add an entry to [`report::MEASUREMENTS`] (or, for a measurement of
//!    an impulse response, [`report::IMPULSE_MEASUREMENTS`]), with its key,
//!    unit and what it measures. The text report, `--against` and `--json` all come from
//!    that table, so nothing else changes.

pub mod impulse;
pub mod report;

use std::f64::consts::TAU;
use std::sync::Arc;

use realfft::num_complex::Complex;
use realfft::{ComplexToReal, RealFftPlanner, RealToComplex};

/// The index of the first sample that isn't silent.
pub fn first_sound(samples: &[f32]) -> Option<usize> {
    samples.iter().position(|&s| s != 0.0)
}

/// The frequency of a steady tone, from the time between its first and last
/// upward zero crossings (interpolated between samples).
pub fn measure_frequency(samples: &[f32], sample_rate: u32) -> f64 {
    let crossings = upward_crossings(samples);
    let (&first, &last) = (
        crossings
            .first()
            .expect("no zero crossings: is the tone playing?"),
        crossings.last().expect("a first crossing is a last"),
    );
    assert!(
        crossings.len() >= 2,
        "need at least two cycles to measure pitch"
    );
    (crossings.len() - 1) as f64 * f64::from(sample_rate) / (last - first)
}

/// Where `samples` crosses zero going up, in samples, interpolated between
/// the two either side.
fn upward_crossings(samples: &[f32]) -> Vec<f64> {
    samples
        .windows(2)
        .enumerate()
        .filter_map(|(i, pair)| {
            let (a, b) = (f64::from(pair[0]), f64::from(pair[1]));
            (a < 0.0 && b >= 0.0).then(|| i as f64 + a / (a - b))
        })
        .collect()
}

/// The average pitch of a pitch track (as [`pitch_track`] or
/// [`pitch_over_time`] gives it), in Hz: the harmonic mean of its points,
/// which for zero-crossing cycles is how many there are over how long they
/// last. `None` if there are fewer than two points, or if the pitch jumps
/// about from one point to the next: if the median change is over 5%. A
/// kick's pitch falls smoothly, by far less than that a point once it
/// settles, but noise has no steady period.
pub fn steady_pitch(cycles: &[(f64, f64)]) -> Option<f64> {
    if cycles.len() < 2 {
        return None;
    }
    let mut changes: Vec<f64> = cycles
        .windows(2)
        .map(|pair| (pair[1].1 / pair[0].1).ln().abs())
        .collect();
    changes.sort_by(f64::total_cmp);
    if changes[changes.len() / 2] > 1.05f64.ln() {
        return None;
    }
    let seconds: f64 = cycles.iter().map(|&(_, hz)| 1.0 / hz).sum();
    Some(cycles.len() as f64 / seconds)
}

pub fn rms(samples: &[f32]) -> f64 {
    let sum: f64 = samples.iter().map(|&s| f64::from(s).powi(2)).sum();
    (sum / samples.len() as f64).sqrt()
}

pub fn peak(samples: &[f32]) -> f32 {
    samples.iter().fold(0.0f32, |peak, s| peak.max(s.abs()))
}

/// The peak over the RMS: 1 for a square wave, √2 for a sine, and higher
/// the punchier a sound is. Squashed, compressed sounds read low.
pub fn crest_factor(samples: &[f32]) -> f64 {
    f64::from(peak(samples)) / rms(samples)
}

/// How long a sound takes to rise from 10% to 90% of its peak, in seconds:
/// its attack, from [`attack`]. `None` for silence.
pub fn attack_seconds(samples: &[f32], sample_rate: u32) -> Option<f64> {
    attack(samples, sample_rate).map(|attack| (attack.to - attack.from) / f64::from(sample_rate))
}

/// Where a sound's attack is: the rise from 10% to 90% of its peak.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Attack {
    /// The sample the rise is measured from.
    pub start: usize,
    /// Where it reaches 10% of its peak, in samples after `start`.
    pub from: f64,
    /// Where it reaches 90%, in samples after `start`.
    pub to: f64,
}

impl Attack {
    /// When it reaches 10% and 90% of its peak, in seconds from the first
    /// sample.
    pub fn seconds(self, sample_rate: u32) -> (f64, f64) {
        let rate = f64::from(sample_rate);
        let start = self.start as f64;
        ((start + self.from) / rate, (start + self.to) / rate)
    }
}

/// Where a sound rises from 10% to 90% of its peak. It's the rise into the
/// loudest sample, from the last 5 ms before it whose loudest sample is
/// under 10% of the peak, so in a file of several hits it's the loudest
/// hit's. Within that, it follows the running peak (the loudest sample so
/// far), interpolated between samples, so it reads a tone's rise to within
/// half a cycle. `None` for silence.
pub fn attack(samples: &[f32], sample_rate: u32) -> Option<Attack> {
    let (loudest, peak) = samples.iter().enumerate().fold((0, 0.0f32), |max, (i, s)| {
        if s.abs() > max.1 { (i, s.abs()) } else { max }
    });
    if peak == 0.0 {
        return None;
    }
    let window = ((0.005 * f64::from(sample_rate)) as usize).max(1);
    let start = (0..loudest / window)
        .rev()
        .map(|k| k * window)
        .find(|&at| self::peak(&samples[at..at + window]) < 0.1 * peak)
        .unwrap_or(0);
    let rise = &samples[start..=loudest];
    // Where the running peak first reaches `level`, in samples.
    let reaches = |level: f32| {
        let mut so_far = 0.0f32;
        for (i, &sample) in rise.iter().enumerate() {
            let here = sample.abs();
            if here >= level {
                return i as f64 - f64::from(here - level) / f64::from(here - so_far);
            }
            so_far = so_far.max(here);
        }
        unreachable!("the rise ends at the peak, which reaches every level below it")
    };
    Some(Attack {
        start,
        from: reaches(0.1 * peak),
        to: reaches(0.9 * peak),
    })
}

/// The points of a pitch track (from [`pitch_track`] or
/// [`pitch_over_time`]) whose time is within `within_db` of the sound's
/// loudest, from its level over 5 ms windows: the tone, without its quiet
/// tail or the gaps between notes.
pub fn while_loud(
    track: Vec<(f64, f64)>,
    samples: &[f32],
    sample_rate: u32,
    within_db: f64,
) -> Vec<(f64, f64)> {
    let envelope = envelope_db(samples, sample_rate, 0.005);
    let loudest = envelope.iter().fold(f64::MIN, |a, &(_, db)| a.max(db));
    let window = (0.005 * f64::from(sample_rate)) as usize;
    track
        .into_iter()
        .filter(|&(seconds, _)| {
            let at = (seconds * f64::from(sample_rate)) as usize / window;
            envelope
                .get(at)
                .is_some_and(|&(_, db)| db >= loudest - within_db)
        })
        .collect()
}

/// The lowest pitch [`pitch_track`] looks for, in Hz: below a kick's.
pub const LOWEST_PITCH_HZ: f64 = 30.0;

/// The highest pitch [`pitch_track`] looks for, in Hz.
pub const HIGHEST_PITCH_HZ: f64 = 2000.0;

/// The pitch over time, with YIN (de Cheveigné and Kawahara, 2002): in
/// windows of 2048 samples every 5 ms, the lag at which the sound best
/// matches itself is its period. Unlike zero crossings, it follows the
/// fundamental of a tone that crosses zero more than once a cycle, as one
/// with strong upper harmonics or a chord can. A window is 43 ms, so a
/// sweep faster than that is smoothed: a kick's first few milliseconds read
/// lower than they are, and [`pitch_over_time`] sees them better on a clean
/// tone. Each point is a window's middle, in seconds, and its pitch in Hz,
/// between [`LOWEST_PITCH_HZ`] and [`HIGHEST_PITCH_HZ`]; a window with no
/// clear period, such as noise or silence, has no point. A window needs
/// 2048 samples and the longest period after it, so a file under about
/// 76 ms has no points, and its last 55 ms or so has none either.
pub fn pitch_track(samples: &[f32], sample_rate: u32) -> Vec<(f64, f64)> {
    let rate = f64::from(sample_rate);
    let window = 2048;
    let longest = (rate / LOWEST_PITCH_HZ).ceil() as usize;
    let shortest = (rate / HIGHEST_PITCH_HZ).floor() as usize;
    let hop = (0.005 * rate) as usize;
    let n = (2 * window + longest).next_power_of_two();
    let mut planner = RealFftPlanner::<f64>::new();
    let (forward, inverse) = (planner.plan_fft_forward(n), planner.plan_fft_inverse(n));
    let mut track = Vec::new();
    let mut start = 0;
    while start + window + longest < samples.len() {
        let frame = &samples[start..start + window + longest + 1];
        if let Some(lag) = yin_period(frame, window, shortest, longest, &forward, &inverse) {
            track.push(((start + window / 2) as f64 / rate, rate / lag));
        }
        start += hop;
    }
    track
}

/// YIN's period of `frame`'s first `window` samples, in samples, searched
/// between `shortest` and `longest`: the first dip in its cumulative mean
/// normalised difference below 0.15, refined between lags with a parabola.
/// `None` if there's no dip that deep. `frame` holds `window + longest + 1`
/// samples, and the FFTs' length is at least `2 * window + longest`, so the
/// correlation doesn't wrap around.
fn yin_period(
    frame: &[f32],
    window: usize,
    shortest: usize,
    longest: usize,
    forward: &Arc<dyn RealToComplex<f64>>,
    inverse: &Arc<dyn ComplexToReal<f64>>,
) -> Option<f64> {
    let energy: Vec<f64> = std::iter::once(0.0)
        .chain(frame.iter().scan(0.0, |sum, &s| {
            *sum += f64::from(s) * f64::from(s);
            Some(*sum)
        }))
        .collect();
    let energy_at = |lag: usize| energy[lag + window] - energy[lag];
    if energy_at(0) < 1e-12 {
        return None;
    }
    // The difference at each lag is the energy of the window, plus that of
    // the window that far on, minus twice their correlation, which comes
    // from one FFT of each and an inverse.
    let transform = |samples: &[f32]| {
        let mut input = forward.make_input_vec();
        for (to, &from) in input.iter_mut().zip(samples) {
            *to = f64::from(from);
        }
        let mut output = forward.make_output_vec();
        forward
            .process(&mut input, &mut output)
            .expect("the buffers are the plan's");
        output
    };
    let (a, b) = (transform(&frame[..window]), transform(frame));
    // The correlation is the inverse FFT of conj(A)·B. The first and last
    // bins of a real signal's spectrum are real, and the inverse insists.
    let mut product: Vec<Complex<f64>> = a.iter().zip(&b).map(|(a, b)| a.conj() * b).collect();
    let last = product.len() - 1;
    (product[0].im, product[last].im) = (0.0, 0.0);
    let mut correlated = inverse.make_output_vec();
    inverse
        .process(&mut product, &mut correlated)
        .expect("the buffers are the plan's");
    let n = inverse.len() as f64;
    let correlation = |lag: usize| correlated[lag] / n;

    let difference = |lag: usize| (energy_at(0) + energy_at(lag) - 2.0 * correlation(lag)).max(0.0);
    let mut normalised = vec![1.0; longest + 2];
    let mut sum = 0.0;
    for (lag, value) in normalised.iter_mut().enumerate().skip(1).take(longest + 1) {
        let d = difference(lag);
        sum += d;
        *value = if sum > 0.0 { d * lag as f64 / sum } else { 1.0 };
    }
    let mut lag = (shortest..=longest).find(|&lag| normalised[lag] < 0.15)?;
    while lag < longest && normalised[lag + 1] < normalised[lag] {
        lag += 1;
    }
    let (a, b, c) = (normalised[lag - 1], normalised[lag], normalised[lag + 1]);
    let curve = a - 2.0 * b + c;
    let offset = if curve > 0.0 {
        0.5 * (a - c) / curve
    } else {
        0.0
    };
    Some(lag as f64 + offset.clamp(-0.5, 0.5))
}

/// The biggest sample-to-sample jump, and where it happens.
pub fn max_jump(samples: &[f32]) -> (f32, usize) {
    samples
        .windows(2)
        .enumerate()
        .map(|(i, pair)| ((pair[1] - pair[0]).abs(), i + 1))
        .fold(
            (0.0, 0),
            |max, jump| if jump.0 > max.0 { jump } else { max },
        )
}

/// The steepest a full-scale sine at `frequency_hz` ever moves in one sample.
/// Any jump well above this is a click, not the tone.
pub fn sine_max_step(frequency_hz: f64, sample_rate: u32) -> f32 {
    (2.0 * (std::f64::consts::PI * frequency_hz / f64::from(sample_rate)).sin()) as f32
}

/// The largest difference between two renders, which must be the same length.
pub fn max_difference(a: &[f32], b: &[f32]) -> f32 {
    assert_eq!(a.len(), b.len(), "renders differ in length");
    a.iter()
        .zip(b)
        .fold(0.0f32, |max, (x, y)| max.max((x - y).abs()))
}

/// The magnitude spectrum of `samples`, any length, under a 4-term
/// Blackman-Harris window, whose side lobes are 92 dB down, so a strong
/// partial doesn't hide quiet ones far from it. Bin `k` is
/// `k * sample_rate / samples.len()` Hz; only the bins up to half the sample
/// rate are returned.
pub fn spectrum(samples: &[f32]) -> Vec<f64> {
    let n = samples.len() as f64;
    let window = |i: usize| {
        let x = TAU * i as f64 / n;
        0.35875 - 0.48829 * x.cos() + 0.14128 * (2.0 * x).cos() - 0.01168 * (3.0 * x).cos()
    };
    real_fft(
        samples
            .iter()
            .enumerate()
            .map(|(i, &s)| f64::from(s) * window(i)),
    )
    .iter()
    .map(|bin| bin.norm())
    .collect()
}

/// The average power spectrum of `samples`, in frames of `frame` samples
/// (any length) that overlap by half, each under a Hann window: Welch's
/// method. One frame of noise is spiky, bin to bin; the average of many is
/// smooth, so noise reads as the flat spectrum it is. Bin `k` is
/// `k * sample_rate / frame` Hz, up to half the sample rate. A sound shorter
/// than a frame is one frame, with silence after it.
pub fn average_power_spectrum(samples: &[f32], frame: usize) -> Vec<f64> {
    let fft = RealFftPlanner::<f64>::new().plan_fft_forward(frame);
    let window: Vec<f64> = (0..frame)
        .map(|i| 0.5 - 0.5 * (TAU * i as f64 / frame as f64).cos())
        .collect();
    let mut power = vec![0.0; frame / 2 + 1];
    let mut frames = 0;
    let (mut input, mut output) = (fft.make_input_vec(), fft.make_output_vec());
    for start in (0..=samples.len().saturating_sub(frame)).step_by((frame / 2).max(1)) {
        input.fill(0.0);
        for ((to, &from), w) in input.iter_mut().zip(&samples[start..]).zip(&window) {
            *to = f64::from(from) * w;
        }
        fft.process(&mut input, &mut output)
            .expect("the buffers are the plan's");
        for (sum, bin) in power.iter_mut().zip(&output) {
            *sum += bin.norm_sqr();
        }
        frames += 1;
    }
    power.iter_mut().for_each(|sum| *sum /= f64::from(frames));
    power
}

/// The loudest sample within `window / 2` samples of `at`. With a window of
/// at least a cycle, it's a tone's level at that point.
pub fn level_at(samples: &[f32], at: usize, window: usize) -> f32 {
    let start = at.saturating_sub(window / 2);
    let end = (at + window / 2).min(samples.len());
    peak(&samples[start..end])
}

/// The pitch over time of a tone that may glide, from each cycle between
/// upward zero crossings (interpolated between samples): each cycle's
/// frequency, at the time of its middle, in seconds from the first sample.
/// For low, clean tones such as a kick or a tom; noise has no pitch.
pub fn pitch_over_time(samples: &[f32], sample_rate: u32) -> Vec<(f64, f64)> {
    let rate = f64::from(sample_rate);
    upward_crossings(samples)
        .windows(2)
        .map(|pair| ((pair[0] + pair[1]) / 2.0 / rate, rate / (pair[1] - pair[0])))
        .collect()
}

/// A pitch track (as [`pitch_track`] or [`pitch_over_time`] gives it)
/// with each point's pitch replaced by the median of it and the two points
/// either side, so one or two odd points, such as where one note ends and
/// the next starts, don't count. Nearer the ends it takes as many either
/// side as there are, so a steady glide, as a kick's, is kept; the first
/// and last points take the median of the three at that end.
pub fn smoothed_pitch(cycles: &[(f64, f64)]) -> Vec<(f64, f64)> {
    let n = cycles.len();
    (0..n)
        .map(|i| {
            let around = match i {
                _ if n < 3 => i..i + 1,
                0 => 0..3,
                _ if i == n - 1 => n - 3..n,
                _ => {
                    let half = 2.min(i).min(n - 1 - i);
                    i - half..i + half + 1
                }
            };
            let mut around: Vec<f64> = cycles[around].iter().map(|&(_, hz)| hz).collect();
            around.sort_by(f64::total_cmp);
            (cycles[i].0, around[around.len() / 2])
        })
        .collect()
}

/// The level over time, in dB relative to full scale: the loudest sample in
/// each `window_seconds`, at the time of the window's middle.
pub fn envelope_db(samples: &[f32], sample_rate: u32, window_seconds: f64) -> Vec<(f64, f64)> {
    let window = ((window_seconds * f64::from(sample_rate)) as usize).max(1);
    samples
        .chunks(window)
        .enumerate()
        .map(|(i, chunk)| {
            let time = (i as f64 + 0.5) * window as f64 / f64::from(sample_rate);
            (time, 20.0 * f64::from(peak(chunk)).max(1e-12).log10())
        })
        .collect()
}

/// How long a sound takes to die away by `drop_db` from its loudest point,
/// in seconds, from its level over 5 ms windows, interpolated in dB between
/// the two windows either side of the drop. `None` if it never drops that
/// far.
pub fn decay_seconds(samples: &[f32], sample_rate: u32, drop_db: f64) -> Option<f64> {
    let envelope = envelope_db(samples, sample_rate, 0.005);
    let (loudest, &(peak_time, peak_db)) = envelope
        .iter()
        .enumerate()
        .max_by(|a, b| a.1.1.total_cmp(&b.1.1))?;
    let target = peak_db - drop_db;
    let after = &envelope[loudest..];
    let below = after.iter().position(|&(_, db)| db < target)?;
    let (t0, db0) = after[below - 1];
    let (t1, db1) = after[below];
    let time = t0 + (t1 - t0) * (db0 - target) / (db0 - db1);
    Some(time - peak_time)
}

/// The spectral centroid of `samples`, in Hz: the average frequency, weighted
/// by each frequency's magnitude. A measure of brightness. The samples are
/// zero-padded to a power of two.
pub fn spectral_centroid(samples: &[f32], sample_rate: u32) -> f64 {
    let n = samples.len().next_power_of_two();
    let mut padded = samples.to_vec();
    padded.resize(n, 0.0);
    let bins = spectrum_unwindowed(&padded);
    let bin_hz = f64::from(sample_rate) / n as f64;
    let total: f64 = bins.iter().sum();
    let weighted: f64 = bins
        .iter()
        .enumerate()
        .map(|(k, magnitude)| k as f64 * bin_hz * magnitude)
        .sum();
    weighted / total
}

/// The share of `samples`' energy above `hz`, from 0 to 1: how much of a
/// sound is its top end. The samples are zero-padded to a power of two.
pub fn share_above(samples: &[f32], sample_rate: u32, hz: f64) -> f64 {
    let n = samples.len().next_power_of_two();
    let mut padded = samples.to_vec();
    padded.resize(n, 0.0);
    let bins = spectrum_unwindowed(&padded);
    let first = (hz * n as f64 / f64::from(sample_rate)).ceil() as usize;
    let energy = |bins: &[f64]| bins.iter().map(|m| m * m).sum::<f64>();
    energy(&bins[first.min(bins.len())..]) / energy(&bins)
}

/// How loud a hit sounds, in dB: its K-weighted RMS over the `seconds` from
/// its first sound. K-weighting is the filter loudness meters put before
/// measuring (ITU-R BS.1770-4): a high shelf of about +4 dB above 2 kHz,
/// where the ear is most sensitive, and a high-pass that hears less of the
/// lowest bass. Plain RMS would rank a kick's 50 Hz far louder than it
/// sounds next to a hat. A hit is shorter than BS.1770's 400 ms blocks, so
/// this is its level over the time the ear sums it, not a loudness in LUFS.
pub fn hit_loudness_db(samples: &[f32], sample_rate: u32, seconds: f64) -> f64 {
    let start = first_sound(samples).expect("the hit sounds");
    let end = (start + (seconds * f64::from(sample_rate)) as usize).min(samples.len());
    let weighted = k_weighted(&samples[start..end], sample_rate);
    20.0 * rms(&weighted).max(1e-12).log10()
}

/// The window of BS.1770's momentary loudness, in seconds.
pub const MOMENTARY_SECONDS: f64 = 0.4;

/// The window of BS.1770's short-term loudness, in seconds.
pub const SHORT_TERM_SECONDS: f64 = 3.0;

/// The loudness of each `window_seconds` of a sound, in LUFS, by ITU-R
/// BS.1770-4: K-weight each channel, take its mean square over the window,
/// add the channels' together (every channel weighs 1, as left, right and
/// centre do in BS.1770), and that's -0.691 + 10 log10 of the sum. With
/// [`MOMENTARY_SECONDS`] it's the momentary loudness, and with
/// [`SHORT_TERM_SECONDS`] the short-term. There's a window starting at every
/// sample, so the loudest of them is the loudest the sound gets wherever it
/// falls; one shorter than the window is one window, with silence after it,
/// as a meter would hear it. A stereo 1 kHz sine at -23 dBFS reads -23.0.
pub fn loudness_over_time(channels: &[&[f32]], sample_rate: u32, window_seconds: f64) -> Vec<f64> {
    let window = ((window_seconds * f64::from(sample_rate)) as usize).max(1);
    let energy = summed_energy(channels, sample_rate);
    let last = energy.len().saturating_sub(1 + window);
    (0..=last)
        .map(|start| {
            let end = (start + window).min(energy.len() - 1);
            lufs((energy[end] - energy[start]) / window as f64)
        })
        .collect()
}

/// The integrated loudness of a whole sound, in LUFS, by ITU-R BS.1770-4
/// (and EBU Tech 3341): the loudness of its 400 ms blocks, one every 100 ms
/// (overlapping by 75%), leaving out first the blocks under -70 LUFS
/// (silence), then those more than 10 LU under the loudness of the rest,
/// and the loudness of the blocks that are left. Blocks are whole, so a
/// sound under 400 ms has none. `None` if no block is left.
pub fn integrated_loudness(channels: &[&[f32]], sample_rate: u32) -> Option<f64> {
    let rate = f64::from(sample_rate);
    let (block, hop) = ((MOMENTARY_SECONDS * rate) as usize, (0.1 * rate) as usize);
    let energy = summed_energy(channels, sample_rate);
    let blocks: Vec<f64> = (0..)
        .map(|i| i * hop)
        .take_while(|start| start + block < energy.len())
        .map(|start| (energy[start + block] - energy[start]) / block as f64)
        .collect();
    let gated = |threshold: f64| -> Option<f64> {
        let kept: Vec<f64> = blocks
            .iter()
            .copied()
            .filter(|&mean_square| lufs(mean_square) > threshold)
            .collect();
        (!kept.is_empty()).then(|| kept.iter().sum::<f64>() / kept.len() as f64)
    };
    let absolute = gated(-70.0)?;
    Some(lufs(gated((lufs(absolute) - 10.0).max(-70.0))?))
}

/// A mean square, summed over channels, as BS.1770's loudness in LUFS.
fn lufs(mean_square: f64) -> f64 {
    -0.691 + 10.0 * mean_square.log10()
}

/// The running total of the channels' K-weighted energy: entry `i` is the
/// sum of every channel's squared samples before sample `i`, so the energy
/// of any stretch is the difference of two entries.
fn summed_energy(channels: &[&[f32]], sample_rate: u32) -> Vec<f64> {
    let length = channels.iter().map(|c| c.len()).max().unwrap_or(0);
    let mut squares = vec![0.0; length];
    for channel in channels {
        for (sum, &s) in squares.iter_mut().zip(&k_weighted(channel, sample_rate)) {
            *sum += f64::from(s) * f64::from(s);
        }
    }
    std::iter::once(0.0)
        .chain(squares.iter().scan(0.0, |total, &square| {
            *total += square;
            Some(*total)
        }))
        .collect()
}

/// `samples` through the K-weighting filter of ITU-R BS.1770-4: the shelf,
/// then the high-pass. BS.1770 gives their coefficients at 48 kHz only;
/// these come from the analogue filters those were made from, through the
/// bilinear transform, as libebur128 does, so they work at any sample rate
/// and match BS.1770's own at 48 kHz.
fn k_weighted(samples: &[f32], sample_rate: u32) -> Vec<f32> {
    let (shelf, high_pass) = k_weighting(sample_rate);
    let biquad = |input: &[f32], (b, a): ([f64; 3], [f64; 3])| {
        let (mut x1, mut x2, mut y1, mut y2) = (0.0, 0.0, 0.0, 0.0);
        input
            .iter()
            .map(|&x| {
                let x = f64::from(x);
                let y = b[0] * x + b[1] * x1 + b[2] * x2 - a[1] * y1 - a[2] * y2;
                (x2, x1, y2, y1) = (x1, x, y1, y);
                y as f32
            })
            .collect::<Vec<f32>>()
    };
    biquad(&biquad(samples, shelf), high_pass)
}

/// The K-weighting's two biquads at `sample_rate`, each as its numerator
/// and denominator coefficients: the shelf, then the high-pass.
#[allow(clippy::type_complexity)]
fn k_weighting(sample_rate: u32) -> (([f64; 3], [f64; 3]), ([f64; 3], [f64; 3])) {
    let rate = f64::from(sample_rate);
    let (f0, gain_db, q) = (
        1_681.974_450_955_533,
        3.999_843_853_973_347,
        0.707_175_236_955_419_6,
    );
    let k = (std::f64::consts::PI * f0 / rate).tan();
    let vh = 10f64.powf(gain_db / 20.0);
    let vb = vh.powf(0.499_666_774_154_541_6);
    let a0 = 1.0 + k / q + k * k;
    let shelf = (
        [
            (vh + vb * k / q + k * k) / a0,
            2.0 * (k * k - vh) / a0,
            (vh - vb * k / q + k * k) / a0,
        ],
        [1.0, 2.0 * (k * k - 1.0) / a0, (1.0 - k / q + k * k) / a0],
    );
    let (f0, q) = (38.135_470_876_024_44, 0.500_327_037_323_877_3);
    let k = (std::f64::consts::PI * f0 / rate).tan();
    let a0 = 1.0 + k / q + k * k;
    let high_pass = (
        [1.0, -2.0, 1.0],
        [1.0, 2.0 * (k * k - 1.0) / a0, (1.0 - k / q + k * k) / a0],
    );
    (shelf, high_pass)
}

/// The frequency of the strongest partial between `low` and `high` Hz,
/// from the spectrum of `samples` zero-padded to a power of two, refined
/// between bins by fitting a parabola to the peak.
pub fn strongest_frequency(samples: &[f32], sample_rate: u32, low: f64, high: f64) -> f64 {
    let n = samples.len().next_power_of_two();
    let mut padded = samples.to_vec();
    padded.resize(n, 0.0);
    let bins = spectrum_unwindowed(&padded);
    let bin_hz = f64::from(sample_rate) / n as f64;
    let range = (low / bin_hz).ceil() as usize..=(high / bin_hz).floor() as usize;
    let k = range
        .max_by(|&a, &b| bins[a].total_cmp(&bins[b]))
        .expect("a range of bins");
    let (a, b, c) = (bins[k - 1].ln(), bins[k].ln(), bins[k + 1].ln());
    let offset = 0.5 * (a - c) / (a - 2.0 * b + c);
    (k as f64 + offset) * bin_hz
}

/// The level over time, in dB relative to full scale: the RMS of each
/// `window_seconds`, at the time of the window's middle. For noise, whose
/// peaks are spiky.
pub fn rms_envelope_db(samples: &[f32], sample_rate: u32, window_seconds: f64) -> Vec<(f64, f64)> {
    let window = ((window_seconds * f64::from(sample_rate)) as usize).max(1);
    samples
        .chunks(window)
        .enumerate()
        .map(|(i, chunk)| {
            let time = (i as f64 + 0.5) * window as f64 / f64::from(sample_rate);
            (time, 20.0 * rms(chunk).max(1e-12).log10())
        })
        .collect()
}

/// The bursts in a sound: when each is loudest, in seconds. The level is the
/// RMS over 2 ms, every 0.5 ms. A burst is a point that's the loudest within
/// 4 ms either side, and more than `prominence_db` over the quietest point
/// in the 5 ms before it and in the 5 ms after it (before the sound starts
/// counts as silence). Noise through a band-pass wavers by a few dB over a
/// few milliseconds, so a burst has to stand out from more than one window.
pub fn burst_peaks(samples: &[f32], sample_rate: u32, prominence_db: f64) -> Vec<f64> {
    let rate = f64::from(sample_rate);
    let (window, hop) = ((0.002 * rate) as usize, (0.0005 * rate) as usize);
    let levels: Vec<f64> = (0..samples.len().saturating_sub(window))
        .step_by(hop)
        .map(|start| 20.0 * rms(&samples[start..start + window]).max(1e-12).log10())
        .collect();
    let level = |i: isize| {
        if i < 0 {
            -240.0
        } else {
            levels.get(i as usize).copied().unwrap_or(f64::MAX)
        }
    };
    let quietest = |from: isize, to: isize| (from..to).map(level).fold(f64::MAX, f64::min);
    let loudest = |from: isize, to: isize| (from..to).map(level).fold(f64::MIN, f64::max);
    (0..levels.len() as isize)
        .filter(|&i| {
            let here = level(i);
            here >= loudest(i - 8, i + 9)
                && here - quietest(i - 10, i) > prominence_db
                && here - quietest(i + 1, i + 11) > prominence_db
        })
        .map(|i| (i as usize * hop + window / 2) as f64 / rate)
        .collect()
}

/// [`spectrum`] without the window: for a whole sound that starts and ends
/// in silence, where a window would only weigh its middle more.
fn spectrum_unwindowed(samples: &[f32]) -> Vec<f64> {
    real_fft(samples.iter().map(|&s| f64::from(s)))
        .iter()
        .map(|bin| bin.norm())
        .collect()
}

/// The FFT of real `samples`, any length, from `realfft`: bins 0 to half
/// the length.
fn real_fft(samples: impl Iterator<Item = f64>) -> Vec<Complex<f64>> {
    let mut input: Vec<f64> = samples.collect();
    if input.is_empty() {
        return Vec::new();
    }
    let fft = RealFftPlanner::<f64>::new().plan_fft_forward(input.len());
    let mut output = fft.make_output_vec();
    fft.process(&mut input, &mut output)
        .expect("the buffers are the plan's");
    output
}

/// The loudest alias in `samples` of a tone at `frequency_hz`: the loudest
/// partial between its harmonics, in dB relative to the loudest harmonic,
/// from their [`spectrum`]. Aliasing is harmonics above half the sample
/// rate folding back down, where they land between the true harmonics, so
/// a naive saw on a high note reads high. For a saw, the loudest harmonic is
/// its fundamental, and for a synced wave, whichever harmonic its restarts
/// make loudest. The pitch has to be steady and right to within a few bins
/// at the top harmonic: one that glides smears its harmonics into the gaps.
pub fn alias_db(samples: &[f32], sample_rate: u32, frequency_hz: f64) -> f64 {
    let spectrum = spectrum(samples);
    let bin_hz = f64::from(sample_rate) / samples.len() as f64;
    let harmonic = |bin: usize| harmonic_at(bin, bin_hz, frequency_hz);
    let loudest = |on: fn(Option<f64>) -> bool| {
        spectrum
            .iter()
            .enumerate()
            .filter(|&(bin, _)| on(harmonic(bin)))
            .fold(0.0f64, |a, (_, &b)| a.max(b))
    };
    let alias = loudest(|harmonic| harmonic.is_none());
    let harmonics = loudest(|harmonic| harmonic.is_some_and(|h| h >= 1.0));
    20.0 * (alias / harmonics).log10()
}

/// The harmonic of `frequency_hz` that bin `bin` of a [`spectrum`] is on,
/// if it's within the window's main lobe of one (4 bins either side, with a
/// little to spare), as [`alias_db`] counts it. 0 is any offset, which isn't
/// an alias but isn't a harmonic either. `None` is between the harmonics,
/// where aliasing lands.
pub fn harmonic_at(bin: usize, bin_hz: f64, frequency_hz: f64) -> Option<f64> {
    let lobe = 6.0;
    let harmonic = (bin as f64 * bin_hz / frequency_hz).round();
    ((bin as f64 - harmonic * frequency_hz / bin_hz).abs() <= lobe).then_some(harmonic)
}

/// The `seconds` of `samples` with the most energy, or all of them if
/// they're shorter. A held note's loudest stretch is its sustain.
pub fn loudest_stretch(samples: &[f32], sample_rate: u32, seconds: f64) -> &[f32] {
    let length = (seconds * f64::from(sample_rate)) as usize;
    if samples.len() <= length {
        return samples;
    }
    let energy: Vec<f64> = std::iter::once(0.0)
        .chain(samples.iter().scan(0.0, |sum, &s| {
            *sum += f64::from(s) * f64::from(s);
            Some(*sum)
        }))
        .collect();
    let start = (0..=samples.len() - length)
        .max_by(|&a, &b| {
            (energy[a + length] - energy[a]).total_cmp(&(energy[b + length] - energy[b]))
        })
        .expect("there's at least one stretch");
    &samples[start..start + length]
}

/// How far a sound's partials sit from whole-number multiples of
/// `frequency_hz`, from 0 (harmonic, as a saw or a square is) to 1: what
/// makes a sound metallic or bell-like. Each partial is a peak in the
/// [`spectrum`] of `samples` (the loudest within the window's main lobe,
/// no more than 60 dB under the loudest, between half the pitch and
/// 20 kHz), refined between bins with a parabola. Its distance from the
/// nearest harmonic is in harmonics, so at most a half; the average
/// distance, weighted by each peak's power, is doubled (Peeters, 2004). A
/// sum of unrelated partials, as a bell or a drum's head has, reads well
/// above 0. `None` if there are no peaks.
pub fn inharmonicity(samples: &[f32], sample_rate: u32, frequency_hz: f64) -> Option<f64> {
    let spectrum = spectrum(samples);
    let bin_hz = f64::from(sample_rate) / samples.len() as f64;
    let lobe = 4;
    let floor = spectrum.iter().fold(0.0f64, |a, &b| a.max(b)) * 1e-3;
    let first = ((0.5 * frequency_hz / bin_hz).ceil() as usize).max(lobe);
    let last = ((20_000.0 / bin_hz) as usize).min(spectrum.len().saturating_sub(lobe + 1));
    let (mut distance, mut power) = (0.0, 0.0);
    for k in first..last {
        let here = spectrum[k];
        let is_peak = here > floor
            && spectrum[k - lobe..=k + lobe]
                .iter()
                .enumerate()
                .all(|(i, &there)| there < here || (there == here && i >= lobe));
        if !is_peak {
            continue;
        }
        let (a, b, c) = (spectrum[k - 1].ln(), here.ln(), spectrum[k + 1].ln());
        let curve = a - 2.0 * b + c;
        let offset = if curve < 0.0 {
            0.5 * (a - c) / curve
        } else {
            0.0
        };
        let ratio = (k as f64 + offset) * bin_hz / frequency_hz;
        let off = (ratio - ratio.round().max(1.0)).abs().min(0.5);
        distance += off * here * here;
        power += here * here;
    }
    (power > 0.0).then(|| 2.0 * distance / power)
}

/// How wide a stereo sound is: the share of its energy in the side
/// (left minus right) rather than the mid (left plus right), from 0 to 1. 0
/// for mono (left = right), a half for two unrelated channels, and 1 for
/// left = -right, which is all side and vanishes in mono.
pub fn stereo_width(left: &[f32], right: &[f32]) -> f64 {
    let (mid, side) = left
        .iter()
        .zip(right)
        .fold((0.0, 0.0), |(mid, side), (&l, &r)| {
            let (l, r) = (f64::from(l), f64::from(r));
            (mid + (l + r).powi(2), side + (l - r).powi(2))
        });
    side / (mid + side)
}

/// How noise-like a sound is, from 0 (pure tones) to 1 (white noise): its
/// spectral flatness, the geometric mean of its [`average_power_spectrum`]
/// (in 40 ms frames) over the arithmetic mean, from 20 Hz to 20 kHz (or
/// half the sample rate, if that's lower). `None` for silence.
pub fn spectral_flatness(samples: &[f32], sample_rate: u32) -> Option<f64> {
    let frame = (0.04 * f64::from(sample_rate)) as usize;
    let power = average_power_spectrum(samples, frame);
    let bin_hz = f64::from(sample_rate) / frame as f64;
    let band = &power
        [(20.0 / bin_hz).ceil() as usize..=((20_000.0 / bin_hz) as usize).min(power.len() - 1)];
    let mean = band.iter().sum::<f64>() / band.len() as f64;
    let log_mean = band.iter().map(|p| p.ln()).sum::<f64>() / band.len() as f64;
    (mean > 0.0).then(|| log_mean.exp() / mean)
}

/// A spectrogram: the spectrum over time, in dB relative to a full-scale
/// sine, so a full-scale sine reads 0 dB at its frequency.
pub struct Spectrogram {
    /// Each frame's spectrum in dB, from 0 Hz in steps of `bin_hz`, up to
    /// half the sample rate. Silence reads [`SILENT_DB`].
    pub frames: Vec<Vec<f32>>,
    /// How far apart the bins are, in Hz.
    pub bin_hz: f64,
    /// How far apart the frames are, in seconds. Frame `i` is centred on
    /// `i * hop_seconds`.
    pub hop_seconds: f64,
    /// How long each frame's window is, in seconds.
    pub window_seconds: f64,
}

/// The level a spectrogram gives silence, in dB.
pub const SILENT_DB: f32 = -240.0;

/// The [`Spectrogram`] of `samples`: a Hann window of `window_seconds`
/// every `hop_seconds`, each centred on its time, so the first is centred
/// on the first sample (with silence before it) and the last at or past the
/// last. Each window is zero-padded to four times its length, which draws
/// the spectrum smoothly between bins without adding detail. A short
/// window shows when things happen (a drum hit's start), a long one which
/// frequencies (a tone's harmonics).
pub fn spectrogram(
    samples: &[f32],
    sample_rate: u32,
    window_seconds: f64,
    hop_seconds: f64,
) -> Spectrogram {
    let rate = f64::from(sample_rate);
    let window = ((window_seconds * rate).round() as usize).max(2);
    let hop = ((hop_seconds * rate).round() as usize).max(1);
    let length = 4 * window;
    let fft = RealFftPlanner::<f64>::new().plan_fft_forward(length);
    let shape: Vec<f64> = (0..window)
        .map(|i| 0.5 - 0.5 * (TAU * i as f64 / window as f64).cos())
        .collect();
    // A sine of amplitude 1 peaks at half the window's sum.
    let full_scale = shape.iter().sum::<f64>() / 2.0;
    let (mut input, mut output) = (fft.make_input_vec(), fft.make_output_vec());
    let frames = (0..=samples.len().div_ceil(hop))
        .map(|frame| {
            input.fill(0.0);
            let centre = frame * hop;
            for (i, w) in shape.iter().enumerate() {
                let at = (centre + i).checked_sub(window / 2);
                if let Some(&sample) = at.and_then(|at| samples.get(at)) {
                    input[i] = f64::from(sample) * w;
                }
            }
            fft.process(&mut input, &mut output)
                .expect("the buffers are the plan's");
            output
                .iter()
                .map(|bin| {
                    let level = bin.norm() / full_scale;
                    if level > 0.0 {
                        (20.0 * level.log10()).max(f64::from(SILENT_DB)) as f32
                    } else {
                        SILENT_DB
                    }
                })
                .collect()
        })
        .collect();
    Spectrogram {
        frames,
        bin_hz: rate / length as f64,
        hop_seconds: hop as f64 / rate,
        window_seconds: window as f64 / rate,
    }
}

/// Where `samples` click: each sample-to-sample step that's more than
/// [`CLICK_RATIO`] times the RMS of the steps in the 2 ms either side of
/// it (not counting the 4 samples nearest), and no quieter than 60 dB under
/// the loudest sample. A sine's steepest step is √2 times its RMS step, and
/// noise's rarely reaches 6, so a step 8 times what's around it is a jump
/// the sound itself doesn't make: a note starting or stopping mid-cycle, a
/// setting changing at once. A sound starting suddenly from silence counts
/// only if its first step is far bigger than the ones after it. Each is
/// the index of the sample after the step: the largest of the steps
/// within 10 ms of the first, in time order.
pub fn clicks(samples: &[f32], sample_rate: u32) -> Vec<usize> {
    let steps: Vec<f64> = samples
        .windows(2)
        .map(|pair| f64::from(pair[1] - pair[0]))
        .collect();
    let floor = f64::from(peak(samples)) * 1e-3;
    if floor == 0.0 {
        return Vec::new();
    }
    let energy: Vec<f64> = std::iter::once(0.0)
        .chain(steps.iter().scan(0.0, |sum, step| {
            *sum += step * step;
            Some(*sum)
        }))
        .collect();
    let rate = f64::from(sample_rate);
    let around = ((0.002 * rate) as usize).max(8);
    let near = 4;
    let found: Vec<usize> = (0..steps.len())
        .filter(|&i| {
            let step = steps[i].abs();
            if step < floor {
                return false;
            }
            let (start, end) = (i.saturating_sub(around), (i + around + 1).min(steps.len()));
            let (skip_start, skip_end) = (i.saturating_sub(near), (i + near + 1).min(steps.len()));
            let sum = energy[end] - energy[start] - (energy[skip_end] - energy[skip_start]);
            let count = (end - start) - (skip_end - skip_start);
            count > 0 && step * step > CLICK_RATIO * CLICK_RATIO * sum / count as f64
        })
        .collect();
    // The largest of each group within 10 ms.
    let apart = ((0.01 * rate) as usize).max(1);
    // Each group's first step and its largest.
    let mut groups: Vec<(usize, usize)> = Vec::new();
    for i in found {
        match groups.last_mut() {
            Some((first, largest)) if i - *first < apart => {
                if steps[i].abs() > steps[*largest].abs() {
                    *largest = i;
                }
            }
            _ => groups.push((i, i)),
        }
    }
    groups.into_iter().map(|(_, largest)| largest + 1).collect()
}

/// How many times the RMS of the steps around it a step must be, for
/// [`clicks`] to count it.
pub const CLICK_RATIO: f64 = 8.0;

/// The correlation of two signals, from -1 to 1.
pub fn correlation(a: &[f32], b: &[f32]) -> f64 {
    let dot = |x: &[f32], y: &[f32]| {
        x.iter()
            .zip(y)
            .map(|(&p, &q)| f64::from(p) * f64::from(q))
            .sum::<f64>()
    };
    dot(a, b) / (dot(a, a) * dot(b, b)).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: u32 = 48_000;

    /// One second of a sine at `frequency_hz` and `amplitude`, at 48 kHz.
    fn sine(frequency_hz: f64, amplitude: f32) -> Vec<f32> {
        (0..RATE)
            .map(|i| {
                let t = f64::from(i) / f64::from(RATE);
                amplitude * (std::f64::consts::TAU * frequency_hz * t).sin() as f32
            })
            .collect()
    }

    #[test]
    fn a_440_hz_sine_reads_440_hz() {
        let frequency = measure_frequency(&sine(440.0, 0.5), RATE);
        assert!((frequency - 440.0).abs() < 0.01, "{frequency} Hz");
    }

    #[test]
    fn a_440_hz_sine_reads_440_hz_all_the_way_through() {
        let pitches = pitch_over_time(&sine(440.0, 0.5), RATE);
        assert!(pitches.len() > 400);
        for (time, frequency) in pitches {
            assert!(
                (frequency - 440.0).abs() < 0.1,
                "{frequency} Hz at {time} s"
            );
        }
    }

    #[test]
    fn a_sine_between_two_bins_is_the_strongest_partial_at_its_frequency() {
        // One second pads to 65 536 samples, so bins are 0.73 Hz apart, and
        // this is halfway between two: the parabola has to find it.
        let bin_hz = f64::from(RATE) / 65_536.0;
        let between = 601.5 * bin_hz;
        let frequency = strongest_frequency(&sine(between, 0.5), RATE, 100.0, 1000.0);
        assert!(
            (frequency - between).abs() < 0.05,
            "{frequency} Hz, expected {between} Hz"
        );
    }

    #[test]
    fn a_sines_rms_is_its_peak_over_root_two() {
        let samples = sine(1000.0, 0.5);
        let peak = f64::from(peak(&samples));
        assert!((peak - 0.5).abs() < 1e-4, "peak {peak}");
        let expected = peak / std::f64::consts::SQRT_2;
        assert!(
            (rms(&samples) - expected).abs() < 1e-4,
            "rms {}",
            rms(&samples)
        );
    }

    #[test]
    fn an_exponential_decay_reads_its_decay_time() {
        // Falls 60 dB a second: 20 dB takes 1/3 s, 40 dB 2/3 s.
        let db_per_second = 60.0;
        let samples: Vec<f32> = (0..2 * RATE)
            .map(|i| {
                let t = f64::from(i) / f64::from(RATE);
                let level = 10f64.powf(-db_per_second * t / 20.0);
                (level * (std::f64::consts::TAU * 1000.0 * t).sin()) as f32
            })
            .collect();
        for drop_db in [20.0, 40.0] {
            let seconds = decay_seconds(&samples, RATE, drop_db).expect("it decays");
            let expected = drop_db / db_per_second;
            // Interpolated between 5 ms windows, so far closer than one.
            assert!(
                (seconds - expected).abs() < 1e-4,
                "{drop_db} dB in {seconds} s, expected {expected} s"
            );
        }
    }

    #[test]
    fn a_sines_crest_factor_is_root_two() {
        let crest = crest_factor(&sine(1000.0, 0.5));
        assert!(
            (crest - std::f64::consts::SQRT_2).abs() < 1e-4,
            "crest factor {crest}"
        );
    }

    #[test]
    fn a_linear_ramp_reads_its_rise_time() {
        // Rises from silence to 0.8 in 50 ms, then holds: 10% to 90% of the
        // way is 80% of the ramp, 40 ms.
        let ramp = (0.05 * f64::from(RATE)) as usize;
        let samples: Vec<f32> = (0..RATE as usize)
            .map(|i| 0.8 * (i as f32 / ramp as f32).min(1.0))
            .collect();
        let attack = attack_seconds(&samples, RATE).expect("it rises");
        assert!((attack - 0.04).abs() < 1e-6, "attack {attack} s");
    }

    #[test]
    fn a_tone_ramped_in_reads_its_rise_time_to_within_half_a_cycle() {
        // A 1 kHz sine rising linearly over 20 ms: 16 ms from 10% to 90%.
        let samples: Vec<f32> = (0..RATE)
            .map(|i| {
                let t = f64::from(i) / f64::from(RATE);
                ((t / 0.02).min(1.0) * (std::f64::consts::TAU * 1000.0 * t).sin()) as f32
            })
            .collect();
        let attack = attack_seconds(&samples, RATE).expect("it rises");
        assert!((attack - 0.016).abs() < 0.0005, "attack {attack} s");
    }

    #[test]
    fn the_attack_is_the_loudest_hits() {
        // A quiet hit that rises in 10 ms, then a loud one that rises in
        // 1 ms: 10% to 90% of the loud one is 0.8 ms.
        let hit = |level: f32, rise_seconds: f64| -> Vec<f32> {
            let rise = rise_seconds * f64::from(RATE);
            (0..RATE / 4)
                .map(|i| {
                    let t = f64::from(i);
                    let envelope = (t / rise).min(1.0) * (-(t - rise).max(0.0) / 2400.0).exp();
                    level * envelope as f32
                })
                .collect()
        };
        let mut samples = hit(0.3, 0.01);
        samples.extend(hit(0.9, 0.001));
        let attack = attack_seconds(&samples, RATE).expect("it rises");
        assert!((attack - 0.0008).abs() < 1e-6, "attack {attack} s");
    }

    #[test]
    fn smoothing_drops_an_odd_cycle_and_keeps_a_glide() {
        let glide: Vec<(f64, f64)> = (0..10)
            .map(|i| (f64::from(i), 200.0 - 10.0 * f64::from(i)))
            .collect();
        assert_eq!(smoothed_pitch(&glide)[1..9], glide[1..9]);
        let mut odd: Vec<(f64, f64)> = (0..10).map(|i| (f64::from(i), 100.0)).collect();
        odd[4].1 = 400.0;
        assert!(smoothed_pitch(&odd).iter().all(|&(_, hz)| hz == 100.0));
    }

    /// A band-limited saw at `frequency_hz`: its harmonics up to 20 kHz, each
    /// at 1/k.
    fn saw(frequency_hz: f64, seconds: f64) -> Vec<f32> {
        let harmonics = (20_000.0 / frequency_hz) as usize;
        (0..(seconds * f64::from(RATE)) as usize)
            .map(|i| {
                let t = i as f64 / f64::from(RATE);
                let sum: f64 = (1..=harmonics)
                    .map(|k| (std::f64::consts::TAU * frequency_hz * k as f64 * t).sin() / k as f64)
                    .sum();
                (0.3 * sum) as f32
            })
            .collect()
    }

    #[test]
    fn the_pitch_track_reads_a_sine_and_a_saw() {
        for (name, samples, hz) in [
            ("sine", sine(440.0, 0.5), 440.0),
            ("low sine", sine(45.0, 0.5), 45.0),
            ("saw", saw(110.0, 0.5), 110.0),
        ] {
            let track = pitch_track(&samples, RATE);
            assert!(track.len() > 50, "{name}: {} points", track.len());
            for (time, frequency) in track {
                let cents = 1200.0 * (frequency / hz).log2();
                assert!(cents.abs() < 1.0, "{name}: {frequency} Hz at {time} s");
            }
        }
    }

    #[test]
    fn the_pitch_track_follows_a_glide() {
        // Falls from 200 Hz to 50 Hz over a second, exponentially.
        let mut phase = 0.0;
        let samples: Vec<f32> = (0..RATE)
            .map(|i| {
                let t = f64::from(i) / f64::from(RATE);
                phase += std::f64::consts::TAU * 200.0 * 0.25f64.powf(t) / f64::from(RATE);
                (0.5 * phase.sin()) as f32
            })
            .collect();
        let track = pitch_track(&samples, RATE);
        assert!(track.len() > 100);
        for (time, frequency) in track {
            let expected = 200.0 * 0.25f64.powf(time);
            let cents = 1200.0 * (frequency / expected).log2();
            assert!(
                cents.abs() < 25.0,
                "{frequency} Hz at {time} s, expected {expected} Hz"
            );
        }
    }

    #[test]
    fn noise_has_no_pitch_track() {
        let mut state = 1u32;
        let noise: Vec<f32> = (0..RATE)
            .map(|_| {
                state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                (state >> 8) as f32 / (1 << 24) as f32 - 0.5
            })
            .collect();
        let track = pitch_track(&noise, RATE);
        assert!(track.len() < 5, "noise has {} points", track.len());
        assert_eq!(steady_pitch(&track), None);
    }

    #[test]
    fn while_loud_leaves_out_the_gap_between_two_tones() {
        // 220 Hz, a gap, then 220 Hz again: a "cycle" across the gap would
        // read far lower.
        let mut samples = sine(220.0, 0.5)[..12_000].to_vec();
        samples.extend(vec![0.0; 4800]);
        samples.extend(&sine(220.0, 0.5)[..12_000]);
        let all = pitch_over_time(&samples, RATE);
        assert!(all.iter().any(|&(_, hz)| hz < 100.0), "the gap is a cycle");
        let loud = while_loud(all.clone(), &samples, RATE, 40.0);
        assert!(loud.iter().all(|&(_, hz)| (hz - 220.0).abs() < 0.1));
        let pitch = steady_pitch(&loud).expect("a steady tone");
        assert!((pitch - 220.0).abs() < 0.01, "{pitch} Hz");
    }

    #[test]
    fn a_signal_correlates_fully_with_itself_and_negatively_with_its_inverse() {
        let samples = sine(440.0, 0.5);
        let inverse: Vec<f32> = samples.iter().map(|s| -s).collect();
        assert!((correlation(&samples, &samples) - 1.0).abs() < 1e-9);
        assert!((correlation(&samples, &inverse) + 1.0).abs() < 1e-9);
    }

    /// A 1 kHz sine at each `(seconds, dBFS)` in turn, its peak at that
    /// level, at `rate`: the tones EBU Tech 3341's test signals are made of.
    fn tones(rate: u32, segments: &[(f64, f64)]) -> Vec<f32> {
        let mut samples = Vec::new();
        for &(seconds, dbfs) in segments {
            let amplitude = 10f64.powf(dbfs / 20.0);
            let start = samples.len();
            samples.extend((0..(seconds * f64::from(rate)).round() as usize).map(|i| {
                let t = (start + i) as f64 / f64::from(rate);
                (amplitude * (TAU * 1000.0 * t).sin()) as f32
            }));
        }
        samples
    }

    /// Silence and the 1 kHz tone at -23 dBFS, for Tech 3341's tests of the
    /// loudest window.
    fn silence_then_tone(rate: u32, silence: f64, tone: f64) -> Vec<f32> {
        tones(rate, &[(silence, -200.0), (tone, -23.0), (1.0, -200.0)])
    }

    fn assert_lufs(name: &str, lufs: f64, expected: f64) {
        assert!(
            (lufs - expected).abs() <= 0.1,
            "{name}: {lufs} LUFS, expected {expected}"
        );
    }

    fn loudest(samples: &[f32], rate: u32, window_seconds: f64) -> f64 {
        loudness_over_time(&[samples, samples], rate, window_seconds)
            .into_iter()
            .fold(f64::NEG_INFINITY, f64::max)
    }

    #[test]
    fn the_k_weighting_at_48_khz_is_bs_1770s_own() {
        let (shelf, high_pass) = k_weighting(48_000);
        let bs_1770 = [
            (
                [
                    1.535_124_859_586_97,
                    -2.691_696_189_406_38,
                    1.198_392_810_852_85,
                ],
                [1.0, -1.690_659_293_182_41, 0.732_480_774_215_85],
            ),
            (
                [1.0, -2.0, 1.0],
                [1.0, -1.990_047_454_833_98, 0.990_072_250_366_21],
            ),
        ];
        for ((b, a), (b_1770, a_1770)) in [shelf, high_pass].into_iter().zip(bs_1770) {
            for (ours, theirs) in b.iter().chain(&a).zip(b_1770.iter().chain(&a_1770)) {
                assert!((ours - theirs).abs() < 1e-8, "{ours}, BS.1770 has {theirs}");
            }
        }
    }

    #[test]
    fn a_stereo_1_khz_sine_reads_the_loudness_ebu_tech_3341_gives() {
        // Tech 3341's tests 1 and 2: 20 s at -23 dBFS and at -33 dBFS read
        // that in LUFS, momentary, short-term and integrated.
        for dbfs in [-23.0, -33.0] {
            let tone = tones(RATE, &[(20.0, dbfs)]);
            assert_lufs("momentary", loudest(&tone, RATE, MOMENTARY_SECONDS), dbfs);
            assert_lufs("short-term", loudest(&tone, RATE, SHORT_TERM_SECONDS), dbfs);
            let integrated = integrated_loudness(&[&tone, &tone], RATE).unwrap();
            assert_lufs("integrated", integrated, dbfs);
        }
    }

    #[test]
    fn the_integrated_loudness_is_gated_as_ebu_tech_3341_says() {
        // Tech 3341's tests 3 (the relative gate leaves out the quiet ends),
        // 4 (the absolute gate leaves out the near-silence) and 5: each
        // reads -23.0.
        for (test, segments) in [
            (3, vec![(10.0, -36.0), (60.0, -23.0), (10.0, -36.0)]),
            (
                4,
                vec![
                    (10.0, -72.0),
                    (10.0, -36.0),
                    (60.0, -23.0),
                    (10.0, -36.0),
                    (10.0, -72.0),
                ],
            ),
            (5, vec![(20.0, -26.0), (20.1, -20.0), (20.0, -26.0)]),
        ] {
            let tone = tones(RATE, &segments);
            let integrated = integrated_loudness(&[&tone, &tone], RATE).unwrap();
            assert_lufs(&format!("test {test}"), integrated, -23.0);
        }
    }

    #[test]
    fn momentary_and_short_term_loudness_read_as_ebu_tech_3341_says() {
        // Test 9: 1.34 s at -20 dBFS and 1.66 s at -30, five times, reads
        // -23.0 short-term all the way through. Test 12: 0.18 s and 0.22 s,
        // 25 times, reads -23.0 momentary all the way through.
        for (test, (loud, quiet, times), window) in [
            (9, (1.34, 1.66, 5), SHORT_TERM_SECONDS),
            (12, (0.18, 0.22, 25), MOMENTARY_SECONDS),
        ] {
            let tone = tones(RATE, &[(loud, -20.0), (quiet, -30.0)].repeat(times));
            let over_time = loudness_over_time(&[&tone, &tone], RATE, window);
            for (at, lufs) in over_time.into_iter().enumerate() {
                assert_lufs(&format!("test {test} at sample {at}"), lufs, -23.0);
            }
        }
        // Tests 10 and 13, for meters that read a file: 3 s and 400 ms of
        // tone at -23 dBFS, after silence, read -23.0 at their loudest
        // wherever they start.
        for i in 0..20 {
            let short = silence_then_tone(RATE, 0.15 * f64::from(i), 3.0);
            assert_lufs(&format!("test 10, {i}"), loudest(&short, RATE, 3.0), -23.0);
            let momentary = silence_then_tone(RATE, 0.02 * f64::from(i), 0.4);
            assert_lufs(
                &format!("test 13, {i}"),
                loudest(&momentary, RATE, 0.4),
                -23.0,
            );
        }
    }

    #[test]
    fn the_loudest_stretch_is_the_loudest_tone_however_it_is_placed() {
        // A quiet tone, silence, the loud tone, silence: the 500 ms picked
        // is the loud tone's, wherever it is in the file. A file shorter
        // than the stretch is all of it.
        let mut samples = tones(RATE, &[(1.0, -30.0), (1.0, -200.0)]);
        let loud = samples.len();
        samples.extend(tones(RATE, &[(0.5, -6.0), (1.0, -200.0)]));
        let stretch = loudest_stretch(&samples, RATE, 0.5);
        assert_eq!(stretch.len(), RATE as usize / 2);
        // Where it starts, give or take the tone's first sample, which is 0.
        let start = stretch.as_ptr() as usize - samples.as_ptr() as usize;
        let start = start / std::mem::size_of::<f32>();
        assert!(
            start.abs_diff(loud) <= 1,
            "starts at {start}, the tone at {loud}"
        );
        let short = &samples[..1000];
        assert_eq!(loudest_stretch(short, RATE, 0.5), short);
    }

    #[test]
    fn the_absolute_gate_leaves_out_what_is_nearly_silent() {
        // A tone at -75 LUFS is all under the -70 LUFS gate, so it has no
        // integrated loudness; at -65 it's all over it. The relative gate
        // alone would keep both, being 10 LU under the tone itself.
        let tone = tones(RATE, &[(5.0, -75.0)]);
        assert_eq!(integrated_loudness(&[&tone, &tone], RATE), None);
        let tone = tones(RATE, &[(5.0, -65.0)]);
        assert_lufs(
            "-65 dBFS",
            integrated_loudness(&[&tone, &tone], RATE).unwrap(),
            -65.0,
        );
    }

    #[test]
    fn loudness_reads_the_same_at_44_1_khz() {
        let rate = 44_100;
        let tone = tones(rate, &[(20.0, -23.0)]);
        assert_lufs("momentary", loudest(&tone, rate, MOMENTARY_SECONDS), -23.0);
        let integrated = integrated_loudness(&[&tone, &tone], rate).unwrap();
        assert_lufs("integrated", integrated, -23.0);
        // A 100 Hz tone, where the high-pass starts to bite, reads the same
        // at both rates.
        let low = |rate: u32| {
            let tone: Vec<f32> = (0..5 * rate)
                .map(|i| (0.1 * (TAU * 100.0 * f64::from(i) / f64::from(rate)).sin()) as f32)
                .collect();
            integrated_loudness(&[&tone, &tone], rate).unwrap()
        };
        assert!(
            (low(44_100) - low(48_000)).abs() < 0.01,
            "{} and {}",
            low(44_100),
            low(48_000)
        );
    }

    #[test]
    fn silence_has_no_loudness() {
        let silence = [0.0; 48_000];
        assert_eq!(integrated_loudness(&[&silence, &silence], RATE), None);
        assert_eq!(
            loudest(&silence, RATE, MOMENTARY_SECONDS),
            f64::NEG_INFINITY
        );
    }

    #[test]
    fn a_spectrum_takes_any_length() {
        // A second at 44.1 kHz is 44 100 samples, not a power of two: bins
        // are 1 Hz apart, and a 1 kHz sine is at bin 1000.
        for rate in [44_100u32, 48_000] {
            let samples: Vec<f32> = (0..rate)
                .map(|i| (TAU * 1000.0 * f64::from(i) / f64::from(rate)).sin() as f32)
                .collect();
            let bins = spectrum(&samples);
            assert_eq!(bins.len(), rate as usize / 2 + 1);
            let loudest = (0..bins.len()).max_by(|&a, &b| bins[a].total_cmp(&bins[b]));
            assert_eq!(loudest, Some(1000), "at {rate} Hz");
        }
    }

    /// One second of white noise, from -0.5 to 0.5, from `seed`.
    fn noise(seed: u32) -> Vec<f32> {
        let mut state = seed;
        (0..RATE)
            .map(|_| {
                state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                (state >> 8) as f32 / (1 << 24) as f32 - 0.5
            })
            .collect()
    }

    #[test]
    fn white_noise_reads_flat_and_a_sine_tonal() {
        let flatness = spectral_flatness(&noise(1), RATE).unwrap();
        assert!(flatness > 0.95, "white noise reads {flatness}");
        let flatness = spectral_flatness(&sine(1000.0, 0.5), RATE).unwrap();
        assert!(flatness < 0.01, "a sine reads {flatness}");
        assert_eq!(spectral_flatness(&[0.0; 4800], RATE), None);
    }

    #[test]
    fn a_squares_partials_are_harmonic_and_unrelated_ones_arent() {
        // A band-limited square: odd harmonics at 1/k. At 220.5 Hz every
        // partial is halfway between two bins (1 Hz apart), so it reads
        // harmonic only if each peak is found between them.
        for hz in [220.0, 220.5] {
            let square: Vec<f32> = (0..RATE)
                .map(|i| {
                    let t = f64::from(i) / f64::from(RATE);
                    let sum: f64 = (1..90)
                        .step_by(2)
                        .map(|k| (TAU * hz * f64::from(k) * t).sin() / f64::from(k))
                        .sum();
                    (0.3 * sum) as f32
                })
                .collect();
            let square = inharmonicity(&square, RATE, hz).unwrap();
            assert!(square < 0.001, "a square at {hz} Hz reads {square}");
        }
        // A drum head's first modes over 200 Hz, all at one level: none is
        // a whole multiple of the first.
        let modes = [1.0, 1.594, 2.136, 2.296, 2.653, 2.918];
        let head: Vec<f32> = (0..RATE)
            .map(|i| {
                let t = f64::from(i) / f64::from(RATE);
                let sum: f64 = modes.iter().map(|m| (TAU * 200.0 * m * t).sin()).sum();
                (0.1 * sum) as f32
            })
            .collect();
        let head = inharmonicity(&head, RATE, 200.0).unwrap();
        assert!(head > 0.3, "a drum head's modes read {head}");
    }

    #[test]
    fn stereo_reads_its_correlation_and_width() {
        let left = noise(1);
        let inverse: Vec<f32> = left.iter().map(|s| -s).collect();
        let unrelated = noise(2);
        for (name, right, expected_correlation, expected_width) in
            [("L = R", &left, 1.0, 0.0), ("L = -R", &inverse, -1.0, 1.0)]
        {
            let correlation = correlation(&left, right);
            assert!(
                (correlation - expected_correlation).abs() < 1e-9,
                "{name}: {correlation}"
            );
            let width = stereo_width(&left, right);
            assert!(
                (width - expected_width).abs() < 1e-9,
                "{name}: width {width}"
            );
        }
        let correlation = correlation(&left, &unrelated);
        assert!(correlation.abs() < 0.05, "unrelated noise: {correlation}");
        let width = stereo_width(&left, &unrelated);
        assert!((width - 0.5).abs() < 0.05, "unrelated noise: width {width}");
    }

    #[test]
    fn a_naive_saw_aliases_and_a_band_limited_one_doesnt() {
        // C7, 2093 Hz: its 12th harmonic is already past half the sample
        // rate, so a naive saw folds strong aliases back down between the
        // harmonics.
        for rate in [44_100u32, 48_000] {
            let frequency = 2093.0;
            let naive: Vec<f32> = (0..rate)
                .map(|n| (2.0 * (f64::from(n) * frequency / f64::from(rate)).fract() - 1.0) as f32)
                .collect();
            let alias = alias_db(&naive, rate, frequency);
            assert!(alias > -30.0, "a naive saw at {rate} Hz reads {alias} dB");
            let clean: Vec<f32> = (0..rate)
                .map(|n| {
                    let t = f64::from(n) / f64::from(rate);
                    let top = (f64::from(rate) / 2.0 / frequency) as usize;
                    let sum: f64 = (1..=top)
                        .map(|k| (TAU * frequency * k as f64 * t).sin() / k as f64)
                        .sum();
                    (0.5 * sum) as f32
                })
                .collect();
            let alias = alias_db(&clean, rate, frequency);
            assert!(
                alias < -80.0,
                "a band-limited saw at {rate} Hz reads {alias} dB"
            );
        }
    }

    #[test]
    fn the_attack_says_where_the_rise_is() {
        // Silence for 100 ms, then the ramp from `a_linear_ramp_reads_its_rise_time`:
        // 10% is 5 ms in, 90% is 45 ms in.
        let ramp = (0.05 * f64::from(RATE)) as usize;
        let silence = (0.1 * f64::from(RATE)) as usize;
        let samples: Vec<f32> = (0..RATE as usize)
            .map(|i| 0.8 * (i.saturating_sub(silence) as f32 / ramp as f32).min(1.0))
            .collect();
        let (from, to) = attack(&samples, RATE).expect("it rises").seconds(RATE);
        assert!((from - 0.105).abs() < 1e-4, "10% at {from} s");
        assert!((to - 0.145).abs() < 1e-4, "90% at {to} s");
    }

    /// The loudest bin of each frame, in dB, and its frequency.
    fn loudest_bin(spectrogram: &Spectrogram, frame: usize) -> (f64, f64) {
        let (bin, &db) = spectrogram.frames[frame]
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.total_cmp(b.1))
            .expect("a frame has bins");
        (bin as f64 * spectrogram.bin_hz, f64::from(db))
    }

    #[test]
    fn a_full_scale_sine_reads_0_db_at_its_frequency() {
        for window in [0.003, 0.05] {
            let spectrogram = spectrogram(&sine(1000.0, 1.0), RATE, window, window / 4.0);
            let middle = spectrogram.frames.len() / 2;
            let (hz, db) = loudest_bin(&spectrogram, middle);
            // Zero-padded four times, the nearest bin is within an eighth of
            // the window's resolution, where a Hann window has lost 0.1 dB.
            assert!(db.abs() < 0.2, "{window} s: {db} dB");
            assert!(
                (hz - 1000.0).abs() <= spectrogram.bin_hz,
                "{window} s: {hz} Hz"
            );
            let far = (8000.0 / spectrogram.bin_hz) as usize;
            if window == 0.05 {
                let far_db = spectrogram.frames[middle][far];
                assert!(far_db < -90.0, "7 kHz away: {far_db} dB");
            }
        }
    }

    #[test]
    fn a_spectrogram_has_a_frame_every_hop_and_silence_reads_silent() {
        let samples = vec![0.0; RATE as usize];
        let spectrogram = spectrogram(&samples, RATE, 0.05, 0.01);
        assert_eq!(spectrogram.frames.len(), 101, "0 s to 1 s, every 10 ms");
        assert!((spectrogram.hop_seconds - 0.01).abs() < 1e-9);
        assert!((spectrogram.window_seconds - 0.05).abs() < 1e-9);
        assert_eq!(spectrogram.frames[0].len(), 4 * 2400 / 2 + 1);
        assert!(
            spectrogram
                .frames
                .iter()
                .flatten()
                .all(|&db| db == SILENT_DB),
            "silence"
        );
    }

    #[test]
    fn a_short_window_finds_when_an_impulse_is() {
        let mut samples = vec![0.0; RATE as usize];
        samples[24_000] = 1.0;
        let spectrogram = spectrogram(&samples, RATE, 0.003, 0.001);
        let loudest = (0..spectrogram.frames.len())
            .max_by(|&a, &b| {
                loudest_bin(&spectrogram, a)
                    .1
                    .total_cmp(&loudest_bin(&spectrogram, b).1)
            })
            .unwrap();
        let at = loudest as f64 * spectrogram.hop_seconds;
        assert!((at - 0.5).abs() <= 0.001, "loudest at {at} s");
    }

    #[test]
    fn a_jump_in_a_sine_is_a_click_and_a_clean_sine_or_noise_isnt() {
        let clean = sine(440.0, 0.5);
        assert_eq!(clicks(&clean, RATE), Vec::<usize>::new(), "a clean sine");
        assert_eq!(clicks(&noise(7), RATE), Vec::<usize>::new(), "noise");
        assert_eq!(clicks(&[0.0; 4800], RATE), Vec::<usize>::new(), "silence");
        // Half a cycle of phase skipped at a peak just after 0.25 s, and the
        // sine cut off at a peak just after 0.75 s. Both are whole numbers
        // of cycles, and a quarter cycle of 440 Hz is 27 samples.
        let jump = 12_000 + 27;
        let stop = 36_000 + 27;
        let samples: Vec<f32> = (0..RATE as usize)
            .map(|i| {
                let skip = if i >= jump { 0.5 / 440.0 } else { 0.0 };
                let t = i as f64 / f64::from(RATE) + skip;
                if i >= stop {
                    0.0
                } else {
                    0.5 * (TAU * 440.0 * t).sin() as f32
                }
            })
            .collect();
        let found = clicks(&samples, RATE);
        assert_eq!(found.len(), 2, "{found:?}");
        assert!(found[0].abs_diff(jump) <= 1, "{found:?}");
        assert!(found[1].abs_diff(stop) <= 1, "{found:?}");
    }

    /// Two steps within 10 ms are one click, at the larger.
    #[test]
    fn steps_close_together_are_one_click_at_the_largest() {
        let samples: Vec<f32> = (0..RATE as usize)
            .map(|i| match i {
                ..12_000 => 0.0,
                12_000..12_100 => 0.2,
                _ => 0.7,
            })
            .collect();
        assert_eq!(clicks(&samples, RATE), vec![12_100]);
    }

    #[test]
    fn a_harmonic_is_on_its_line_and_an_alias_between() {
        // Bins 1 Hz apart, harmonics of 100 Hz.
        assert_eq!(harmonic_at(300, 1.0, 100.0), Some(3.0));
        assert_eq!(harmonic_at(305, 1.0, 100.0), Some(3.0));
        assert_eq!(harmonic_at(350, 1.0, 100.0), None);
        assert_eq!(harmonic_at(2, 1.0, 100.0), Some(0.0));
    }
}
