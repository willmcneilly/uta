//! The report `uta analyse` prints: every measurement in [`MEASUREMENTS`],
//! taken from one sound, as text or as data.
//!
//! # The report as data
//!
//! `uta analyse --json` prints a [`Report`] as a JSON object:
//!
//! - `file`: the file it measured, as it was named on the command line;
//! - `sample_rate`: in Hz;
//! - `channels`: the file's channels. A stereo file is measured as its mid,
//!   half of left plus right, except for the loudness in LUFS and the
//!   stereo measurements, which take each channel;
//! - `seconds`: how long the file is;
//! - `note`: the note `pitch_cents` is measured from, such as `"C3"`: the
//!   one `--pitch` names, or else the nearest to the measured pitch. `null`
//!   when there's no pitch to measure from;
//! - `above_hz`: the frequency `share_above_percent` is measured above;
//! - `impulse_response`: whether it was measured as an impulse response
//!   (`uta analyse --impulse-response`);
//! - `measurements`: an object with one number for each entry in
//!   [`MEASUREMENTS`], then, for an impulse response, each in
//!   [`IMPULSE_MEASUREMENTS`], under its `key`, in its `unit`, in the
//!   tables' order. A measurement that can't be taken (the pitch of noise,
//!   the decay of a sound that never dies away, anything of silence) is
//!   `null`.
//!
//! With `--against`, it prints `{"this": <report>, "against": <report>,
//! "difference": {...}}`, where `difference` has each measurement's key and
//! `this` minus `against`, or `null` if either is.

use std::cell::OnceCell;
use std::fmt::Write;

use serde::ser::{Serialize, SerializeMap, Serializer};

use crate::impulse::{
    band_decay_times, colouration_db, dense_after, early_and_late_correlation, echo_density, onset,
};
use crate::{
    MOMENTARY_SECONDS, SHORT_TERM_SECONDS, alias_db, attack_seconds, correlation, crest_factor,
    decay_seconds, first_sound, hit_loudness_db, inharmonicity, integrated_loudness,
    loudest_stretch, loudness_over_time, max_jump, peak, pitch_track, rms, share_above,
    smoothed_pitch, spectral_centroid, spectral_flatness, steady_pitch, stereo_width, while_loud,
};

/// One measurement in the report.
pub struct Measurement {
    /// Its name in `--json`, which ends in its unit.
    pub key: &'static str,
    /// Its name in the text report. `{above}` is replaced by the report's
    /// `above_hz`, and `{note}` by its note.
    pub label: &'static str,
    /// The unit it's in.
    pub unit: &'static str,
    /// How many decimal places the text report shows.
    pub decimals: usize,
    /// What it measures, and how.
    pub about: &'static str,
    /// Takes it. `None` when it can't be taken.
    pub measure: fn(&Sound) -> Option<f64>,
}

