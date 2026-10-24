//! Measurements of an impulse response: the sound of a single click through
//! a reverb (or any effect) at Mix 100%, which holds everything about how it
//! behaves (RFC-010). How long it rings, per octave ([`band_decay_times`]);
//! how soon its tail turns into a smooth wash ([`echo_density`]); when the
//! first sound arrives ([`onset`]); how alike its left and right are, early
//! and late ([`early_and_late_correlation`]); and how bumpy its tail's
//! spectrum is ([`colouration_db`]). [`energy_decay_relief`] is the decay by
//! frequency, for the image `uta analyse` draws of it.
//!
//! Each is tested on a synthetic signal with a known answer: noise decaying
//! at a known rate, Gaussian noise, a sparse click train, channels known to
//! be alike or unrelated, and a spectrum with a ripple of a known depth.

use std::f64::consts::SQRT_2;

use realfft::RealFftPlanner;
use realfft::num_complex::Complex;

use crate::{SILENT_DB, Spectrogram, correlation, peak, spectrogram};

/// The centres of the octave bands decay is measured in, in Hz: the
/// standard octaves from 63 Hz to 8 kHz, exactly 1 kHz times a power of two
/// (so the lowest, called 63 Hz, is 62.5).
pub const OCTAVE_BANDS_HZ: [f64; 8] = [62.5, 125.0, 250.0, 500.0, 1000.0, 2000.0, 4000.0, 8000.0];

/// How steep an octave band's edges are: the order of the Butterworth shape
/// its power follows, which falls 6 dB an octave for each, so 96 dB an
/// octave beyond the edge. Steep, so a band takes in little of its
/// neighbours, whose decay may differ; any steeper and the lowest band would
/// ring for longer than a short decay.
const BAND_ORDER: i32 = 16;

/// Where a decay time's fit starts and ends on the decay curve, in dB: from
/// -5 to -35 dB, ISO 3382's T30.
pub const FIT_FROM_DB: f64 = -5.0;
pub const FIT_TO_DB: f64 = -35.0;

/// The share of Gaussian noise that's more than one standard deviation from
/// zero, erfc(1/√2): what Abel and Huang's echo density divides by, so a
/// smooth wash reads 1.
const OUTSIDE_ONE_SIGMA: f64 = 0.317_310_507_862_914_15;

/// The window echo density is measured over, in seconds, and how often.
pub const ECHO_DENSITY_WINDOW_SECONDS: f64 = 0.02;
pub const ECHO_DENSITY_HOP_SECONDS: f64 = 0.001;

/// The echo density that counts as near 1: a smooth wash. A 20 ms window of
/// Gaussian noise reads 1 give or take about 0.06, so a tail that's dense
/// reaches this within a few windows.
pub const DENSE: f64 = 0.9;

/// How far below the loudest sample the first sound has to be, at most, to
/// count as the onset, in dB. Far enough down to catch a quiet first
/// reflection, and far above a render's own noise, which is none.
pub const ONSET_DB: f64 = -40.0;

/// Where the early part of an impulse response ends and the late starts, in
/// seconds after the onset: 80 ms, the split room acoustics uses for
/// clarity (C80).
pub const EARLY_SECONDS: f64 = 0.08;

/// The frames colouration's spectrum is averaged over, in seconds: 10 Hz
/// apart, fine enough to see a ripple a third of an octave wide from
/// 200 Hz.
const COLOURATION_FRAME_SECONDS: f64 = 0.1;
/// How far down its decay curve the tail colouration is measured over
/// goes, in dB.
const TAIL_TO_DB: f64 = -60.0;
/// The frequencies colouration is measured over, in Hz.
pub const COLOURATION_LOWEST_HZ: f64 = 200.0;
pub const COLOURATION_HIGHEST_HZ: f64 = 10_000.0;
/// How many points an octave its spectrum is read at, on a log axis, and
/// how wide a band each point averages, in octaves.
const POINTS_PER_OCTAVE: usize = 48;
const POINT_OCTAVES: f64 = 1.0 / 48.0;

/// When the first sound arrives, to the sample: the first sample, in any
/// channel, no more than [`ONSET_DB`] below the loudest in all of them.
/// `None` for silence.
pub fn onset(channels: &[&[f32]]) -> Option<usize> {
    let loudest = channels.iter().map(|c| peak(c)).fold(0.0f32, f32::max);
    if loudest == 0.0 {
        return None;
    }
    let threshold = loudest * 10f32.powf(ONSET_DB as f32 / 20.0);
    channels
        .iter()
        .filter_map(|c| c.iter().position(|s| s.abs() >= threshold))
        .min()
}

