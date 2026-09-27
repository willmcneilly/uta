//! The real [`Output`]: the system's default output device, through cpal.
//!
//! Not covered by CI, which has no sound device. The supervisor's logic is
//! tested with a fake instead, and this stays a thin translation.

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

use super::{AudioCallback, DeviceInfo, ErrorCallback, Output};

pub struct CpalOutput {
    host: cpal::Host,
}

impl CpalOutput {
    pub fn new() -> Self {
        Self {
            host: cpal::default_host(),
        }
    }
}

impl Default for CpalOutput {
    fn default() -> Self {
        Self::new()
    }
}

impl Output for CpalOutput {
    type Device = cpal::Device;
    type Stream = cpal::Stream;

    /// Only a device from `default_output_device` follows the system's
    /// default output, so this is the only way to get one.
    fn default_device(&mut self) -> Option<(cpal::Device, DeviceInfo)> {
        let device = self.host.default_output_device()?;
        let config = device.default_output_config().ok()?;
        let name = match device.description() {
            Ok(description) => description.name().to_owned(),
            Err(_) => "Unknown device".to_owned(),
        };
        let buffer_range = match *config.buffer_size() {
            cpal::SupportedBufferSize::Range { min, max } => Some((min, max)),
            cpal::SupportedBufferSize::Unknown => None,
        };
        let info = DeviceInfo {
            name,
            sample_rate: config.sample_rate(),
            channels: config.channels(),
            buffer_range,
        };
        Some((device, info))
    }

    fn open(
        &mut self,
        device: &cpal::Device,
        info: &DeviceInfo,
        buffer_size: u32,
        mut audio: AudioCallback,
        mut errors: ErrorCallback,
    ) -> Result<cpal::Stream, String> {
        let config = cpal::StreamConfig {
            channels: info.channels,
            sample_rate: info.sample_rate,
            buffer_size: cpal::BufferSize::Fixed(buffer_size),
        };
        let stream = device
            .build_output_stream::<f32, _, _>(
                config,
                move |output: &mut [f32], _: &cpal::OutputCallbackInfo| audio.render(output),
                move |error| errors.report(error),
                None,
            )
            .map_err(|error| error.to_string())?;
        stream.play().map_err(|error| error.to_string())?;
        Ok(stream)
    }

    fn buffer_size(&self, stream: &cpal::Stream) -> Option<u32> {
        stream.buffer_size().ok()
    }
}
