//! The images `uta analyse --png <dir>` draws: two spectrograms, the level
//! over time, close-ups of the clicks, the harmonics (when there's a pitch)
//! and, with `--against`, a difference spectrogram.
//!
//! They're a sanity check on the numbers, not the evidence, so each has its
//! file's name and its key numbers printed on it, labelled axes, and fixed
//! scales, so two images compare at a glance. Each is drawn as SVG, with
//! real text, and turned into a PNG of the same name with `resvg`; both are
//! kept. Every image is [`WIDTH`] by [`HEIGHT`].
//!
//! The measurements come from `uta-analysis`, so the attack, decay, clicks
//! and aliasing marked on an image are the ones the report prints.

use std::fmt::Write;
use std::path::{Path, PathBuf};

use base64::Engine;
use resvg::{tiny_skia, usvg};
use uta_analysis::report::{ALIAS_SECONDS, Note, Report};
use uta_analysis::{
    Spectrogram, attack, clicks, envelope_db, harmonic_at, loudest_stretch, max_jump, peak,
    spectrogram, spectrum,
};

/// Every image's size, in pixels.
pub const WIDTH: u32 = 1200;
pub const HEIGHT: u32 = 640;

/// The plot's edges, in pixels from the image's.
const LEFT: f64 = 84.0;
const RIGHT: f64 = 1080.0;
const TOP: f64 = 104.0;
const BOTTOM: f64 = 590.0;

/// The spectrograms' fixed dB scale, from full scale down.
const FLOOR_DB: f64 = -90.0;
/// The harmonics' fixed dB scale, from the loudest harmonic down.
const HARMONICS_FLOOR_DB: f64 = -120.0;
/// The difference spectrogram's fixed scale, either way.
const DIFFERENCE_DB: f64 = 24.0;
/// The spectrograms' frequency axis, in Hz, up to half the sample rate.
const LOWEST_HZ: f64 = 20.0;
const HIGHEST_HZ: f64 = 20_000.0;
/// The two spectrograms' windows: short for when (a drum hit), long for
/// which frequencies (a tone and its tail).
const SHORT_WINDOW_SECONDS: f64 = 0.003;
const LONG_WINDOW_SECONDS: f64 = 0.05;
/// How many clicks are shown close up, at most: the largest.
const MOST_CLICKS: usize = 6;
/// How many samples either side of a click its close-up shows.
const CLOSE_UP_SAMPLES: usize = 48;

/// A WAV to draw: its mid, as the report measures it, and its report.
pub struct Sound<'a> {
    file: &'a Path,
    mid: Vec<f32>,
    sample_rate: u32,
    report: &'a Report,
}

impl<'a> Sound<'a> {
    /// The WAV at `file`, whose mid is `mid`, measured as `report`.
    pub fn new(file: &'a Path, mid: Vec<f32>, report: &'a Report) -> Self {
        Self {
            file,
            mid,
            sample_rate: report.sample_rate,
            report,
        }
    }

    /// Its name on the images: the file's name, without the folder.
    fn name(&self) -> String {
        self.file.file_name().map_or_else(
            || self.file.display().to_string(),
            |name| name.to_string_lossy().into_owned(),
        )
    }

    /// The name its images start with: the file's, without `.wav`.
    fn stem(&self) -> String {
        self.file.file_stem().map_or_else(
            || "sound".to_string(),
            |stem| stem.to_string_lossy().into_owned(),
        )
    }

    fn seconds(&self) -> f64 {
        self.mid.len() as f64 / f64::from(self.sample_rate)
    }

    /// The pitch its harmonics are measured from, as the report's
    /// aliasing is: the measured pitch, or else the note it was given.
    fn harmonic_hz(&self) -> Option<f64> {
        self.report
            .get("pitch_hz")
            .or_else(|| self.report.note.map(Note::frequency))
    }

    /// A line about the file: its rate, channels and length.
    fn describe(&self) -> String {
        let channels = match self.report.channels {
            1 => "mono".to_string(),
            2 => "stereo, drawn as its mid".to_string(),
            n => format!("{n} channels, drawn as their mid"),
        };
        format!(
            "{} Hz, {channels}, {:.3} s",
            self.sample_rate,
            self.seconds()
        )
    }
}

/// Draws every image for `this` (and the difference from `against`) into
/// `dir`, as SVG and PNG, and returns the PNGs' paths. The harmonics are
/// drawn only when there's a pitch to draw them from.
pub fn draw(dir: &Path, this: &Sound, against: Option<&Sound>) -> Result<Vec<PathBuf>, String> {
    std::fs::create_dir_all(dir)
        .map_err(|error| format!("couldn't create {}: {error}", dir.display()))?;
    let mut images = vec![
        (
            "spectrogram-short",
            spectrogram_image(this, SHORT_WINDOW_SECONDS),
        ),
        (
            "spectrogram-long",
            spectrogram_image(this, LONG_WINDOW_SECONDS),
        ),
        ("level", level_image(this)),
        ("clicks", clicks_image(this)),
    ];
    if let Some(harmonics) = harmonics_image(this) {
        images.push(("harmonics", harmonics));
    }
    let mut named: Vec<(String, String)> = images
        .into_iter()
        .map(|(image, svg)| (format!("{}-{image}", this.stem()), svg))
        .collect();
    if let Some(that) = against {
        named.push((
            format!("{}-vs-{}-difference", this.stem(), that.stem()),
            difference_image(this, that),
        ));
    }
    let renderer = Renderer::new();
    named
        .into_iter()
        .map(|(name, svg)| {
            let svg_path = dir.join(format!("{name}.svg"));
            let png_path = dir.join(format!("{name}.png"));
            std::fs::write(&svg_path, &svg)
                .map_err(|error| format!("couldn't write {}: {error}", svg_path.display()))?;
            let png = renderer.png(&svg)?;
            std::fs::write(&png_path, png)
                .map_err(|error| format!("couldn't write {}: {error}", png_path.display()))?;
            Ok(png_path)
        })
        .collect()
}