/// Everything the report measures, in the order it's printed. To add a
/// measurement, write it as a function in this crate, test it on a
/// synthetic signal with a known answer, and add an entry here: the text
/// report, `--against` and `--json` all come from this table.
pub const MEASUREMENTS: &[Measurement] = &[
    Measurement {
        key: "peak_dbfs",
        label: "peak",
        unit: "dBFS",
        decimals: 1,
        about: "The loudest sample, in dB relative to full scale (1.0).",
        measure: |sound| db(f64::from(peak(&sound.samples))),
    },
    Measurement {
        key: "rms_dbfs",
        label: "RMS",
        unit: "dBFS",
        decimals: 1,
        about: "The RMS level from the first sound to the end, in dB relative to full scale.",
        measure: |sound| db(rms(sound.since_first_sound())),
    },
    Measurement {
        key: "crest_factor_db",
        label: "crest factor",
        unit: "dB",
        decimals: 1,
        about: "The peak over the RMS, from the first sound to the end: 3.0 dB for a sine, \
                higher the punchier, lower the more squashed.",
        measure: |sound| db(crest_factor(sound.since_first_sound())),
    },
    Measurement {
        key: "loudness_db",
        label: "loudness",
        unit: "dB",
        decimals: 1,
        about: "How loud it sounds as a hit: the K-weighted RMS (the filter of ITU-R \
                BS.1770) of the mid from the first sound to the end, as the drum tests measure \
                a hit's balance. For a loudness to compare with other files, use the LUFS below.",
        measure: |sound| {
            let samples = sound.since_first_sound();
            (peak(samples) > 0.0).then(|| {
                hit_loudness_db(
                    samples,
                    sound.sample_rate,
                    samples.len() as f64 / f64::from(sound.sample_rate),
                )
            })
        },
    },
    Measurement {
        key: "loudness_momentary_lufs",
        label: "loudness, momentary max",
        unit: "LUFS",
        decimals: 1,
        about: "The loudest 400 ms, by ITU-R BS.1770's momentary loudness: every channel \
                K-weighted, their energy added, in LUFS. A stereo 1 kHz sine at -23 dBFS reads \
                -23.0. A file under 400 ms is measured with silence after it.",
        measure: |sound| sound.loudest(MOMENTARY_SECONDS),
    },
    Measurement {
        key: "loudness_short_term_lufs",
        label: "loudness, short-term max",
        unit: "LUFS",
        decimals: 1,
        about: "The loudest 3 s, by BS.1770's short-term loudness, measured the same way.",
        measure: |sound| sound.loudest(SHORT_TERM_SECONDS),
    },
    Measurement {
        key: "loudness_integrated_lufs",
        label: "loudness, integrated",
        unit: "LUFS",
        decimals: 1,
        about: "The loudness of the whole file, by BS.1770's gated integrated loudness: its \
                400 ms blocks (every 100 ms), leaving out silence under -70 LUFS and then \
                blocks more than 10 LU under the rest. `null` for silence or a file under \
                400 ms.",
        measure: |sound| integrated_loudness(&sound.channels(), sound.sample_rate),
    },
    Measurement {
        key: "brightness_hz",
        label: "brightness",
        unit: "Hz",
        decimals: 0,
        about: "The spectral centroid of the whole file: its average frequency, weighted by \
                each frequency's magnitude. Higher is brighter.",
        measure: |sound| finite(spectral_centroid(&sound.samples, sound.sample_rate)),
    },
    Measurement {
        key: "share_above_percent",
        label: "share above {above} Hz",
        unit: "%",
        decimals: 1,
        about: "The share of the whole file's energy above `above_hz`.",
        measure: |sound| {
            finite(100.0 * share_above(&sound.samples, sound.sample_rate, sound.above_hz))
        },
    },
    Measurement {
        key: "flatness",
        label: "noisiness (flatness)",
        unit: "",
        decimals: 3,
        about: "The spectral flatness of the whole file: from 0 for pure tones to 1 for white \
                noise. The geometric mean of its average power spectrum (40 ms frames, \
                overlapping by half) over the arithmetic mean, from 20 Hz to 20 kHz.",
        measure: |sound| spectral_flatness(&sound.samples, sound.sample_rate),
    },
    Measurement {
        key: "inharmonicity",
        label: "inharmonicity",
        unit: "",
        decimals: 3,
        about: "How far the whole file's partials sit from whole-number multiples of the pitch \
                (the measured pitch, or else `--pitch`'s note), from 0 (harmonic, as a saw is) \
                to 1: each spectral peak's distance from its nearest harmonic, weighted by its \
                power, doubled. Metallic and bell-like sounds read well above 0. `null` \
                without a pitch.",
        measure: |sound| inharmonicity(&sound.samples, sound.sample_rate, sound.harmonic_hz()?),
    },
    Measurement {
        key: "alias_db",
        label: "loudest alias",
        unit: "dB re harmonics",
        decimals: 1,
        about: "The loudest partial between the pitch's harmonics, in dB relative to the \
                loudest harmonic: aliasing folds back between the harmonics. A naive saw on a \
                high note reads high; a clean tone reads its noise floor. From the spectrum of \
                the loudest 500 ms (or the whole file, if it's shorter), so it reads the same \
                however long the file is. Needs a steady pitch: the measured pitch, or else \
                `--pitch`'s note. `null` without a pitch.",
        measure: |sound| {
            let stretch = loudest_stretch(&sound.samples, sound.sample_rate, ALIAS_SECONDS);
            finite(alias_db(stretch, sound.sample_rate, sound.harmonic_hz()?))
        },
    },
    Measurement {
        key: "stereo_correlation",
        label: "stereo correlation",
        unit: "",
        decimals: 2,
        about: "How alike the left and right channels are, from -1 to 1: 1 for mono (left = \
                right), near 0 for unrelated channels, -1 for left = -right, which cancels in \
                mono. `null` for a mono file or a silent channel.",
        measure: |sound| {
            let [left, right] = sound.left_right()?;
            finite(correlation(left, right))
        },
    },
    Measurement {
        key: "stereo_width_percent",
        label: "stereo width",
        unit: "%",
        decimals: 1,
        about: "The share of the energy in the side (left - right) rather than the mid (left + \
                right): 0 for mono, 50 for unrelated channels, 100 for left = -right. `null` \
                for a mono file.",
        measure: |sound| {
            let [left, right] = sound.left_right()?;
            finite(100.0 * stereo_width(left, right))
        },
    },
    Measurement {
        key: "attack_ms",
        label: "attack (10-90%)",
        unit: "ms",
        decimals: 1,
        about: "How long it takes to rise from 10% to 90% of its peak.",
        measure: |sound| attack_seconds(&sound.samples, sound.sample_rate).map(|s| s * 1000.0),
    },
    Measurement {
        key: "decay_20db_s",
        label: "decay to -20 dB",
        unit: "s",
        decimals: 3,
        about: "How long it takes to fall 20 dB from its loudest, from its level over 5 ms \
                windows. `null` if it never does.",
        measure: |sound| decay_seconds(&sound.samples, sound.sample_rate, 20.0),
    },
    Measurement {
        key: "decay_40db_s",
        label: "decay to -40 dB",
        unit: "s",
        decimals: 3,
        about: "How long it takes to fall 40 dB from its loudest. `null` if it never does.",
        measure: |sound| decay_seconds(&sound.samples, sound.sample_rate, 40.0),
    },
    Measurement {
        key: "pitch_hz",
        label: "pitch",
        unit: "Hz",
        decimals: 2,
        about: "The average pitch while it's within 40 dB of its loudest, from YIN's \
                pitch track (43 ms windows, every 5 ms), each point the median of those \
                around it. `null` if it has no steady pitch, as noise doesn't.",
        measure: Sound::pitch,
    },
    Measurement {
        key: "pitch_cents",
        label: "pitch off {note}",
        unit: "cents",
        decimals: 1,
        about: "How far the pitch is from `note`, in cents (hundredths of a semitone): \
                above it if positive.",
        measure: |sound| Some(cents(sound.pitch()?, sound.note()?.frequency())),
    },
    Measurement {
        key: "pitch_start_hz",
        label: "pitch at the start",
        unit: "Hz",
        decimals: 2,
        about: "The pitch at the first point of the track within 40 dB of its loudest.",
        measure: |sound| sound.pitch_over_time().first().map(|&(_, hz)| hz),
    },
    Measurement {
        key: "pitch_end_hz",
        label: "pitch at the end",
        unit: "Hz",
        decimals: 2,
        about: "The pitch at the last point of the track within 40 dB of its loudest.",
        measure: |sound| sound.pitch_over_time().last().map(|&(_, hz)| hz),
    },
    Measurement {
        key: "pitch_drift_cents",
        label: "largest pitch drift",
        unit: "cents",
        decimals: 1,
        about: "Where the pitch track is furthest from the average pitch, in cents from \
                it: above it if positive. In a file of several notes, it's the furthest note. A kick's falling pitch drifts by hundreds; a steady tone by a few.",
        measure: |sound| sound.largest_drift().map(|(_, cents)| cents),
    },
    Measurement {
        key: "pitch_drift_at_s",
        label: "largest pitch drift at",
        unit: "s",
        decimals: 3,
        about: "When that is, from the start of the file.",
        measure: |sound| sound.largest_drift().map(|(seconds, _)| seconds),
    },
    Measurement {
        key: "click_step_db",
        label: "largest step",
        unit: "dB re peak",
        decimals: 1,
        about: "The biggest change from one sample to the next, in dB relative to the peak. \
                A sine's steepest step is -24.8 dB at 440 Hz and -3.8 dB at 5 kHz; a click is a step well above the sound's own.",
        measure: |sound| {
            let (step, _) = max_jump(&sound.samples);
            db(f64::from(step) / f64::from(peak(&sound.samples)))
        },
    },
    Measurement {
        key: "click_at_s",
        label: "largest step at",
        unit: "s",
        decimals: 4,
        about: "When that step is, from the start of the file.",
        measure: |sound| {
            let (step, at) = max_jump(&sound.samples);
            (step > 0.0).then(|| at as f64 / f64::from(sound.sample_rate))
        },
    },
];

