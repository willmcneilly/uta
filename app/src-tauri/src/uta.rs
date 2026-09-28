//! The app's control side: the project session, the engine's controller and
//! the live output. The Tauri commands in `commands` call into it, and the
//! frame thread reads it once per screen frame. Nothing here runs on the
//! audio thread.

use std::time::{Duration, Instant};

use crate::stress;

use serde::Serialize;
use uta_core::time::{TICKS_PER_QUARTER, Ticks};
use uta_core::{
    ClipId, Command, CommandError, Note, NoteId, Project, Session, Source, SynthParam,
    SynthSettings, TrackId, Waveform,
};
use uta_engine::live::{self, DeviceInfo, DeviceState, DeviceStatus, LiveOutput};
use uta_engine::{Controller, EngineConfig, NoteKey, Processor, Snapshot, Status};

/// The volume control's range, in dB. The top is the engine's ceiling, so
/// the control never asks for a volume the engine would clamp.
pub const VOLUME_RANGE_DB: (f32, f32) = (-60.0, Snapshot::MAX_VOLUME_DB);

/// What the UI shows of the project. Sent after every change to it.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectView {
    pub volume_db: f32,
    pub min_volume_db: f32,
    pub max_volume_db: f32,
    pub can_undo: bool,
    pub can_redo: bool,
    pub bpm: f32,
    pub min_bpm: f32,
    pub max_bpm: f32,
    pub loop_bars: u32,
    pub min_loop_bars: u32,
    pub max_loop_bars: u32,
    /// Where the loop starts, in ticks.
    pub loop_start: Ticks,
    /// How long the loop is, in ticks.
    pub loop_length: Ticks,
    pub ticks_per_quarter: Ticks,
    /// Always 4 for now (4/4).
    pub beats_per_bar: u32,
    /// The limits of the synth's settings, the same for every track.
    pub synth_limits: SynthLimits,
    /// The project's one track.
    pub track: TrackView,
}

/// A track as the UI shows it: its synth and its one clip.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrackView {
    pub id: TrackId,
    pub synth: SynthView,
    pub clip: ClipView,
}

/// The synth's settings, in the units of the "Synth settings" table in
/// `docs/plans/make-a-loop.md`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SynthView {
    pub waveform: Waveform,
    pub cutoff_hz: f32,
    pub resonance: f32,
    pub attack_seconds: f32,
    pub decay_seconds: f32,
    pub sustain: f32,
    pub release_seconds: f32,
}

impl From<&SynthSettings> for SynthView {
    fn from(settings: &SynthSettings) -> Self {
        Self {
            waveform: settings.waveform,
            cutoff_hz: settings.cutoff_hz,
            resonance: settings.resonance,
            attack_seconds: settings.attack_seconds,
            decay_seconds: settings.decay_seconds,
            sustain: settings.sustain,
            release_seconds: settings.release_seconds,
        }
    }
}

/// The inclusive `[min, max]` of each synth setting that has a range.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SynthLimits {
    pub cutoff_hz: (f32, f32),
    pub resonance: (f32, f32),
    /// For attack, decay and release.
    pub envelope_seconds: (f32, f32),
    pub sustain: (f32, f32),
}

impl SynthLimits {
    const ALL: Self = {
        use SynthSettings as S;
        Self {
            cutoff_hz: (S::MIN_CUTOFF_HZ, S::MAX_CUTOFF_HZ),
            resonance: (S::MIN_RESONANCE, S::MAX_RESONANCE),
            envelope_seconds: (S::MIN_ENVELOPE_SECONDS, S::MAX_ENVELOPE_SECONDS),
            sustain: (S::MIN_SUSTAIN, S::MAX_SUSTAIN),
        }
    };
}

/// A clip and its notes. Note starts are in ticks from the clip's start.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClipView {
    pub id: ClipId,
    pub start: Ticks,
    pub length: Ticks,
    /// In order of ID.
    pub notes: Vec<Note>,
}

/// Everything fast-changing, sent to the UI once per screen frame.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Frame {
    pub playing: bool,
    /// The playhead, in ticks from the start of the song. It stays inside
    /// the loop.
    pub playhead: Ticks,
    /// The loudest sample since the last frame, as a linear level.
    pub peak: f32,
    /// Dropouts since the app started, across every output.
    pub dropouts: u64,
    pub output: OutputView,
}

/// The output device as the UI shows it.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OutputView {
    pub state: OutputState,
    /// The device playing, or the last one that did.
    pub device: Option<String>,
    pub sample_rate: Option<u32>,
    /// The buffer size the stream is actually using.
    pub buffer_size: u32,
    /// The buffer size picked in the UI.
    pub requested_buffer_size: u32,
    /// The sizes the UI offers: those of [`live::BUFFER_SIZES`] the device
    /// supports.
    pub buffer_sizes: Vec<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OutputState {
    Running,
    Waiting,
    Failed,
}

/// How long a note sounds when it's auditioned: placed in the piano roll, or
/// dragged to a new pitch.
pub const AUDITION_TIME: Duration = Duration::from_millis(200);

/// A note sounding through the live route, and when to release it.
#[derive(Debug, Clone, Copy)]
struct Audition {
    key: NoteKey,
    ends: Instant,
}

/// Where the processor is.
enum Playback {
    /// Playing through the default output.
    Live(LiveOutput),
    /// Held here instead of playing, for tests to drive by hand.
    #[cfg_attr(not(test), allow(dead_code))]
    Offline(Processor),
}

