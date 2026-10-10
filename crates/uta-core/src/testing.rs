//! Test helpers: a known project, and random commands to run against it.

use proptest::prelude::*;
use uuid::Uuid;

use crate::{
    Clip, ClipId, ClipPosition, Command, DrumParam, DrumSound, KIT, KitSettings, MixerStrip, Note,
    NoteId, PlacedClip, PlacedTrack, Project, ProjectId, Source, SynthParam, SynthSettings, Track,
    TrackId, Waveform,
};

/// A project with a fixed ID, so its track and clip IDs are known.
pub fn project() -> Project {
    Project::with_id(ProjectId::from_uuid(Uuid::from_u128(1)))
}

/// One of a small pool of note IDs, so random commands often refer to notes
/// that earlier ones added.
pub fn note_id(index: u128) -> NoteId {
    NoteId::from_uuid(Uuid::from_u128(1000 + index))
}

/// One of a small pool of track IDs for tracks commands add. None is in
/// [`project`] at first.
pub fn track_id(index: u128) -> TrackId {
    TrackId::from_uuid(Uuid::from_u128(2000 + index))
}

/// One of a small pool of clip IDs for clips commands add. None is in
/// [`project`] at first.
pub fn clip_id(index: u128) -> ClipId {
    ClipId::from_uuid(Uuid::from_u128(3000 + index))
}

pub fn note(index: u128, pitch: u8, start: u64) -> Note {
    Note {
        id: note_id(index),
        pitch,
        velocity: 100,
        start,
        length: 480,
    }
}

fn pooled_note_id() -> impl Strategy<Value = NoteId> {
    (0u128..10).prop_map(note_id)
}

/// The first track, or one from the pool.
fn any_track_id() -> impl Strategy<Value = TrackId> {
    let first = project().tracks()[0].id();
    prop_oneof![2 => Just(first), 3 => (0u128..6).prop_map(track_id)]
}

/// The first clip, or one from the pool.
fn any_clip_id() -> impl Strategy<Value = ClipId> {
    let first = project().tracks()[0].clips()[0].id();
    prop_oneof![2 => Just(first), 3 => (0u128..6).prop_map(clip_id)]
}

/// A start and length for a clip: mostly valid, some rejected.
fn any_span() -> impl Strategy<Value = (u64, u64)> {
    (
        prop_oneof![30 => 0u64..40_000, 1 => Just(crate::time::MAX_TICKS)],
        prop_oneof![30 => 1u64..20_000, 1 => Just(0u64)],
    )
}

fn any_clip() -> impl Strategy<Value = Clip> {
    (
        (0u128..6).prop_map(clip_id),
        any_span(),
        prop::collection::vec(any_note(), 0..3),
    )
        .prop_map(|(id, (start, length), notes)| Clip::new(id, start, length).with_notes(notes))
}

/// Mostly valid mixer settings, with some that must be rejected.
fn any_mixer() -> impl Strategy<Value = MixerStrip> {
    (
        prop_oneof![
            20 => MixerStrip::MIN_VOLUME_DB..=MixerStrip::MAX_VOLUME_DB,
            1 => prop_oneof![Just(f32::NAN), 6.5f32..100.0, -100.0f32..-60.5],
        ],
        prop_oneof![20 => -1.0f32..=1.0, 1 => prop_oneof![Just(f32::NAN), 1.01f32..2.0]],
        any::<bool>(),
        any::<bool>(),
    )
        .prop_map(|(volume_db, pan, mute, solo)| MixerStrip {
            volume_db,
            pan,
            mute,
            solo,
        })
}

/// Valid synth settings: the defaults with a few settings changed.
fn any_synth_settings() -> impl Strategy<Value = SynthSettings> {
    prop::collection::vec(any_synth_param(), 0..3).prop_map(|params| {
        let mut settings = SynthSettings::default();
        for param in params {
            let _ = settings.set(param);
        }
        settings
    })
}

/// Valid kit settings: the defaults with a few settings changed.
fn any_kit_settings() -> impl Strategy<Value = KitSettings> {
    prop::collection::vec(any_drum_param(), 0..3).prop_map(|params| {
        let mut kit = KitSettings::default();
        for (sound, param) in params {
            let _ = kit.set(sound, param);
        }
        kit
    })
}

/// A synth or, now and then, a drum kit.
fn any_source() -> impl Strategy<Value = Source> {
    prop_oneof![
        3 => any_synth_settings().prop_map(Source::Synth),
        1 => any_kit_settings().prop_map(Source::Drums),
    ]
}

fn any_track() -> impl Strategy<Value = Track> {
    (
        (0u128..6).prop_map(track_id),
        prop_oneof![20 => (1u32..6).prop_map(|n| format!("Track {n}")), 1 => Just(String::new())],
        any_source(),
        any_mixer(),
        prop::collection::vec(any_clip(), 0..2),
    )
        .prop_map(|(id, name, source, mixer, clips)| {
            Track::new(id, name, source)
                .with_mixer(mixer)
                .with_clips(clips)
        })
}