/// The measurements of an impulse response (RFC-010), in the order they're
/// printed after [`MEASUREMENTS`], for a sound marked as one with
/// [`Sound::as_impulse_response`]. They're from [`crate::impulse`].
pub const IMPULSE_MEASUREMENTS: &[Measurement] = &[
    Measurement {
        key: "onset_ms",
        label: "onset",
        unit: "ms",
        decimals: 3,
        about: "When the first sound arrives, to the sample, from the start of the file: the \
                first sample in either channel no more than 40 dB below the loudest. A \
                reverb's predelay.",
        measure: |sound| {
            let at = sound.impulse().onset?;
            Some(at as f64 * 1000.0 / f64::from(sound.sample_rate))
        },
    },
    Measurement {
        key: "decay_mid_s",
        label: "decay time, mid (500 Hz-1 kHz)",
        unit: "s",
        decimals: 3,
        about: "How long it rings: the time to fall 60 dB, from a straight line fitted to its \
                decay curve (the energy summed backwards from the end, Schroeder's method) from \
                -5 to -35 dB, ISO 3382's T30. The average of the 500 Hz and 1 kHz octave \
                bands', as a reverb's Decay is set. `null` if either never falls 35 dB.",
        measure: |sound| {
            let bands = &sound.impulse().band_decays;
            Some((bands[3]? + bands[4]?) / 2.0)
        },
    },
    Measurement {
        key: "decay_63hz_s",
        label: "decay time at 63 Hz",
        unit: "s",
        decimals: 3,
        about: "The decay time in the octave band around 63 Hz (62.5 Hz exactly), from its \
                own decay curve, as above. Each band's edges are half an octave either side of \
                its centre, falling 96 dB an octave beyond them. `null` for a band with nothing \
                in it, or that never falls 35 dB.",
        measure: |sound| sound.impulse().band_decays[0],
    },
    Measurement {
        key: "decay_125hz_s",
        label: "decay time at 125 Hz",
        unit: "s",
        decimals: 3,
        about: "The same in the octave band around 125 Hz.",
        measure: |sound| sound.impulse().band_decays[1],
    },
    Measurement {
        key: "decay_250hz_s",
        label: "decay time at 250 Hz",
        unit: "s",
        decimals: 3,
        about: "The same in the octave band around 250 Hz.",
        measure: |sound| sound.impulse().band_decays[2],
    },
    Measurement {
        key: "decay_500hz_s",
        label: "decay time at 500 Hz",
        unit: "s",
        decimals: 3,
        about: "The same in the octave band around 500 Hz.",
        measure: |sound| sound.impulse().band_decays[3],
    },
    Measurement {
        key: "decay_1khz_s",
        label: "decay time at 1 kHz",
        unit: "s",
        decimals: 3,
        about: "The same in the octave band around 1 kHz.",
        measure: |sound| sound.impulse().band_decays[4],
    },
    Measurement {
        key: "decay_2khz_s",
        label: "decay time at 2 kHz",
        unit: "s",
        decimals: 3,
        about: "The same in the octave band around 2 kHz.",
        measure: |sound| sound.impulse().band_decays[5],
    },
    Measurement {
        key: "decay_4khz_s",
        label: "decay time at 4 kHz",
        unit: "s",
        decimals: 3,
        about: "The same in the octave band around 4 kHz.",
        measure: |sound| sound.impulse().band_decays[6],
    },
    Measurement {
        key: "decay_8khz_s",
        label: "decay time at 8 kHz",
        unit: "s",
        decimals: 3,
        about: "The same in the octave band around 8 kHz.",
        measure: |sound| sound.impulse().band_decays[7],
    },
    Measurement {
        key: "dense_after_ms",
        label: "echo density near 1 after",
        unit: "ms",
        decimals: 1,
        about: "How long after the onset the tail becomes a smooth wash: when its echo \
                density (Abel and Huang's, over 20 ms windows every 1 ms) first reaches 0.9. \
                Echo density is the share of samples further from zero than the window's \
                standard deviation, over Gaussian noise's: near 0 for a few separate echoes, \
                1 for a smooth wash. `null` if it never gets there.",
        measure: |sound| {
            let impulse = sound.impulse();
            let onset = impulse.onset? as f64 / f64::from(sound.sample_rate);
            dense_after(&impulse.echo_density, onset).map(|s| s * 1000.0)
        },
    },
    Measurement {
        key: "stereo_correlation_early",
        label: "stereo correlation, early",
        unit: "",
        decimals: 2,
        about: "How alike the left and right are in the first 80 ms after the onset, from -1 \
                to 1: 1 for mono, near 0 for a wide space. `null` for a mono file.",
        measure: |sound| sound.impulse().correlation[0],
    },
    Measurement {
        key: "stereo_correlation_late",
        label: "stereo correlation, late",
        unit: "",
        decimals: 2,
        about: "The same, for everything after the first 80 ms: the tail.",
        measure: |sound| sound.impulse().correlation[1],
    },
    Measurement {
        key: "colouration_db",
        label: "colouration",
        unit: "dB",
        decimals: 2,
        about: "How bumpy the tail's spectrum is, from 200 Hz to 10 kHz: the RMS of its \
                spectrum (averaged over the tail from -5 to -35 dB, each 100 ms frame scaled \
                alike) against its own average over the octave around each point. A metallic \
                ring sticks out and reads higher; a smooth tail reads under a dB or so. Our own \
                measure, with no limit set yet. `null` if the tail is under 100 ms.",
        measure: |sound| sound.impulse().colouration,
    },
];

/// How long a stretch `alias_db` is measured over, in seconds. Its spectrum
/// has bins 2 Hz apart, so the harmonics' allowance is 12 Hz whatever the
/// file's length, which a pitch measured to a fraction of a cent is well
/// within at the top harmonic.
pub const ALIAS_SECONDS: f64 = 0.5;

/// The sound a report measures: its mid, its channels, and what it's
/// measured with.
pub struct Sound {
    /// The mid, which most measurements are taken from.
    samples: Vec<f32>,
    /// Each channel, for the loudness and the stereo measurements.
    each_channel: Vec<Vec<f32>>,
    sample_rate: u32,
    channels: u16,
    note: Option<Note>,
    above_hz: f64,
    /// Its pitch track within 40 dB of its loudest, from [`pitch_track`].
    track: Vec<(f64, f64)>,
    /// Whether it's an impulse response, measured with
    /// [`IMPULSE_MEASUREMENTS`] too.
    impulse_response: bool,
    /// The impulse response measurements, taken once, when first asked for.
    impulse: OnceCell<Impulse>,
}

