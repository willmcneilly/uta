//! The supervisor: keeps the processor playing on the system's default
//! output as devices come, go and change. See RFC-001, "Sound devices".
//!
//! It runs on its own thread (see [`super::LiveOutput`]), never the audio
//! thread, so it may wait and allocate. The device side is behind the
//! [`Output`] trait, so the logic is tested with a fake stream.

use std::time::{Duration, Instant};

use rtrb::Consumer;

use super::{AudioCallback, DeviceError, ErrorCallback};
use crate::Processor;

/// How often the supervisor looks for a device while there isn't one. cpal
/// can't announce new devices, so it has to ask.
pub const DEVICE_POLL_INTERVAL: Duration = Duration::from_millis(1500);
/// How long to wait for a dropped stream to hand the processor back.
const PROCESSOR_RETURN_TIMEOUT: Duration = Duration::from_secs(2);

/// An output device as the supervisor sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceInfo {
    pub name: String,
    /// The device's own sample rate, which Uta plays at.
    pub sample_rate: u32,
    pub channels: u16,
    /// The smallest and largest buffer sizes it supports, if known.
    pub buffer_range: Option<(u32, u32)>,
}

impl DeviceInfo {
    /// The buffer size to ask for: `requested`, brought into the device's
    /// supported range.
    pub fn buffer_size_for(&self, requested: u32) -> u32 {
        match self.buffer_range {
            Some((min, max)) => requested.clamp(min, max),
            None => requested,
        }
    }
}

/// Where the supervisor gets devices and streams from: cpal, or a fake in
/// tests.
pub trait Output {
    type Device;
    type Stream;

    /// The system's current default output, if there is one.
    fn default_device(&mut self) -> Option<(Self::Device, DeviceInfo)>;

    /// Builds and starts a stream on `device` at its own rate. On failure the
    /// callbacks must have been dropped, which sends the processor home.
    fn open(
        &mut self,
        device: &Self::Device,
        info: &DeviceInfo,
        buffer_size: u32,
        audio: AudioCallback,
        errors: ErrorCallback,
    ) -> Result<Self::Stream, String>;

    /// The stream's current buffer size, if the backend can tell.
    fn buffer_size(&self, stream: &Self::Stream) -> Option<u32>;
}

/// What the supervisor is doing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceState {
    /// A stream is playing the processor.
    Running,
    /// There's no usable device. The supervisor checks every
    /// [`DEVICE_POLL_INTERVAL`].
    Waiting,
    /// A stream never gave the processor back, so there's nothing left to
    /// play. It shouldn't happen.
    Failed,
}

/// The supervisor's view of the output, for display.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceStatus {
    pub state: DeviceState,
    /// The device playing, or the last one that did.
    pub device: Option<DeviceInfo>,
    /// The stream's buffer size, or the requested one before there's a
    /// stream.
    pub buffer_size: u32,
    /// Xruns reported by the device driver.
    pub dropouts: u64,
    /// How many times the stream has been rebuilt or reopened.
    pub rebuilds: u64,
    /// Why the last attempt to open a stream failed, if it did.
    pub last_error: Option<String>,
}

struct Running<S> {
    stream: S,
    errors: Consumer<DeviceError>,
    home: Consumer<Processor>,
}

enum State<S> {
    Running(Running<S>),
    Waiting {
        processor: Processor,
        next_poll: Instant,
    },
    Failed,
}

pub struct Supervisor<O: Output> {
    output: O,
    requested_buffer: u32,
    state: State<O::Stream>,
    status: DeviceStatus,
}

impl<O: Output> Supervisor<O> {
    /// Starts playing `processor` on the default output, or waits for one.
    pub fn start(output: O, processor: Processor, requested_buffer: u32, now: Instant) -> Self {
        let mut supervisor = Self {
            output,
            requested_buffer,
            state: State::Failed,
            status: DeviceStatus {
                state: DeviceState::Waiting,
                device: None,
                buffer_size: requested_buffer,
                dropouts: 0,
                rebuilds: 0,
                last_error: None,
            },
        };
        supervisor.open(processor, now);
        supervisor.status.rebuilds = 0;
        supervisor
    }

    pub fn status(&self) -> &DeviceStatus {
        &self.status
    }

    /// Handles the errors the stream has reported, and looks for a device if
    /// it's waiting and a poll is due. Call it often: every few tens of
    /// milliseconds.
    pub fn tick(&mut self, now: Instant) {
        let (mut changed, mut invalidated, mut gone) = (false, false, false);
        if let State::Running(running) = &mut self.state {
            while let Ok(error) = running.errors.pop() {
                match error {
                    DeviceError::Changed => changed = true,
                    DeviceError::Invalidated => invalidated = true,
                    DeviceError::Gone => gone = true,
                    DeviceError::Xrun => self.status.dropouts += 1,
                    DeviceError::Other => {}
                }
            }
        }

        if gone {
            self.pause(now);
        } else if invalidated {
            self.rebuild(now);
        } else if changed {
            self.device_changed(now);
        }

        if let State::Waiting { next_poll, .. } = &self.state
            && now >= *next_poll
        {
            let State::Waiting { processor, .. } = self.take_state() else {
                unreachable!()
            };
            self.open(processor, now);
        }
    }