pub struct Uta {
    session: Session,
    controller: Controller,
    /// `None` only if restarting the output failed.
    playback: Option<Playback>,
    requested_buffer: u32,
    /// Whether Play was pressed more recently than Stop, so a restarted
    /// output carries on playing.
    wants_playing: bool,
    /// Set when the engine's command queue was full, so its snapshot is older
    /// than the project. The next frame tries again.
    engine_behind: bool,
    /// Dropouts from outputs that have since been replaced.
    earlier_dropouts: u64,
    /// The drag the latest change came from.
    gesture: Option<u32>,
    /// The note being auditioned, if one is still sounding.
    audition: Option<Audition>,
    /// The key of the latest auditioned note. Live keys count up from 1, so
    /// they never match a sequenced note's key, which is a random UUID's
    /// number (always at least 2^78, because of the UUID's version bits).
    last_audition_key: u128,
}

impl Uta {
    /// Starts with a new project, playing through the default output (or
    /// waiting for one).
    pub fn start(buffer_size: u32) -> std::io::Result<Self> {
        let session = Session::new(Project::new());
        let (controller, processor) = new_engine(&session);
        let output = LiveOutput::start(processor, buffer_size)?;
        Ok(Self::new(
            session,
            controller,
            Playback::Live(output),
            buffer_size,
        ))
    }

    fn new(session: Session, controller: Controller, playback: Playback, buffer: u32) -> Self {
        Self {
            session,
            controller,
            playback: Some(playback),
            requested_buffer: buffer,
            wants_playing: false,
            engine_behind: false,
            earlier_dropouts: 0,
            gesture: None,
            audition: None,
            last_audition_key: 0,
        }
    }

    pub fn project(&self) -> ProjectView {
        let project = self.session.project();
        let transport = project.transport();
        let bar = transport.time_signature().ticks_per_bar();
        // Project 1 always has exactly one track with one clip.
        let track = &project.tracks()[0];
        let clip = &track.clips()[0];
        let Source::Synth(synth) = track.source();
        ProjectView {
            volume_db: project.master_volume_db(),
            min_volume_db: VOLUME_RANGE_DB.0,
            max_volume_db: VOLUME_RANGE_DB.1,
            can_undo: self.session.can_undo(),
            can_redo: self.session.can_redo(),
            bpm: transport.tempo_map().bpm(),
            min_bpm: Project::MIN_BPM,
            max_bpm: Project::MAX_BPM,
            loop_bars: u32::try_from(transport.loop_length() / bar).unwrap_or(u32::MAX),
            min_loop_bars: Project::MIN_LOOP_BARS,
            max_loop_bars: Project::MAX_LOOP_BARS,
            loop_start: transport.loop_start(),
            loop_length: transport.loop_length(),
            ticks_per_quarter: TICKS_PER_QUARTER,
            beats_per_bar: transport.time_signature().beats_per_bar,
            synth_limits: SynthLimits::ALL,
            track: TrackView {
                id: track.id(),
                synth: synth.into(),
                clip: ClipView {
                    id: clip.id(),
                    start: clip.start(),
                    length: clip.length(),
                    notes: clip.notes().copied().collect(),
                },
            },
        }
    }

    /// Sets the master volume through the project core. Changes that share a
    /// `gesture` (one drag of the volume control) undo as one.
    pub fn set_volume(&mut self, volume_db: f32, gesture: Option<u32>) -> Result<(), String> {
        self.change(Command::SetMasterVolume { volume_db }, gesture)
    }

    /// Sets the tempo, in BPM. Changes that share a `gesture` undo as one.
    pub fn set_tempo(&mut self, bpm: f32, gesture: Option<u32>) -> Result<(), String> {
        self.change(Command::SetTempo { bpm }, gesture)
    }

    /// Sets the loop's length, in bars. Changes that share a `gesture` undo
    /// as one.
    pub fn set_loop_length(&mut self, bars: u32, gesture: Option<u32>) -> Result<(), String> {
        self.change(Command::SetLoopLength { bars }, gesture)
    }

    /// Sets one of a track's synth settings. Changes to the same setting
    /// that share a `gesture` (one drag of its control) undo as one.
    pub fn set_synth_param(
        &mut self,
        track: TrackId,
        param: SynthParam,
        gesture: Option<u32>,
    ) -> Result<(), String> {
        self.change(Command::SetSynthParam { track, param }, gesture)
    }

    /// Fills the loop with [`stress::NOTE_COUNT`] notes, as one undoable
    /// `AddNotes`, to test how the piano roll copes with many notes.
    pub fn add_stress_notes(&mut self) -> Result<(), String> {
        let project = self.session.project();
        let clip = &project.tracks()[0].clips()[0];
        // Seeded by the notes already there, so pressing it again adds a
        // different pattern.
        let notes = stress::notes(project.transport().loop_length(), clip.notes().len() as u64);
        self.change(
            Command::AddNotes {
                clip: clip.id(),
                notes,
            },
            None,
        )
    }

    /// Adds `notes` to `clip`. Their IDs were chosen by the caller. A later
    /// [`Uta::set_notes`] of the same notes with the same `gesture` (drawing
    /// a note, then dragging out its length) undoes with it as one step.
    pub fn add_notes(
        &mut self,
        clip: ClipId,
        notes: Vec<Note>,
        gesture: Option<u32>,
    ) -> Result<(), String> {
        self.change(Command::AddNotes { clip, notes }, gesture)
    }

    /// Sets every value of existing notes in `clip`. Changes that share a
    /// `gesture` (one drag) undo as one.
    pub fn set_notes(
        &mut self,
        clip: ClipId,
        notes: Vec<Note>,
        gesture: Option<u32>,
    ) -> Result<(), String> {
        self.change(Command::SetNotes { clip, notes }, gesture)
    }