/// `samples` split into the [`OCTAVE_BANDS_HZ`]: each band's edges are a
/// half octave either side of its centre, where its power is half, and its
/// power falls 96 dB an octave beyond them (a 16th-order Butterworth
/// shape). Neighbouring bands' power adds up to the sound's at their shared
/// edge, so nothing falls between them. It's done in one FFT of the whole
/// sound, with silence after it so nothing wraps round, and no phase shift,
/// so a band's ringing is as short as its shape allows. A band whose top
/// edge is above half the sample rate is empty.
pub fn octave_bands(samples: &[f32], sample_rate: u32) -> Vec<Vec<f64>> {
    let length = (2 * samples.len()).next_power_of_two().max(2);
    let mut planner = RealFftPlanner::<f64>::new();
    let forward = planner.plan_fft_forward(length);
    let inverse = planner.plan_fft_inverse(length);
    let mut input = forward.make_input_vec();
    for (to, &from) in input.iter_mut().zip(samples) {
        *to = f64::from(from);
    }
    let mut spectrum = forward.make_output_vec();
    forward
        .process(&mut input, &mut spectrum)
        .expect("the buffers are the plan's");
    let bin_hz = f64::from(sample_rate) / length as f64;
    let nyquist = f64::from(sample_rate) / 2.0;
    OCTAVE_BANDS_HZ
        .iter()
        .map(|&centre| {
            if centre * SQRT_2 > nyquist {
                return vec![0.0; samples.len()];
            }
            let mut band: Vec<Complex<f64>> = spectrum
                .iter()
                .enumerate()
                .map(|(k, &bin)| bin * band_gain(k as f64 * bin_hz, centre))
                .collect();
            // A real signal's first and last bins are real.
            let last = band.len() - 1;
            band[0].im = 0.0;
            band[last].im = 0.0;
            let mut output = inverse.make_output_vec();
            inverse
                .process(&mut band, &mut output)
                .expect("the buffers are the plan's");
            output.truncate(samples.len());
            output.iter_mut().for_each(|s| *s /= length as f64);
            output
        })
        .collect()
}

/// How much of `hz` the octave band centred on `centre_hz` lets through,
/// from 0 to 1, as a gain on the amplitude.
fn band_gain(hz: f64, centre_hz: f64) -> f64 {
    if hz <= 0.0 {
        return 0.0;
    }
    let (low, high) = (centre_hz / SQRT_2, centre_hz * SQRT_2);
    let power =
        1.0 / (1.0 + (hz / high).powi(2 * BAND_ORDER)) / (1.0 + (low / hz).powi(2 * BAND_ORDER));
    power.sqrt()
}

/// The decay curve of `samples`, by Schroeder's backward integration: at
/// each sample, the energy from there to the end, in dB relative to the
/// whole sound's, so it starts at 0 dB and only ever falls. Decay is
/// measured from it rather than from the level, because the sum smooths out
/// the ups and downs a tail's level has from moment to moment. It assumes
/// the sound rings out to silence, as a render does, so the end isn't noise.
/// `None` for silence.
pub fn decay_curve_db(samples: &[f64]) -> Option<Vec<f64>> {
    let mut remaining: Vec<f64> = samples.iter().map(|s| s * s).collect();
    let mut sum = 0.0;
    for energy in remaining.iter_mut().rev() {
        sum += *energy;
        *energy = sum;
    }
    if sum <= 0.0 {
        return None;
    }
    Some(
        remaining
            .iter()
            .map(|&energy| 10.0 * (energy / sum).max(1e-30).log10())
            .collect(),
    )
}

/// The decay time of a decay curve (as [`decay_curve_db`] gives it), in
/// seconds: how long it takes to fall 60 dB, from a straight line fitted
/// (least squares) to its fall from [`FIT_FROM_DB`] to [`FIT_TO_DB`]. `None`
/// if it never falls that far.
pub fn decay_time(curve: &[f64], sample_rate: u32) -> Option<f64> {
    let from = curve.iter().position(|&db| db <= FIT_FROM_DB)?;
    let to = curve.iter().position(|&db| db <= FIT_TO_DB)?;
    if to <= from + 1 {
        return None;
    }
    let points = &curve[from..=to];
    let n = points.len() as f64;
    let mean_x = (n - 1.0) / 2.0;
    let mean_y = points.iter().sum::<f64>() / n;
    let (mut covariance, mut variance) = (0.0, 0.0);
    for (i, &db) in points.iter().enumerate() {
        let x = i as f64 - mean_x;
        covariance += x * (db - mean_y);
        variance += x * x;
    }
    // dB per sample.
    let slope = covariance / variance;
    (slope < 0.0).then(|| -60.0 / slope / f64::from(sample_rate))
}

