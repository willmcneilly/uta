//! The control side of the engine. Nothing here runs on the audio thread, so
//! it may allocate and free.

use rtrb::{Consumer, Producer};

use uta_core::TrackId;
use uta_core::time::{MAX_TICKS, Ticks};

use crate::{Command, NoteKey, Snapshot, Status, SynthSettings, TRACK_SLOTS};

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
    /// The slot was [`TRACK_SLOTS`] or more.
    Slot(usize),
    /// The pitch was above 127.
    Pitch(u8),
    /// The velocity was 0 or above 127.
    Velocity(u8),
    QueueFull,
}

impl std::fmt::Display for NoteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Slot(slot) => write!(
                f,
                "slot {slot} isn't a track slot (0 to {})",
                TRACK_SLOTS - 1
            ),
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
    /// The rate the engine last reported. Snapshots are sent timed at it.
    sample_rate: u32,
    /// A snapshot retimed for a new rate couldn't be sent yet.
    retime_pending: bool,
    latest: Status,
    /// Snapshots sent so far.
    sent: u64,
    /// For each slot whose track has gone, the number of the snapshot that
    /// took it away (counting from 1). The slot isn't given to a new track
    /// until the audio thread has swapped that snapshot in and the old
    /// track's notes have faded out. See RFC-003, "The shared model,
    /// extended", point 4.
    retiring: [Option<u64>; TRACK_SLOTS],
}

impl Controller {
    pub(crate) fn new(
        snapshot: Snapshot,
        sample_rate: u32,
        commands: Producer<Command>,
        status: Consumer<Status>,
        used_snapshots: Consumer<Box<Snapshot>>,
    ) -> Self {
        Self {
            commands,
            status,
            used_snapshots,
            snapshot,
            sample_rate,
            retime_pending: false,
            latest: Status::default(),
            sent: 0,
            retiring: [None; TRACK_SLOTS],
        }
    }

    /// Plays from the play start: round the loop if it's on and the play
    /// start is before its end, or else to the song's end, where it stops
    /// and goes back to the play start.
    pub fn play(&mut self) -> Result<(), QueueFull> {
        self.send(Command::Play)
    }

    /// Stops, and goes back to where Play was last pressed.
    pub fn stop(&mut self) -> Result<(), QueueFull> {
        self.send(Command::Stop)
    }

    /// Stops where the playhead is, for [`Self::resume`] to carry on from.
    /// Stop still goes back to where Play was last pressed.
    pub fn pause(&mut self) -> Result<(), QueueFull> {
        self.send(Command::Pause)
    }

    /// Plays from where [`Self::pause`] stopped, chasing the notes already
    /// under way there, or from the play start if it wasn't paused.
    pub fn resume(&mut self) -> Result<(), QueueFull> {
        self.send(Command::Continue)
    }

    /// While stopped, sets where Play starts from, in ticks from the start
    /// of the song. While playing, jumps there instead, landing on its exact
    /// sample at the start of the next block, and Stop still goes back to
    /// where Play was pressed. What clicking the ruler does. See RFC-003,
    /// "Playing a song".
    pub fn locate(&mut self, ticks: Ticks) -> Result<(), QueueFull> {
        self.send(Command::Locate(ticks.min(MAX_TICKS)))
    }

    /// Sends a whole new snapshot, which the audio thread swaps in at the
    /// start of its next block. Its notes are retimed to the rate the engine
    /// is running at, so it can be built at any rate.
    pub fn set_snapshot(&mut self, snapshot: Snapshot) -> Result<(), QueueFull> {
        self.free_used_snapshots();
        let snapshot = snapshot.at_sample_rate(self.sample_rate);
        self.send(Command::SetSnapshot(Box::new(snapshot.clone())))?;
        self.sent += 1;
        for (slot, retiring) in self.retiring.iter_mut().enumerate() {
            if snapshot.track_in(slot).is_some() {
                *retiring = None;
            } else if self.snapshot.track_in(slot).is_some() {
                *retiring = Some(self.sent);
            }
        }
        self.snapshot = snapshot;
        self.retime_pending = false;
        Ok(())
    }