    pub fn remove_notes(&mut self, clip: ClipId, notes: Vec<NoteId>) -> Result<(), String> {
        self.change(Command::RemoveNotes { clip, notes }, None)
    }

    /// Trims the notes of the same pitch that `notes` now cover, so none
    /// hides behind another (see [`Clip::trims_under`]): when a drag of them
    /// ends, or a paste lands. The trim joins `gesture`'s undo step, so one
    /// undo brings the trimmed notes back too. It ends the gesture. Does
    /// nothing if the latest change came from something else, such as a
    /// drag that hasn't changed anything.
    pub fn trim_notes(
        &mut self,
        clip: ClipId,
        notes: Vec<NoteId>,
        gesture: u32,
    ) -> Result<(), String> {
        if self.gesture != Some(gesture) {
            return Ok(());
        }
        self.gesture = None;
        let trims = self
            .session
            .project()
            .clip(clip)
            .ok_or_else(|| CommandError::UnknownClip(clip).to_string())?
            .trims_under(&notes, NoteId::random);
        if trims.is_empty() {
            return Ok(());
        }
        for command in trims {
            self.session
                .join(command)
                .expect("trims are worked out from the clip as it is");
        }
        self.sync_engine();
        Ok(())
    }

    /// Puts back everything `gesture` changed, as if the drag never happened:
    /// Esc during a drag. Does nothing if the latest change came from
    /// something else, such as a drag that hasn't changed anything yet.
    pub fn cancel_gesture(&mut self, gesture: u32) {
        if self.gesture != Some(gesture) {
            return;
        }
        self.gesture = None;
        if !self.session.withdraw().is_empty() {
            self.sync_engine();
        }
    }

    /// Plays a note briefly through the live route, whether or not the loop
    /// is playing: for hearing a note as it's placed. It's not a change to
    /// the project. Any note still being auditioned is released first, and
    /// the frame thread releases this one after [`AUDITION_TIME`].
    pub fn audition(&mut self, pitch: u8, velocity: u8) -> Result<(), String> {
        self.release_audition();
        self.last_audition_key += 1;
        let key = NoteKey(self.last_audition_key);
        self.controller
            .note_on(key, pitch, velocity)
            .map_err(|error| error.to_string())?;
        self.audition = Some(Audition {
            key,
            ends: Instant::now() + AUDITION_TIME,
        });
        Ok(())
    }

    /// Releases the auditioned note if it's due by `now`.
    fn end_audition_by(&mut self, now: Instant) {
        if self.audition.is_some_and(|audition| audition.ends <= now) {
            self.release_audition();
        }
    }

    /// Releases the auditioned note. If the engine's queue is full, it's
    /// kept, and the next frame tries again.
    fn release_audition(&mut self) {
        if let Some(audition) = self.audition.take()
            && self.controller.note_off(audition.key).is_err()
        {
            self.audition = Some(audition);
        }
    }

    /// Applies `command` through the project core and sends the engine the
    /// result. A change that continues the latest one's `gesture` (the same
    /// drag) is amended into it, so the drag undoes as one step.
    fn change(&mut self, command: Command, gesture: Option<u32>) -> Result<(), String> {
        let continues = gesture.is_some() && gesture == self.gesture;
        let result = if continues {
            self.session.amend(command)
        } else {
            self.session.apply(command)
        };
        result.map_err(|error| error.to_string())?;
        self.gesture = gesture;
        self.sync_engine();
        Ok(())
    }

    /// Undoes the latest change. Does nothing if there's none.
    pub fn undo(&mut self) {
        self.gesture = None;
        if !self.session.undo().is_empty() {
            self.sync_engine();
        }
    }

    /// Redoes the latest undone change. Does nothing if there's none.
    pub fn redo(&mut self) {
        self.gesture = None;
        if !self.session.redo().is_empty() {
            self.sync_engine();
        }
    }

    pub fn play(&mut self) -> Result<(), String> {
        self.controller.play().map_err(|error| error.to_string())?;
        self.wants_playing = true;
        Ok(())
    }

    pub fn stop(&mut self) -> Result<(), String> {
        self.controller.stop().map_err(|error| error.to_string())?;
        self.wants_playing = false;
        Ok(())
    }

    /// Reopens the output at a new buffer size, carrying on playing if it
    /// was. The sound fades out, and the loop starts again from its start.
    pub fn set_buffer_size(&mut self, size: u32) -> Result<(), String> {
        if !live::BUFFER_SIZES.contains(&size) {
            return Err(format!(
                "buffer size {size} isn't one of {:?}",
                live::BUFFER_SIZES
            ));
        }
        if size == self.requested_buffer && self.playback.is_some() {
            return Ok(());
        }
        let live = !matches!(self.playback, Some(Playback::Offline(_)));
        self.close_output();

        let (controller, processor) = new_engine(&self.session);
        self.controller = controller;
        self.engine_behind = false;
        // It was sounding on the engine that's gone.
        self.audition = None;
        self.requested_buffer = size;
        self.playback = Some(if live {
            Playback::Live(LiveOutput::start(processor, size).map_err(|e| e.to_string())?)
        } else {
            Playback::Offline(processor)
        });
        if self.wants_playing {
            self.controller.play().expect("fresh queue has room");
        }
        Ok(())
    }