/// The decay time in each of the [`OCTAVE_BANDS_HZ`], in seconds: each
/// band's [`decay_curve_db`] and its [`decay_time`]. `None` for a band with
/// nothing in it, or that never falls 35 dB.
pub fn band_decay_times(samples: &[f32], sample_rate: u32) -> Vec<Option<f64>> {
    octave_bands(samples, sample_rate)
        .iter()
        .map(|band| decay_time(&decay_curve_db(band)?, sample_rate))
        .collect()
}

/// The echo density of `samples` over time, by Abel and Huang's normalised
/// measure: in each 20 ms Hann window, the share of the samples (weighted
/// by the window) further from zero than the window's standard deviation,
/// over the share Gaussian noise has, erfc(1/√2). A smooth wash, which a
/// dense tail is, reads about 1; a few separate echoes read near 0, since
/// only they stand out from the silence between them. Every 1 ms, at the
/// time of the window's middle in seconds; before the file starts and after
/// it ends is silence. A silent window reads 0.
///
/// It's reported to match listening tests of how smooth a tail sounds
/// well. Noise that isn't Gaussian reads otherwise: evenly spread noise, as
/// a plain random number generator gives, reads about 1.33.
pub fn echo_density(samples: &[f32], sample_rate: u32) -> Vec<(f64, f64)> {
    let rate = f64::from(sample_rate);
    let window = ((ECHO_DENSITY_WINDOW_SECONDS * rate).round() as usize).max(2);
    let hop = ((ECHO_DENSITY_HOP_SECONDS * rate).round() as usize).max(1);
    let mut shape: Vec<f64> = (0..window)
        .map(|i| 0.5 - 0.5 * (std::f64::consts::TAU * (i as f64 + 0.5) / window as f64).cos())
        .collect();
    let total: f64 = shape.iter().sum();
    shape.iter_mut().for_each(|w| *w /= total);
    let at = |centre: usize, i: usize| {
        (centre + i)
            .checked_sub(window / 2)
            .and_then(|at| samples.get(at))
            .map_or(0.0, |&s| f64::from(s))
    };
    (0..=samples.len() / hop)
        .map(|frame| {
            let centre = frame * hop;
            let variance: f64 = shape
                .iter()
                .enumerate()
                .map(|(i, w)| w * at(centre, i).powi(2))
                .sum();
            let sigma = variance.sqrt();
            let outside: f64 = if sigma > 0.0 {
                shape
                    .iter()
                    .enumerate()
                    .filter(|&(i, _)| at(centre, i).abs() > sigma)
                    .map(|(_, w)| w)
                    .sum()
            } else {
                0.0
            };
            (centre as f64 / rate, outside / OUTSIDE_ONE_SIGMA)
        })
        .collect()
}

/// How long after the onset the echo density first reaches [`DENSE`]
/// (near 1, a smooth wash), in seconds, from an [`echo_density`] profile
/// and the onset's time in seconds. `None` if it never does.
pub fn dense_after(profile: &[(f64, f64)], onset_seconds: f64) -> Option<f64> {
    profile
        .iter()
        .find(|&&(_, density)| density >= DENSE)
        .map(|&(time, _)| (time - onset_seconds).max(0.0))
}

/// How alike the left and right are early and late, from -1 to 1 (as
/// [`correlation`]): early is the first [`EARLY_SECONDS`] after the onset,
/// late is everything after that. 1 is mono, near 0 is two unrelated
/// sides, a wide space. `None` for a part that's silent in either channel.
pub fn early_and_late_correlation(
    left: &[f32],
    right: &[f32],
    sample_rate: u32,
) -> [Option<f64>; 2] {
    let Some(start) = onset(&[left, right]) else {
        return [None, None];
    };
    let length = left.len().min(right.len());
    let split = (start + (EARLY_SECONDS * f64::from(sample_rate)).round() as usize).min(length);
    [start..split, split..length].map(|part| {
        let value = correlation(&left[part.clone()], &right[part]);
        value.is_finite().then_some(value)
    })
}

