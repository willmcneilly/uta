//! Measurements for the sound tests, taken from the rendered waveform, and
//! the projects they play.

#![allow(dead_code)] // Each test binary uses a different subset.

use uta_core::time::{TICKS_PER_QUARTER, Ticks};
use uta_core::{
    Clip, ClipId, Command, CommandList, DrumParam, DrumSound, KitSettings, Note, NoteId,
    PlacedTrack, Project, ProjectId, Source, SynthParam, Track, TrackId,
};
use uuid::Uuid;

/// The demo loop, built from its committed command list.
pub fn demo_loop() -> Project {
    let json = include_str!("../../../../examples/demo-loop.json");
    let list: CommandList = serde_json::from_str(json).expect("the demo loop parses");
    list.build().expect("the demo loop builds")
}

/// The demo song, built from its committed command list: three tracks, each
/// with its own sound, panned apart.
pub fn demo_song() -> Project {
    let json = include_str!("../../../../examples/demo-song.json");
    let list: CommandList = serde_json::from_str(json).expect("the demo song parses");
    list.build().expect("the demo song builds")
}

/// A note for [`project`]: its ID is worked out from `index`.
pub fn note(index: u128, pitch: u8, start: Ticks, length: Ticks) -> Note {
    Note {
        id: NoteId::from_uuid(Uuid::from_u128(1000 + index)),
        pitch,
        velocity: 100,
        start,
        length,
    }
}

/// A project at `bpm` with a loop of `bars`, the synth settings in `params`,
/// and `notes`, built through commands as the app would.
pub fn project(bpm: f32, bars: u32, params: &[SynthParam], notes: Vec<Note>) -> Project {
    let mut project = Project::with_id(ProjectId::from_uuid(Uuid::from_u128(1)));
    let track = project.tracks()[0].id();
    let clip = project.tracks()[0].clips()[0].id();
    let mut commands = vec![Command::SetTempo { bpm }, Command::SetLoopLength { bars }];
    commands.extend(
        params
            .iter()
            .map(|&param| Command::SetSynthParam { track, param }),
    );
    if !notes.is_empty() {
        commands.push(Command::AddNotes { clip, notes });
    }
    for command in &commands {
        project.apply(command).expect("a valid test project");
    }
    project
}

/// The drum track [`drum_project`] adds.
pub fn drum_track() -> TrackId {
    TrackId::from_uuid(Uuid::from_u128(70))
}

/// A hit on a drum track: a sixteenth note at `start`, its ID worked out from
/// `index`.
pub fn hit(index: u128, pitch: u8, velocity: u8, start: Ticks) -> Note {
    Note {
        id: NoteId::from_uuid(Uuid::from_u128(5000 + index)),
        pitch,
        velocity,
        start,
        length: TICKS_PER_QUARTER / 4,
    }
}

/// The kick's note.
pub const KICK: u8 = 36;
/// The snare's note.
pub const SNARE: u8 = 38;
/// The clap's note.
pub const CLAP: u8 = 39;

/// A project at `bpm` with a loop of `bars`, the master at 0 dB, and a drum
/// track, "Drums 1", under the empty synth track: its clip fills the loop
/// and holds `hits`, and its kit has the settings in `params`. Built
/// through commands, as the app would.
pub fn drum_project(
    bpm: f32,
    bars: u32,
    params: &[(DrumSound, DrumParam)],
    hits: Vec<Note>,
) -> Project {
    let mut project = Project::with_id(ProjectId::from_uuid(Uuid::from_u128(1)));
    let track = drum_track();
    let bar = project.transport().time_signature().ticks_per_bar();
    let clip = Clip::new(
        ClipId::from_uuid(Uuid::from_u128(71)),
        0,
        Ticks::from(bars) * bar,
    )
    .with_notes(hits);
    let mut commands = vec![
        Command::SetMasterVolume { volume_db: 0.0 },
        Command::SetTempo { bpm },
        Command::SetLoopLength { bars },
        Command::AddTracks {
            tracks: vec![PlacedTrack {
                index: 1,
                track: Track::new(track, "Drums 1", Source::Drums(KitSettings::default()))
                    .with_clips([clip]),
            }],
        },
    ];
    commands.extend(params.iter().map(|&(sound, param)| Command::SetDrumParam {
        track,
        sound,
        param,
    }));
    for command in &commands {
        project.apply(command).expect("a valid test project");
    }
    project
}