    /// cpal has rerouted the stream to the new default output. That's enough
    /// unless the new device plays at a different rate, or the stream lost
    /// the buffer size it was asked for.
    fn device_changed(&mut self, now: Instant) {
        let Some((_, info)) = self.output.default_device() else {
            self.pause(now);
            return;
        };
        let State::Running(running) = &self.state else {
            return;
        };
        let rate_changed =
            self.status.device.as_ref().map(|d| d.sample_rate) != Some(info.sample_rate);
        let buffer = self.output.buffer_size(&running.stream);
        let buffer_changed =
            buffer.is_some_and(|b| b != info.buffer_size_for(self.requested_buffer));
        if rate_changed || buffer_changed {
            self.rebuild(now);
        } else {
            self.status.device = Some(info);
        }
    }

    /// Closes the stream and opens a new one on the current default output.
    fn rebuild(&mut self, now: Instant) {
        if let Some(processor) = self.close() {
            self.open(processor, now);
        }
    }

    /// Closes the stream and waits for a device.
    fn pause(&mut self, now: Instant) {
        if let Some(processor) = self.close() {
            self.wait(processor, now);
        }
    }

    /// Drops the stream and takes the processor back.
    fn close(&mut self) -> Option<Processor> {
        match self.take_state() {
            State::Running(running) => {
                let Running {
                    stream, mut home, ..
                } = running;
                drop(stream);
                let processor = take_home(&mut home);
                if processor.is_none() {
                    self.fail();
                }
                processor
            }
            State::Waiting { processor, .. } => Some(processor),
            State::Failed => None,
        }
    }

    /// Opens a stream on the default output, or waits for one.
    fn open(&mut self, mut processor: Processor, now: Instant) {
        let Some((device, info)) = self.output.default_device() else {
            self.wait(processor, now);
            return;
        };
        let buffer_size = info.buffer_size_for(self.requested_buffer);
        processor.prepare(info.sample_rate, usize::from(info.channels));

        let (audio, mut home) = AudioCallback::new(processor);
        let (error_callback, errors) = ErrorCallback::new();
        match self
            .output
            .open(&device, &info, buffer_size, audio, error_callback)
        {
            Ok(stream) => {
                self.status.buffer_size = self.output.buffer_size(&stream).unwrap_or(buffer_size);
                self.status.state = DeviceState::Running;
                self.status.device = Some(info);
                self.status.rebuilds += 1;
                self.status.last_error = None;
                self.state = State::Running(Running {
                    stream,
                    errors,
                    home,
                });
            }
            Err(error) => {
                self.status.last_error = Some(error);
                match take_home(&mut home) {
                    Some(processor) => self.wait(processor, now),
                    None => self.fail(),
                }
            }
        }
    }

    fn wait(&mut self, processor: Processor, now: Instant) {
        self.status.state = DeviceState::Waiting;
        self.state = State::Waiting {
            processor,
            next_poll: now + DEVICE_POLL_INTERVAL,
        };
    }

    fn fail(&mut self) {
        self.status.state = DeviceState::Failed;
        self.state = State::Failed;
    }

    fn take_state(&mut self) -> State<O::Stream> {
        std::mem::replace(&mut self.state, State::Failed)
    }

    #[cfg(test)]
    fn stream_mut(&mut self) -> Option<&mut O::Stream> {
        match &mut self.state {
            State::Running(running) => Some(&mut running.stream),
            _ => None,
        }
    }
}