    /// Reads what the engine and the output have reported since the last
    /// frame. Only the frame thread calls it, so each peak is seen once.
    pub fn frame(&mut self) -> Frame {
        if self.engine_behind {
            self.sync_engine();
        }
        self.end_audition_by(Instant::now());
        let status: Status = self.controller.poll();
        let device = self.device_status();
        let sample_rate = device.device.as_ref().map(|d| d.sample_rate);
        Frame {
            playing: status.playing,
            playhead: status.playhead,
            peak: status.peak,
            dropouts: self.earlier_dropouts + device.dropouts,
            output: OutputView {
                state: match device.state {
                    DeviceState::Running => OutputState::Running,
                    DeviceState::Waiting => OutputState::Waiting,
                    DeviceState::Failed => OutputState::Failed,
                },
                device: device.device.as_ref().map(|d| d.name.clone()),
                sample_rate,
                buffer_size: device.buffer_size,
                requested_buffer_size: self.requested_buffer,
                buffer_sizes: buffer_sizes(device.device.as_ref()),
            },
        }
    }

    /// Stops the loop and fades the output out before closing it, so quitting
    /// doesn't click.
    pub fn shut_down(&mut self) {
        self.close_output();
    }

    fn close_output(&mut self) {
        let Some(playback) = self.playback.take() else {
            return;
        };
        match playback {
            Playback::Live(output) => {
                if self.controller.stop().is_ok() {
                    std::thread::sleep(live::HANDOVER_TIME);
                }
                self.earlier_dropouts += output.status().dropouts;
                output.stop();
            }
            Playback::Offline(_) => {}
        }
    }

    fn device_status(&self) -> DeviceStatus {
        match &self.playback {
            Some(Playback::Live(output)) => output.status(),
            Some(Playback::Offline(_)) => DeviceStatus {
                state: DeviceState::Running,
                device: Some(DeviceInfo {
                    name: "Offline".into(),
                    sample_rate: EngineConfig::default().sample_rate,
                    channels: 1,
                    buffer_range: None,
                }),
                buffer_size: self.requested_buffer,
                dropouts: 0,
                rebuilds: 0,
                last_error: None,
            },
            None => DeviceStatus {
                state: DeviceState::Failed,
                device: None,
                buffer_size: self.requested_buffer,
                dropouts: 0,
                rebuilds: 0,
                last_error: None,
            },
        }
    }

    /// Sends the engine a snapshot of the current project. If its queue is
    /// full (no device is taking audio), the next frame tries again.
    fn sync_engine(&mut self) {
        self.engine_behind = self.controller.set_project(self.session.project()).is_err();
    }
}

fn new_engine(session: &Session) -> (Controller, Processor) {
    // The supervisor moves the processor to the device's own rate before the
    // first block, so the rate here doesn't matter.
    uta_engine::engine(EngineConfig::default(), Snapshot::from(session.project()))
}

/// The buffer sizes to offer for `device`: all of them unless its supported
/// range is known.
fn buffer_sizes(device: Option<&DeviceInfo>) -> Vec<u32> {
    let range = device.and_then(|d| d.buffer_range);
    live::BUFFER_SIZES
        .into_iter()
        .filter(|size| range.is_none_or(|(min, max)| (min..=max).contains(size)))
        .collect()
}

/// How often the frame thread sends a [`Frame`]: once per frame of the web
/// view, which macOS runs at 60 frames a second (RFC-001, "Risks").
pub const FRAME_INTERVAL: Duration = Duration::from_micros(16_667);

#[cfg(test)]
mod tests {
    use super::*;

    fn offline() -> Uta {
        let session = Session::new(Project::new());
        let (controller, processor) = new_engine(&session);
        Uta::new(
            session,
            controller,
            Playback::Offline(processor),
            live::DEFAULT_BUFFER_SIZE,
        )
    }

    impl Uta {
        /// Runs the audio thread's side for `frames` frames, in blocks of 128.
        fn render(&mut self, frames: usize) {
            let Some(Playback::Offline(processor)) = &mut self.playback else {
                panic!("not offline");
            };
            let mut block = [0.0f32; 128];
            for _ in 0..frames / block.len() {
                processor.process(&mut block);
            }
        }

        fn engine_gain(&self) -> f32 {
            self.controller.snapshot().gain
        }
    }

    #[test]
    fn a_new_project_shows_the_default_volume_and_no_history() {
        let view = offline().project();
        assert_eq!(view.volume_db, Project::DEFAULT_MASTER_VOLUME_DB);
        assert!(!view.can_undo && !view.can_redo);
        assert_eq!((view.min_volume_db, view.max_volume_db), (-60.0, 0.0));
    }

    #[test]
    fn volume_goes_through_the_project_to_the_engine() {
        let mut uta = offline();
        uta.set_volume(-6.0, None).unwrap();
        assert_eq!(uta.project().volume_db, -6.0);
        assert!(uta.project().can_undo);
        assert_eq!(uta.engine_gain(), uta_engine::db_to_gain(-6.0));
    }

    #[test]
    fn undo_and_redo_reach_the_engine() {
        let mut uta = offline();
        uta.set_volume(-6.0, None).unwrap();
        uta.undo();
        assert_eq!(uta.project().volume_db, -12.0);
        assert_eq!(uta.engine_gain(), uta_engine::db_to_gain(-12.0));
        assert!(uta.project().can_redo);
        uta.redo();
        assert_eq!(uta.engine_gain(), uta_engine::db_to_gain(-6.0));
    }

    #[test]
    fn one_drag_undoes_as_one_step() {
        let mut uta = offline();
        for volume_db in [-13.0, -20.0, -30.0] {
            uta.set_volume(volume_db, Some(1)).unwrap();
        }
        uta.set_volume(-3.0, Some(2)).unwrap();

        uta.undo();
        assert_eq!(uta.project().volume_db, -30.0);
        uta.undo();
        assert_eq!(uta.project().volume_db, -12.0);
        assert!(!uta.project().can_undo);
    }

