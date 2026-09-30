//! Measurements for the sound tests, taken from the rendered waveform, and
//! the projects they play.

#![allow(dead_code)] // Each test binary uses a different subset.

use uta_core::time::Ticks;
use uta_core::{Command, CommandList, Note, NoteId, Project, ProjectId, SynthParam};
use uuid::Uuid;

/// The demo loop, built from its committed command list.
pub fn demo_loop() -> Project {
    let json = include_str!("../../../../examples/demo-loop.json");
    let list: CommandList = serde_json::from_str(json).expect("the demo loop parses");
    list.build().expect("the demo loop builds")
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

    // Iterative radix-2 FFT: bit-reversal, then butterflies.
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
    snapshot.tracks_mut()[0].synth = synth;
    snapshot
}