/// The measurements of an impulse response that more than one entry in
/// [`IMPULSE_MEASUREMENTS`] reads, taken once.
struct Impulse {
    onset: Option<usize>,
    /// One for each of [`crate::impulse::OCTAVE_BANDS_HZ`].
    band_decays: Vec<Option<f64>>,
    echo_density: Vec<(f64, f64)>,
    /// Early and late.
    correlation: [Option<f64>; 2],
    colouration: Option<f64>,
}

impl Sound {
    /// A sound from interleaved samples. More than one channel is measured
    /// as the mid: their average, which for a sound in the centre is the
    /// same as either channel.
    /// The loudness and the stereo measurements are taken from each
    /// channel.
    pub fn new(interleaved: &[f32], channels: u16, sample_rate: u32) -> Self {
        let count = usize::from(channels.max(1));
        let samples = interleaved
            .chunks(count)
            .map(|frame| frame.iter().sum::<f32>() / frame.len() as f32)
            .collect::<Vec<f32>>();
        let each_channel = (0..count)
            .map(|c| interleaved.iter().skip(c).step_by(count).copied().collect())
            .collect();
        let track = while_loud(
            pitch_track(&samples, sample_rate),
            &samples,
            sample_rate,
            40.0,
        );
        Self {
            samples,
            each_channel,
            sample_rate,
            channels,
            note: None,
            above_hz: 5000.0,
            track,
            impulse_response: false,
            impulse: OnceCell::new(),
        }
    }

    /// Measures it as an impulse response too: the sound of a single click
    /// through a reverb or another effect, with [`IMPULSE_MEASUREMENTS`].
    pub fn as_impulse_response(mut self) -> Self {
        self.impulse_response = true;
        self
    }

    /// The impulse response measurements, taken the first time they're
    /// asked for. The decay, echo density and colouration are of the mid;
    /// the onset and correlation take each channel.
    fn impulse(&self) -> &Impulse {
        self.impulse.get_or_init(|| {
            let channels = self.channels();
            let correlation = match self.left_right() {
                Some([left, right]) => early_and_late_correlation(left, right, self.sample_rate),
                None => [None, None],
            };
            Impulse {
                onset: onset(&channels),
                band_decays: band_decay_times(&self.samples, self.sample_rate),
                echo_density: echo_density(&self.samples, self.sample_rate),
                correlation,
                colouration: colouration_db(&self.samples, self.sample_rate),
            }
        })
    }

    /// The mid it's measured from: the average of its channels.
    pub fn mid(&self) -> &[f32] {
        &self.samples
    }

    /// Measures the pitch from `note`, rather than from the nearest note.
    pub fn with_note(mut self, note: Note) -> Self {
        self.note = Some(note);
        self
    }

    /// Measures the share of the energy above `hz`, rather than 5 kHz.
    pub fn with_above_hz(mut self, hz: f64) -> Self {
        self.above_hz = hz;
        self
    }

    fn channels(&self) -> Vec<&[f32]> {
        self.each_channel.iter().map(Vec::as_slice).collect()
    }

    /// The left and right channels, if it's stereo.
    fn left_right(&self) -> Option<[&[f32]; 2]> {
        match self.channels().as_slice() {
            &[left, right] => Some([left, right]),
            _ => None,
        }
    }

    /// The loudest `window_seconds` of it, in LUFS, from
    /// [`loudness_over_time`].
    fn loudest(&self, window_seconds: f64) -> Option<f64> {
        let loudness = loudness_over_time(&self.channels(), self.sample_rate, window_seconds);
        finite(loudness.into_iter().fold(f64::NEG_INFINITY, f64::max))
    }

    /// The pitch the harmonics are measured from, for the aliasing and the
    /// inharmonicity: the measured pitch, which is where a tone's harmonics
    /// really are, or else, for a sound with no steady pitch of its own,
    /// the note it was given (`--pitch`).
    fn harmonic_hz(&self) -> Option<f64> {
        self.pitch().or_else(|| self.note.map(Note::frequency))
    }

    fn since_first_sound(&self) -> &[f32] {
        &self.samples[first_sound(&self.samples).unwrap_or(self.samples.len())..]
    }

    /// The pitch over time while it's within 40 dB of its loudest, with
    /// each point's time from the start of the file, so the noise floor of a
    /// tail doesn't count. It's [`smoothed_pitch`], so one or two odd
    /// points, such as where one note ends and the next starts, aren't read
    /// as a drift. Empty if it has no steady pitch.
    fn pitch_over_time(&self) -> Vec<(f64, f64)> {
        if self.pitch().is_none() {
            return Vec::new();
        }
        smoothed_pitch(&self.track)
    }

    /// The average pitch, from [`smoothed_pitch`] so an odd point doesn't
    /// move it, if the track as it is has a steady pitch.
    fn pitch(&self) -> Option<f64> {
        steady_pitch(&self.track)?;
        steady_pitch(&smoothed_pitch(&self.track))
    }

    /// The note the pitch is measured from: the one it was given, or the
    /// nearest.
    fn note(&self) -> Option<Note> {
        self.note.or_else(|| Some(Note::nearest(self.pitch()?)))
    }

    /// The point of the track furthest from the average pitch: when, and how
    /// far, in cents.
    fn largest_drift(&self) -> Option<(f64, f64)> {
        let pitch = self.pitch()?;
        self.pitch_over_time()
            .into_iter()
            .map(|(seconds, hz)| (seconds, cents(hz, pitch)))
            .max_by(|a, b| a.1.abs().total_cmp(&b.1.abs()))
    }
}

/// A note, as a MIDI note number: middle C (C4) is 60, and A4 is 440 Hz.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Note(pub i32);

impl Note {
    const NAMES: [&str; 12] = [
        "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
    ];

    /// The equal-tempered note nearest to `hz`.
    pub fn nearest(hz: f64) -> Self {
        Self((69.0 + 12.0 * (hz / 440.0).log2()).round() as i32)
    }

    /// Its frequency, in equal temperament with A4 at 440 Hz.
    pub fn frequency(self) -> f64 {
        440.0 * 2f64.powf(f64::from(self.0 - 69) / 12.0)
    }
}

impl std::fmt::Display for Note {
    /// `C3`, `F#4`, `A-1`.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = Self::NAMES[self.0.rem_euclid(12) as usize];
        write!(f, "{name}{}", self.0.div_euclid(12) - 1)
    }
}