/// Turns SVG into PNG, with the system's fonts loaded once.
pub struct Renderer {
    options: usvg::Options<'static>,
}

impl Renderer {
    pub fn new() -> Self {
        let mut options = usvg::Options::default();
        options.fontdb_mut().load_system_fonts();
        Self { options }
    }

    /// The PNG of `svg`, at its own size.
    pub fn png(&self, svg: &str) -> Result<Vec<u8>, String> {
        let tree = usvg::Tree::from_str(svg, &self.options)
            .map_err(|error| format!("drew an image that isn't SVG: {error}"))?;
        let size = tree.size().to_int_size();
        let mut pixmap = tiny_skia::Pixmap::new(size.width(), size.height())
            .ok_or("drew an image with no size")?;
        resvg::render(&tree, tiny_skia::Transform::default(), &mut pixmap.as_mut());
        pixmap
            .encode_png()
            .map_err(|error| format!("couldn't encode a PNG: {error}"))
    }
}

/// A spectrogram with a window of `window_seconds`, on a log-frequency axis
/// with note names, from 0 to -90 dB.
fn spectrogram_image(sound: &Sound, window_seconds: f64) -> String {
    let seconds = sound.seconds();
    let spectrogram = spectrogram(
        &sound.mid,
        sound.sample_rate,
        window_seconds,
        hop_seconds(seconds, window_seconds),
    );
    let top_hz = highest_hz(sound.sample_rate);
    let levels = heat_map(&spectrogram, seconds, top_hz);
    let heat = encode_heat(&levels, |db| {
        spectrogram_colour((db - FLOOR_DB) / -FLOOR_DB)
    });
    let report = sound.report;
    let mut svg = Svg::new();
    svg.header(
        &format!(
            "{} · spectrogram, {:.0} ms window",
            sound.name(),
            window_seconds * 1000.0
        ),
        &[
            ("peak", report.shown("peak_dbfs")),
            ("loudness", report.shown("loudness_integrated_lufs")),
            ("brightness", report.shown("brightness_hz")),
            ("flatness", report.shown("flatness")),
            ("pitch", pitch_text(report)),
        ],
        &sound.describe(),
    );
    svg.image(&heat);
    svg.frequency_axis(top_hz);
    svg.time_axis(seconds);
    svg.colour_bar(
        &[(0.0, "0 dB"), (0.5, "-45"), (1.0, "-90 dB")],
        "spectrogram",
        "re a full-scale sine",
    );
    svg.finish()
}

/// The difference between two long-window spectrograms, `this` minus
/// `against`, in dB, from -24 to +24: red where `this` is louder, blue
/// where it's quieter. Both are floored at -90 dB first, so silence against
/// silence is no difference.
fn difference_image(this: &Sound, against: &Sound) -> String {
    let seconds = this.seconds().max(against.seconds());
    let top_hz = highest_hz(this.sample_rate.min(against.sample_rate));
    let difference = difference_levels(this, against, seconds, top_hz);
    let heat = encode_heat(&difference, |db| difference_colour(db / DIFFERENCE_DB));
    let (r1, r2) = (this.report, against.report);
    let mut svg = Svg::new();
    svg.header(
        &format!(
            "{} minus {} · difference, {:.0} ms window",
            this.name(),
            against.name(),
            LONG_WINDOW_SECONDS * 1000.0
        ),
        &[
            ("peak", r1.shown_difference(r2, "peak_dbfs")),
            (
                "loudness",
                r1.shown_difference(r2, "loudness_integrated_lufs"),
            ),
            ("brightness", r1.shown_difference(r2, "brightness_hz")),
            ("decay to -40 dB", r1.shown_difference(r2, "decay_40db_s")),
        ],
        &format!("{}; against {}", this.describe(), against.describe()),
    );
    svg.image(&heat);
    svg.frequency_axis(top_hz);
    svg.time_axis(seconds);
    svg.colour_bar(
        &[(0.0, "+24 dB"), (0.5, "0"), (1.0, "-24 dB")],
        "difference",
        "this louder / quieter",
    );
    svg.finish()
}

