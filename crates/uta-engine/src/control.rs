//! The control side of the engine. Nothing here runs on the audio thread, so
//! it may allocate and free.

use rtrb::{Consumer, Producer};

use crate::{Command, NoteKey, Snapshot, Status, SynthSettings};

/// The command queue was full, so the command wasn't sent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QueueFull;

impl std::fmt::Display for QueueFull {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("the engine's command queue is full")
    }
}

impl std::error::Error for QueueFull {}

/// Why [`Controller::set_volume_db`] didn't change the volume.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum VolumeError {
    /// The volume was NaN or infinite.
    NotFinite(f32),
    QueueFull,
}

impl std::fmt::Display for VolumeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFinite(volume_db) => write!(f, "{volume_db} dB isn't a volume"),
            Self::QueueFull => QueueFull.fmt(f),
        }
    }
}

impl std::error::Error for VolumeError {}

impl From<QueueFull> for VolumeError {
    fn from(_: QueueFull) -> Self {
        Self::QueueFull
    }
}

/// Why [`Controller::note_on`] didn't start a note.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoteError {
    /// The pitch was above 127.
    Pitch(u8),
    /// The velocity was 0 or above 127.
    Velocity(u8),
    QueueFull,
}

impl std::fmt::Display for NoteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Pitch(pitch) => write!(f, "pitch {pitch} isn't a MIDI note (0 to 127)"),
            Self::Velocity(velocity) => write!(f, "velocity {velocity} isn't 1 to 127"),
            Self::QueueFull => QueueFull.fmt(f),
        }
    }
}

impl std::error::Error for NoteError {}

impl From<QueueFull> for NoteError {
    fn from(_: QueueFull) -> Self {
        Self::QueueFull
    }
}

/// Drives the engine from the control side.
pub struct Controller {
    commands: Producer<Command>,
    status: Consumer<Status>,
    used_snapshots: Consumer<Box<Snapshot>>,
    /// The last snapshot sent, as the base for the next change.
    snapshot: Snapshot,
    latest: Status,
}

impl Controller {
    pub(crate) fn new(
        snapshot: Snapshot,
        commands: Producer<Command>,
        status: Consumer<Status>,
        used_snapshots: Consumer<Box<Snapshot>>,
    ) -> Self {
        Self {
            commands,
            status,
            used_snapshots,
            snapshot,
            latest: Status::default(),
        }
    }

    pub fn play(&mut self) -> Result<(), QueueFull> {
        self.send(Command::Play)
    }

    pub fn stop(&mut self) -> Result<(), QueueFull> {
        self.send(Command::Stop)
    }

    /// Sends a whole new snapshot, which the audio thread swaps in at the
    /// start of its next block.
    pub fn set_snapshot(&mut self, snapshot: Snapshot) -> Result<(), QueueFull> {
        self.free_used_snapshots();
        self.send(Command::SetSnapshot(Box::new(snapshot.clone())))?;
        self.snapshot = snapshot;
        Ok(())
    }

    /// Sets the volume in dB, by sending a new snapshot. NaN and infinite
    /// values are rejected, and anything above [`Snapshot::MAX_VOLUME_DB`] is
    /// clamped to it, so no volume can push samples past full scale.
    pub fn set_volume_db(&mut self, volume_db: f32) -> Result<(), VolumeError> {
        if !volume_db.is_finite() {
            return Err(VolumeError::NotFinite(volume_db));
        }
        self.set_snapshot(self.snapshot.clone().with_volume_db(volume_db))?;
        Ok(())
    }

    /// Sets the synth's settings, by sending a new snapshot. They glide to
    /// their new values. Out-of-range values are clamped by the synth.
    pub fn set_synth_settings(&mut self, settings: SynthSettings) -> Result<(), QueueFull> {
        self.set_snapshot(Snapshot {
            synth: settings,
            ..self.snapshot.clone()
        })
    }