/// Waits for a dropped stream's callback to send the processor back. With
/// cpal that's normally immediate, but a listener thread can briefly hold the
/// last reference to the stream.
fn take_home(home: &mut Consumer<Processor>) -> Option<Processor> {
    let deadline = Instant::now() + PROCESSOR_RETURN_TIMEOUT;
    loop {
        if let Ok(processor) = home.pop() {
            return Some(processor);
        }
        if Instant::now() >= deadline {
            return None;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use super::*;
    use crate::{Controller, EngineConfig, Snapshot};

    /// The fake system: its default output, and what the supervisor did.
    #[derive(Default)]
    struct World {
        device: Option<DeviceInfo>,
        fail_next_open: bool,
        queries: usize,
        opens: usize,
    }

    struct FakeOutput(Rc<RefCell<World>>);

    /// A stream that plays only when the test calls [`play`].
    struct FakeStream {
        audio: AudioCallback,
        errors: ErrorCallback,
        buffer_size: u32,
    }

    impl Output for FakeOutput {
        type Device = ();
        type Stream = FakeStream;

        fn default_device(&mut self) -> Option<((), DeviceInfo)> {
            let mut world = self.0.borrow_mut();
            world.queries += 1;
            world.device.clone().map(|device| ((), device))
        }

        fn open(
            &mut self,
            _: &(),
            _: &DeviceInfo,
            buffer_size: u32,
            audio: AudioCallback,
            errors: ErrorCallback,
        ) -> Result<FakeStream, String> {
            let mut world = self.0.borrow_mut();
            if std::mem::take(&mut world.fail_next_open) {
                return Err("the device is busy".into());
            }
            world.opens += 1;
            Ok(FakeStream {
                audio,
                errors,
                buffer_size,
            })
        }

        fn buffer_size(&self, stream: &FakeStream) -> Option<u32> {
            Some(stream.buffer_size)
        }
    }

    fn speakers() -> DeviceInfo {
        DeviceInfo {
            name: "Speakers".into(),
            sample_rate: 48_000,
            channels: 2,
            buffer_range: Some((15, 4096)),
        }
    }

    fn headphones() -> DeviceInfo {
        DeviceInfo {
            name: "Headphones".into(),
            ..speakers()
        }
    }

    fn interface() -> DeviceInfo {
        DeviceInfo {
            name: "Interface".into(),
            sample_rate: 44_100,
            channels: 2,
            buffer_range: Some((64, 4096)),
        }
    }

    struct Test {
        controller: Controller,
        supervisor: Supervisor<FakeOutput>,
        world: Rc<RefCell<World>>,
        start: Instant,
    }

    /// A supervisor started on `device` with the tone playing.
    fn start(device: Option<DeviceInfo>, buffer: u32) -> Test {
        let world = Rc::new(RefCell::new(World {
            device,
            ..World::default()
        }));
        let (mut controller, processor) =
            crate::engine(EngineConfig::default(), Snapshot::default());
        controller.play().unwrap();
        let start = Instant::now();
        let supervisor = Supervisor::start(FakeOutput(world.clone()), processor, buffer, start);
        Test {
            controller,
            supervisor,
            world,
            start,
        }
    }

    impl Test {
        fn stream(&mut self) -> &mut FakeStream {
            self.supervisor.stream_mut().expect("no stream")
        }

        /// Plays `frames` through the stream, as the device would.
        fn play(&mut self, frames: usize) -> Vec<f32> {
            let channels = usize::from(self.supervisor.status().device.as_ref().unwrap().channels);
            let mut samples = vec![0.0; frames * channels];
            for block in samples.chunks_mut(128 * channels) {
                self.stream().audio.render(block);
                // Keeps the status queue from filling up.
                self.controller.poll();
            }
            samples
        }

        fn report(&mut self, kind: cpal::ErrorKind) {
            self.stream().errors.report(kind.into());
        }

        fn tick_at(&mut self, seconds: f64) {
            let now = self.start + Duration::from_secs_f64(seconds);
            self.supervisor.tick(now);
        }

        fn processor_rate(&mut self) -> u32 {
            self.stream().audio.processor().unwrap().sample_rate()
        }

        fn opens(&self) -> usize {
            self.world.borrow().opens
        }

        fn position(&mut self) -> u64 {
            self.controller.poll().position
        }
    }

    /// The first samples after a rebuild start from silence and rise, so the
    /// switch doesn't click.
    fn assert_fades_in(samples: &[f32]) {
        assert_eq!(
            samples[0], 0.0,
            "the first sample after a rebuild must be silent"
        );
        let early = samples[..20].iter().fold(0.0f32, |p, s| p.max(s.abs()));
        let later = samples.iter().fold(0.0f32, |p, s| p.max(s.abs()));
        assert!(
            early < later / 4.0,
            "no fade-in: {early} early vs {later} later"
        );
    }

    #[test]
    fn plays_on_the_default_output_at_its_rate() {
        let mut test = start(Some(interface()), 128);
        let status = test.supervisor.status().clone();
        assert_eq!(status.state, DeviceState::Running);
        assert_eq!(status.device, Some(interface()));
        assert_eq!(status.buffer_size, 128);
        assert_eq!(test.processor_rate(), 44_100);
        assert!(test.play(1024).iter().any(|&s| s != 0.0));
    }

    #[test]
    fn a_buffer_the_device_cant_do_falls_back_into_its_range() {
        let test = start(Some(interface()), 32);
        assert_eq!(test.supervisor.status().buffer_size, 64);
        let test = start(Some(speakers()), 32);
        assert_eq!(test.supervisor.status().buffer_size, 32);
    }

    #[test]
    fn device_changed_at_the_same_rate_keeps_the_stream() {
        let mut test = start(Some(speakers()), 128);
        test.play(4800);
        test.world.borrow_mut().device = Some(headphones());
        test.report(cpal::ErrorKind::DeviceChanged);
        test.tick_at(0.1);

        assert_eq!(test.opens(), 1, "cpal already rerouted it: no rebuild");
        let status = test.supervisor.status();
        assert_eq!(status.device.as_ref().unwrap().name, "Headphones");
        assert_eq!(status.rebuilds, 0);
    }

    #[test]
    fn device_changed_to_a_new_rate_rebuilds_keeps_the_position_and_fades_in() {
        let mut test = start(Some(speakers()), 128);
        test.play(48_000);
        assert_eq!(test.position(), 48_000);

        test.world.borrow_mut().device = Some(interface());
        test.report(cpal::ErrorKind::DeviceChanged);
        test.tick_at(1.0);

        assert_eq!(test.opens(), 2);
        assert_eq!(test.supervisor.status().rebuilds, 1);
        assert_eq!(test.processor_rate(), 44_100);
        let samples = test.play(4410);
        // One second in, at the new rate.
        assert_eq!(test.position(), 44_100 + 4410);
        assert_fades_in(&samples);
    }

    #[test]
    fn device_changed_that_lost_the_buffer_size_rebuilds() {
        let mut test = start(Some(speakers()), 64);
        test.world.borrow_mut().device = Some(headphones());
        // cpal doesn't re-apply a fixed buffer size when it reroutes.
        test.stream().buffer_size = 512;
        test.report(cpal::ErrorKind::DeviceChanged);
        test.tick_at(0.1);

        assert_eq!(test.opens(), 2);
        assert_eq!(test.supervisor.status().buffer_size, 64);
    }

    #[test]
    fn stream_invalidated_rebuilds_and_fades_in() {
        let mut test = start(Some(speakers()), 128);
        test.play(9600);
        test.report(cpal::ErrorKind::StreamInvalidated);
        test.tick_at(0.2);

        assert_eq!(test.opens(), 2);
        let samples = test.play(4800);
        assert_eq!(test.position(), 9600 + 4800);
        assert_fades_in(&samples);
    }

    #[test]
    fn device_gone_pauses_polls_and_resumes_where_it_was() {
        let mut test = start(Some(speakers()), 128);
        test.play(24_000);
        test.world.borrow_mut().device = None;
        test.report(cpal::ErrorKind::DeviceNotAvailable);
        test.tick_at(0.5);
        assert_eq!(test.supervisor.status().state, DeviceState::Waiting);
        let queries = test.world.borrow().queries;

        // No polling until the interval has passed, then one poll each time.
        test.tick_at(1.9);
        assert_eq!(test.world.borrow().queries, queries);
        test.tick_at(2.0);
        assert_eq!(test.world.borrow().queries, queries + 1);
        test.tick_at(3.0);
        assert_eq!(test.world.borrow().queries, queries + 1);
        assert_eq!(test.supervisor.status().state, DeviceState::Waiting);

        // The device comes back and is found at the next poll.
        test.world.borrow_mut().device = Some(speakers());
        test.tick_at(3.5);
        assert_eq!(test.supervisor.status().state, DeviceState::Running);
        assert_eq!(test.opens(), 2);
        let samples = test.play(4800);
        assert_eq!(test.position(), 24_000 + 4800);
        assert_fades_in(&samples);
    }

    #[test]
    fn starting_with_no_device_waits_for_one() {
        let mut test = start(None, 128);
        assert_eq!(test.supervisor.status().state, DeviceState::Waiting);
        test.world.borrow_mut().device = Some(speakers());
        test.tick_at(DEVICE_POLL_INTERVAL.as_secs_f64());
        assert_eq!(test.supervisor.status().state, DeviceState::Running);
        assert_fades_in(&test.play(4800));
    }

    #[test]
    fn a_failed_open_waits_and_tries_again() {
        let mut test = start(Some(speakers()), 128);
        test.world.borrow_mut().fail_next_open = true;
        test.report(cpal::ErrorKind::StreamInvalidated);
        test.tick_at(0.1);
        let status = test.supervisor.status();
        assert_eq!(status.state, DeviceState::Waiting);
        assert_eq!(status.last_error.as_deref(), Some("the device is busy"));

        test.tick_at(0.1 + DEVICE_POLL_INTERVAL.as_secs_f64());
        assert_eq!(test.supervisor.status().state, DeviceState::Running);
        assert!(test.play(128).iter().any(|&s| s != 0.0));
    }

    #[test]
    fn xruns_count_as_dropouts() {
        let mut test = start(Some(speakers()), 128);
        for _ in 0..3 {
            test.report(cpal::ErrorKind::Xrun);
        }
        test.tick_at(0.1);
        assert_eq!(test.supervisor.status().dropouts, 3);
        assert_eq!(test.opens(), 1);
    }
}