/// The level over time in dB, from 0 to -90 dBFS, with the peak, the
/// attack and the decay to -20 and -40 dB marked, and the attack close up
/// beneath.
fn level_image(sound: &Sound) -> String {
    let rate = sound.sample_rate;
    let seconds = sound.seconds();
    let report = sound.report;
    let mut svg = Svg::new();
    svg.header(
        &format!("{} · level over time", sound.name()),
        &[
            ("peak", report.shown("peak_dbfs")),
            ("RMS", report.shown("rms_dbfs")),
            ("crest", report.shown("crest_factor_db")),
            ("attack", report.shown("attack_ms")),
            ("decay -20", report.shown("decay_20db_s")),
            ("-40", report.shown("decay_40db_s")),
        ],
        &sound.describe(),
    );
    // The whole sound above, the attack close up below.
    let whole = Plot::new(LEFT, RIGHT, TOP, 400.0);
    let close = Plot::new(LEFT, RIGHT, 460.0, BOTTOM);

    // The same 5 ms windows the decay is measured from.
    let envelope = envelope_db(&sound.mid, rate, 0.005);
    let x = |t: f64| whole.x(t / seconds.max(1e-9));
    let y = |db: f64| whole.y((db - FLOOR_DB) / -FLOOR_DB);
    svg.grid_db(&whole, FLOOR_DB, 10.0, "dBFS");
    let points: Vec<(f64, f64)> = envelope
        .iter()
        .map(|&(t, db)| (x(t), y(db.max(FLOOR_DB))))
        .collect();
    svg.polyline(&points, "trace");
    svg.time_axis_on(&whole, seconds);

    let loudest = envelope
        .iter()
        .max_by(|a, b| a.1.total_cmp(&b.1))
        .filter(|&&(_, db)| db > FLOOR_DB);
    if let Some(&(peak_time, peak_db)) = loudest {
        svg.dot(x(peak_time), y(peak_db), 4.0, "mark");
        for (drop, key) in [(20.0, "decay_20db_s"), (40.0, "decay_40db_s")] {
            let level = peak_db - drop;
            if level > FLOOR_DB {
                svg.hline(whole.left, whole.right, y(level), "guide");
            }
            if let Some(decay) = report.get(key) {
                let at = x(peak_time + decay);
                svg.vline(at, whole.top, whole.bottom, "mark");
                svg.text(
                    at + 4.0,
                    y(level.max(FLOOR_DB)) - 6.0,
                    &format!("-{drop:.0} dB at +{decay:.3} s"),
                    "label mark-text",
                );
            }
        }
    }

    // The attack: the samples' size against 10% and 90% of the peak.
    match attack(&sound.mid, rate) {
        Some(found) => {
            let (from, to) = found.seconds(rate);
            // The attack band on the whole level, at least 2 px wide.
            let (a, b) = (x(from), x(to).max(x(from) + 2.0));
            svg.band(a, b, whole.top, whole.bottom, "attack-band");
            svg.text(
                b + 4.0,
                whole.top + 14.0,
                &format!("attack {}", report.shown("attack_ms")),
                "label mark-text",
            );
            let rise = to - from;
            let pad = rise.max(0.002);
            let (start, end) = ((from - pad).max(0.0), (to + pad).min(seconds));
            let peak = f64::from(peak(&sound.mid));
            let first = (start * f64::from(rate)) as usize;
            let last = ((end * f64::from(rate)) as usize).min(sound.mid.len());
            let cx = |t: f64| close.x((t - start) / (end - start).max(1e-9));
            let cy = |level: f64| close.y(level / peak);
            svg.frame(&close);
            for level in [0.1, 0.9] {
                svg.hline(close.left, close.right, cy(level * peak), "guide");
                svg.text(
                    close.left - 6.0,
                    cy(level * peak) + 4.0,
                    &format!("{:.0}%", level * 100.0),
                    "label end",
                );
            }
            let samples: Vec<(f64, f64)> = (first..last)
                .map(|i| {
                    let t = i as f64 / f64::from(rate);
                    (cx(t), cy(f64::from(sound.mid[i].abs())))
                })
                .collect();
            svg.polyline(&samples, "trace");
            for at in [from, to] {
                svg.vline(cx(at), close.top, close.bottom, "mark");
            }
            svg.text(
                close.left,
                close.top - 8.0,
                &format!(
                    "the attack close up: the size of each sample, from {:.4} s to {:.4} s; 10% at {from:.4} s, 90% at {to:.4} s",
                    start, end
                ),
                "label",
            );
        }
        None => svg.text(close.left, close.top + 20.0, "silent: no attack", "label"),
    }
    svg.finish()
}

/// Close-ups of the waveform, each sample a dot, at each place a click was
/// found: the largest [`MOST_CLICKS`]. With none, the largest step instead.
fn clicks_image(sound: &Sound) -> String {
    let rate = sound.sample_rate;
    let report = sound.report;
    let found = clicks(&sound.mid, rate);
    let peak = f64::from(peak(&sound.mid));
    let step_at = |at: usize| {
        if at == 0 || at >= sound.mid.len() {
            0.0
        } else {
            f64::from((sound.mid[at] - sound.mid[at - 1]).abs())
        }
    };
    let mut shown: Vec<usize> = found.clone();
    shown.sort_by(|&a, &b| step_at(b).total_cmp(&step_at(a)));
    shown.truncate(MOST_CLICKS);
    shown.sort_unstable();
    let summary = match found.len() {
        0 => "none, so the largest step instead".to_string(),
        n if n > MOST_CLICKS => format!("{n}, the {MOST_CLICKS} largest shown"),
        n => n.to_string(),
    };
    if shown.is_empty() {
        shown.push(max_jump(&sound.mid).1);
    }
    let mut svg = Svg::new();
    svg.header(
        &format!("{} · clicks close up", sound.name()),
        &[
            ("largest step", report.shown("click_step_db")),
            ("at", report.shown("click_at_s")),
            ("clicks", summary),
        ],
        &format!(
            "{}. A click is a step over {:.0} times the steps around it. Each sample is a dot; the scale is each panel's own.",
            sound.describe(),
            uta_analysis::CLICK_RATIO
        ),
    );
    let (columns, gap) = (3, 24.0);
    let width = (RIGHT - LEFT - gap * (columns as f64 - 1.0)) / columns as f64;
    let height = (BOTTOM - TOP - gap - 2.0 * 22.0) / 2.0;
    for (index, &at) in shown.iter().enumerate() {
        let (column, row) = (index % columns, index / columns);
        let left = LEFT + column as f64 * (width + gap);
        let top = TOP + 22.0 + row as f64 * (height + gap + 22.0);
        let plot = Plot::new(left, left + width, top, top + height);
        // The samples either side, kept whole at the file's ends.
        let first = at
            .saturating_sub(CLOSE_UP_SAMPLES)
            .min(sound.mid.len().saturating_sub(2 * CLOSE_UP_SAMPLES));
        let last = (first + 2 * CLOSE_UP_SAMPLES).min(sound.mid.len());
        let window = &sound.mid[first..last];
        let scale = full_scale_ladder(f64::from(uta_analysis::peak(window)));
        let x = |i: usize| plot.x((i - first) as f64 / (2 * CLOSE_UP_SAMPLES) as f64);
        let y = |sample: f32| plot.y(0.5 + 0.5 * f64::from(sample) / scale);
        svg.frame(&plot);
        svg.hline(plot.left, plot.right, y(0.0), "guide");
        let points: Vec<(f64, f64)> = (first..last).map(|i| (x(i), y(sound.mid[i]))).collect();
        svg.polyline(&points, "thin");
        for &(px, py) in &points {
            svg.dot(px, py, 2.2, "sample");
        }
        if at > first {
            svg.band(x(at - 1), x(at), plot.top, plot.bottom, "attack-band");
        }
        let step_db = fixed(20.0 * (step_at(at) / peak).max(1e-12).log10(), 1);
        svg.text(
            plot.left,
            plot.top - 6.0,
            &format!(
                "{:.4} s · step {step_db} dB re peak",
                at as f64 / f64::from(rate)
            ),
            "label",
        );
        svg.text(
            plot.right - 4.0,
            plot.top + 14.0,
            &format!("±{}", trim_number(scale)),
            "label end",
        );
    }
    svg.finish()
}