/// Mostly one or two of something, now and then none (which is rejected).
fn one_or_two<T: std::fmt::Debug + Clone>(
    item: impl Strategy<Value = T>,
) -> impl Strategy<Value = Vec<T>> {
    prop_oneof![
        1 => Just(Vec::new()),
        12 => prop::collection::vec(item, 1..3),
    ]
}

/// Mostly valid notes, with some that must be rejected.
fn any_note() -> impl Strategy<Value = Note> {
    (
        pooled_note_id(),
        // The kit's notes often, so drum tracks get notes too.
        prop_oneof![20 => 0u8..=127, 10 => prop::sample::select(KIT.map(|row| row.pitch).to_vec()), 1 => 128u8..=255],
        prop_oneof![30 => 1u8..=127, 1 => Just(0u8), 1 => 128u8..=255],
        // Some past the 4-bar clip's end.
        0u64..20_000,
        prop_oneof![30 => 1u64..4_000, 1 => Just(0u64)],
    )
        .prop_map(|(id, pitch, velocity, start, length)| Note {
            id,
            pitch,
            velocity,
            start,
            length,
        })
}

fn any_synth_param() -> impl Strategy<Value = SynthParam> {
    let waveform = prop_oneof![
        Just(Waveform::Sine),
        Just(Waveform::Triangle),
        Just(Waveform::Saw),
        Just(Waveform::Square),
    ];
    // Ranges a bit wider than the limits, so some are rejected.
    prop_oneof![
        waveform.prop_map(SynthParam::Waveform),
        (0.0f32..22_000.0).prop_map(SynthParam::CutoffHz),
        (-0.1f32..1.1).prop_map(SynthParam::Resonance),
        (0.0f32..11.0).prop_map(SynthParam::AttackSeconds),
        (0.0f32..11.0).prop_map(SynthParam::DecaySeconds),
        (-0.1f32..1.1).prop_map(SynthParam::Sustain),
        (0.0f32..11.0).prop_map(SynthParam::ReleaseSeconds),
        Just(SynthParam::CutoffHz(f32::NAN)),
    ]
}

/// A drum setting for any sound: mostly one the sound has (the kick's,
/// snare's, clap's, hats' and toms'), with values a bit wider than its limits, so
/// some are rejected, and now and then any setting on any sound.
fn any_drum_param() -> impl Strategy<Value = (DrumSound, DrumParam)> {
    let level = || (-70.0f32..10.0).prop_map(DrumParam::LevelDb);
    let kick = prop_oneof![
        (35.0f32..85.0).prop_map(DrumParam::TuneHz),
        (-0.1f32..1.1).prop_map(DrumParam::Tone),
        (0.0f32..1.0).prop_map(DrumParam::DecaySeconds),
        level(),
        Just(DrumParam::TuneHz(f32::NAN)),
    ];
    let snare = prop_oneof![
        (130.0f32..270.0).prop_map(DrumParam::TuneHz),
        (0.0f32..0.45).prop_map(DrumParam::Tone),
        (-0.1f32..1.1).prop_map(DrumParam::Snappy),
        level(),
    ];
    let clap = prop_oneof![
        (600.0f32..2100.0).prop_map(DrumParam::Tone),
        (0.0f32..0.45).prop_map(DrumParam::DecaySeconds),
        level(),
    ];
    let closed_hat = prop_oneof![
        (90.0f32..420.0).prop_map(DrumParam::TuneHz),
        (3500.0f32..13_000.0).prop_map(DrumParam::Tone),
        (0.0f32..0.2).prop_map(DrumParam::DecaySeconds),
        level(),
    ];
    let open_hat = prop_oneof![(0.0f32..0.7).prop_map(DrumParam::DecaySeconds), level(),];
    let tom = |tune: std::ops::Range<f32>| {
        prop_oneof![
            tune.prop_map(DrumParam::TuneHz),
            (0.0f32..0.7).prop_map(DrumParam::DecaySeconds),
            level(),
        ]
    };
    let any = prop_oneof![
        (35.0f32..280.0).prop_map(DrumParam::TuneHz),
        (-0.1f32..1.1).prop_map(DrumParam::Tone),
        (0.0f32..1.0).prop_map(DrumParam::DecaySeconds),
        (-0.1f32..1.1).prop_map(DrumParam::Snappy),
        level(),
    ];
    prop_oneof![
        3 => kick.prop_map(|param| (DrumSound::Kick, param)),
        3 => snare.prop_map(|param| (DrumSound::Snare, param)),
        3 => clap.prop_map(|param| (DrumSound::Clap, param)),
        3 => closed_hat.prop_map(|param| (DrumSound::ClosedHat, param)),
        3 => open_hat.prop_map(|param| (DrumSound::OpenHat, param)),
        3 => tom(75.0..105.0).prop_map(|param| (DrumSound::LowTom, param)),
        3 => tom(160.0..225.0).prop_map(|param| (DrumSound::HighTom, param)),
        1 => (prop::sample::select(KIT.map(|row| row.sound).to_vec()), any),
    ]
}

