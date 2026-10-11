//! Offline render: drives the real [`Processor`] without a sound device.
//!
//! This is how every audio test runs, and what `uta render` uses. It calls
//! exactly the same `process` a device would, in blocks of a chosen size.

use std::path::Path;

use crate::{Controller, EngineConfig, Processor, Snapshot, Status};

/// Renders audio block by block into memory.
pub struct Renderer {
    pub controller: Controller,
    processor: Processor,
    config: EngineConfig,
    block_size: usize,
    /// Scratch for one block, allocated once.
    block: Vec<f32>,
    /// Everything rendered so far, interleaved.
    samples: Vec<f32>,
    /// The latest status, with the loudest peaks since it was last taken.
    status: Status,
}

impl Renderer {
    /// A renderer for a fresh engine, processing `block_size` frames at a time.
    pub fn new(config: EngineConfig, snapshot: Snapshot, block_size: usize) -> Self {
        assert!(block_size > 0, "block size must be positive");
        let (controller, processor) = crate::engine(config, snapshot);
        Self {
            controller,
            processor,
            config,
            block_size,
            block: vec![0.0; block_size * config.channels],
            samples: Vec::new(),
            status: Status::default(),
        }
    }

    /// Renders `frames` more frames. Commands sent through
    /// [`Renderer::controller`] beforehand take effect at the first block. If
    /// `frames` isn't a multiple of the block size, the last block is shorter.
    pub fn render(&mut self, frames: usize) {
        let channels = self.config.channels;
        self.samples.reserve(frames * channels);
        let mut remaining = frames;
        while remaining > 0 {
            let len = remaining.min(self.block_size);
            let block = &mut self.block[..len * channels];
            self.processor.process(block);
            self.samples.extend_from_slice(block);
            remaining -= len;
            self.poll();
        }
    }

    /// Renders `frames` more frames without keeping them, for getting far
    /// into a render quickly without holding it all in memory.
    pub fn skip(&mut self, frames: usize) {
        let channels = self.config.channels;
        let mut remaining = frames;
        while remaining > 0 {
            let len = remaining.min(self.block_size);
            self.processor.process(&mut self.block[..len * channels]);
            remaining -= len;
            self.poll();
        }
    }

    /// The latest status, with `peak` and `track_peaks` the loudest, and
    /// `slowest_block` the slowest, since the last call (or since the
    /// renderer was made).
    pub fn take_status(&mut self) -> Status {
        let status = self.status;
        self.status.peak = 0.0;
        self.status.track_peaks = [0.0; crate::TRACK_SLOTS];
        self.status.slowest_block = 0.0;
        status
    }

    fn poll(&mut self) {
        let status = self.controller.poll();
        let mut peaks = self.status.track_peaks;
        for (peak, &track_peak) in peaks.iter_mut().zip(&status.track_peaks) {
            *peak = peak.max(track_peak);
        }
        self.status = Status {
            peak: self.status.peak.max(status.peak),
            track_peaks: peaks,
            slowest_block: self.status.slowest_block.max(status.slowest_block),
            ..status
        };
    }

    /// Renders `seconds` more seconds, rounded to the nearest frame.
    pub fn render_seconds(&mut self, seconds: f64) {
        self.render(self.frames_for(seconds));
    }

    /// The number of frames in `seconds`, rounded.
    pub fn frames_for(&self, seconds: f64) -> usize {
        (seconds * f64::from(self.config.sample_rate)).round() as usize
    }

    /// The processor, for driving it directly (for example under
    /// `assert_no_alloc`).
    pub fn processor(&mut self) -> &mut Processor {
        &mut self.processor
    }

    pub fn samples(&self) -> &[f32] {
        &self.samples
    }

    pub fn into_samples(self) -> Vec<f32> {
        self.samples
    }

    pub fn config(&self) -> EngineConfig {
        self.config
    }
}