/// The spectrum of the loudest half second against a grid of the pitch's
/// harmonics: grey bars on the lines are harmonics, red bars between them
/// are aliasing (or anything else that isn't a harmonic). In dB from the
/// loudest harmonic, from 0 to -120. `None` without a pitch.
fn harmonics_image(sound: &Sound) -> Option<String> {
    let pitch = sound.harmonic_hz()?;
    let rate = sound.sample_rate;
    let stretch = loudest_stretch(&sound.mid, rate, ALIAS_SECONDS);
    let spectrum = spectrum(stretch);
    if spectrum.is_empty() {
        return None;
    }
    let bin_hz = f64::from(rate) / stretch.len() as f64;
    let harmonic: Vec<Option<f64>> = (0..spectrum.len())
        .map(|bin| harmonic_at(bin, bin_hz, pitch))
        .collect();
    let reference = spectrum
        .iter()
        .zip(&harmonic)
        .filter(|(_, h)| h.is_some_and(|h| h >= 1.0))
        .fold(0.0f64, |a, (&b, _)| a.max(b));
    if reference == 0.0 {
        return None;
    }
    let nyquist = f64::from(rate) / 2.0;
    let plot = Plot::new(LEFT, RIGHT, TOP, BOTTOM);
    let columns = (plot.right - plot.left) as usize;
    // Each column's loudest harmonic bin and loudest bin between them.
    let mut on = vec![f64::NEG_INFINITY; columns];
    let mut between = vec![f64::NEG_INFINITY; columns];
    for (bin, (&magnitude, h)) in spectrum.iter().zip(&harmonic).enumerate() {
        let column = ((bin as f64 * bin_hz / nyquist) * columns as f64) as usize;
        let column = column.min(columns - 1);
        let db = 20.0 * (magnitude / reference).max(1e-12).log10();
        let to = if h.is_some() { &mut on } else { &mut between };
        to[column] = to[column].max(db);
    }
    let report = sound.report;
    let mut svg = Svg::new();
    svg.header(
        &format!(
            "{} · harmonics of {pitch:.2} Hz{}",
            sound.name(),
            report
                .note
                .map_or(String::new(), |note| format!(" ({note})"))
        ),
        &[
            ("pitch", pitch_text(report)),
            ("loudest alias", report.shown("alias_db")),
            ("inharmonicity", report.shown("inharmonicity")),
        ],
        &format!(
            "{}. The loudest {:.2} s. Grey on the lines: harmonics. Red between them: aliasing.",
            sound.describe(),
            stretch.len() as f64 / f64::from(rate)
        ),
    );
    svg.grid_db(&plot, HARMONICS_FLOOR_DB, 20.0, "dB");
    let y = |db: f64| plot.y((db - HARMONICS_FLOOR_DB) / -HARMONICS_FLOOR_DB);
    // The harmonics' lines, when they're far enough apart to see, labelled
    // far enough apart to read.
    let spacing = pitch / nyquist * (plot.right - plot.left);
    let every = (40.0 / spacing).ceil().max(1.0) as usize;
    let count = (nyquist / pitch) as usize;
    if spacing >= 4.0 {
        for n in 1..=count {
            let at = plot.x(n as f64 * pitch / nyquist);
            svg.vline(at, plot.top, plot.bottom, "harmonic-line");
            if n % every == 0 || n == 1 {
                svg.text(at, plot.bottom + 16.0, &n.to_string(), "label middle");
            }
        }
        svg.text(plot.left, plot.bottom + 34.0, "harmonic", "label");
    } else {
        svg.text(
            plot.left,
            plot.bottom + 34.0,
            &format!("{count} harmonics, too close to draw as lines: told apart by colour"),
            "label",
        );
    }
    svg.frequency_ticks_linear(&plot, nyquist);
    for (values, class) in [(&on, "bar-harmonic"), (&between, "bar-alias")] {
        let mut path = String::new();
        for (column, &db) in values.iter().enumerate() {
            if db > HARMONICS_FLOOR_DB {
                let px = plot.left + column as f64 + 0.5;
                write!(path, "M{px:.1} {:.1}V{:.1}", plot.bottom, y(db)).unwrap();
            }
        }
        svg.path(&path, class);
    }
    // The loudest alias, marked, as the report measures it.
    let loudest = between
        .iter()
        .enumerate()
        .max_by(|a, b| a.1.total_cmp(b.1))
        .filter(|&(_, &db)| db > HARMONICS_FLOOR_DB);
    if let Some((column, &db)) = loudest {
        let (px, py) = (plot.left + column as f64 + 0.5, y(db));
        svg.dot(px, py - 6.0, 4.0, "mark");
        let (x, class) = if px > (plot.left + plot.right) / 2.0 {
            (px - 8.0, "label mark-text end")
        } else {
            (px + 8.0, "label mark-text")
        };
        svg.text(x, py - 12.0, &format!("loudest alias {db:.1} dB"), class);
    }
    Some(svg.finish())
}

/// The pitch, and how far off its note, as one piece of text.
fn pitch_text(report: &Report) -> String {
    match (report.get("pitch_hz"), report.note) {
        (Some(_), Some(note)) => format!(
            "{} ({} off {note})",
            report.shown("pitch_hz"),
            report.shown("pitch_cents")
        ),
        (Some(_), None) => report.shown("pitch_hz"),
        (None, _) => "none".to_string(),
    }
}