    #[test]
    fn a_drag_after_an_undo_starts_a_new_step() {
        let mut uta = offline();
        uta.set_volume(-20.0, Some(1)).unwrap();
        uta.undo();
        uta.set_volume(-30.0, Some(1)).unwrap();
        uta.undo();
        assert_eq!(uta.project().volume_db, -12.0);
    }

    #[test]
    fn invalid_volumes_are_refused() {
        let mut uta = offline();
        assert!(uta.set_volume(f32::NAN, None).is_err());
        assert!(uta.set_volume(20.0, None).is_err());
        assert!(!uta.project().can_undo);
    }

    #[test]
    fn frames_report_playing_and_position() {
        let mut uta = offline();
        let idle = uta.frame();
        assert!(!idle.playing);
        assert_eq!(idle.peak, 0.0);

        uta.play().unwrap();
        // Half a second fits in the status queue, as a frame's worth always does.
        uta.render(24_000);
        let frame = uta.frame();
        assert!(frame.playing);
        // Half a second at 120 BPM is one beat, to within a block.
        let block_ticks = 128 * TICKS_PER_QUARTER / 24_000;
        assert!(
            frame.playhead.abs_diff(TICKS_PER_QUARTER) <= block_ticks,
            "{frame:?}"
        );
        // A new project's loop has no notes, so it plays silence.
        assert_eq!(frame.peak, 0.0);
    }

    #[test]
    fn a_new_project_shows_its_tempo_loop_notes_and_synth() {
        let view = offline().project();
        assert_eq!(view.bpm, Project::DEFAULT_BPM);
        assert_eq!((view.min_bpm, view.max_bpm), (20.0, 300.0));
        assert_eq!(view.loop_bars, Project::DEFAULT_LOOP_BARS);
        assert_eq!((view.min_loop_bars, view.max_loop_bars), (1, 16));
        assert_eq!(view.loop_start, 0);
        assert_eq!(view.loop_length, 4 * 4 * TICKS_PER_QUARTER);
        assert_eq!((view.ticks_per_quarter, view.beats_per_bar), (960, 4));
        assert_eq!(view.track.clip.length, view.loop_length);
        assert!(view.track.clip.notes.is_empty());
        assert_eq!(view.track.synth, (&SynthSettings::default()).into());
    }

    #[test]
    fn the_project_view_serialises_for_the_ui() {
        let mut uta = offline();
        uta.add_stress_notes().unwrap();
        let json = serde_json::to_value(uta.project()).unwrap();
        assert_eq!(json["bpm"], 120.0);
        assert_eq!(json["loopBars"], 4);
        assert_eq!(json["track"]["synth"]["waveform"], "saw");
        assert_eq!(json["track"]["synth"]["cutoffHz"], 20_000.0);
        let note = &json["track"]["clip"]["notes"][0];
        for field in ["id", "pitch", "velocity", "start", "length"] {
            assert!(!note[field].is_null(), "{field} missing from {note}");
        }
        assert!(json["track"]["id"].is_string());
    }

    #[test]
    fn tempo_and_loop_length_go_through_the_project_to_the_engine() {
        let mut uta = offline();
        uta.set_tempo(90.0, None).unwrap();
        uta.set_loop_length(2, None).unwrap();
        let view = uta.project();
        assert_eq!((view.bpm, view.loop_bars), (90.0, 2));
        assert_eq!(view.track.clip.length, 2 * 4 * TICKS_PER_QUARTER);
        let rate = uta.controller.snapshot().sequence.sample_rate();
        // Two bars of 4/4 at 90 BPM: 8 beats of 2/3 s.
        let loop_samples = uta.controller.snapshot().sequence.loop_samples();
        assert_eq!(
            loop_samples.end - loop_samples.start,
            u64::from(rate) * 16 / 3
        );

        uta.undo();
        assert_eq!(uta.project().loop_bars, 4);
        uta.undo();
        assert_eq!(uta.project().bpm, 120.0);
    }

    #[test]
    fn out_of_range_tempo_and_loop_lengths_are_refused() {
        let mut uta = offline();
        assert!(uta.set_tempo(19.0, None).is_err());
        assert!(uta.set_tempo(f32::NAN, None).is_err());
        assert!(uta.set_loop_length(0, None).is_err());
        assert!(uta.set_loop_length(17, None).is_err());
        assert!(!uta.project().can_undo);
    }

    #[test]
    fn one_tempo_or_loop_drag_undoes_as_one_step() {
        let mut uta = offline();
        for bpm in [121.0, 130.0, 140.0] {
            uta.set_tempo(bpm, Some(1)).unwrap();
        }
        for bars in [5, 6, 8] {
            uta.set_loop_length(bars, Some(2)).unwrap();
        }
        uta.set_tempo(150.0, Some(3)).unwrap();

        uta.undo();
        assert_eq!(uta.project().bpm, 140.0);
        uta.undo();
        assert_eq!(uta.project().loop_bars, 4);
        assert_eq!(uta.project().bpm, 140.0);
        uta.undo();
        assert_eq!(uta.project().bpm, 120.0);
        assert!(!uta.project().can_undo);
    }

    #[test]
    fn the_project_view_carries_the_synth_limits() {
        let json = serde_json::to_value(offline().project()).unwrap();
        let limits = &json["synthLimits"];
        assert_eq!(limits["cutoffHz"], serde_json::json!([20.0, 20_000.0]));
        assert_eq!(limits["resonance"], serde_json::json!([0.0, 1.0]));
        assert_eq!(limits["sustain"], serde_json::json!([0.0, 1.0]));
        let envelope = limits["envelopeSeconds"].as_array().unwrap();
        assert_eq!(envelope[1], 10.0);
        // 0.001 as an f32 isn't exactly 0.001 as a JSON number.
        assert!((envelope[0].as_f64().unwrap() - 0.001).abs() < 1e-9);
    }

