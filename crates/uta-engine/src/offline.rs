//! Offline render: drives the real [`Processor`] without a sound device.
//!
//! This is how every audio test runs, and what `uta render` uses. It calls
//! exactly the same `process` a device would, in blocks of a chosen size.

use std::path::Path;

use crate::{Controller, EngineConfig, Processor, Snapshot};

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
            self.controller.poll();
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
            self.controller.poll();
        }
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

/// Plays the loop in `snapshot` from its start for `seconds`, then stops and
/// renders the notes' release (the longest of any track's), so it ends in
/// silence. What `uta render`
/// writes.
pub fn render_loop(config: EngineConfig, snapshot: Snapshot, seconds: f64) -> Vec<f32> {
    let release = snapshot
        .tracks()
        .iter()
        .map(|track| track.synth.clamped().release_seconds)
        .fold(0.0, f32::max);
    let tail = f64::from(release) + RELEASE_MARGIN_SECONDS;
    let mut renderer = Renderer::new(config, snapshot, 128);
    renderer.controller.play().expect("fresh queue has room");
    renderer.render_seconds(seconds);
    renderer.controller.stop().expect("queue was drained");
    renderer.render_seconds(tail);
    renderer.into_samples()
}

/// How long [`render_loop`] renders after the release time, so the release
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