impl std::str::FromStr for Note {
    type Err = String;

    /// A note name such as `C3`, `F#4`, `Bb2` or `A-1` (C4 is middle C), or
    /// a MIDI note number such as `48`.
    fn from_str(text: &str) -> Result<Self, String> {
        let wrong = || format!("{text:?} isn't a note: write it like C3, F#4 or Bb2");
        if let Ok(number) = text.parse::<i32>() {
            return Ok(Self(number));
        }
        let mut chars = text.chars();
        let letter = chars.next().ok_or_else(wrong)?.to_ascii_uppercase();
        let step = Self::NAMES
            .iter()
            .position(|name| name.starts_with(letter) && name.len() == 1)
            .ok_or_else(wrong)? as i32;
        let rest = chars.as_str();
        let accidentals = rest.len() - rest.trim_start_matches(['#', 'b']).len();
        let shift: i32 = rest[..accidentals]
            .chars()
            .map(|c| if c == '#' { 1 } else { -1 })
            .sum();
        let octave: i32 = rest[accidentals..].parse().map_err(|_| wrong())?;
        Ok(Self((octave + 1) * 12 + step + shift))
    }
}

/// Every measurement of one sound.
pub struct Report {
    /// The file it measured, as it was named.
    pub file: String,
    pub sample_rate: u32,
    pub channels: u16,
    pub seconds: f64,
    /// The note `pitch_cents` is measured from.
    pub note: Option<Note>,
    pub above_hz: f64,
    /// Whether it was measured as an impulse response, with
    /// [`IMPULSE_MEASUREMENTS`] too.
    pub impulse_response: bool,
    /// One for each of its [`Report::measurements`], in their order.
    pub values: Vec<Option<f64>>,
}

impl Report {
    /// Takes every measurement of `sound`, which came from `file`.
    pub fn measure(file: impl Into<String>, sound: &Sound) -> Self {
        Self {
            file: file.into(),
            sample_rate: sound.sample_rate,
            channels: sound.channels,
            seconds: sound.samples.len() as f64 / f64::from(sound.sample_rate),
            note: sound.note(),
            above_hz: sound.above_hz,
            impulse_response: sound.impulse_response,
            values: measurements(sound.impulse_response)
                .iter()
                .map(|measurement| (measurement.measure)(sound))
                .collect(),
        }
    }

    /// The measurements it took, in order: [`MEASUREMENTS`], then for an
    /// impulse response [`IMPULSE_MEASUREMENTS`].
    pub fn measurements(&self) -> Vec<&'static Measurement> {
        measurements(self.impulse_response)
    }

    /// The value of the measurement whose key is `key`, or `None` if it
    /// couldn't be taken or wasn't.
    pub fn get(&self, key: &str) -> Option<f64> {
        let index = self.measurements().iter().position(|m| m.key == key)?;
        self.values[index]
    }

    /// The measurement whose key is `key` as the text report shows it, with
    /// its unit, such as `-3.1 dBFS`, or `-` if it couldn't be taken.
    /// Panics if no measurement has that key.
    pub fn shown(&self, key: &str) -> String {
        self.shown_with(key, |index| self.values[index], false)
    }

    /// The difference in the measurement whose key is `key` between this
    /// and `other`, as `--against` shows it, with its unit, such as
    /// `+1.5 dB`.
    pub fn shown_difference(&self, other: &Report, key: &str) -> String {
        let difference = self.difference(other);
        self.shown_with(key, |index| difference[index], true)
    }

    fn shown_with(
        &self,
        key: &str,
        value: impl Fn(usize) -> Option<f64>,
        difference: bool,
    ) -> String {
        let measurements = self.measurements();
        let index = measurements
            .iter()
            .position(|m| m.key == key)
            .unwrap_or_else(|| panic!("no measurement is called {key:?}"));
        let measurement = measurements[index];
        let shown = show(value(index), measurement.decimals, difference);
        if shown == "-" || measurement.unit.is_empty() {
            shown
        } else {
            format!("{shown} {}", measurement.unit)
        }
    }

    /// Each measurement here minus the same in `other`, or `None` if either
    /// is. Both have to be measured alike: as impulse responses, or not.
    pub fn difference(&self, other: &Report) -> Vec<Option<f64>> {
        assert_eq!(
            self.impulse_response, other.impulse_response,
            "both or neither measured as impulse responses"
        );
        self.values
            .iter()
            .zip(&other.values)
            .map(|(a, b)| Some((*a)? - (*b)?))
            .collect()
    }

    /// The report as text: a line about the file, then one line for each
    /// measurement, with its unit.
    pub fn text(&self) -> String {
        let mut text = format!("{}\n", self.describe());
        let measurements = self.measurements();
        let labels: Vec<String> = measurements.iter().map(|m| self.label(m)).collect();
        let width = labels.iter().map(String::len).max().unwrap_or(0);
        for ((measurement, label), value) in measurements.iter().zip(&labels).zip(&self.values) {
            let value = show(*value, measurement.decimals, false);
            let line = format!("{label:<width$}  {value:>10} {}", measurement.unit);
            writeln!(text, "{}", line.trim_end()).unwrap();
        }
        text
    }

    /// This report and `other` side by side, with the difference (this
    /// minus other) in a third column.
    pub fn text_against(&self, other: &Report) -> String {
        let mut text = format!("{}\n{}\n\n", self.describe(), other.describe());
        let measurements = self.measurements();
        let labels: Vec<String> = measurements.iter().map(|m| self.label(m)).collect();
        let width = labels.iter().map(String::len).max().unwrap_or(0);
        let rows: Vec<[String; 3]> = measurements
            .iter()
            .zip(self.values.iter().zip(&other.values))
            .zip(self.difference(other))
            .map(|((m, (a, b)), difference)| {
                [
                    show(*a, m.decimals, false),
                    show(*b, m.decimals, false),
                    show(difference, m.decimals, true),
                ]
            })
            .collect();
        let headings = [self.file.as_str(), other.file.as_str(), "difference"];
        let widths: Vec<usize> = (0..3)
            .map(|column| {
                rows.iter()
                    .map(|row| row[column].len())
                    .chain([headings[column].len()])
                    .max()
                    .unwrap_or(0)
            })
            .collect();
        writeln!(
            text,
            "{:width$}  {:>a$}  {:>b$}  {:>c$}",
            "",
            headings[0],
            headings[1],
            headings[2],
            a = widths[0],
            b = widths[1],
            c = widths[2]
        )
        .unwrap();
        for ((m, label), row) in measurements.iter().zip(&labels).zip(&rows) {
            let line = format!(
                "{label:<width$}  {:>a$}  {:>b$}  {:>c$} {}",
                row[0],
                row[1],
                row[2],
                m.unit,
                a = widths[0],
                b = widths[1],
                c = widths[2]
            );
            writeln!(text, "{}", line.trim_end()).unwrap();
        }
        text
    }

    /// `kick.wav: 48000 Hz, 2 channels (most measured as their mid), 1.250 s`,
    /// and `, an impulse response` if it's measured as one.
    fn describe(&self) -> String {
        let channels = match self.channels {
            1 => "mono".to_string(),
            n => format!("{n} channels (most measured as their mid)"),
        };
        format!(
            "{}: {} Hz, {channels}, {:.3} s{}",
            self.file,
            self.sample_rate,
            self.seconds,
            if self.impulse_response {
                ", an impulse response"
            } else {
                ""
            }
        )
    }

    fn label(&self, measurement: &Measurement) -> String {
        let note = self
            .note
            .map_or("the nearest note".to_string(), |n| n.to_string());
        measurement
            .label
            .replace("{above}", &format!("{}", self.above_hz))
            .replace("{note}", &note)
    }
}

