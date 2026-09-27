//! Test helpers: a known project, and random commands to run against it.

use proptest::prelude::*;
use uuid::Uuid;

use crate::{Command, Note, NoteId, Project, ProjectId, SynthParam, Waveform};

/// A project with a fixed ID, so its track and clip IDs are known.
pub fn project() -> Project {
    Project::with_id(ProjectId::from_uuid(Uuid::from_u128(1)))
}

/// One of a small pool of note IDs, so random commands often refer to notes
/// that earlier ones added.
pub fn note_id(index: u128) -> NoteId {
    NoteId::from_uuid(Uuid::from_u128(1000 + index))
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
    (0u128..6).prop_map(note_id)
}

/// Mostly valid notes, with some that must be rejected.
fn any_note() -> impl Strategy<Value = Note> {
    (
        pooled_note_id(),
        prop_oneof![30 => 0u8..=127, 1 => 128u8..=255],
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

/// Any command against [`project`]: mostly valid, some rejected, and now and
/// then aimed at a clip or track that doesn't exist.
pub fn any_command() -> impl Strategy<Value = Command> {
    let project = project();
    let track = project.tracks()[0].id();
    let clip = project.tracks()[0].clips()[0].id();
    let clip = prop_oneof![
        19 => Just(clip),
        1 => Just(crate::ClipId::from_uuid(Uuid::from_u128(2))),
    ];
    let track = prop_oneof![
        19 => Just(track),
        1 => Just(crate::TrackId::from_uuid(Uuid::from_u128(3))),
    ];
    // Mostly one or two notes, now and then none (which is rejected).
    let notes = || {
        prop_oneof![
            1 => Just(Vec::new()),
            12 => prop::collection::vec(any_note(), 1..3),
        ]
    };
    let ids = prop_oneof![
        1 => Just(Vec::new()),
        12 => prop::collection::vec(pooled_note_id(), 1..3),
    ];
    prop_oneof![
        1 => prop_oneof![
            8 => Project::MIN_VOLUME_DB..=Project::MAX_VOLUME_DB,
            1 => prop_oneof![Just(f32::NAN), 7.0f32..1000.0, -1000.0f32..-121.0],
        ]
        .prop_map(|volume_db| Command::SetMasterVolume { volume_db }),
        2 => (clip.clone(), notes()).prop_map(|(clip, notes)| Command::AddNotes { clip, notes }),
        1 => (clip.clone(), ids).prop_map(|(clip, notes)| Command::RemoveNotes { clip, notes }),
        2 => (clip, notes()).prop_map(|(clip, notes)| Command::SetNotes { clip, notes }),
        1 => prop_oneof![8 => 20.0f32..=300.0, 1 => 0.0f32..20.0, 1 => 300.5f32..1000.0]
            .prop_map(|bpm| Command::SetTempo { bpm }),
        1 => (0u32..=20).prop_map(|bars| Command::SetLoopLength { bars }),
        1 => (track, any_synth_param()).prop_map(|(track, param)| Command::SetSynthParam { track, param }),
    ]
}

/// A series of commands, of which the valid ones build up a project with
/// notes in it.
pub fn any_commands(max: usize) -> impl Strategy<Value = Vec<Command>> {
    prop::collection::vec(any_command(), 0..max)
}