    /// Starts a note on the synth straight away, without going through the
    /// project: the route for auditioning notes and, later, playing live. It
    /// sounds whether or not the transport is playing, until
    /// [`Controller::note_off`] with the same key. A note already sounding
    /// with that key is released first.
    pub fn note_on(&mut self, key: NoteKey, pitch: u8, velocity: u8) -> Result<(), NoteError> {
        if pitch > 127 {
            return Err(NoteError::Pitch(pitch));
        }
        if !(1..=127).contains(&velocity) {
            return Err(NoteError::Velocity(velocity));
        }
        self.send(Command::NoteOn {
            key,
            pitch,
            velocity,
        })?;
        Ok(())
    }

    /// Releases the note started with `key`. Does nothing if it isn't
    /// sounding.
    pub fn note_off(&mut self, key: NoteKey) -> Result<(), QueueFull> {
        self.send(Command::NoteOff { key })
    }

    /// The last snapshot sent.
    pub fn snapshot(&self) -> &Snapshot {
        &self.snapshot
    }

    /// Reads everything the audio thread has reported and frees the snapshots
    /// it has finished with. Returns the newest status, with `peak` the
    /// loudest level since the previous call.
    pub fn poll(&mut self) -> Status {
        self.free_used_snapshots();
        let mut peak = 0.0f32;
        while let Ok(status) = self.status.pop() {
            peak = peak.max(status.peak);
            self.latest = status;
        }
        Status {
            peak,
            ..self.latest
        }
    }

    /// Drops the snapshots the audio thread has sent back. Returns how many.
    pub fn free_used_snapshots(&mut self) -> usize {
        let mut freed = 0;
        while let Ok(snapshot) = self.used_snapshots.pop() {
            drop(snapshot);
            freed += 1;
        }
        freed
    }

    fn send(&mut self, command: Command) -> Result<(), QueueFull> {
        self.commands.push(command).map_err(|_| QueueFull)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::EngineConfig;
    use crate::offline::Renderer;

    fn playing_renderer() -> Renderer {
        let mut renderer = Renderer::new(EngineConfig::default(), Snapshot::default(), 128);
        renderer.controller.play().unwrap();
        renderer
    }

    /// From the UTA-2 review: NaN reached the samples.
    #[test]
    fn nan_and_infinite_volumes_are_rejected() {
        let mut renderer = playing_renderer();
        for volume_db in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            assert!(matches!(
                renderer.controller.set_volume_db(volume_db),
                Err(VolumeError::NotFinite(_))
            ));
        }
        assert_eq!(renderer.controller.snapshot(), &Snapshot::default());

        renderer.render_seconds(0.1);
        assert!(renderer.samples().iter().all(|s| s.is_finite()));
    }

    #[test]
    fn notes_outside_midi_ranges_are_rejected() {
        let mut renderer = playing_renderer();
        let key = NoteKey(1);
        let controller = &mut renderer.controller;
        assert_eq!(
            controller.note_on(key, 128, 100),
            Err(NoteError::Pitch(128))
        );
        assert_eq!(controller.note_on(key, 60, 0), Err(NoteError::Velocity(0)));
        assert_eq!(
            controller.note_on(key, 60, 128),
            Err(NoteError::Velocity(128))
        );
        assert_eq!(controller.note_on(key, 0, 1), Ok(()));
        assert_eq!(controller.note_on(key, 127, 127), Ok(()));
    }

    /// From the UTA-2 review: +40 dB made samples at 100x full scale.
    #[test]
    fn loud_volumes_are_clamped_to_full_scale() {
        let mut renderer = playing_renderer();
        renderer.controller.set_volume_db(40.0).unwrap();
        assert_eq!(renderer.controller.snapshot().gain, 1.0);

        renderer.render_seconds(0.1);
        let peak = renderer
            .samples()
            .iter()
            .fold(0.0f32, |p, s| p.max(s.abs()));
        assert!(peak <= 1.0, "peak {peak} is past full scale");
        assert!(peak > 0.99, "peak {peak}: the tone should reach full scale");
    }
}