/// A song rendered by [`render_song`].
#[derive(Debug, Clone, PartialEq)]
pub struct RenderedSong {
    /// Interleaved samples.
    pub samples: Vec<f32>,
    /// Whether a chain was still ringing when [`MAX_TAIL_SECONDS`] cut the
    /// render off: echoes with feedback that never fade, say.
    pub cut_off: bool,
}

/// The longest [`render_song`] goes on after Stop for the chains to ring
/// out. See RFC-007, "Stopping, switching off, and tails" (Rendering).
pub const MAX_TAIL_SECONDS: f64 = 30.0;

/// Plays `snapshot` from the top of the song for `seconds`, round the loop
/// if it's on, then stops and renders the notes' release and the drums'
/// ring (the longest of any track with notes), and on until every chain
/// has rung out, up to [`MAX_TAIL_SECONDS`] after Stop, so it ends in
/// silence. With the loop off it stops by itself at the song's end, and
/// anything after that is the tail. What `uta render` writes.
pub fn render_song(config: EngineConfig, snapshot: Snapshot, seconds: f64) -> RenderedSong {
    // A track with no notes to play never sounds, so it adds no tail: a new
    // project's empty drum track doesn't lengthen a synth render.
    let release = snapshot
        .tracks()
        .iter()
        .filter(|track| !track.notes().events().is_empty())
        .map(|track| track.sound.tail_seconds())
        .fold(0.0, f32::max);
    let tail = f64::from(release) + RELEASE_MARGIN_SECONDS;
    let mut renderer = Renderer::new(config, snapshot, SONG_BLOCK);
    renderer.controller.play().expect("fresh queue has room");
    renderer.render_seconds(seconds);
    renderer.controller.stop().expect("queue was drained");
    renderer.render_seconds(tail);
    // Each block's status says whether a chain, a track's or the master's,
    // is still ringing. With no effects, none ever is, so the render ends
    // here as it always has.
    let limit = renderer.frames_for(MAX_TAIL_SECONDS);
    let mut tail_frames = renderer.frames_for(tail);
    let mut cut_off = false;
    while {
        let status = renderer.take_status();
        status.ringing_chains != 0 || status.master_ringing
    } {
        if tail_frames >= limit {
            cut_off = true;
            break;
        }
        let frames = SONG_BLOCK.min(limit - tail_frames);
        renderer.render(frames);
        tail_frames += frames;
    }
    RenderedSong {
        samples: renderer.into_samples(),
        cut_off,
    }
}

/// The block size [`render_song`] renders in.
const SONG_BLOCK: usize = 128;

/// How long [`render_song`] renders after the release time, so the release
/// has finished.
const RELEASE_MARGIN_SECONDS: f64 = 0.01;

/// Writes interleaved samples to a 32-bit float WAV.
pub fn write_wav(path: &Path, config: EngineConfig, samples: &[f32]) -> Result<(), hound::Error> {
    let spec = hound::WavSpec {
        channels: config.channels as u16,
        sample_rate: config.sample_rate,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let mut writer = hound::WavWriter::create(path, spec)?;
    for &sample in samples {
        writer.write_sample(sample)?;
    }
    writer.finalize()
}

/// Reads a WAV written by [`write_wav`]: its format and interleaved samples.
pub fn read_wav(path: &Path) -> Result<(hound::WavSpec, Vec<f32>), hound::Error> {
    let mut reader = hound::WavReader::open(path)?;
    let spec = reader.spec();
    let samples = reader.samples::<f32>().collect::<Result<_, _>>()?;
    Ok((spec, samples))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An offline render times its blocks as a device's would, and reports
    /// the slowest since the status was last taken.
    #[test]
    fn an_offline_render_reports_its_slowest_block() {
        let project = uta_core::Project::new();
        let mut renderer = Renderer::new(EngineConfig::default(), Snapshot::from(&project), 128);
        renderer.controller.play().unwrap();
        renderer.render_seconds(1.0);
        let status = renderer.take_status();
        assert!(
            status.slowest_block > 0.0 && status.slowest_block.is_finite(),
            "{status:?}"
        );
        assert_eq!(renderer.take_status().slowest_block, 0.0);
    }
}