/// How bumpy the tail's spectrum is, in dB: a measure of colouration, the
/// resonances that stick out, as a metallic ring does. Our own proxy
/// (RFC-010's Risks), with no limit until it's checked against listening.
///
/// The tail is the stretch where the [`decay_curve_db`] falls from
/// [`FIT_FROM_DB`] to -60 dB (or to the end). Its spectrum is averaged over 100 ms
/// frames (Hann, overlapping by half), each scaled to the same power first,
/// so the end of the tail counts as much as its start, and a ring that
/// outlives the rest stands out. That spectrum is read in dB at 48 points
/// an octave, each the average of the bins in the 48th of an octave around
/// it, and
/// compared with its own average over the octave around it (its broad
/// shape, which is the reverb's tone, not its colouration). The colouration
/// is the RMS of the difference from [`COLOURATION_LOWEST_HZ`] to
/// [`COLOURATION_HIGHEST_HZ`] (or as high as the sample rate allows).
///
/// A ripple of ±A dB, a third of an octave from peak to peak, reads A/√2,
/// a little under where the frames' 10 Hz bins blur it, near 200 Hz. A
/// smooth decaying noise reads under 1 dB, less the longer it rings, since
/// there's more to average. `None` if the tail is shorter than a frame.
pub fn colouration_db(samples: &[f32], sample_rate: u32) -> Option<f64> {
    let curve = decay_curve_db(&samples.iter().map(|&s| f64::from(s)).collect::<Vec<_>>())?;
    let from = curve.iter().position(|&db| db <= FIT_FROM_DB)?;
    let to = curve
        .iter()
        .position(|&db| db <= TAIL_TO_DB)
        .unwrap_or(curve.len());
    let tail = &samples[from..to];
    let rate = f64::from(sample_rate);
    let frame = (COLOURATION_FRAME_SECONDS * rate).round() as usize;
    if tail.len() < frame {
        return None;
    }
    let power = level_power_spectrum(tail, frame);
    let bin_hz = rate / frame as f64;

    // The spectrum in dB on a log axis, from half an octave below the lowest
    // frequency to half an octave above the highest, so each has its octave.
    let top = (COLOURATION_HIGHEST_HZ * SQRT_2).min(0.9 * rate / 2.0);
    let bottom = COLOURATION_LOWEST_HZ / SQRT_2;
    let count = (POINTS_PER_OCTAVE as f64 * (top / bottom).log2()).floor() as usize + 1;
    let half_step = 2f64.powf(POINT_OCTAVES / 2.0);
    let levels: Vec<f64> = (0..count)
        .map(|i| {
            let hz = bottom * 2f64.powf(i as f64 / POINTS_PER_OCTAVE as f64);
            let (low, high) = (hz / half_step / bin_hz, hz * half_step / bin_hz);
            let (first, last) = (
                low.ceil() as usize,
                (high.floor() as usize).min(power.len() - 1),
            );
            let mean = if last >= first {
                power[first..=last].iter().sum::<f64>() / (last - first + 1) as f64
            } else {
                // Narrower than a bin: between the two either side.
                let at = (hz / bin_hz).min((power.len() - 1) as f64);
                let below = at.floor() as usize;
                let above = (below + 1).min(power.len() - 1);
                power[below] + (power[above] - power[below]) * at.fract()
            };
            10.0 * mean.max(1e-30).log10()
        })
        .collect();

    // Each point against the octave around it, the ends of the octave
    // weighed half, so a ripple that fits the octave averages to nothing.
    let half = POINTS_PER_OCTAVE / 2;
    let differences: Vec<f64> = (half..count.saturating_sub(half))
        .map(|i| {
            let window = &levels[i - half..=i + half];
            let sum: f64 = window.iter().sum::<f64>() - (window[0] + window[2 * half]) / 2.0;
            levels[i] - sum / (2 * half) as f64
        })
        .collect();
    if differences.is_empty() {
        return None;
    }
    let mean_square = differences.iter().map(|d| d * d).sum::<f64>() / differences.len() as f64;
    Some(mean_square.sqrt())
}

/// The average power spectrum of `samples` in frames of `frame` samples,
/// overlapping by half, under a Hann window, each frame scaled to the same
/// total power before they're averaged, so a quiet frame counts as much as
/// a loud one. Silent frames are left out.
fn level_power_spectrum(samples: &[f32], frame: usize) -> Vec<f64> {
    let fft = RealFftPlanner::<f64>::new().plan_fft_forward(frame);
    let window: Vec<f64> = (0..frame)
        .map(|i| 0.5 - 0.5 * (std::f64::consts::TAU * i as f64 / frame as f64).cos())
        .collect();
    let mut power = vec![0.0; frame / 2 + 1];
    let (mut input, mut output) = (fft.make_input_vec(), fft.make_output_vec());
    for start in (0..=samples.len() - frame).step_by((frame / 2).max(1)) {
        for ((to, &from), w) in input.iter_mut().zip(&samples[start..]).zip(&window) {
            *to = f64::from(from) * w;
        }
        fft.process(&mut input, &mut output)
            .expect("the buffers are the plan's");
        let total: f64 = output.iter().map(Complex::norm_sqr).sum();
        if total > 0.0 {
            for (sum, bin) in power.iter_mut().zip(&output) {
                *sum += bin.norm_sqr() / total;
            }
        }
    }
    power
}