    /// Sends a snapshot of `project`, sharing the notes of every clip and
    /// track that hasn't changed since the last one sent (see
    /// [`Snapshot::sharing`]). Every track keeps its slot, and a new track
    /// gets one whose last track's notes have faded out, if there is one.
    /// What the app does after each change to the project.
    pub fn set_project(&mut self, project: &uta_core::Project) -> Result<(), QueueFull> {
        let retiring = self
            .retiring
            .iter()
            .enumerate()
            .filter(|(_, retiring)| retiring.is_some())
            .fold(0u32, |mask, (slot, _)| mask | 1 << slot);
        self.set_snapshot(Snapshot::sharing_avoiding(
            project,
            &self.snapshot,
            retiring,
        ))
    }

    /// The slot the track with this ID plays in, in the last snapshot sent.
    pub fn slot(&self, track: TrackId) -> Option<usize> {
        self.snapshot.slot_of(track)
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

    /// Sets the synth settings of the track in `slot`, by sending a new
    /// snapshot. They glide to their new values. Out-of-range values are
    /// clamped by the synth. With no track in the slot, the snapshot is sent
    /// unchanged.
    pub fn set_synth_settings(
        &mut self,
        slot: usize,
        settings: SynthSettings,
    ) -> Result<(), QueueFull> {
        let mut snapshot = self.snapshot.clone();
        if let Some(track) = snapshot
            .tracks_mut()
            .iter_mut()
            .find(|track| track.slot() == slot)
        {
            track.synth = settings;
        }
        self.set_snapshot(snapshot)
    }

    /// Starts a note on the synth of the track in `slot` straight away,
    /// without going through the project: the route for auditioning notes
    /// and, later, playing live. It sounds whether or not the transport is
    /// playing, until [`Controller::note_off`] with the same slot and key. A
    /// note already sounding with that key in that slot is released first.
    /// If no track has the slot when it arrives, it doesn't sound.
    pub fn note_on(
        &mut self,
        slot: usize,
        key: NoteKey,
        pitch: u8,
        velocity: u8,
    ) -> Result<(), NoteError> {
        if slot >= TRACK_SLOTS {
            return Err(NoteError::Slot(slot));
        }
        if pitch > 127 {
            return Err(NoteError::Pitch(pitch));
        }
        if !(1..=127).contains(&velocity) {
            return Err(NoteError::Velocity(velocity));
        }
        self.send(Command::NoteOn {
            slot,
            key,
            pitch,
            velocity,
        })?;
        Ok(())
    }

    /// Releases the note started with `key` in `slot`. Does nothing if it
    /// isn't sounding.
    pub fn note_off(&mut self, slot: usize, key: NoteKey) -> Result<(), QueueFull> {
        self.send(Command::NoteOff { slot, key })
    }

    /// The last snapshot sent.
    pub fn snapshot(&self) -> &Snapshot {
        &self.snapshot
    }

    /// Reads everything the audio thread has reported and frees the snapshots
    /// it has finished with. Returns the newest status, with `peak` and
    /// `track_peaks` the loudest levels since the previous call.
    ///
    /// If the engine has moved to a new sample rate (a device switch), sends
    /// the snapshot again, retimed for it.
    pub fn poll(&mut self) -> Status {
        self.free_used_snapshots();
        let mut peak = 0.0f32;
        let mut track_peaks = [0.0f32; TRACK_SLOTS];
        while let Ok(status) = self.status.pop() {
            peak = peak.max(status.peak);
            for (peak, &track_peak) in track_peaks.iter_mut().zip(&status.track_peaks) {
                *peak = peak.max(track_peak);
            }
            self.latest = status;
        }
        // A slot is free again once the snapshot that took its track away is
        // playing and the track's notes have faded out.
        let latest = self.latest;
        for (slot, retiring) in self.retiring.iter_mut().enumerate() {
            if retiring.is_some_and(|sent| {
                latest.snapshots >= sent && latest.sounding_slots & (1 << slot) == 0
            }) {
                *retiring = None;
            }
        }
        let rate = self.latest.sample_rate;
        if rate > 0 && rate != self.sample_rate {
            self.sample_rate = rate;
            self.retime_pending = true;
        }
        if self.retime_pending {
            // If the queue is full, it's tried again at the next poll.
            let _ = self.set_snapshot(self.snapshot.clone());
        }
        Status {
            peak,
            track_peaks,
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
    use crate::snapshot::busy_loop;

    fn playing_renderer() -> Renderer {
        let mut renderer = Renderer::new(EngineConfig::default(), Snapshot::default(), 128);
        renderer.controller.play().unwrap();
        renderer
    }

    /// From the UTA-2 review: NaN reached the samples.
    #[test]
    fn nan_and_infinite_volumes_are_rejected() {
        let mut renderer = playing_renderer();
        let before = renderer.controller.snapshot().clone();
        for volume_db in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            assert!(matches!(
                renderer.controller.set_volume_db(volume_db),
                Err(VolumeError::NotFinite(_))
            ));
        }
        assert_eq!(renderer.controller.snapshot(), &before);

        renderer.render_seconds(0.1);
        assert!(renderer.samples().iter().all(|s| s.is_finite()));
    }

    #[test]
    fn notes_outside_midi_ranges_are_rejected() {
        let mut renderer = playing_renderer();
        let key = NoteKey(1);
        let controller = &mut renderer.controller;
        assert_eq!(
            controller.note_on(0, key, 128, 100),
            Err(NoteError::Pitch(128))
        );
        assert_eq!(
            controller.note_on(0, key, 60, 0),
            Err(NoteError::Velocity(0))
        );
        assert_eq!(
            controller.note_on(0, key, 60, 128),
            Err(NoteError::Velocity(128))
        );
        assert_eq!(controller.note_on(0, key, 0, 1), Ok(()));
        assert_eq!(controller.note_on(0, key, 127, 127), Ok(()));
    }

    /// From the UTA-2 review: +40 dB made samples at 100x full scale.
    #[test]
    fn loud_volumes_are_clamped_to_full_scale() {
        let mut renderer = playing_renderer();
        renderer.controller.set_volume_db(40.0).unwrap();
        assert_eq!(renderer.controller.snapshot().gain, 1.0);

        // A full-velocity chord, as loud as the voices get.
        for pitch in [48, 55, 60, 64] {
            renderer
                .controller
                .note_on(0, NoteKey(u128::from(pitch)), pitch, 127)
                .unwrap();
        }
        renderer.render_seconds(0.1);
        let peak = renderer
            .samples()
            .iter()
            .fold(0.0f32, |p, s| p.max(s.abs()));
        assert!(peak <= 1.0, "peak {peak} is past full scale");
        assert!(peak > 0.5, "peak {peak}: the chord should play at 0 dB");
    }

    /// A device switch moves the engine to a new rate: the controller sees
    /// it in the status and sends the snapshot again, retimed, and from then
    /// on times every snapshot it sends for the new rate.
    #[test]
    fn snapshots_follow_the_engines_sample_rate() {
        let mut renderer = playing_renderer();
        renderer.controller.set_snapshot(busy_loop()).unwrap();
        renderer.render(128);
        assert_eq!(
            renderer.controller.snapshot().sequence.sample_rate(),
            48_000
        );

        // What the supervisor does between streams.
        renderer.processor().prepare(44_100, 1);
        renderer.render(128);
        let snapshot = renderer.controller.snapshot();
        assert_eq!(snapshot.sequence.sample_rate(), 44_100);
        assert_eq!(snapshot, &busy_loop().at_sample_rate(44_100));

        // A snapshot built at the default rate is sent retimed.
        renderer
            .controller
            .set_snapshot(Snapshot::default())
            .unwrap();
        assert_eq!(
            renderer.controller.snapshot().sequence.sample_rate(),
            44_100
        );
        renderer.render(128);
        assert_eq!(renderer.controller.poll().sample_rate, 44_100);
    }
}