/// How far apart a spectrogram's frames are: a quarter of the window, or
/// more for a long file, so there are about two frames to each pixel, but
/// never more than half the window, so nothing falls between them.
fn hop_seconds(seconds: f64, window_seconds: f64) -> f64 {
    let per_pixel = seconds / (RIGHT - LEFT) / 2.0;
    per_pixel.clamp(window_seconds / 4.0, window_seconds / 2.0)
}

/// The top of the frequency axis: 20 kHz, or half the sample rate if
/// that's lower.
fn highest_hz(sample_rate: u32) -> f64 {
    HIGHEST_HZ.min(f64::from(sample_rate) / 2.0)
}

/// The difference spectrogram's levels: `this`'s long spectrogram minus
/// `against`'s, pixel by pixel, each floored at -90 dB first, over
/// `seconds` (the longer file's length).
fn difference_levels(this: &Sound, against: &Sound, seconds: f64, top_hz: f64) -> Vec<f64> {
    let levels = |sound: &Sound| {
        let spectrogram = spectrogram(
            &sound.mid,
            sound.sample_rate,
            LONG_WINDOW_SECONDS,
            hop_seconds(seconds, LONG_WINDOW_SECONDS),
        );
        heat_map(&spectrogram, seconds, top_hz)
    };
    levels(this)
        .iter()
        .zip(&levels(against))
        .map(|(&a, &b)| a.max(FLOOR_DB) - b.max(FLOOR_DB))
        .collect()
}

/// The level in each pixel of the plot, in dB, row by row from the top: the
/// loudest frame in its time and the loudest bin in its band of the log
/// frequency axis (interpolated between bins where a pixel is narrower
/// than a bin), so a click or a partial is never lost between pixels.
fn heat_map(spectrogram: &Spectrogram, seconds: f64, top_hz: f64) -> Vec<f64> {
    let (width, height) = ((RIGHT - LEFT) as usize, (BOTTOM - TOP) as usize);
    let frames = &spectrogram.frames;
    let bins = frames.first().map_or(0, Vec::len);
    let mut levels = vec![f64::from(uta_analysis::SILENT_DB); width * height];
    if bins == 0 {
        return levels;
    }
    let ratio = top_hz / LOWEST_HZ;
    let hz_at = |row: f64| LOWEST_HZ * ratio.powf(1.0 - row / height as f64);
    let mut column = vec![f32::MIN; bins];
    for x in 0..width {
        let (t0, t1) = (
            x as f64 / width as f64 * seconds,
            (x + 1) as f64 / width as f64 * seconds,
        );
        let first = (t0 / spectrogram.hop_seconds).ceil() as usize;
        let last = ((t1 / spectrogram.hop_seconds).ceil() as usize).min(frames.len());
        let range = if first < last {
            first..last
        } else {
            let nearest = ((t0 + t1) / 2.0 / spectrogram.hop_seconds).round() as usize;
            nearest..nearest + 1
        };
        // Past the sound's last frame (a shorter file drawn against a
        // longer one) is silence.
        if range.start >= frames.len() {
            continue;
        }
        column.fill(f32::MIN);
        for frame in &frames[range] {
            for (to, &db) in column.iter_mut().zip(frame) {
                *to = to.max(db);
            }
        }
        for y in 0..height {
            let (high, low) = (hz_at(y as f64), hz_at(y as f64 + 1.0));
            let (from, to) = (low / spectrogram.bin_hz, high / spectrogram.bin_hz);
            let db = if to.floor() > from.ceil() {
                let end = (to.floor() as usize + 1).min(bins);
                column[from.ceil() as usize..end]
                    .iter()
                    .fold(f32::MIN, |a, &b| a.max(b))
            } else {
                // Narrower than a bin: between the two either side.
                let at = ((from + to) / 2.0).min((bins - 1) as f64);
                let (below, frac) = (at.floor() as usize, at.fract() as f32);
                let above = (below + 1).min(bins - 1);
                column[below] + (column[above] - column[below]) * frac
            };
            levels[y * width + x] = f64::from(db);
        }
    }
    levels
}

/// A heat map as a PNG data URL, each level coloured by `colour`.
fn encode_heat(levels: &[f64], colour: impl Fn(f64) -> [u8; 3]) -> String {
    let (width, height) = ((RIGHT - LEFT) as u32, (BOTTOM - TOP) as u32);
    let mut pixmap = tiny_skia::Pixmap::new(width, height).expect("the plot has a size");
    for (pixel, &level) in pixmap
        .data_mut()
        .as_chunks_mut::<4>()
        .0
        .iter_mut()
        .zip(levels)
    {
        let [r, g, b] = colour(level);
        pixel.copy_from_slice(&[r, g, b, 255]);
    }
    let png = pixmap.encode_png().expect("a pixmap encodes");
    format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(png)
    )
}

/// The spectrograms' colours, from 0 (the floor, black) through purple and
/// orange to 1 (full scale, pale yellow): a perceptually even scale, like
/// matplotlib's magma, so equal steps in dB look like equal steps.
fn spectrogram_colour(t: f64) -> [u8; 3] {
    const STOPS: [[f64; 3]; 6] = [
        [0.0, 0.0, 4.0],
        [59.0, 15.0, 112.0],
        [140.0, 41.0, 129.0],
        [222.0, 73.0, 104.0],
        [254.0, 159.0, 109.0],
        [252.0, 253.0, 191.0],
    ];
    ramp(&STOPS, t)
}

/// The difference spectrogram's colours, from -1 (blue: quieter) through
/// black (no difference) to 1 (red: louder).
fn difference_colour(t: f64) -> [u8; 3] {
    const STOPS: [[f64; 3]; 5] = [
        [120.0, 190.0, 255.0],
        [30.0, 80.0, 200.0],
        [10.0, 10.0, 14.0],
        [200.0, 50.0, 30.0],
        [255.0, 190.0, 120.0],
    ];
    ramp(&STOPS, (t + 1.0) / 2.0)
}