/// Plays `project` from the top for `seconds`, in mono at 48 kHz, in blocks
/// of `block_size`.
pub fn render_drums(project: &Project, seconds: f64, block_size: usize) -> Vec<f32> {
    let config = uta_engine::EngineConfig {
        sample_rate: 48_000,
        channels: 1,
    };
    let mut renderer =
        uta_engine::offline::Renderer::new(config, uta_engine::Snapshot::from(project), block_size);
    renderer.controller.play().unwrap();
    renderer.render_seconds(seconds);
    renderer.into_samples()
}

/// One hit on `pitch` at the top of a bar at 60 BPM, so nothing else plays
/// for 4 s, with these kit settings, at this velocity, rendered for
/// `seconds` at 48 kHz.
pub fn one_hit(
    pitch: u8,
    params: &[(DrumSound, DrumParam)],
    velocity: u8,
    seconds: f64,
) -> Vec<f32> {
    let project = drum_project(60.0, 1, params, vec![hit(0, pitch, velocity, 0)]);
    render_drums(&project, seconds, 128)
}

/// Fails if `samples` jump from one sample to the next by more than `limit`.
pub fn assert_no_click(samples: &[f32], limit: f32, what: &str) {
    let (jump, at) = max_jump(samples);
    assert!(
        jump <= limit,
        "{what}: jump of {jump} at sample {at}, limit {limit}"
    );
}

/// `project` filled up to [`Project::MAX_TRACKS`] with empty synth tracks,
/// so every slot is taken and a track added in place of a removed one has
/// to take the removed one's slot.
pub fn with_every_slot_taken(mut project: Project) -> Project {
    let missing = Project::MAX_TRACKS - project.tracks().len();
    let fill = (0..missing)
        .map(|n| PlacedTrack {
            index: project.tracks().len() + n,
            track: Track::new(
                TrackId::from_uuid(Uuid::from_u128(8000 + n as u128)),
                format!("Fill {n}"),
                Source::Synth(uta_core::SynthSettings::default()),
            ),
        })
        .collect();
    project
        .apply(&Command::AddTracks { tracks: fill })
        .expect("room for the fill");
    project
}

/// `project` with the track `old` removed and `new` added in its place in
/// the order, in one change, as deleting a track and adding another while
/// it still sounds would.
pub fn replace_track(project: &Project, old: TrackId, new: Track) -> Project {
    let index = project
        .tracks()
        .iter()
        .position(|track| track.id() == old)
        .expect("the old track is in the project");
    let mut replaced = project.clone();
    replaced
        .apply(&Command::RemoveTracks { tracks: vec![old] })
        .expect("the old track goes");
    replaced
        .apply(&Command::AddTracks {
            tracks: vec![PlacedTrack { index, track: new }],
        })
        .expect("the new track goes in");
    replaced
}

/// A drum track with no clips, at the defaults.
pub fn empty_drum_track(id: u128) -> Track {
    Track::new(
        TrackId::from_uuid(Uuid::from_u128(id)),
        "Drums 2",
        Source::Drums(KitSettings::default()),
    )
}

/// A synth track with no clips, playing a sine at full sustain.
pub fn empty_sine_track(id: u128) -> Track {
    let settings = uta_core::SynthSettings {
        waveform: uta_core::Waveform::Sine,
        sustain: 1.0,
        ..uta_core::SynthSettings::default()
    };
    Track::new(
        TrackId::from_uuid(Uuid::from_u128(id)),
        "Synth 2",
        Source::Synth(settings),
    )
}

/// Synth settings for measuring exact sample positions: a sine at full
/// sustain, with the shortest release, so a note is silent 1 ms after it
/// ends.
pub const PLAIN_SINE: [SynthParam; 3] = [
    SynthParam::Waveform(uta_core::Waveform::Sine),
    SynthParam::Sustain(1.0),
    SynthParam::ReleaseSeconds(0.001),
];