/// The energy decay relief of `samples`: the decay curve of each frequency.
/// It's a [`spectrogram`] (with `window_seconds` and `hop_seconds`) whose
/// every bin is summed backwards from the end, as [`decay_curve_db`] sums
/// the whole sound, in dB relative to that bin's own total, so each
/// frequency starts at 0 dB and falls as it dies away. A frequency that
/// rings longer stays bright longer. Frequencies more than 60 dB quieter in
/// total than the loudest read [`SILENT_DB`], so a frequency with next to
/// nothing in it isn't drawn as if it were loud.
pub fn energy_decay_relief(
    samples: &[f32],
    sample_rate: u32,
    window_seconds: f64,
    hop_seconds: f64,
) -> Spectrogram {
    let mut relief = spectrogram(samples, sample_rate, window_seconds, hop_seconds);
    let bins = relief.frames.first().map_or(0, Vec::len);
    let mut totals = vec![0.0f64; bins];
    // From the end: each frame's energy and everything after it.
    for frame in relief.frames.iter_mut().rev() {
        for (total, db) in totals.iter_mut().zip(frame.iter_mut()) {
            if *db > SILENT_DB {
                *total += 10f64.powf(f64::from(*db) / 10.0);
            }
            *db = *total as f32;
        }
    }
    let loudest = totals.iter().fold(0.0f64, |a, &b| a.max(b));
    for frame in &mut relief.frames {
        for (remaining, &total) in frame.iter_mut().zip(&totals) {
            *remaining = if total > 0.0 && total >= loudest * 1e-6 && *remaining > 0.0 {
                (10.0 * (f64::from(*remaining) / total).log10()).max(f64::from(SILENT_DB)) as f32
            } else {
                SILENT_DB
            };
        }
    }
    relief
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: u32 = 48_000;

    /// Gaussian noise with a standard deviation of 1, the same every run:
    /// a xorshift, made Gaussian by Box and Muller's method.
    struct Gaussian(u64);

    impl Gaussian {
        fn new(seed: u64) -> Self {
            Self(seed.max(1))
        }

        fn uniform(&mut self) -> f64 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            ((self.0 >> 11) as f64 + 0.5) / (1u64 << 53) as f64
        }

        fn next(&mut self) -> f64 {
            let (a, b) = (self.uniform(), self.uniform());
            (-2.0 * a.ln()).sqrt() * (std::f64::consts::TAU * b).cos()
        }

        fn samples(&mut self, count: usize) -> Vec<f64> {
            (0..count).map(|_| self.next()).collect()
        }
    }

    /// How far an average of `runs` decay times measured from noise may be
    /// from the true `decay` in the band centred on `centre_hz`, as a
    /// fraction: three times ISO 3382-2's standard deviation for T30
    /// (0.55 T √((1 + 1.52/N) / (N B T)), for N decays in a band B Hz
    /// wide), since one noise's decay reads a little off by chance, more in
    /// a narrow band or a short decay; and 1% more.
    fn allowance(centre_hz: f64, decay: f64, runs: usize) -> f64 {
        let n = runs as f64;
        let bandwidth = centre_hz / SQRT_2;
        3.0 * 0.55 * ((1.0 + 1.52 / n) / (n * bandwidth * decay)).sqrt() + 0.01
    }

    /// The amplitude of a sound decaying 60 dB in `decay` seconds, at `t`.
    fn envelope(t: f64, decay: f64) -> f64 {
        10f64.powf(-3.0 * t / decay)
    }

    /// `seconds` of Gaussian noise decaying 60 dB in `decay` seconds, at
    /// 0.25 at its start.
    fn decaying_noise(seed: u64, seconds: f64, decay: f64) -> Vec<f32> {
        let count = (seconds * f64::from(RATE)) as usize;
        Gaussian::new(seed)
            .samples(count)
            .iter()
            .enumerate()
            .map(|(i, s)| (0.25 * s * envelope(i as f64 / f64::from(RATE), decay)) as f32)
            .collect()
    }

    /// `noise` with each frequency's amplitude scaled by `gain(hz)`, in one
    /// FFT of the whole: an exact filter with no phase shift.
    fn shaped(noise: &[f64], gain: impl Fn(f64) -> f64) -> Vec<f64> {
        let mut planner = RealFftPlanner::<f64>::new();
        let forward = planner.plan_fft_forward(noise.len());
        let inverse = planner.plan_fft_inverse(noise.len());
        let mut input = noise.to_vec();
        let mut spectrum = forward.make_output_vec();
        forward.process(&mut input, &mut spectrum).unwrap();
        let bin_hz = f64::from(RATE) / noise.len() as f64;
        for (k, bin) in spectrum.iter_mut().enumerate() {
            *bin *= gain(k as f64 * bin_hz);
        }
        let last = spectrum.len() - 1;
        spectrum[0].im = 0.0;
        spectrum[last].im = 0.0;
        let mut output = inverse.make_output_vec();
        inverse.process(&mut spectrum, &mut output).unwrap();
        output.iter().map(|s| s / noise.len() as f64).collect()
    }

    #[test]
    fn decaying_noise_reads_its_decay_time() {
        for decay in [0.3, 1.0, 2.0, 10.0] {
            // Long enough to fall well past 35 dB, so the end doesn't bend
            // the fit.
            let noise = decaying_noise(7, 1.5 * decay, decay);
            let wide: Vec<f64> = noise.iter().map(|&s| f64::from(s)).collect();
            let measured = decay_time(&decay_curve_db(&wide).unwrap(), RATE).unwrap();
            assert!(
                (measured / decay - 1.0).abs() < 0.02,
                "{decay} s reads {measured} s"
            );
        }
    }

    #[test]
    fn each_octave_reads_its_own_decay_time() {
        // Gaussian noise split exactly at the bands' edges, each band
        // decaying at its own rate: longer in the lows and shorter in the
        // highs, as a reverb with Bass and High Mults is. One noise's decay
        // reads a few percent off by chance, more in the narrow low bands
        // (see `allowance`), so this takes the average of four. A band
        // takes in a little of its neighbours just past its edges, so where
        // a neighbour rings longer it reads a little long: about 2% here,
        // where the steps are a third.
        let decays = [1.0, 1.0, 0.8, 0.6, 0.5, 0.4, 0.3, 0.25];
        let seconds = 1.75;
        let count = (seconds * f64::from(RATE)) as usize;
        let seeds = 1..=4;
        let mut average = [0.0; 8];
        for seed in seeds.clone() {
            let noise = Gaussian::new(seed).samples(count);
            let mut sum = vec![0.0f64; count];
            for (&centre, &decay) in OCTAVE_BANDS_HZ.iter().zip(&decays) {
                let (low, high) = (centre / SQRT_2, centre * SQRT_2);
                let band = shaped(&noise, |hz| f64::from(u8::from((low..high).contains(&hz))));
                for (i, (to, s)) in sum.iter_mut().zip(band).enumerate() {
                    *to += s * envelope(i as f64 / f64::from(RATE), decay);
                }
            }
            let ir: Vec<f32> = sum.iter().map(|&s| (0.25 * s) as f32).collect();
            for (to, measured) in average.iter_mut().zip(band_decay_times(&ir, RATE)) {
                let measured = measured.unwrap();
                *to += measured / seeds.clone().count() as f64;
            }
        }
        for ((centre, decay), measured) in OCTAVE_BANDS_HZ.iter().zip(decays).zip(average) {
            assert!(
                (measured / decay - 1.0).abs() < allowance(*centre, decay, 4),
                "{centre} Hz: {decay} s reads {measured} s"
            );
        }
    }

    #[test]
    fn a_short_decay_reads_short_in_every_band() {
        // A band's own ringing is shorter than its sound's: a 0.3 s decay,
        // the shortest RFC-010 tests, reads 0.3 s even at 63 Hz, within
        // what chance allows there (see `allowance`).
        for decay in [0.3, 0.5] {
            let seeds = 1..=4;
            let mut average = [0.0; 8];
            for seed in seeds.clone() {
                let ir = decaying_noise(seed, 3.0 * decay, decay);
                for (to, measured) in average.iter_mut().zip(band_decay_times(&ir, RATE)) {
                    *to += measured.unwrap() / seeds.clone().count() as f64;
                }
            }
            for (&centre, measured) in OCTAVE_BANDS_HZ.iter().zip(average) {
                assert!(
                    (measured / decay - 1.0).abs() < allowance(centre, decay, 4),
                    "{centre} Hz: {decay} s reads {measured} s"
                );
            }
        }
    }

    #[test]
    fn the_octave_bands_add_up_to_the_sound() {
        // Inside the bands' range, nothing falls between them.
        let noise = Gaussian::new(5).samples(RATE as usize);
        let inside = shaped(&noise, |hz| {
            f64::from(u8::from((100.0..8000.0).contains(&hz)))
        });
        let sound: Vec<f32> = inside.iter().map(|&s| (0.1 * s) as f32).collect();
        let bands = octave_bands(&sound, RATE);
        let energy = |s: &[f64]| s.iter().map(|x| x * x).sum::<f64>();
        let total = energy(&sound.iter().map(|&s| f64::from(s)).collect::<Vec<_>>());
        let banded: f64 = bands.iter().map(|b| energy(b)).sum();
        assert!((banded / total - 1.0).abs() < 0.02, "{banded} of {total}");
    }

    #[test]
    fn the_decay_curve_only_falls_and_silence_has_none() {
        let noise = decaying_noise(9, 1.0, 0.5);
        let wide: Vec<f64> = noise.iter().map(|&s| f64::from(s)).collect();
        let curve = decay_curve_db(&wide).unwrap();
        assert_eq!(curve[0], 0.0);
        assert!(curve.windows(2).all(|pair| pair[1] <= pair[0]));
        assert_eq!(decay_curve_db(&[0.0; 10]), None);
        // A curve that never falls 35 dB has no decay time.
        assert_eq!(decay_time(&[0.0, -6.0, -12.0, -20.0], RATE), None);
    }

    #[test]
    fn gaussian_noise_is_dense_and_a_sparse_click_train_isnt() {
        let noise: Vec<f32> = Gaussian::new(11)
            .samples(RATE as usize)
            .iter()
            .map(|&s| (0.1 * s) as f32)
            .collect();
        let profile = echo_density(&noise, RATE);
        // Away from the ends, where the window is half silence.
        let middle: Vec<f64> = profile
            .iter()
            .filter(|(t, _)| (0.05..0.95).contains(t))
            .map(|&(_, d)| d)
            .collect();
        let mean = middle.iter().sum::<f64>() / middle.len() as f64;
        assert!((mean - 1.0).abs() < 0.02, "noise reads {mean}");
        assert!(
            middle.iter().all(|&d| (d - 1.0).abs() < 0.25),
            "noise strays to {:?}",
            middle
                .iter()
                .fold((2.0f64, 0.0f64), |(lo, hi), &d| (lo.min(d), hi.max(d)))
        );
        let dense = dense_after(&profile, 0.0).unwrap();
        assert!(dense < 0.01, "dense after {dense} s");

        // A click every 10 ms: two in each window, and silence between.
        let mut clicks = vec![0.0f32; RATE as usize];
        for at in (0..clicks.len()).step_by(480) {
            clicks[at] = 0.5;
        }
        let profile = echo_density(&clicks, RATE);
        let loudest = profile.iter().map(|&(_, d)| d).fold(0.0, f64::max);
        assert!(loudest < 0.05, "clicks read up to {loudest}");
        assert_eq!(dense_after(&profile, 0.0), None);
        assert!(
            echo_density(&[0.0; 4800], RATE)
                .iter()
                .all(|&(_, d)| d == 0.0)
        );
    }

    #[test]
    fn echoes_that_thicken_into_noise_are_dense_when_they_do() {
        // Silence, then sparse clicks for 50 ms, then noise: dense soon
        // after the noise starts, not before.
        let mut ir = vec![0.0f32; RATE as usize / 2];
        let onset_at = 4800;
        for at in (onset_at..onset_at + 2400).step_by(480) {
            ir[at] = 0.5;
        }
        let mut noise = Gaussian::new(13);
        for s in &mut ir[onset_at + 2400..] {
            *s = (0.1 * noise.next()) as f32;
        }
        assert_eq!(onset(&[&ir]), Some(onset_at));
        let onset_seconds = onset_at as f64 / f64::from(RATE);
        let dense = dense_after(&echo_density(&ir, RATE), onset_seconds).unwrap();
        assert!((0.05..0.06).contains(&dense), "dense after {dense} s");
    }

    #[test]
    fn the_onset_is_the_first_sample_within_40_db_of_the_loudest() {
        // Noise 60 dB down from the start, then a click at 0.1 s: the noise
        // is too quiet to count, the click is to the sample.
        let mut channel = vec![0.0f32; 9600];
        let mut noise = Gaussian::new(17);
        for s in &mut channel {
            *s = (0.0001 * noise.next()) as f32;
        }
        channel[4800] = 0.5;
        channel[4801] = -0.3;
        assert_eq!(onset(&[&channel]), Some(4800));
        // In stereo, the earlier channel's.
        let mut right = vec![0.0f32; 9600];
        right[4700] = 0.01;
        assert_eq!(onset(&[&channel, &right]), Some(4700));
        right[4700] = 0.004;
        assert_eq!(onset(&[&channel, &right]), Some(4800), "41 dB down");
        assert_eq!(onset(&[&[0.0; 10]]), None);
    }

    #[test]
    fn alike_early_and_unrelated_late_read_so() {
        // The same in both channels for the first 80 ms after the onset,
        // then unrelated noise: a narrow start and a wide tail.
        let length = RATE as usize;
        let onset_at = 2400;
        let split = onset_at + 3840;
        let (mut a, mut b) = (Gaussian::new(19), Gaussian::new(23));
        let mut left = vec![0.0f32; length];
        let mut right = vec![0.0f32; length];
        for i in onset_at..length {
            let l = (0.1 * a.next()) as f32;
            left[i] = l;
            right[i] = if i < split {
                l
            } else {
                (0.1 * b.next()) as f32
            };
        }
        let [early, late] = early_and_late_correlation(&left, &right, RATE);
        assert!((early.unwrap() - 1.0).abs() < 1e-9, "early {early:?}");
        assert!(late.unwrap().abs() < 0.02, "late {late:?}");
        // Left = -right reads -1 throughout.
        let inverted: Vec<f32> = left.iter().map(|s| -s).collect();
        let [early, _] = early_and_late_correlation(&left, &inverted, RATE);
        assert!((early.unwrap() + 1.0).abs() < 1e-9);
        // A silent side has nothing to compare.
        let silent = vec![0.0f32; length];
        assert_eq!(
            early_and_late_correlation(&left, &silent, RATE),
            [None, None]
        );
    }

    #[test]
    fn a_ripple_of_a_known_depth_reads_its_depth() {
        // A tail whose spectrum ripples ±6 dB, a third of an octave from
        // peak to peak: its RMS is 6/√2 = 4.24 dB.
        let depth = 6.0;
        let seconds = 3.0;
        let decay = 2.0;
        let noise = Gaussian::new(29).samples((seconds * f64::from(RATE)) as usize);
        let rippled = shaped(&noise, |hz| {
            if hz <= 0.0 {
                return 0.0;
            }
            let db = depth * (std::f64::consts::TAU * 3.0 * (hz / 200.0).log2()).sin();
            10f64.powf(db / 20.0)
        });
        let tail: Vec<f32> = rippled
            .iter()
            .enumerate()
            .map(|(i, s)| (0.1 * s * envelope(i as f64 / f64::from(RATE), decay)) as f32)
            .collect();
        // A little under, where 10 Hz bins blur a ripple 46 Hz from peak to
        // peak, at 200 Hz.
        let expected = depth / SQRT_2;
        let measured = colouration_db(&tail, RATE).unwrap();
        assert!(
            (measured - expected).abs() < 0.4,
            "{measured} dB, expected {expected} dB"
        );
    }

    #[test]
    fn smooth_noise_reads_little_colouration_and_a_ring_reads_more() {
        let smooth = decaying_noise(31, 1.5, 1.0);
        let measured = colouration_db(&smooth, RATE).unwrap();
        assert!(measured < 1.0, "smooth noise reads {measured} dB");
        // The same, with a 1.5 kHz ring that outlives it: 3 s against 1 s,
        // starting 30 dB down.
        let ringing: Vec<f32> = smooth
            .iter()
            .enumerate()
            .map(|(i, &s)| {
                let t = i as f64 / f64::from(RATE);
                let ring = 0.25 * 0.0316 * (std::f64::consts::TAU * 1500.0 * t).sin();
                s + (ring * envelope(t, 3.0)) as f32
            })
            .collect();
        let rung = colouration_db(&ringing, RATE).unwrap();
        assert!(
            rung > measured + 1.0,
            "a ring reads {rung} dB, smooth {measured} dB"
        );
        // A tail too short to measure.
        assert_eq!(colouration_db(&decaying_noise(37, 0.2, 0.1), RATE), None);
    }

    #[test]
    fn the_decay_relief_falls_from_0_db_and_holds_longer_where_it_rings_longer() {
        // 250 Hz decaying in 2 s, 4 kHz in 0.5 s.
        let ir: Vec<f32> = (0..2 * RATE as usize)
            .map(|i| {
                let t = i as f64 / f64::from(RATE);
                let low = (std::f64::consts::TAU * 250.0 * t).sin() * envelope(t, 2.0);
                let high = (std::f64::consts::TAU * 4000.0 * t).sin() * envelope(t, 0.5);
                (0.25 * (low + high)) as f32
            })
            .collect();
        let relief = energy_decay_relief(&ir, RATE, 0.05, 0.01);
        let bin = |hz: f64| (hz / relief.bin_hz).round() as usize;
        let (low, high) = (bin(250.0), bin(4000.0));
        // Each starts at 0 dB and never rises.
        for b in [low, high] {
            assert!(relief.frames[0][b].abs() < 0.01);
            assert!(
                relief
                    .frames
                    .windows(2)
                    .all(|pair| pair[1][b] <= pair[0][b] + 1e-3)
            );
        }
        // At 0.25 s, the 2 s decay has fallen about 7.5 dB, the 0.5 s one
        // about 30.
        let at = (0.25 / relief.hop_seconds).round() as usize;
        let (l, h) = (relief.frames[at][low], relief.frames[at][high]);
        assert!((f64::from(l) + 7.5).abs() < 1.5, "250 Hz at {l} dB");
        assert!((f64::from(h) + 30.0).abs() < 3.0, "4 kHz at {h} dB");
        // Nothing at 12 kHz: drawn as silence.
        assert_eq!(relief.frames[0][bin(12_000.0)], SILENT_DB);
    }
}