/// The colour at `t`, from 0 to 1, between evenly spaced `stops`.
fn ramp(stops: &[[f64; 3]], t: f64) -> [u8; 3] {
    let t = if t.is_nan() { 0.0 } else { t.clamp(0.0, 1.0) };
    let at = t * (stops.len() - 1) as f64;
    let below = (at.floor() as usize).min(stops.len() - 2);
    let frac = at - below as f64;
    let (a, b) = (stops[below], stops[below + 1]);
    [0, 1, 2].map(|i| (a[i] + (b[i] - a[i]) * frac).round() as u8)
}

/// The colour bar's colours as SVG gradient stops, from the top.
fn gradient_stops(colour: impl Fn(f64) -> [u8; 3]) -> String {
    (0..=10)
        .map(|i| {
            let t = f64::from(i) / 10.0;
            let [r, g, b] = colour(t);
            format!(r##"<stop offset="{t}" stop-color="#{r:02x}{g:02x}{b:02x}"/>"##)
        })
        .collect()
}

/// The smallest of 1, 0.5, 0.2, 0.1, 0.05, … at or above `level`, so a
/// close-up's scale is a round number.
fn full_scale_ladder(level: f64) -> f64 {
    if level > 1.0 {
        return level.ceil();
    }
    // 1, 0.5, 0.2, 0.1, 0.05, 0.02, …, down to a millionth.
    (0..=6)
        .flat_map(|power| [1.0, 0.5, 0.2].map(|m| m / 10f64.powi(power)))
        .take_while(|&scale| scale >= level)
        .last()
        .unwrap_or(1.0)
}

/// `value` to `decimals` places, without a minus sign on a zero.
fn fixed(value: f64, decimals: usize) -> String {
    let text = format!("{value:.decimals$}");
    if text
        .trim_start_matches('-')
        .chars()
        .all(|c| c == '0' || c == '.')
    {
        text.trim_start_matches('-').to_string()
    } else {
        text
    }
}

/// A number without trailing zeros: `0.5`, `0.05`, `1`.
fn trim_number(value: f64) -> String {
    let text = format!("{value:.6}");
    text.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// A round step for an axis from 0 to `span`: 1, 2 or 5 times a power of
/// ten, so there are at most `most` ticks.
fn tick_step(span: f64, most: usize) -> f64 {
    if span <= 0.0 {
        return 1.0;
    }
    let rough = span / most as f64;
    let power = 10f64.powf(rough.log10().floor());
    [1.0, 2.0, 5.0, 10.0]
        .into_iter()
        .map(|m| m * power)
        .find(|&step| step >= rough)
        .unwrap_or(10.0 * power)
}

/// A tick's label: `0.25 s`, `2 s`, or `50 ms` below a second when the
/// step is under 10 ms.
fn time_label(t: f64, step: f64) -> String {
    if step < 0.01 {
        format!("{} ms", trim_number((t * 1000.0 * 1000.0).round() / 1000.0))
    } else {
        format!("{} s", trim_number((t * 1000.0).round() / 1000.0))
    }
}

/// A rectangle to plot in, with fractions of it mapped to pixels: x from
/// the left, y from the bottom.
struct Plot {
    left: f64,
    right: f64,
    top: f64,
    bottom: f64,
}

impl Plot {
    fn new(left: f64, right: f64, top: f64, bottom: f64) -> Self {
        Self {
            left,
            right,
            top,
            bottom,
        }
    }

    fn x(&self, fraction: f64) -> f64 {
        self.left + fraction.clamp(0.0, 1.0) * (self.right - self.left)
    }

    fn y(&self, fraction: f64) -> f64 {
        self.bottom - fraction.clamp(0.0, 1.0) * (self.bottom - self.top)
    }
}

/// An SVG image being drawn, [`WIDTH`] by [`HEIGHT`].
struct Svg {
    body: String,
}

/// The images' colours and type, in one place.
const STYLE: &str = "
    .background { fill: #121216; }
    .title { fill: #f2f2f5; font: 600 17px 'Helvetica Neue', Helvetica, Arial, sans-serif; }
    .numbers { fill: #f2f2f5; font: 14px Menlo, 'SF Mono', monospace; }
    .about { fill: #9a9aa6; font: 12px 'Helvetica Neue', Helvetica, Arial, sans-serif; }
    .label { fill: #c4c4cc; font: 11px Menlo, 'SF Mono', monospace; }
    .end { text-anchor: end; }
    .middle { text-anchor: middle; }
    .frame { fill: #1b1b21; stroke: #3a3a44; stroke-width: 1; }
    .grid { stroke: #3a3a44; stroke-width: 1; }
    .note-line { stroke: #ffffff; stroke-opacity: 0.18; stroke-width: 1; }
    .guide { stroke: #8a8a96; stroke-width: 1; stroke-dasharray: 4 4; }
    .trace { fill: none; stroke: #6ec6ff; stroke-width: 1.5; }
    .thin { fill: none; stroke: #6ec6ff; stroke-opacity: 0.5; stroke-width: 1; }
    .sample { fill: #6ec6ff; }
    .mark { fill: #ffb454; stroke: #ffb454; stroke-width: 1.5; }
    .mark-text { fill: #ffb454; }
    .attack-band { fill: #ffb454; fill-opacity: 0.25; }
    .harmonic-line { stroke: #ffffff; stroke-opacity: 0.12; stroke-width: 1; }
    .bar-harmonic { fill: none; stroke: #b8b8c4; stroke-width: 1; }
    .bar-alias { fill: none; stroke: #ff5a4a; stroke-width: 1; }
";

impl Svg {
    fn new() -> Self {
        let mut body = String::new();
        write!(
            body,
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="{WIDTH}" height="{HEIGHT}" viewBox="0 0 {WIDTH} {HEIGHT}"><style>{STYLE}</style><rect class="background" width="{WIDTH}" height="{HEIGHT}"/>"#
        )
        .unwrap();
        Self { body }
    }

    fn finish(mut self) -> String {
        self.body.push_str("</svg>\n");
        self.body
    }

    /// The title, the key numbers, and a line about the file.
    fn header(&mut self, title: &str, numbers: &[(&str, String)], about: &str) {
        self.text(LEFT, 30.0, title, "title");
        let numbers = numbers
            .iter()
            .map(|(name, value)| format!("{name} {value}"))
            .collect::<Vec<_>>()
            .join(" · ");
        self.text(LEFT, 56.0, &numbers, "numbers");
        self.text(LEFT, 78.0, about, "about");
    }

    fn text(&mut self, x: f64, y: f64, text: &str, class: &str) {
        write!(
            self.body,
            r#"<text x="{x:.1}" y="{y:.1}" class="{class}">{}</text>"#,
            escape(text)
        )
        .unwrap();
    }

    /// The heat map, filling the plot.
    fn image(&mut self, href: &str) {
        write!(
            self.body,
            r#"<image x="{LEFT}" y="{TOP}" width="{}" height="{}" preserveAspectRatio="none" href="{href}"/>"#,
            RIGHT - LEFT,
            BOTTOM - TOP
        )
        .unwrap();
    }

    fn frame(&mut self, plot: &Plot) {
        write!(
            self.body,
            r#"<rect class="frame" x="{:.1}" y="{:.1}" width="{:.1}" height="{:.1}"/>"#,
            plot.left,
            plot.top,
            plot.right - plot.left,
            plot.bottom - plot.top
        )
        .unwrap();
    }

    fn hline(&mut self, from: f64, to: f64, y: f64, class: &str) {
        write!(
            self.body,
            r#"<line class="{class}" x1="{from:.1}" x2="{to:.1}" y1="{y:.1}" y2="{y:.1}"/>"#
        )
        .unwrap();
    }

    fn vline(&mut self, x: f64, from: f64, to: f64, class: &str) {
        write!(
            self.body,
            r#"<line class="{class}" x1="{x:.1}" x2="{x:.1}" y1="{from:.1}" y2="{to:.1}"/>"#
        )
        .unwrap();
    }

    fn band(&mut self, from: f64, to: f64, top: f64, bottom: f64, class: &str) {
        write!(
            self.body,
            r#"<rect class="{class}" x="{from:.1}" y="{top:.1}" width="{:.1}" height="{:.1}"/>"#,
            (to - from).max(1.0),
            bottom - top
        )
        .unwrap();
    }

    fn dot(&mut self, x: f64, y: f64, radius: f64, class: &str) {
        write!(
            self.body,
            r#"<circle class="{class}" cx="{x:.1}" cy="{y:.1}" r="{radius}"/>"#
        )
        .unwrap();
    }

    fn path(&mut self, d: &str, class: &str) {
        if !d.is_empty() {
            write!(self.body, r#"<path class="{class}" d="{d}"/>"#).unwrap();
        }
    }

    fn polyline(&mut self, points: &[(f64, f64)], class: &str) {
        let mut d = String::new();
        for (i, (x, y)) in points.iter().enumerate() {
            write!(d, "{}{x:.1} {y:.1}", if i == 0 { "M" } else { "L" }).unwrap();
        }
        self.path(&d, class);
    }

    /// The log-frequency axis on the left of the main plot: a line and the
    /// note's name at every C, with its frequency.
    fn frequency_axis(&mut self, top_hz: f64) {
        let plot = Plot::new(LEFT, RIGHT, TOP, BOTTOM);
        let ratio = (top_hz / LOWEST_HZ).ln();
        for octave in 0..=10 {
            let note = Note(12 * (octave + 1));
            let hz = note.frequency();
            if !(LOWEST_HZ..=top_hz).contains(&hz) {
                continue;
            }
            let y = plot.y((hz / LOWEST_HZ).ln() / ratio);
            self.hline(plot.left, plot.right, y, "note-line");
            self.text(
                plot.left - 6.0,
                y + 4.0,
                &format!("{note} {}", short_hz(hz)),
                "label end",
            );
        }
        self.text(plot.left - 6.0, plot.top - 6.0, "note Hz", "label end");
    }

    /// Frequency ticks along the bottom of a linear axis up to `top_hz`.
    fn frequency_ticks_linear(&mut self, plot: &Plot, top_hz: f64) {
        let step = tick_step(top_hz, 10);
        let mut hz = 0.0;
        while hz <= top_hz + 1e-9 {
            let x = plot.x(hz / top_hz);
            self.vline(x, plot.top - 4.0, plot.top, "grid");
            self.text(x, plot.top - 8.0, &short_hz(hz), "label middle");
            hz += step;
        }
        self.text(plot.left - 6.0, plot.top - 8.0, "Hz", "label end");
    }

    /// The time axis along the bottom of the main plot.
    fn time_axis(&mut self, seconds: f64) {
        self.time_axis_on(&Plot::new(LEFT, RIGHT, TOP, BOTTOM), seconds);
    }

    fn time_axis_on(&mut self, plot: &Plot, seconds: f64) {
        let step = tick_step(seconds, 12);
        let mut t = 0.0;
        while t <= seconds + 1e-9 {
            let x = plot.x(t / seconds.max(1e-9));
            self.vline(x, plot.bottom, plot.bottom + 5.0, "grid");
            self.text(x, plot.bottom + 18.0, &time_label(t, step), "label middle");
            t += step;
        }
    }

    /// Horizontal lines every `step` dB from 0 down to `floor`, labelled.
    fn grid_db(&mut self, plot: &Plot, floor: f64, step: f64, unit: &str) {
        self.frame(plot);
        let mut db = 0.0;
        while db >= floor - 1e-9 {
            let y = plot.y((db - floor) / -floor);
            self.hline(plot.left, plot.right, y, "grid");
            let label = if db == 0.0 {
                format!("0 {unit}")
            } else {
                format!("{db:.0}")
            };
            self.text(plot.left - 6.0, y + 4.0, &label, "label end");
            db -= step;
        }
    }

    /// The colour scale on the right, with labels at fractions from the
    /// top.
    fn colour_bar(&mut self, labels: &[(f64, &str)], id: &str, about: &str) {
        let (x, width) = (RIGHT + 24.0, 18.0);
        let stops = if id == "difference" {
            gradient_stops(|t| difference_colour(1.0 - 2.0 * t))
        } else {
            gradient_stops(|t| spectrogram_colour(1.0 - t))
        };
        write!(
            self.body,
            r#"<defs><linearGradient id="{id}" x1="0" y1="0" x2="0" y2="1">{stops}</linearGradient></defs><rect x="{x}" y="{TOP}" width="{width}" height="{}" fill="url(#{id})"/>"#,
            BOTTOM - TOP
        )
        .unwrap();
        for &(at, label) in labels {
            let y = TOP + at * (BOTTOM - TOP);
            self.text(x + width + 4.0, y + 4.0, label, "label");
        }
        self.text(f64::from(WIDTH) - 8.0, TOP - 8.0, about, "label end");
    }
}

/// A frequency in few characters: `262`, `4.2k`.
fn short_hz(hz: f64) -> String {
    if hz >= 1000.0 {
        format!("{}k", trim_number((hz / 100.0).round() / 10.0))
    } else {
        format!("{hz:.0}")
    }
}

/// Text made safe for SVG.
fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_colour_scales_run_from_end_to_end() {
        assert_eq!(spectrogram_colour(0.0), [0, 0, 4]);
        assert_eq!(spectrogram_colour(1.0), [252, 253, 191]);
        assert_eq!(spectrogram_colour(-5.0), spectrogram_colour(0.0));
        assert_eq!(spectrogram_colour(f64::NAN), spectrogram_colour(0.0));
        assert_eq!(difference_colour(0.0), [10, 10, 14]);
        assert_eq!(difference_colour(1.0), [255, 190, 120]);
        assert_eq!(difference_colour(-1.0), [120, 190, 255]);
    }

    #[test]
    fn axes_take_round_steps() {
        assert_eq!(tick_step(1.0, 12), 0.1);
        assert_eq!(tick_step(2.5, 12), 0.5);
        assert_eq!(tick_step(30.0, 12), 5.0);
        assert_eq!(tick_step(0.02, 12), 0.002);
        assert_eq!(time_label(0.25, 0.05), "0.25 s");
        assert_eq!(time_label(0.004, 0.002), "4 ms");
        assert_eq!(short_hz(261.6), "262");
        assert_eq!(short_hz(4186.0), "4.2k");
    }

    #[test]
    fn a_close_up_scale_is_round_and_holds_the_samples() {
        assert_eq!(full_scale_ladder(0.9), 1.0);
        assert_eq!(full_scale_ladder(0.5), 0.5);
        assert_eq!(full_scale_ladder(0.3), 0.5);
        assert_eq!(full_scale_ladder(0.15), 0.2);
        assert_eq!(full_scale_ladder(0.01), 0.01);
        assert_eq!(trim_number(full_scale_ladder(0.004)), "0.005");
    }

    /// A 1 kHz sine for `seconds`, at 48 kHz.
    fn sine(seconds: f64) -> Vec<f32> {
        (0..(seconds * 48_000.0) as usize)
            .map(|i| (std::f64::consts::TAU * 1000.0 * i as f64 / 48_000.0).sin() as f32 * 0.5)
            .collect()
    }

    /// A file that stops mid-tone, drawn against a longer one, is silent
    /// past its end: quieter there, never louder.
    #[test]
    fn a_shorter_file_is_silent_past_its_end() {
        let (short, long) = (sine(1.0), sine(2.0));
        let report = |mid: &[f32]| {
            let sound = uta_analysis::report::Sound::new(mid, 1, 48_000);
            Report::measure("sine.wav", &sound)
        };
        let (short_report, long_report) = (report(&short), report(&long));
        let short = Sound::new(Path::new("short.wav"), short, &short_report);
        let long = Sound::new(Path::new("long.wav"), long, &long_report);
        let width = (RIGHT - LEFT) as usize;
        let columns = |levels: &[f64], range: std::ops::Range<usize>| {
            levels
                .chunks(width)
                .flat_map(|row| row[range.clone()].to_vec())
                .collect::<Vec<f64>>()
        };
        // The last quarter is well past the short file's end and its window.
        let after = 3 * width / 4..width;
        let difference = difference_levels(&short, &long, 2.0, 20_000.0);
        let past = columns(&difference, after.clone());
        assert!(
            past.iter().all(|&db| db <= 0.0),
            "never louder past its end"
        );
        let loudest_quieter = past.iter().fold(0.0f64, |a, &b| a.min(b));
        assert!(
            loudest_quieter <= -DIFFERENCE_DB,
            "quieter by {loudest_quieter} dB at 1 kHz"
        );
        // Where both play, the same.
        let both = columns(&difference, width / 8..3 * width / 8);
        assert!(
            both.iter().all(|&db| db.abs() < 1e-6),
            "the same where both play"
        );
        // And on its own, past its end it's silent.
        let spectrogram = spectrogram(&short.mid, 48_000, LONG_WINDOW_SECONDS, 0.01);
        let levels = heat_map(&spectrogram, 2.0, 20_000.0);
        assert!(
            columns(&levels, after)
                .iter()
                .all(|&db| db == f64::from(uta_analysis::SILENT_DB))
        );
    }

    #[test]
    fn a_zero_has_no_minus_sign() {
        assert_eq!(fixed(-0.04, 1), "0.0");
        assert_eq!(fixed(-0.06, 1), "-0.1");
        assert_eq!(fixed(2.5, 2), "2.50");
    }

    #[test]
    fn text_is_escaped() {
        assert_eq!(escape(r#"a<b & "c">"#), "a&lt;b &amp; &quot;c&quot;&gt;");
    }
}