    #[test]
    fn synth_settings_go_through_the_project_to_the_engine() {
        let mut uta = offline();
        let track = uta.project().track.id;
        uta.set_synth_param(track, SynthParam::Waveform(Waveform::Square), None)
            .unwrap();
        uta.set_synth_param(track, SynthParam::CutoffHz(800.0), None)
            .unwrap();
        let view = uta.project().track.synth;
        assert_eq!((view.waveform, view.cutoff_hz), (Waveform::Square, 800.0));
        let engine = uta.controller.snapshot().synth;
        assert_eq!(engine.waveform, uta_engine::Waveform::Square);
        assert_eq!(engine.cutoff_hz, 800.0);

        uta.undo();
        assert_eq!(uta.project().track.synth.cutoff_hz, 20_000.0);
        assert_eq!(uta.controller.snapshot().synth.cutoff_hz, 20_000.0);
        uta.redo();
        assert_eq!(uta.project().track.synth.cutoff_hz, 800.0);
    }

    #[test]
    fn out_of_range_synth_settings_are_refused() {
        let mut uta = offline();
        let track = uta.project().track.id;
        assert!(
            uta.set_synth_param(track, SynthParam::Resonance(1.5), None)
                .is_err()
        );
        assert!(
            uta.set_synth_param(track, SynthParam::AttackSeconds(0.0), None)
                .is_err()
        );
        assert!(!uta.project().can_undo);
    }

    #[test]
    fn one_synth_drag_undoes_as_one_step() {
        let mut uta = offline();
        let track = uta.project().track.id;
        for hz in [10_000.0, 2_000.0, 500.0] {
            uta.set_synth_param(track, SynthParam::CutoffHz(hz), Some(1))
                .unwrap();
        }
        for level in [0.5, 0.2] {
            uta.set_synth_param(track, SynthParam::Sustain(level), Some(2))
                .unwrap();
        }

        uta.undo();
        let synth = uta.project().track.synth;
        assert_eq!((synth.cutoff_hz, synth.sustain), (500.0, 0.7));
        uta.undo();
        assert_eq!(uta.project().track.synth.cutoff_hz, 20_000.0);
        assert!(!uta.project().can_undo);
    }

    #[test]
    fn synth_params_arrive_from_the_ui_as_name_and_value() {
        let param: SynthParam =
            serde_json::from_value(serde_json::json!({"name": "cutoff_hz", "value": 440.0}))
                .unwrap();
        assert_eq!(param, SynthParam::CutoffHz(440.0));
        let param: SynthParam =
            serde_json::from_value(serde_json::json!({"name": "waveform", "value": "triangle"}))
                .unwrap();
        assert_eq!(param, SynthParam::Waveform(Waveform::Triangle));
    }

    #[test]
    fn stress_notes_are_one_undo_step() {
        let mut uta = offline();
        uta.add_stress_notes().unwrap();
        assert_eq!(uta.project().track.clip.notes.len(), stress::NOTE_COUNT);
        assert!(
            !uta.controller.snapshot().sequence.events().is_empty(),
            "the engine plays them"
        );
        uta.add_stress_notes().unwrap();
        assert_eq!(uta.project().track.clip.notes.len(), 2 * stress::NOTE_COUNT);
        uta.undo();
        assert_eq!(uta.project().track.clip.notes.len(), stress::NOTE_COUNT);
        uta.undo();
        assert!(uta.project().track.clip.notes.is_empty());
        assert!(!uta.project().can_undo);
    }

    /// A known note ID, as the UI would send it.
    fn note_id(id: u64) -> NoteId {
        serde_json::from_value(format!("00000000-0000-4000-8000-{id:012x}").into()).unwrap()
    }

    fn note(id: u64, pitch: u8, start: Ticks, length: Ticks) -> Note {
        Note {
            id: note_id(id),
            pitch,
            velocity: 100,
            start,
            length,
        }
    }

    fn clip_id(uta: &Uta) -> ClipId {
        uta.project().track.clip.id
    }

    #[test]
    fn notes_are_added_set_and_removed_through_the_project_to_the_engine() {
        let mut uta = offline();
        let clip = clip_id(&uta);
        uta.add_notes(clip, vec![note(1, 60, 0, 240)], None)
            .unwrap();
        assert_eq!(uta.project().track.clip.notes, vec![note(1, 60, 0, 240)]);
        assert_eq!(uta.controller.snapshot().sequence.events().len(), 2);

        uta.set_notes(clip, vec![note(1, 64, 960, 480)], None)
            .unwrap();
        assert_eq!(uta.project().track.clip.notes, vec![note(1, 64, 960, 480)]);

        uta.remove_notes(clip, vec![note_id(1)]).unwrap();
        assert!(uta.project().track.clip.notes.is_empty());
        assert!(uta.controller.snapshot().sequence.events().is_empty());
        assert!(uta.remove_notes(clip, vec![note_id(1)]).is_err());
    }

