//! Measurements of rendered audio, taken from the waveform: pitch, level,
//! clicks, brightness, attack, decay and loudness. The sound tests check
//! Uta's renders with them, and `uta analyse` prints them as a [`report`],
//! so the numbers read from a render are the numbers the tests check.
//!
//! This crate only reads samples. It has no project, no engine, no Tauri and
//! no audio thread. Most measurements assume 48 kHz, the rate the sound tests
//! render at; the ones that take a `sample_rate` say so.
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
//! 3. Add an entry to [`report::MEASUREMENTS`], with its key, unit and what
//!    it measures. The text report, `--against` and `--json` all come from
//!    that table, so nothing else changes.

pub mod report;

/// The sample rate the measurements that don't take one assume.
const RATE: u32 = 48_000;

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

/// The pitch of a tone, in Hz, from its pitch track (as [`pitch_track`]
/// or [`pitch_over_time`] gives it): how many there are over how long they last.
/// `None` if there are fewer than two, or if their lengths jump about, as
/// noise's do. "Jump about" is the median change from one cycle to the next
/// being over 5%: a kick's pitch falls smoothly, by far less than that a
/// cycle once it settles, but noise crosses zero at random. Zero crossings
/// follow the fundamental of a clean tone; a tone with strong upper
/// partials can cross more than once a cycle.
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
/// its attack. It's the rise into the loudest sample, from the last 5 ms
/// before it whose loudest sample is under 10% of the peak, so in a file of
/// several hits it's the loudest hit's. Within that, it follows the running
/// peak (the loudest sample so far), interpolated between samples, so it
/// reads a tone's rise to within half a cycle. `None` for silence.
pub fn attack_seconds(samples: &[f32], sample_rate: u32) -> Option<f64> {
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
    Some((reaches(0.9 * peak) - reaches(0.1 * peak)) / f64::from(sample_rate))
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
/// fundamental of a tone rich in harmonics, such as a saw. A window is
/// 43 ms, so a sweep faster than that is smoothed: a kick's first few
/// milliseconds read lower than they are, and [`pitch_over_time`] sees
/// them better on a clean tone. Each point is a
/// window's middle, in seconds, and its pitch in Hz, between
/// [`LOWEST_PITCH_HZ`] and [`HIGHEST_PITCH_HZ`]; a window with no clear
/// period, such as noise or silence, has no point.
pub fn pitch_track(samples: &[f32], sample_rate: u32) -> Vec<(f64, f64)> {
    let rate = f64::from(sample_rate);
    let window = 2048;
    let longest = (rate / LOWEST_PITCH_HZ).ceil() as usize;
    let shortest = (rate / HIGHEST_PITCH_HZ).floor() as usize;
    let hop = (0.005 * rate) as usize;
    let n = (2 * window + longest).next_power_of_two();
    let mut track = Vec::new();
    let mut start = 0;
    while start + window + longest < samples.len() {
        let frame = &samples[start..start + window + longest + 1];
        if let Some(lag) = yin_period(frame, window, shortest, longest, n) {
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
/// samples, and `n` is a power of two at least `2 * window + longest`.
fn yin_period(
    frame: &[f32],
    window: usize,
    shortest: usize,
    longest: usize,
    n: usize,
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
        let mut re: Vec<f64> = samples.iter().map(|&s| f64::from(s)).collect();
        re.resize(n, 0.0);
        let mut im = vec![0.0; n];
        fft(&mut re, &mut im);
        (re, im)
    };
    let (a_re, a_im) = transform(&frame[..window]);
    let (b_re, b_im) = transform(frame);
    // The inverse FFT of conj(A)·B, as the conjugate of the FFT of its
    // conjugate.
    let mut re: Vec<f64> = (0..n)
        .map(|k| a_re[k] * b_re[k] + a_im[k] * b_im[k])
        .collect();
    let mut im: Vec<f64> = (0..n)
        .map(|k| -(a_re[k] * b_im[k] - a_im[k] * b_re[k]))
        .collect();
    fft(&mut re, &mut im);
    let correlation = |lag: usize| re[lag] / n as f64;

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

/// The magnitude spectrum of `samples` (whose length must be a power of two)
/// under a 4-term Blackman-Harris window, whose side lobes are 92 dB down,
/// so a strong partial doesn't hide quiet ones far from it. Bin `k` is
/// `k * sample_rate / samples.len()` Hz; only the bins up to half the sample
/// rate are returned.
pub fn spectrum(samples: &[f32]) -> Vec<f64> {
    let n = samples.len();
    assert!(n.is_power_of_two(), "FFT length must be a power of two");
    let window = |i: usize| {
        let x = std::f64::consts::TAU * i as f64 / n as f64;
        0.35875 - 0.48829 * x.cos() + 0.14128 * (2.0 * x).cos() - 0.01168 * (3.0 * x).cos()
    };
    let mut re: Vec<f64> = samples
        .iter()
        .enumerate()
        .map(|(i, &s)| f64::from(s) * window(i))
        .collect();
    let mut im = vec![0.0; n];

    fft(&mut re, &mut im);
    (0..=n / 2).map(|k| re[k].hypot(im[k])).collect()
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
/// with each cycle's frequency replaced by the median of it and the two
/// cycles either side, so one or two odd cycles, such as one spanning the
/// end of a note and the start of the next, don't count. Nearer the ends
/// it takes as many either side as there are, so a steady glide, as a
/// kick's, is kept; the first and last cycles take the median of the three
/// at that end.
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
/// its first sound, at 48 kHz. K-weighting is the filter loudness meters
/// put before measuring (ITU-R BS.1770-4): a high shelf of about +4 dB
/// above 2 kHz, where the ear is most sensitive, and a high-pass that hears
/// less of the lowest bass. Plain RMS would rank a kick's 50 Hz far louder
/// than it sounds next to a hat.
pub fn hit_loudness_db(samples: &[f32], seconds: f64) -> f64 {
    let start = first_sound(samples).expect("the hit sounds");
    let end = (start + (seconds * 48_000.0) as usize).min(samples.len());
    let weighted = k_weighted(&samples[start..end]);
    20.0 * rms(&weighted).max(1e-12).log10()
}

/// `samples` through the K-weighting filter, with BS.1770-4's own
/// coefficients for 48 kHz: the shelf, then the high-pass.
fn k_weighted(samples: &[f32]) -> Vec<f32> {
    let biquad = |input: &[f32], b: [f64; 3], a: [f64; 3]| {
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
    let shelf = biquad(
        samples,
        [
            1.535_124_859_586_97,
            -2.691_696_189_406_38,
            1.198_392_810_852_85,
        ],
        [1.0, -1.690_659_293_182_41, 0.732_480_774_215_85],
    );
    biquad(
        &shelf,
        [1.0, -2.0, 1.0],
        [1.0, -1.990_047_454_833_98, 0.990_072_250_366_21],
    )
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
    let n = samples.len();
    let mut re: Vec<f64> = samples.iter().map(|&s| f64::from(s)).collect();
    let mut im = vec![0.0; n];
    fft(&mut re, &mut im);
    (0..=n / 2).map(|k| re[k].hypot(im[k])).collect()
}

/// An iterative radix-2 FFT, in place: bit-reversal, then butterflies.
fn fft(re: &mut [f64], im: &mut [f64]) {
    let n = re.len();
    let mut j = 0;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j |= bit;
        if i < j {
            re.swap(i, j);
            im.swap(i, j);
        }
    }
    let mut len = 2;
    while len <= n {
        let angle = -std::f64::consts::TAU / len as f64;
        let twiddles: Vec<(f64, f64)> = (0..len / 2)
            .map(|k| ((angle * k as f64).cos(), (angle * k as f64).sin()))
            .collect();
        for start in (0..n).step_by(len) {
            for (k, &(w_re, w_im)) in twiddles.iter().enumerate() {
                let (a, b) = (start + k, start + k + len / 2);
                let t_re = re[b] * w_re - im[b] * w_im;
                let t_im = re[b] * w_im + im[b] * w_re;
                re[b] = re[a] - t_re;
                im[b] = im[a] - t_im;
                re[a] += t_re;
                im[a] += t_im;
            }
        }
        len <<= 1;
    }
}

/// The loudest partial in `spectrum` that isn't a harmonic of
/// `frequency_hz`, in dB relative to the loudest harmonic: for a saw, its
/// fundamental, and for a synced wave, whichever harmonic its restarts make
/// loudest.
pub fn loudest_alias_db(spectrum: &[f64], frequency_hz: f64, fft_len: usize) -> f64 {
    let bin_hz = f64::from(RATE) / fft_len as f64;
    // The window's main lobe is 4 bins either side; allow a little more.
    let lobe = 6.0;
    // The harmonic each bin is on, if it's on one: 0 is any offset, which
    // isn't an alias but isn't a harmonic either.
    let harmonic = |bin: usize| {
        let harmonic = (bin as f64 * bin_hz / frequency_hz).round();
        ((bin as f64 - harmonic * frequency_hz / bin_hz).abs() <= lobe).then_some(harmonic)
    };
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
}