/// Any command against [`project`], across several tracks and clips:
/// mostly valid, some rejected, and now and then aimed at a clip or track
/// that doesn't exist. Tracks and clips come from small pools, so later
/// commands often find the ones earlier commands added.
pub fn any_command() -> impl Strategy<Value = Command> {
    let clip_position =
        (any_clip_id(), any_track_id(), any_span()).prop_map(|(id, track, (start, length))| {
            ClipPosition {
                id,
                track,
                start,
                length,
            }
        });
    prop_oneof![
        1 => prop_oneof![
            8 => Project::MIN_VOLUME_DB..=Project::MAX_VOLUME_DB,
            1 => prop_oneof![Just(f32::NAN), 7.0f32..1000.0, -1000.0f32..-121.0],
        ]
        .prop_map(|volume_db| Command::SetMasterVolume { volume_db }),
        3 => (any_clip_id(), one_or_two(any_note())).prop_map(|(clip, notes)| Command::AddNotes { clip, notes }),
        1 => (any_clip_id(), one_or_two(pooled_note_id())).prop_map(|(clip, notes)| Command::RemoveNotes { clip, notes }),
        2 => (any_clip_id(), one_or_two(any_note())).prop_map(|(clip, notes)| Command::SetNotes { clip, notes }),
        1 => prop_oneof![8 => 20.0f32..=300.0, 1 => 0.0f32..20.0, 1 => 300.5f32..1000.0]
            .prop_map(|bpm| Command::SetTempo { bpm }),
        1 => (0u32..=20).prop_map(|bars| Command::SetLoopLength { bars }),
        1 => (any_track_id(), any_synth_param()).prop_map(|(track, param)| Command::SetSynthParam { track, param }),
        1 => (any_track_id(), any_drum_param()).prop_map(|(track, (sound, param))| Command::SetDrumParam { track, sound, param }),
        // Mostly at the top, which is always in range.
        4 => one_or_two((prop_oneof![3 => Just(0usize), 1 => 0usize..4], any_track()).prop_map(|(index, track)| PlacedTrack { index, track }))
            .prop_map(|tracks| Command::AddTracks { tracks }),
        1 => one_or_two(any_track_id()).prop_map(|tracks| Command::RemoveTracks { tracks }),
        1 => (any_track_id(), 0usize..5).prop_map(|(track, index)| Command::MoveTrack { track, index }),
        1 => (any_track_id(), any_mixer()).prop_map(|(track, mixer)| Command::SetTrackMixer { track, mixer }),
        2 => one_or_two((any_track_id(), any_clip()).prop_map(|(track, clip)| PlacedClip { track, clip }))
            .prop_map(|clips| Command::AddClips { clips }),
        1 => one_or_two(any_clip_id()).prop_map(|clips| Command::RemoveClips { clips }),
        2 => one_or_two(clip_position).prop_map(|clips| Command::SetClips { clips }),
        1 => (0u32..10, 0u32..20).prop_map(|(start_bar, bars)| Command::SetLoop { start_bar, bars }),
        1 => any::<bool>().prop_map(|enabled| Command::SetLoopEnabled { enabled }),
    ]
}

/// A series of commands, of which the valid ones build up a project with
/// notes in it.
pub fn any_commands(max: usize) -> impl Strategy<Value = Vec<Command>> {
    prop::collection::vec(any_command(), 0..max)
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use proptest::strategy::ValueTree;
    use proptest::test_runner::{Config, TestRunner};

    use super::*;

    /// The random series reach well past the first track and clip: every
    /// kind of command applies at some point, and some projects end up with
    /// several tracks, clips on tracks other than the first, and notes in
    /// clips other than the first.
    #[test]
    fn random_commands_reach_many_tracks_and_clips() {
        let mut runner = TestRunner::new_with_rng(
            Config::default(),
            proptest::test_runner::TestRng::deterministic_rng(
                proptest::test_runner::RngAlgorithm::ChaCha,
            ),
        );
        let first_clip = project().tracks()[0].clips()[0].id();
        let mut applied = HashSet::new();
        let (mut most_tracks, mut most_clips, mut notes_elsewhere) = (0, 0, 0);
        for _ in 0..200 {
            let commands = any_commands(60).new_tree(&mut runner).unwrap().current();
            let mut project = project();
            for command in &commands {
                if project.apply(command).is_ok() {
                    applied.insert(std::mem::discriminant(command));
                }
            }
            let clips: Vec<_> = project.tracks().iter().flat_map(|t| t.clips()).collect();
            most_tracks = most_tracks.max(project.tracks().len());
            most_clips = most_clips.max(clips.len());
            notes_elsewhere += clips
                .iter()
                .filter(|clip| clip.id() != first_clip)
                .map(|clip| clip.notes().len())
                .sum::<usize>();
        }
        assert_eq!(applied.len(), 17, "every kind of command applies");
        assert!(most_tracks >= 5, "at most {most_tracks} tracks");
        assert!(most_clips >= 5, "at most {most_clips} clips");
        assert!(notes_elsewhere > 0);
    }
}
