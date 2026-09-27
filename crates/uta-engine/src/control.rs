//! The control side of the engine. Nothing here runs on the audio thread, so
//! it may allocate and free.

use rtrb::{Consumer, Producer};

use crate::{Command, Snapshot, Status};

/// The command queue was full, so the command wasn't sent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QueueFull;

impl std::fmt::Display for QueueFull {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("the engine's command queue is full")
    }
}

impl std::error::Error for QueueFull {}

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

    /// Sets the volume in dB, by sending a new snapshot.
    pub fn set_volume_db(&mut self, volume_db: f32) -> Result<(), QueueFull> {
        self.set_snapshot(self.snapshot.clone().with_volume_db(volume_db))
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