    #[test]
    fn drawing_a_note_and_dragging_its_length_is_one_undo_step() {
        let mut uta = offline();
        let clip = clip_id(&uta);
        uta.add_notes(clip, vec![note(1, 60, 0, 240)], Some(7))
            .unwrap();
        for length in [480, 720, 960] {
            uta.set_notes(clip, vec![note(1, 60, 0, length)], Some(7))
                .unwrap();
        }
        assert_eq!(uta.project().track.clip.notes[0].length, 960);
        uta.undo();
        assert!(uta.project().track.clip.notes.is_empty());
        assert!(!uta.project().can_undo);
        uta.redo();
        assert_eq!(uta.project().track.clip.notes, vec![note(1, 60, 0, 960)]);
    }

    #[test]
    fn cancelling_a_drag_puts_everything_back() {
        let mut uta = offline();
        let clip = clip_id(&uta);
        uta.add_notes(clip, vec![note(1, 60, 0, 240)], None)
            .unwrap();
        let before = uta.project();
        for start in [240, 480, 720] {
            uta.set_notes(clip, vec![note(1, 62, start, 240)], Some(3))
                .unwrap();
        }
        uta.cancel_gesture(3);
        assert_eq!(
            uta.project(),
            before,
            "the note is back, and so is the history"
        );
        assert_eq!(
            uta.controller.snapshot().sequence.events()[0].sample,
            0,
            "and the engine plays it where it was"
        );
        uta.undo();
        assert!(uta.project().track.clip.notes.is_empty());
    }

    #[test]
    fn cancelling_a_drawn_note_removes_it() {
        let mut uta = offline();
        let clip = clip_id(&uta);
        uta.add_notes(clip, vec![note(1, 60, 0, 240)], Some(4))
            .unwrap();
        uta.set_notes(clip, vec![note(1, 60, 0, 960)], Some(4))
            .unwrap();
        uta.cancel_gesture(4);
        assert!(uta.project().track.clip.notes.is_empty());
        assert!(!uta.project().can_undo && !uta.project().can_redo);
    }

    #[test]
    fn cancelling_only_undoes_that_drag() {
        let mut uta = offline();
        let clip = clip_id(&uta);
        uta.add_notes(clip, vec![note(1, 60, 0, 240)], Some(1))
            .unwrap();
        // A drag that hasn't changed anything, or an older one, cancels nothing.
        uta.cancel_gesture(2);
        assert_eq!(uta.project().track.clip.notes.len(), 1);
        uta.set_volume(-6.0, None).unwrap();
        uta.cancel_gesture(1);
        assert_eq!(uta.project().track.clip.notes.len(), 1);
        assert_eq!(uta.project().volume_db, -6.0);
        // And after an undo, the drag is no longer the latest change.
        uta.set_notes(clip, vec![note(1, 61, 0, 240)], Some(5))
            .unwrap();
        uta.undo();
        uta.cancel_gesture(5);
        assert!(uta.project().can_redo, "the undo is still there to redo");
    }

    #[test]
    fn a_drag_that_covers_notes_trims_them_when_it_ends_as_one_undo_step() {
        let mut uta = offline();
        let clip = clip_id(&uta);
        let others = vec![note(2, 60, 480, 480), note(3, 60, 1200, 480)];
        uta.add_notes(
            clip,
            [vec![note(1, 60, 0, 240)], others.clone()].concat(),
            None,
        )
        .unwrap();
        let before = uta.project();

        // Lengthen note 1 to 1440: over note 2, and half over note 3.
        for length in [720, 1440] {
            uta.set_notes(clip, vec![note(1, 60, 0, length)], Some(8))
                .unwrap();
        }
        assert_eq!(
            uta.project().track.clip.notes.len(),
            3,
            "nothing is trimmed mid-drag"
        );
        uta.trim_notes(clip, vec![note_id(1)], 8).unwrap();
        let mut notes = uta.project().track.clip.notes;
        notes.sort_by_key(|note| note.start);
        assert_eq!(notes, vec![note(1, 60, 0, 1440), note(3, 60, 1440, 240)]);
        assert_eq!(
            uta.controller.snapshot().sequence.events().len(),
            4,
            "the engine plays the trimmed notes"
        );

        uta.undo();
        assert_eq!(
            uta.project().track.clip.notes,
            before.track.clip.notes,
            "one undo brings them back"
        );
        uta.redo();
        assert_eq!(uta.project().track.clip.notes.len(), 2);
    }

    #[test]
    fn a_paste_trims_what_it_lands_on_as_one_undo_step() {
        let mut uta = offline();
        let clip = clip_id(&uta);
        uta.add_notes(clip, vec![note(1, 60, 0, 960)], None)
            .unwrap();
        uta.add_notes(clip, vec![note(2, 60, 480, 960)], Some(9))
            .unwrap();
        uta.trim_notes(clip, vec![note_id(2)], 9).unwrap();
        let mut notes = uta.project().track.clip.notes;
        notes.sort_by_key(|note| note.start);
        assert_eq!(notes, vec![note(1, 60, 0, 480), note(2, 60, 480, 960)]);
        uta.undo();
        assert_eq!(uta.project().track.clip.notes, vec![note(1, 60, 0, 960)]);
    }

    #[test]
    fn a_note_dropped_inside_a_longer_one_splits_it_as_one_undo_step() {
        let mut uta = offline();
        let clip = clip_id(&uta);
        uta.add_notes(clip, vec![note(1, 60, 0, 1920), note(2, 64, 0, 240)], None)
            .unwrap();
        let before = uta.project();
        // Move note 2 into the middle of note 1.
        uta.set_notes(clip, vec![note(2, 60, 480, 240)], Some(10))
            .unwrap();
        uta.trim_notes(clip, vec![note_id(2)], 10).unwrap();

        let mut notes = uta.project().track.clip.notes;
        notes.sort_by_key(|note| note.start);
        let spans: Vec<_> = notes.iter().map(|note| (note.start, note.length)).collect();
        assert_eq!(spans, [(0, 480), (480, 240), (720, 1200)]);
        assert_eq!(notes[0].id, note_id(1), "the head keeps the note's ID");
        assert!(
            ![note_id(1), note_id(2)].contains(&notes[2].id),
            "the tail is new"
        );

        uta.undo();
        assert_eq!(uta.project().track.clip.notes, before.track.clip.notes);
    }

