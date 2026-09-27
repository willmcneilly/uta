//! The "what to play" snapshot.

use std::sync::Arc;

use uta_core::time::{TempoMap, Ticks};
use uta_core::{Note, Project, Source};

use crate::{DEFAULT_SAMPLE_RATE, NoteKey, SynthSettings, Waveform};

/// Everything the audio thread needs to know about what to play.
///
/// The control side builds a whole new snapshot for every change, and the
/// audio thread swaps it in at the start of a block. The old one goes back to
/// the control side to be freed. It lives in a `Box` on its way through the
/// queues, so the audio thread only ever moves a pointer.
#[derive(Debug, Clone, PartialEq)]
pub struct Snapshot {
    /// The output volume as a linear gain (1.0 is full scale).
    pub gain: f32,
    /// How the synth sounds.
    pub synth: SynthSettings,
    /// The notes and the loop, timed in samples.
    pub sequence: Sequence,
}

impl Snapshot {
    /// The default volume, in dB.
    pub const DEFAULT_VOLUME_DB: f32 = -12.0;
    /// The loudest volume, in dB. Above it, loud chords could push samples
    /// past full scale.
    pub const MAX_VOLUME_DB: f32 = 0.0;

    /// What the engine plays for `project`, with its notes timed at
    /// `sample_rate`.
    pub fn new(project: &Project, sample_rate: u32) -> Self {
        let synth = project
            .tracks()
            .first()
            .map(|track| match track.source() {
                Source::Synth(settings) => synth_settings(settings),
            })
            .unwrap_or_default();
        Self {
            gain: 1.0,
            synth,
            sequence: Sequence::new(project, sample_rate),
        }
        .with_volume_db(project.master_volume_db())
    }

    /// A snapshot with the given volume in dB, clamped to
    /// [`Self::MAX_VOLUME_DB`]. NaN is silence.
    pub fn with_volume_db(self, volume_db: f32) -> Self {
        let gain = if volume_db.is_nan() {
            0.0
        } else {
            db_to_gain(volume_db.min(Self::MAX_VOLUME_DB))
        };
        Self { gain, ..self }
    }

    /// This snapshot with its notes timed at `sample_rate`. The clips' notes
    /// are shared, not copied. Returns a plain clone if it's already at that
    /// rate.
    pub fn at_sample_rate(&self, sample_rate: u32) -> Self {
        Self {
            sequence: self.sequence.at_sample_rate(sample_rate),
            ..self.clone()
        }
    }
}

/// A new project's snapshot, at [`DEFAULT_SAMPLE_RATE`]: an empty loop.
impl Default for Snapshot {
    fn default() -> Self {
        Self::from(&Project::new())
    }
}

/// What the engine plays for a project, at [`DEFAULT_SAMPLE_RATE`]. The
/// control side sends the result with [`crate::Controller::set_snapshot`]
/// after each change, and the controller retimes it to the engine's current
/// rate.
impl From<&Project> for Snapshot {
    fn from(project: &Project) -> Self {
        Self::new(project, DEFAULT_SAMPLE_RATE)
    }
}

/// The project's notes and loop, as the audio thread plays them.
///
/// It keeps the musical version (each clip's notes, in ticks, in an `Arc`,
/// with the tempo map and the loop) and, worked out from it at one sample
/// rate, every note start and end in samples as one list sorted by time.
/// Notes are trimmed to the loop: only notes that start inside it play, and
/// any that run past its end stop there. See RFC-002, "The shared model",
/// points 5 and 7.
#[derive(Debug, Clone, PartialEq)]
pub struct Sequence {
    tempo_map: TempoMap,
    loop_start: Ticks,
    loop_length: Ticks,
    clips: Vec<Arc<ClipNotes>>,

    sample_rate: u32,
    loop_start_sample: u64,
    loop_end_sample: u64,
    events: Arc<[NoteEvent]>,
    /// The same notes, one entry each, sorted by key, so a sounding note can
    /// be looked up when a new snapshot arrives.
    notes: Arc<[NoteSpan]>,
}

/// One clip's notes, in ticks.
#[derive(Debug, Clone, PartialEq)]
pub struct ClipNotes {
    /// Where the clip starts, in ticks from the start of the song.
    pub start: Ticks,
    /// Notes starting at or after this, from the clip's start, don't play.
    pub length: Ticks,
    pub notes: Vec<Note>,
}