impl Serialize for Report {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(Some(8))?;
        map.serialize_entry("file", &self.file)?;
        map.serialize_entry("sample_rate", &self.sample_rate)?;
        map.serialize_entry("channels", &self.channels)?;
        map.serialize_entry("seconds", &self.seconds)?;
        map.serialize_entry("note", &self.note.map(|note| note.to_string()))?;
        map.serialize_entry("above_hz", &self.above_hz)?;
        map.serialize_entry("impulse_response", &self.impulse_response)?;
        map.serialize_entry("measurements", &self.values_as_data(&self.values))?;
        map.end()
    }
}

impl Report {
    /// `values`, one for each of this report's measurements (its own, or
    /// its [`Report::difference`] from another), as data.
    pub fn values_as_data<'a>(&self, values: &'a [Option<f64>]) -> Values<'a> {
        Values {
            measurements: self.measurements(),
            values,
        }
    }
}

/// Values for each of a report's measurements, as data: an object with each
/// one under its key, in the tables' order. Made by
/// [`Report::values_as_data`].
pub struct Values<'a> {
    measurements: Vec<&'static Measurement>,
    values: &'a [Option<f64>],
}

impl Serialize for Values<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(Some(self.values.len()))?;
        for (measurement, value) in self.measurements.iter().zip(self.values) {
            map.serialize_entry(measurement.key, value)?;
        }
        map.end()
    }
}

/// The measurements a report takes: [`MEASUREMENTS`], then for an impulse
/// response [`IMPULSE_MEASUREMENTS`].
fn measurements(impulse_response: bool) -> Vec<&'static Measurement> {
    let impulse: &'static [Measurement] = if impulse_response {
        IMPULSE_MEASUREMENTS
    } else {
        &[]
    };
    MEASUREMENTS.iter().chain(impulse).collect()
}

/// `value` with `decimals` places, `-` if there isn't one, and with a sign
/// if it's a difference. A difference that rounds to nothing is `0`.
fn show(value: Option<f64>, decimals: usize, difference: bool) -> String {
    let Some(value) = value else {
        return "-".to_string();
    };
    let shown = format!("{value:.decimals$}");
    if difference {
        if shown
            .trim_start_matches('-')
            .chars()
            .all(|c| c == '0' || c == '.')
        {
            "0".to_string()
        } else if value > 0.0 {
            format!("+{shown}")
        } else {
            shown
        }
    } else if shown
        .trim_start_matches('-')
        .chars()
        .all(|c| c == '0' || c == '.')
    {
        shown.trim_start_matches('-').to_string()
    } else {
        shown
    }
}

/// `ratio` in dB, or `None` for silence.
fn db(ratio: f64) -> Option<f64> {
    finite(20.0 * ratio.log10())
}

/// How far `hz` is above `from`, in cents.
fn cents(hz: f64, from: f64) -> f64 {
    1200.0 * (hz / from).log2()
}