/// The index of the first sample that isn't silent.
pub fn first_sound(samples: &[f32]) -> Option<usize> {
    samples.iter().position(|&s| s != 0.0)
}

/// The frequency of a steady tone, from the time between its first and last
/// upward zero crossings (interpolated between samples).
pub fn measure_frequency(samples: &[f32], sample_rate: u32) -> f64 {
    let mut first = None;
    let mut last = 0.0;
    let mut crossings = 0usize;
    for (i, pair) in samples.windows(2).enumerate() {
        let (a, b) = (f64::from(pair[0]), f64::from(pair[1]));
        if a < 0.0 && b >= 0.0 {
            let at = i as f64 + a / (a - b);
            first.get_or_insert(at);
            last = at;
            crossings += 1;
        }
    }
    let first = first.expect("no zero crossings: is the tone playing?");
    assert!(crossings >= 2, "need at least two cycles to measure pitch");
    (crossings - 1) as f64 * f64::from(sample_rate) / (last - first)
}

pub fn rms(samples: &[f32]) -> f64 {
    let sum: f64 = samples.iter().map(|&s| f64::from(s).powi(2)).sum();
    (sum / samples.len() as f64).sqrt()
}

pub fn peak(samples: &[f32]) -> f32 {
    samples.iter().fold(0.0f32, |peak, s| peak.max(s.abs()))
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

/// `snapshot` with its first track's mixer strip set to `mixer`.
pub fn with_mixer(
    mut snapshot: uta_engine::Snapshot,
    mixer: uta_engine::MixerStrip,
) -> uta_engine::Snapshot {
    snapshot.tracks_mut()[0].mixer = mixer;
    snapshot
}

/// `snapshot` with its first track's synth set to `synth`.
pub fn with_synth(
    mut snapshot: uta_engine::Snapshot,
    synth: uta_engine::SynthSettings,
) -> uta_engine::Snapshot {
    snapshot.tracks_mut()[0].sound = uta_engine::TrackSound::Synth(synth);
    snapshot
}

/// The app's stress notes (`stress::notes` in `uta-app`) for one bar of 4/4, seed
/// 0: the same pattern Develop → Add Stress Notes and the benchmark make.
/// The app gives each note a random ID; here they count up from `first_id`,
/// so every run plays the same song: notes on one sample sound in ID order.
pub fn stress_notes(first_id: u128) -> Vec<Note> {
    let sixteenth = TICKS_PER_QUARTER / 4;
    let steps = 16;
    let mut state: u64 = 1;
    let mut next = || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };
    (0..3_000)
        .map(|index| Note {
            id: NoteId::from_uuid(Uuid::from_u128(first_id + index)),
            pitch: 24 + (next() % 85) as u8,
            velocity: Note::MIN_VELOCITY
                + (next() % u64::from(Note::MAX_VELOCITY - Note::MIN_VELOCITY + 1)) as u8,
            start: (next() % steps) * sixteenth,
            length: (1 + next() % 16) * sixteenth,
        })
        .collect()
}

/// The pitch over time of a tone that may glide, from each cycle between
/// upward zero crossings (interpolated between samples): each cycle's
/// frequency, at the time of its middle, in seconds from the first sample.
/// For low, clean tones such as a kick or a tom; noise has no pitch.
pub fn pitch_over_time(samples: &[f32], sample_rate: u32) -> Vec<(f64, f64)> {
    let rate = f64::from(sample_rate);
    let mut crossings = Vec::new();
    for (i, pair) in samples.windows(2).enumerate() {
        let (a, b) = (f64::from(pair[0]), f64::from(pair[1]));
        if a < 0.0 && b >= 0.0 {
            crossings.push(i as f64 + a / (a - b));
        }
    }
    crossings
        .windows(2)
        .map(|pair| ((pair[0] + pair[1]) / 2.0 / rate, rate / (pair[1] - pair[0])))
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
        for start in (0..n).step_by(len) {
            for k in 0..len / 2 {
                let (w_re, w_im) = ((angle * k as f64).cos(), (angle * k as f64).sin());
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