/// A note starting or ending, on an exact sample.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NoteEvent {
    /// The sample it happens on, counted from the start of the song.
    pub sample: u64,
    pub kind: NoteEventKind,
}

/// When one note plays, in samples, after trimming to the loop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NoteSpan {
    pub key: NoteKey,
    pub pitch: u8,
    /// The sample it starts on, from the start of the song.
    pub start: u64,
    /// The sample it ends on: it sounds up to, not including, this one.
    pub end: u64,
}

impl NoteSpan {
    /// Whether it's sounding at `sample`.
    pub fn contains(&self, sample: u64) -> bool {
        (self.start..self.end).contains(&sample)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoteEventKind {
    /// Starts a note. The key is the note's permanent ID.
    On {
        key: NoteKey,
        pitch: u8,
        velocity: u8,
    },
    /// Releases the note started with this key.
    Off { key: NoteKey },
}

impl NoteEventKind {
    /// Ends sort before starts on the same sample, so a voice is released
    /// before the next note needs one.
    fn order(&self) -> u8 {
        match self {
            Self::Off { .. } => 0,
            Self::On { .. } => 1,
        }
    }
}

impl Sequence {
    fn new(project: &Project, sample_rate: u32) -> Self {
        let transport = project.transport();
        let clips = project
            .tracks()
            .iter()
            .flat_map(|track| track.clips())
            .map(|clip| {
                Arc::new(ClipNotes {
                    start: clip.start(),
                    length: clip.length(),
                    notes: clip.notes().copied().collect(),
                })
            })
            .collect();
        Self::build(
            transport.tempo_map().clone(),
            transport.loop_start(),
            transport.loop_length(),
            clips,
            sample_rate,
        )
    }

    fn build(
        tempo_map: TempoMap,
        loop_start: Ticks,
        loop_length: Ticks,
        clips: Vec<Arc<ClipNotes>>,
        sample_rate: u32,
    ) -> Self {
        assert!(sample_rate > 0, "sample rate must be positive");
        assert!(loop_length > 0, "the loop can't be empty");
        let loop_end = loop_start + loop_length;
        let samples = |ticks| tempo_map.ticks_to_samples(ticks, sample_rate);

        let mut events = Vec::new();
        let mut notes = Vec::new();
        for clip in &clips {
            for note in &clip.notes {
                if note.start >= clip.length {
                    continue;
                }
                let start = clip.start + note.start;
                if !(loop_start..loop_end).contains(&start) {
                    continue;
                }
                let end = (clip.start + (note.start + note.length).min(clip.length)).min(loop_end);
                let (start, end) = (samples(start), samples(end));
                if end <= start {
                    continue;
                }
                let key = NoteKey(note.id.as_uuid().as_u128());
                notes.push(NoteSpan {
                    key,
                    pitch: note.pitch,
                    start,
                    end,
                });
                events.push(NoteEvent {
                    sample: start,
                    kind: NoteEventKind::On {
                        key,
                        pitch: note.pitch,
                        velocity: note.velocity,
                    },
                });
                events.push(NoteEvent {
                    sample: end,
                    kind: NoteEventKind::Off { key },
                });
            }
        }
        events.sort_by_key(|event| (event.sample, event.kind.order()));
        notes.sort_by_key(|note| note.key.0);

        Self {
            loop_start_sample: samples(loop_start),
            loop_end_sample: samples(loop_end),
            tempo_map,
            loop_start,
            loop_length,
            clips,
            sample_rate,
            events: events.into(),
            notes: notes.into(),
        }
    }

    /// This sequence timed at `sample_rate`, sharing the clips' notes.
    pub fn at_sample_rate(&self, sample_rate: u32) -> Self {
        if sample_rate == self.sample_rate {
            return self.clone();
        }
        Self::build(
            self.tempo_map.clone(),
            self.loop_start,
            self.loop_length,
            self.clips.clone(),
            sample_rate,
        )
    }

    /// The sample rate the events are timed at.
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Where the loop starts and ends, in samples from the start of the song.
    pub fn loop_samples(&self) -> std::ops::Range<u64> {
        self.loop_start_sample..self.loop_end_sample
    }

    /// Every note start and end in the loop, sorted by sample.
    pub fn events(&self) -> &[NoteEvent] {
        &self.events
    }

    /// The note with this key, if it plays in the loop. A binary search, so
    /// it's real-time safe.
    pub fn note(&self, key: NoteKey) -> Option<&NoteSpan> {
        self.notes
            .binary_search_by_key(&key.0, |note| note.key.0)
            .ok()
            .map(|index| &self.notes[index])
    }

    /// Where a playhead at `playhead` in `old` carries on in this sequence.
    /// Real-time safe.
    ///
    /// If the tempo or sample rate changed, it keeps its bar and beat: it
    /// moves to the first tick `old` hadn't reached yet, so every note event
    /// `old` had already played stays played and none is skipped. Otherwise
    /// it stays on the same sample. If that's outside the loop (the loop got
    /// shorter, or the playhead was waiting at the loop's end) it carries on
    /// from the loop's start.
    pub fn playhead_from(&self, old: &Sequence, playhead: u64) -> u64 {
        let playhead = if self.sample_rate == old.sample_rate && self.tempo_map == old.tempo_map {
            playhead
        } else {
            let ticks = old.ticks_at(playhead);
            let ticks = if old.sample_at(ticks) < playhead {
                ticks + 1
            } else {
                ticks
            };
            self.sample_at(ticks)
        };
        if self.loop_samples().contains(&playhead) {
            playhead
        } else {
            self.loop_start_sample
        }
    }

    /// Each clip's notes, shared between snapshots.
    pub fn clips(&self) -> &[Arc<ClipNotes>] {
        &self.clips
    }

    /// The musical position of a sample, in ticks from the start of the song.
    /// Real-time safe.
    pub fn ticks_at(&self, sample: u64) -> Ticks {
        self.tempo_map.samples_to_ticks(sample, self.sample_rate)
    }

    /// The sample a tick falls on. Real-time safe.
    pub fn sample_at(&self, ticks: Ticks) -> u64 {
        self.tempo_map.ticks_to_samples(ticks, self.sample_rate)
    }
}

/// A 1-bar loop at 120 BPM filled with legato 32nd notes, so something is
/// always sounding while it plays. For tests that need sound at any moment.
#[cfg(test)]
pub(crate) fn busy_loop() -> Snapshot {
    let id = |n| uuid::Uuid::from_u128(n);
    let mut project = Project::with_id(uta_core::ProjectId::from_uuid(id(1)));
    project
        .apply(&uta_core::Command::SetLoopLength { bars: 1 })
        .unwrap();
    let clip = project.tracks()[0].clips()[0].id();
    let notes = (0..32u8)
        .map(|i| Note {
            id: uta_core::NoteId::from_uuid(id(100 + u128::from(i))),
            pitch: 60 + i % 12,
            velocity: 100,
            start: Ticks::from(i) * 120,
            length: 120,
        })
        .collect();
    project
        .apply(&uta_core::Command::AddNotes { clip, notes })
        .unwrap();
    Snapshot::from(&project)
}

/// The core's synth settings as the engine plays them: the same fields,
/// units and ranges.
fn synth_settings(settings: &uta_core::SynthSettings) -> SynthSettings {
    SynthSettings {
        waveform: match settings.waveform {
            uta_core::Waveform::Sine => Waveform::Sine,
            uta_core::Waveform::Triangle => Waveform::Triangle,
            uta_core::Waveform::Saw => Waveform::Saw,
            uta_core::Waveform::Square => Waveform::Square,
        },
        cutoff_hz: settings.cutoff_hz,
        resonance: settings.resonance,
        attack_seconds: settings.attack_seconds,
        decay_seconds: settings.decay_seconds,
        sustain: settings.sustain,
        release_seconds: settings.release_seconds,
    }
}

/// Converts decibels to a linear gain. Anything at or below the project's
/// minimum volume, [`Project::MIN_VOLUME_DB`], is silence.
pub fn db_to_gain(db: f32) -> f32 {
    if db <= Project::MIN_VOLUME_DB {
        0.0
    } else {
        10f32.powf(db / 20.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uta_core::{ClipId, Command, NoteId, ProjectId, SynthParam};
    use uuid::Uuid;

    fn project() -> Project {
        Project::with_id(ProjectId::from_uuid(Uuid::from_u128(1)))
    }

    fn clip(project: &Project) -> ClipId {
        project.tracks()[0].clips()[0].id()
    }

    fn note(id: u128, pitch: u8, start: Ticks, length: Ticks) -> Note {
        Note {
            id: NoteId::from_uuid(Uuid::from_u128(id)),
            pitch,
            velocity: 100,
            start,
            length,
        }
    }

    fn with_notes(notes: Vec<Note>) -> Project {
        let mut project = project();
        let clip = clip(&project);
        project.apply(&Command::AddNotes { clip, notes }).unwrap();
        project
    }

    fn key(id: u128) -> NoteKey {
        NoteKey(Uuid::from_u128(id).as_u128())
    }

    fn on(sample: u64, id: u128, pitch: u8) -> NoteEvent {
        NoteEvent {
            sample,
            kind: NoteEventKind::On {
                key: key(id),
                pitch,
                velocity: 100,
            },
        }
    }

    fn off(sample: u64, id: u128) -> NoteEvent {
        NoteEvent {
            sample,
            kind: NoteEventKind::Off { key: key(id) },
        }
    }

    #[test]
    fn db_to_gain_known_values() {
        assert_eq!(db_to_gain(0.0), 1.0);
        assert!((db_to_gain(-6.0) - 0.501_187).abs() < 1e-6);
        assert!((db_to_gain(-20.0) - 0.1).abs() < 1e-7);
        assert_eq!(db_to_gain(-120.0), 0.0);
        assert_eq!(db_to_gain(f32::NEG_INFINITY), 0.0);
    }

    #[test]
    fn a_new_project_is_an_empty_four_bar_loop_at_the_default_volume() {
        let snapshot = Snapshot::default();
        assert_eq!(snapshot.gain, db_to_gain(Snapshot::DEFAULT_VOLUME_DB));
        assert_eq!(snapshot.synth, SynthSettings::default());
        let sequence = &snapshot.sequence;
        assert_eq!(sequence.sample_rate(), DEFAULT_SAMPLE_RATE);
        // 4 bars at 120 BPM is 8 s.
        assert_eq!(sequence.loop_samples(), 0..8 * 48_000);
        assert!(sequence.events().is_empty());
    }

    #[test]
    fn volume_is_clamped_to_the_ceiling() {
        let snapshot = Snapshot::default();
        assert_eq!(snapshot.clone().with_volume_db(40.0).gain, 1.0);
        assert_eq!(snapshot.clone().with_volume_db(f32::INFINITY).gain, 1.0);
        assert_eq!(snapshot.clone().with_volume_db(0.0).gain, 1.0);
        assert_eq!(snapshot.clone().with_volume_db(f32::NAN).gain, 0.0);
        assert_eq!(snapshot.with_volume_db(-6.0).gain, db_to_gain(-6.0));
    }

    #[test]
    fn the_snapshot_follows_the_master_volume_and_synth() {
        let mut project = project();
        let track = project.tracks()[0].id();
        project
            .apply(&Command::SetMasterVolume { volume_db: -6.0 })
            .unwrap();
        project
            .apply(&Command::SetSynthParam {
                track,
                param: SynthParam::Waveform(uta_core::Waveform::Square),
            })
            .unwrap();
        project
            .apply(&Command::SetSynthParam {
                track,
                param: SynthParam::CutoffHz(800.0),
            })
            .unwrap();
        let snapshot = Snapshot::from(&project);
        assert_eq!(snapshot.gain, db_to_gain(-6.0));
        assert_eq!(snapshot.synth.waveform, Waveform::Square);
        assert_eq!(snapshot.synth.cutoff_hz, 800.0);
    }

    #[test]
    fn the_synth_settings_copy_across_field_by_field() {
        let core = uta_core::SynthSettings::default();
        assert_eq!(synth_settings(&core), SynthSettings::default());
    }

    #[test]
    fn notes_become_starts_and_ends_in_samples_sorted_by_time() {
        // At 120 BPM and 48 kHz a quarter note (960 ticks) is 24,000 samples.
        let project = with_notes(vec![
            note(3, 67, 1920, 960),
            note(1, 60, 0, 480),
            note(2, 64, 480, 1440),
        ]);
        let snapshot = Snapshot::new(&project, 48_000);
        assert_eq!(
            snapshot.sequence.events(),
            [
                on(0, 1, 60),
                off(12_000, 1),
                on(12_000, 2, 64),
                off(48_000, 2),
                on(48_000, 3, 67),
                off(72_000, 3),
            ],
            "ends come before starts on the same sample"
        );
    }

    #[test]
    fn notes_are_trimmed_to_the_loop() {
        let mut project = with_notes(vec![
            // Ends exactly at the loop's end (4 bars, 15,360 ticks).
            note(1, 60, 15_360 - 480, 480),
            // Crosses the loop's end: it stops there.
            note(2, 62, 15_360 - 480, 960),
            // Starts at the loop's end, and past it: neither plays.
            note(3, 64, 15_360, 480),
            note(4, 65, 20_000, 480),
        ]);
        let snapshot = Snapshot::new(&project, 48_000);
        let end = 8 * 48_000;
        assert_eq!(
            snapshot.sequence.events(),
            [
                on(end - 12_000, 1, 60),
                on(end - 12_000, 2, 62),
                off(end, 1),
                off(end, 2),
            ]
        );

        // Shortening the loop to 1 bar trims the same way at the new end.
        project.apply(&Command::SetLoopLength { bars: 1 }).unwrap();
        let snapshot = Snapshot::new(&project, 48_000);
        assert_eq!(snapshot.sequence.loop_samples(), 0..96_000);
        assert!(snapshot.sequence.events().is_empty());
    }

    /// In project 1 the clip is always as long as the loop, so the clip's
    /// length trims notes before the loop's end can. Here the clip is longer
    /// than the loop and starts before it, so only the loop checks keep notes
    /// inside it.
    #[test]
    fn notes_are_trimmed_to_a_loop_shorter_than_the_clip() {
        // A 4-bar clip; the loop is bar 2 (ticks 3,840 to 7,680).
        let clip = Arc::new(ClipNotes {
            start: 0,
            length: 4 * 3840,
            notes: vec![
                // Starts before the loop and runs into it: doesn't play.
                note(1, 60, 3840 - 480, 960),
                // Starts on the loop's start.
                note(2, 62, 3840, 480),
                // Crosses the loop's end: released there.
                note(3, 64, 7680 - 480, 960),
                // Starts on the loop's end: doesn't play.
                note(4, 65, 7680, 480),
            ],
        });
        let sequence = Sequence::build(TempoMap::new(120.0), 3840, 3840, vec![clip], 48_000);
        assert_eq!(sequence.loop_samples(), 96_000..192_000);
        assert_eq!(
            sequence.events(),
            [
                on(96_000, 2, 62),
                off(108_000, 2),
                on(180_000, 3, 64),
                off(192_000, 3),
            ]
        );
    }

    #[test]
    fn events_follow_the_tempo_and_sample_rate() {
        let mut project = with_notes(vec![note(1, 60, 960, 960)]);
        project.apply(&Command::SetTempo { bpm: 90.0 }).unwrap();
        // At 90 BPM a quarter note is 2/3 s: 29,400 samples at 44.1 kHz.
        let snapshot = Snapshot::new(&project, 44_100);
        assert_eq!(
            snapshot.sequence.events(),
            [on(29_400, 1, 60), off(58_800, 1)]
        );
        assert_eq!(snapshot.sequence.loop_samples(), 0..16 * 29_400);
    }

    #[test]
    fn retiming_shares_the_clips_and_matches_a_fresh_build() {
        let project = with_notes(vec![note(1, 60, 100, 700), note(2, 72, 5000, 3)]);
        let at_48k = Snapshot::new(&project, 48_000);
        let at_96k = at_48k.at_sample_rate(96_000);
        assert_eq!(at_96k, Snapshot::new(&project, 96_000));
        assert!(Arc::ptr_eq(
            &at_48k.sequence.clips()[0],
            &at_96k.sequence.clips()[0]
        ));
        // Retiming to the same rate shares the events too.
        let same = at_48k.at_sample_rate(48_000);
        assert!(Arc::ptr_eq(&at_48k.sequence.events, &same.sequence.events));
    }

    #[test]
    fn positions_convert_between_ticks_and_samples() {
        let sequence = &Snapshot::default().sequence;
        assert_eq!(sequence.sample_at(960), 24_000);
        assert_eq!(sequence.ticks_at(24_000), 960);
    }
}