fn finite(value: f64) -> Option<f64> {
    value.is_finite().then_some(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: u32 = 48_000;

    fn sine(frequency_hz: f64, seconds: f64) -> Vec<f32> {
        (0..(seconds * f64::from(RATE)) as usize)
            .map(|i| {
                (0.5 * (std::f64::consts::TAU * frequency_hz * i as f64 / 48_000.0).sin()) as f32
            })
            .collect()
    }

    /// A second of white noise, from -0.5 to 0.5.
    fn noise() -> Vec<f32> {
        let mut state = 1u32;
        (0..RATE)
            .map(|_| {
                state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                (state >> 8) as f32 / (1 << 24) as f32 - 0.5
            })
            .collect()
    }

    /// A second of a band-limited saw at 110 Hz (A2), with its harmonics up
    /// to 20 kHz.
    fn saw() -> Vec<f32> {
        (0..RATE)
            .map(|i| {
                let t = f64::from(i) / f64::from(RATE);
                let sum: f64 = (1..=180)
                    .map(|k| {
                        (std::f64::consts::TAU * 110.0 * f64::from(k) * t).sin() / f64::from(k)
                    })
                    .sum();
                (0.3 * sum) as f32
            })
            .collect()
    }

    #[test]
    fn every_measurement_has_its_own_key() {
        let all = measurements(true);
        for (i, a) in all.iter().enumerate() {
            assert!(
                all[i + 1..].iter().all(|b| b.key != a.key),
                "{} is there twice",
                a.key
            );
        }
    }

    #[test]
    fn notes_are_named_with_middle_c_as_c4() {
        for (name, number) in [
            ("C4", 60),
            ("C3", 48),
            ("c3", 48),
            ("A4", 69),
            ("F#4", 66),
            ("Bb2", 46),
            ("A-1", 9),
            ("48", 48),
        ] {
            assert_eq!(name.parse::<Note>(), Ok(Note(number)), "{name}");
        }
        assert_eq!(Note(66).to_string(), "F#4");
        assert_eq!(Note(9).to_string(), "A-1");
        assert!((Note(69).frequency() - 440.0).abs() < 1e-9);
        assert!("H3".parse::<Note>().is_err());
        assert!("C".parse::<Note>().is_err());
    }

    #[test]
    fn a_tone_reads_its_pitch_and_how_far_it_is_from_the_note() {
        // 10 cents above C3.
        let c3 = Note(48).frequency();
        let hz = c3 * 2f64.powf(10.0 / 1200.0);
        let report = Report::measure(
            "tone",
            &Sound::new(&sine(hz, 1.0), 1, RATE).with_note(Note(48)),
        );
        let pitch = report.get("pitch_hz").unwrap();
        assert!((pitch - hz).abs() < 0.01, "{pitch} Hz");
        let off = report.get("pitch_cents").unwrap();
        assert!((off - 10.0).abs() < 0.1, "{off} cents");
        let drift = report.get("pitch_drift_cents").unwrap();
        assert!(drift.abs() < 1.0, "a steady tone drifts {drift} cents");
    }

    #[test]
    fn noise_has_no_pitch() {
        let report = Report::measure("noise", &Sound::new(&noise(), 1, RATE));
        for key in [
            "pitch_hz",
            "pitch_cents",
            "pitch_start_hz",
            "pitch_drift_cents",
        ] {
            assert_eq!(report.get(key), None, "{key}");
        }
        assert!(report.get("peak_dbfs").is_some());
    }

    #[test]
    fn silence_measures_nothing_and_says_so() {
        let report = Report::measure("silence", &Sound::new(&[0.0; 4800], 1, RATE));
        assert_eq!(report.values, vec![None; MEASUREMENTS.len()]);
    }

    #[test]
    fn stereo_is_measured_as_its_mid() {
        // A tone on the left only: its mid is the tone at half the level.
        let left = sine(440.0, 0.5);
        let stereo: Vec<f32> = left.iter().flat_map(|&s| [s, 0.0]).collect();
        let mid: Vec<f32> = left.iter().map(|s| s * 0.5).collect();
        let a = Report::measure("mid", &Sound::new(&mid, 1, RATE));
        let b = Report::measure("stereo", &Sound::new(&stereo, 2, RATE));
        // Except the loudness in LUFS and the stereo measurements, which
        // take each channel.
        let each_channel = [
            "loudness_momentary_lufs",
            "loudness_short_term_lufs",
            "loudness_integrated_lufs",
            "stereo_correlation",
            "stereo_width_percent",
        ];
        for (m, (a, b)) in MEASUREMENTS.iter().zip(a.values.iter().zip(&b.values)) {
            if !each_channel.contains(&m.key) {
                assert_eq!(a, b, "{}", m.key);
            }
        }
        let peak = b.get("peak_dbfs").unwrap();
        assert!((peak - 20.0 * 0.25f64.log10()).abs() < 1e-3, "{peak} dBFS");
        // BS.1770 adds the channels' energy: the tone at full level in one
        // channel is 6 dB louder than at half level in a mono file's one.
        for key in ["loudness_momentary_lufs", "loudness_integrated_lufs"] {
            let louder = b.get(key).unwrap() - a.get(key).unwrap();
            assert!(
                (louder - 20.0 * 2f64.log10()).abs() < 1e-3,
                "{key} {louder} LU louder"
            );
        }
        // A tone in one channel and silence in the other: half its energy is
        // side, and the silent channel has no correlation.
        assert_eq!(b.get("stereo_width_percent"), Some(50.0));
        assert_eq!(b.get("stereo_correlation"), None);
        assert_eq!(
            a.get("stereo_width_percent"),
            None,
            "a mono file has no width"
        );
    }

    #[test]
    fn a_pitch_gives_the_aliasing_and_the_inharmonicity() {
        // The saw from `a_saw_reads_its_fundamental` has a pitch, so it has
        // both: harmonic, with nothing between its harmonics.
        let saw = Report::measure("saw", &Sound::new(&saw(), 1, RATE));
        let inharmonicity = saw.get("inharmonicity").unwrap();
        assert!(inharmonicity < 0.01, "{inharmonicity}");
        let alias = saw.get("alias_db").unwrap();
        assert!(alias < -80.0, "{alias} dB");
        // Noise has no pitch, so neither, unless it's told one.
        let noise_alone = Report::measure("noise", &Sound::new(&noise(), 1, RATE));
        assert_eq!(noise_alone.get("inharmonicity"), None);
        assert_eq!(noise_alone.get("alias_db"), None);
        let flatness = noise_alone.get("flatness").unwrap();
        assert!(flatness > 0.95, "noise reads {flatness}");
        let told = Report::measure("noise", &Sound::new(&noise(), 1, RATE).with_note(Note(48)));
        let inharmonicity = told.get("inharmonicity").unwrap();
        assert!(inharmonicity > 0.3, "noise told C3 reads {inharmonicity}");
    }

    #[test]
    fn a_clean_tone_reads_no_aliasing_however_long_it_is() {
        // A band-limited saw at 220 Hz, up to 20 kHz: from one spectrum of
        // the whole file, its 90th harmonic's allowance would shrink to
        // under 1 Hz at 10 s, less than the measured pitch's tiny error is
        // there, and the clean saw would read as aliased.
        let saw = |seconds: f64| -> Vec<f32> {
            (0..(seconds * f64::from(RATE)) as usize)
                .map(|i| {
                    let t = i as f64 / f64::from(RATE);
                    let sum: f64 = (1..=90)
                        .map(|k| {
                            (std::f64::consts::TAU * 220.0 * f64::from(k) * t).sin() / f64::from(k)
                        })
                        .sum();
                    (0.3 * sum) as f32
                })
                .collect()
        };
        for seconds in [1.0, 10.0] {
            let report = Report::measure("saw", &Sound::new(&saw(seconds), 1, RATE));
            let alias = report.get("alias_db").unwrap();
            assert!(alias < -80.0, "{seconds} s reads {alias} dB");
        }
    }

    #[test]
    fn a_single_odd_cycle_isnt_a_drift() {
        // 100 Hz, restarted from zero a quarter of a cycle after a crossing,
        // as where one note stops and the next starts: from one zero
        // crossing to the next, one cycle reads 80 Hz, 386 cents flat.
        let mut tone = sine(100.0, 0.1025);
        tone.extend(sine(100.0, 0.2));
        let report = Report::measure("restarted", &Sound::new(&tone, 1, RATE));
        let drift = report.get("pitch_drift_cents").unwrap();
        assert!(drift.abs() < 1.0, "drifts {drift} cents");
        for key in ["pitch_start_hz", "pitch_end_hz"] {
            let hz = report.get(key).unwrap();
            assert!((hz - 100.0).abs() < 0.01, "{key} {hz} Hz");
        }
    }

    #[test]
    fn a_saw_reads_its_fundamental() {
        // It reads its fundamental, not a harmonic.
        let report = Report::measure("saw", &Sound::new(&saw(), 1, RATE));
        assert_eq!(report.note, Some(Note(45)));
        for key in ["pitch_cents", "pitch_drift_cents"] {
            let cents = report.get(key).unwrap();
            assert!(cents.abs() < 1.0, "{key} {cents}");
        }
    }

    #[test]
    fn a_tone_that_crosses_zero_twice_a_cycle_reads_its_fundamental() {
        // A 110 Hz sine with its second harmonic at 1.5 times its level:
        // sin x (1 + 3 cos x) crosses zero going up twice a cycle, so zero
        // crossings read it an octave or more up.
        let tone: Vec<f32> = (0..RATE)
            .map(|i| {
                let x = std::f64::consts::TAU * 110.0 * f64::from(i) / f64::from(RATE);
                (0.3 * (x.sin() + 1.5 * (2.0 * x).sin())) as f32
            })
            .collect();
        let crossings = crate::steady_pitch(&crate::pitch_over_time(&tone, RATE));
        assert!(
            crossings.is_none_or(|hz| hz > 200.0),
            "zero crossings read {crossings:?}"
        );
        let report = Report::measure("tone", &Sound::new(&tone, 1, RATE));
        assert_eq!(report.note, Some(Note(45)));
        for key in ["pitch_cents", "pitch_drift_cents"] {
            let cents = report.get(key).unwrap();
            assert!(cents.abs() < 1.0, "{key} {cents}");
        }
    }

    #[test]
    fn a_quiet_tail_doesnt_count_towards_the_pitch() {
        // A second of 110 Hz, then a second an octave up 50 dB down: the
        // tail is below the 40 dB cut, so the pitch doesn't drift by the
        // octave (1200 cents), only by a few where windows straddle the cut.
        let mut tone = sine(110.0, 1.0);
        tone.extend(sine(220.0, 1.0).iter().map(|s| s * 0.003_162));
        let report = Report::measure("tail", &Sound::new(&tone, 1, RATE));
        let drift = report.get("pitch_drift_cents").unwrap();
        assert!(drift.abs() < 20.0, "drifts {drift} cents");
        let end = report.get("pitch_end_hz").unwrap();
        assert!((end - 110.0).abs() < 1.0, "ends at {end} Hz");
    }

    /// `seconds` of stereo Gaussian noise decaying 60 dB in `decay` seconds,
    /// after `silence` seconds of silence: the same in both channels for
    /// the first 80 ms, unrelated after. A stand-in for a reverb's impulse
    /// response, interleaved.
    fn impulse_response(silence: f64, seconds: f64, decay: f64) -> Vec<f32> {
        let mut state = 0x2545_f491_4f6c_dd1du64;
        let mut uniform = move || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            ((state >> 11) as f64 + 0.5) / (1u64 << 53) as f64
        };
        let mut gaussian = move || {
            let (a, b) = (uniform(), uniform());
            (-2.0 * a.ln()).sqrt() * (std::f64::consts::TAU * b).cos()
        };
        let rate = f64::from(RATE);
        let before = (silence * rate) as usize;
        let mut frames = vec![0.0f32; 2 * before];
        for i in 0..(seconds * rate) as usize {
            let t = i as f64 / rate;
            let level = 0.25 * 10f64.powf(-3.0 * t / decay);
            let left = gaussian();
            let right = if t < 0.08 { left } else { gaussian() };
            frames.extend([(level * left) as f32, (level * right) as f32]);
        }
        frames
    }

    #[test]
    fn an_impulse_response_is_measured_with_the_reverb_measurements() {
        // 25 ms of silence (a predelay), then a 1 s decay.
        let ir = impulse_response(0.025, 1.5, 1.0);
        let report = Report::measure("ir", &Sound::new(&ir, 2, RATE).as_impulse_response());
        assert!(report.impulse_response);
        assert_eq!(
            report.measurements().len(),
            MEASUREMENTS.len() + IMPULSE_MEASUREMENTS.len()
        );
        assert!(
            report
                .text()
                .lines()
                .next()
                .unwrap()
                .ends_with("an impulse response")
        );
        let get = |key: &str| report.get(key).unwrap_or_else(|| panic!("{key}"));
        assert!((get("onset_ms") - 25.0).abs() < 1e-9, "{}", get("onset_ms"));
        // One noise's decay reads a few percent off by chance.
        for key in [
            "decay_mid_s",
            "decay_500hz_s",
            "decay_1khz_s",
            "decay_4khz_s",
        ] {
            assert!((get(key) - 1.0).abs() < 0.06, "{key} {}", get(key));
        }
        let dense = get("dense_after_ms");
        assert!(dense < 15.0, "dense after {dense} ms");
        assert!((get("stereo_correlation_early") - 1.0).abs() < 1e-6);
        assert!(get("stereo_correlation_late").abs() < 0.05);
        assert!(get("colouration_db") < 1.0, "{}", get("colouration_db"));
        // Measured as an ordinary sound, it has none of them.
        let plain = Report::measure("ir", &Sound::new(&ir, 2, RATE));
        assert!(!plain.impulse_response);
        assert_eq!(plain.measurements().len(), MEASUREMENTS.len());
        assert_eq!(plain.get("decay_mid_s"), None);
    }

    #[test]
    fn a_mono_impulse_response_has_no_stereo_and_silence_has_nothing() {
        let stereo = impulse_response(0.0, 1.0, 0.5);
        let mono: Vec<f32> = stereo.iter().step_by(2).copied().collect();
        let report = Report::measure("ir", &Sound::new(&mono, 1, RATE).as_impulse_response());
        assert_eq!(report.get("stereo_correlation_early"), None);
        assert_eq!(report.get("stereo_correlation_late"), None);
        assert_eq!(report.get("onset_ms"), Some(0.0));
        let silence = Report::measure(
            "silence",
            &Sound::new(&[0.0; 4800], 2, RATE).as_impulse_response(),
        );
        assert_eq!(silence.values, vec![None; silence.measurements().len()]);
    }

    #[test]
    fn a_difference_that_rounds_to_nothing_is_0_and_a_value_has_no_minus_zero() {
        assert_eq!(show(Some(-0.01), 1, true), "0");
        assert_eq!(show(Some(6.02), 1, true), "+6.0");
        assert_eq!(show(Some(-6.02), 1, true), "-6.0");
        assert_eq!(show(Some(-0.01), 1, false), "0.0");
        assert_eq!(show(None, 1, false), "-");
    }
}