    #[test]
    fn trimming_only_joins_the_drag_it_ends() {
        let mut uta = offline();
        let clip = clip_id(&uta);
        uta.add_notes(clip, vec![note(1, 60, 0, 960), note(2, 60, 480, 960)], None)
            .unwrap();
        // A drag that changed nothing trims nothing.
        uta.trim_notes(clip, vec![note_id(1)], 6).unwrap();
        assert_eq!(uta.project().track.clip.notes.len(), 2);

        uta.set_notes(clip, vec![note(1, 60, 0, 1200)], Some(7))
            .unwrap();
        uta.trim_notes(clip, vec![note_id(1)], 7).unwrap();
        uta.cancel_gesture(7);
        assert_eq!(
            uta.project().track.clip.notes[0].length,
            1200,
            "the drag has ended, so Esc changes nothing"
        );
        uta.trim_notes(clip, vec![note_id(1)], 7).unwrap();
        uta.undo();
        assert_eq!(uta.project().track.clip.notes[0].length, 960);
        assert_eq!(uta.project().track.clip.notes[1].start, 480);
    }

    #[test]
    fn an_auditioned_note_sounds_while_stopped_then_stops() {
        let mut uta = offline();
        uta.audition(69, 127).unwrap();
        uta.render(4_800);
        let frame = uta.frame();
        assert!(!frame.playing);
        assert!(frame.peak > 0.05, "{frame:?}");

        // The frame thread releases it once it's due.
        uta.end_audition_by(Instant::now() + AUDITION_TIME);
        assert!(uta.audition.is_none());
        // Past the release (0.2 s by default), it's silent.
        uta.render(24_000);
        uta.frame();
        uta.render(4_800);
        assert_eq!(uta.frame().peak, 0.0);
    }

    #[test]
    fn a_new_audition_releases_the_last_one() {
        let mut uta = offline();
        uta.audition(60, 100).unwrap();
        let first = uta.audition.unwrap().key;
        uta.audition(62, 100).unwrap();
        let second = uta.audition.unwrap().key;
        assert_ne!(first, second);
        // Not due yet, so still sounding.
        uta.end_audition_by(Instant::now());
        assert!(uta.audition.is_some());
        assert!(uta.audition(128, 100).is_err());
    }

    #[test]
    fn frames_describe_the_output() {
        let output = offline().frame().output;
        assert_eq!(output.state, OutputState::Running);
        assert_eq!(output.device.as_deref(), Some("Offline"));
        assert_eq!(output.buffer_size, 128);
        assert_eq!(output.requested_buffer_size, 128);
        assert_eq!(output.buffer_sizes, vec![32, 64, 128]);
    }

    #[test]
    fn frames_serialise_for_the_ui() {
        let json = serde_json::to_value(offline().frame()).unwrap();
        assert_eq!(json["output"]["state"], "running");
        assert_eq!(json["output"]["requestedBufferSize"], 128);
        assert!(json["playhead"].is_number());
    }

    #[test]
    fn buffer_sizes_are_limited_to_what_the_device_supports() {
        let device = |buffer_range| DeviceInfo {
            name: "Interface".into(),
            sample_rate: 48_000,
            channels: 2,
            buffer_range,
        };
        assert_eq!(buffer_sizes(None), vec![32, 64, 128]);
        assert_eq!(buffer_sizes(Some(&device(None))), vec![32, 64, 128]);
        assert_eq!(buffer_sizes(Some(&device(Some((64, 4096))))), vec![64, 128]);
        assert_eq!(
            buffer_sizes(Some(&device(Some((14, 512))))),
            vec![32, 64, 128]
        );
        assert!(buffer_sizes(Some(&device(Some((256, 4096))))).is_empty());
    }

    #[test]
    fn changing_the_buffer_keeps_playing_and_keeps_the_volume() {
        let mut uta = offline();
        uta.set_volume(-6.0, None).unwrap();
        uta.play().unwrap();
        uta.set_buffer_size(64).unwrap();
        uta.render(4_800);
        let frame = uta.frame();
        assert!(frame.playing);
        assert_eq!(frame.output.requested_buffer_size, 64);
        assert_eq!(uta.engine_gain(), uta_engine::db_to_gain(-6.0));
        assert!(uta.project().can_undo, "history survives the restart");
    }

    #[test]
    fn only_offered_buffer_sizes_are_accepted() {
        let mut uta = offline();
        assert!(uta.set_buffer_size(256).is_err());
        assert!(uta.set_buffer_size(0).is_err());
        assert_eq!(uta.frame().output.requested_buffer_size, 128);
    }

    #[test]
    fn a_full_engine_queue_catches_up_on_the_next_frame() {
        let mut uta = offline();
        // Nothing drains the queue until the processor runs.
        for i in 0..uta_engine::COMMAND_CAPACITY + 10 {
            uta.set_volume(-20.0 - (i % 30) as f32, None).unwrap();
        }
        uta.set_volume(-7.0, None).unwrap();
        assert!(uta.engine_behind);
        uta.render(128 * 400);
        uta.frame();
        assert!(!uta.engine_behind);
        assert_eq!(uta.engine_gain(), uta_engine::db_to_gain(-7.0));
    }
}
