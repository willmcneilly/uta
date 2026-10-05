//! More note events due on one sample than a block may handle, as at a clip
//! boundary in Make a song's manual check 7: about 1,600 note ends and 200
//! starts on one track. Past the budget the engine catches up: it releases
//! every note that has ended and starts the ones that should be sounding, as
//! far as the voices go. See UTA-32.

mod common;

use common::*;
use uta_core::time::Ticks;
use uta_core::{Clip, ClipId, Command, Note, NoteId, PlacedClip, Project, ProjectId};
use uta_engine::offline::Renderer;
use uta_engine::{EngineConfig, MAX_NOTE_EVENTS_PER_BLOCK, Snapshot, VOICES};
use uuid::Uuid;

const BLOCK_SIZES: [usize; 3] = [32, 128, 1024];
/// A bar of 4/4, in ticks.
const BAR: Ticks = 3840;
/// A bar at 120 BPM and 48 kHz, in samples: where the second clip starts.
const BOUNDARY: usize = 96_000;
/// A sixteenth at 120 BPM and 48 kHz, in samples.
const SIXTEENTH: usize = 6_000;
/// Long enough for the 1 ms release and the 5 ms take-over fade to finish.
const SETTLE: usize = 500;

fn config() -> EngineConfig {
    EngineConfig {
        sample_rate: 48_000,
        channels: 1,
    }
}

/// A song at 120 BPM with the loop off: one track of plain sines, with a
/// one-bar clip of check 7's 3,000 stress notes, then a one-bar clip of
/// `second` (none if empty).
fn dense_song(second: Vec<Note>) -> Project {
    let mut project = Project::with_id(ProjectId::from_uuid(Uuid::from_u128(1)));
    let track = project.tracks()[0].id();
    let first = project.tracks()[0].clips()[0].id();
    let mut commands = vec![
        Command::RemoveClips { clips: vec![first] },
        Command::SetLoopEnabled { enabled: false },
    ];
    commands.extend(
        PLAIN_SINE
            .iter()
            .map(|&param| Command::SetSynthParam { track, param }),
    );
    let mut clips = vec![PlacedClip {
        track,
        clip: Clip::new(ClipId::from_uuid(Uuid::from_u128(10)), 0, BAR).with_notes(stress_notes()),
    }];
    if !second.is_empty() {
        clips.push(PlacedClip {
            track,
            clip: Clip::new(ClipId::from_uuid(Uuid::from_u128(11)), BAR, BAR).with_notes(second),
        });
    }
    commands.push(Command::AddClips { clips });
    for command in &commands {
        project.apply(command).expect("a valid test song");
    }
    project
}

/// The stress notes again, with new IDs, for the second clip.
fn stress_again() -> Vec<Note> {
    stress_notes()
        .into_iter()
        .map(|note| Note {
            id: NoteId::random(),
            ..note
        })
        .collect()
}

/// The RMS of each `window`-sample stretch of `samples`.
fn window_rms(samples: &[f32], window: usize) -> Vec<f64> {
    samples.chunks_exact(window).map(rms).collect()
}

/// The test song really is past the budget at the boundary: more note ends
/// fall there than one block may handle, and the second clip's starts come
/// after them.
#[test]
fn the_boundary_has_more_events_than_a_block_may_handle() {
    let ends = stress_notes()
        .iter()
        .filter(|note| note.start + note.length >= BAR)
        .count();
    assert!(
        ends > MAX_NOTE_EVENTS_PER_BLOCK,
        "{ends} notes end at the boundary"
    );
}

/// The bug UTA-32 fixes: a snapshot swap every block across a dense
/// boundary used to leave the track near silent for a sixteenth, because the
/// boundary's starts were skipped and the swap released the notes the
/// skipped ends had left sounding. Now it sounds the same as without swaps.
#[test]
fn a_dense_boundary_sounds_the_same_with_a_swap_every_block() {
    let project = dense_song(stress_again());
    for block_size in BLOCK_SIZES {
        let session = |swapping: bool| {
            let mut renderer = Renderer::new(config(), Snapshot::from(&project), block_size);
            renderer.controller.play().unwrap();
            let swaps_from = BOUNDARY - 2048;
            renderer.render(swaps_from);
            if swapping {
                while renderer.samples().len() < BOUNDARY + SIXTEENTH {
                    renderer
                        .controller
                        .set_snapshot(Snapshot::from(&project))
                        .unwrap();
                    renderer.render(block_size);
                }
            } else {
                renderer.render(BOUNDARY + SIXTEENTH - swaps_from);
            }
            renderer.into_samples()
        };
        let plain = session(false);
        let swapped = session(true);
        let after = BOUNDARY..BOUNDARY + SIXTEENTH;
        let plain = window_rms(&plain[after.clone()], 500);
        let swapped = window_rms(&swapped[after], 500);
        // Near silence is well under the sixteenth's own level. The first
        // window is lower than the rest: the new notes take over voices
        // that are still releasing, which fade out first.
        let level = plain.iter().sum::<f64>() / plain.len() as f64;
        for (window, (&plain, &swapped)) in plain.iter().zip(&swapped).enumerate() {
            assert!(
                plain > level / 3.0 && swapped > level / 3.0,
                "blocks of {block_size}, window {window}: RMS {plain:.3} plain, \
                 {swapped:.3} swapped, against {level:.3} for the sixteenth"
            );
            assert!(
                (plain - swapped).abs() <= 0.05 * plain,
                "blocks of {block_size}, window {window}: RMS {plain:.3} plain, \
                 {swapped:.3} swapped"
            );
        }
    }
}

/// Every note in the first clip ends by the boundary. With nothing after
/// it, the track falls silent there: no note whose end was past the budget
/// is left sounding.
#[test]
fn no_note_sounds_past_its_end_at_a_dense_boundary() {
    let project = dense_song(vec![]);
    for block_size in BLOCK_SIZES {
        let mut renderer = Renderer::new(config(), Snapshot::from(&project), block_size);
        renderer.controller.play().unwrap();
        renderer.render(BOUNDARY + SIXTEENTH);
        let samples = renderer.samples();
        assert!(rms(&samples[BOUNDARY - 1000..BOUNDARY]) > 0.03, "it plays");
        assert_eq!(
            first_sound(&samples[BOUNDARY + SETTLE..]),
            None,
            "blocks of {block_size}: silent once the releases end"
        );
    }
}

/// A note starting after more ends than a block may handle, on the same
/// sample, still plays, and nothing is counted as dropped: every note that
/// should sound has a voice.
#[test]
fn a_note_starting_at_a_dense_boundary_plays() {
    let project = dense_song(vec![note(0, 69, 0, BAR)]);
    for block_size in BLOCK_SIZES {
        let mut renderer = Renderer::new(config(), Snapshot::from(&project), block_size);
        renderer.controller.play().unwrap();
        renderer.render(BOUNDARY + SIXTEENTH);
        let after = &renderer.samples()[BOUNDARY + SETTLE..];
        let frequency = measure_frequency(after, 48_000);
        assert!(
            (frequency - 440.0).abs() < 1.0,
            "blocks of {block_size}: {frequency:.2} Hz"
        );
        assert_eq!(renderer.take_status().dropped_note_events, 0);
    }
}

/// When more notes start at a dense boundary than the voices can hold, only
/// those are counted as dropped, not the ends and starts caught up.
#[test]
fn only_notes_the_voices_cannot_hold_are_counted() {
    let starting = 2 * VOICES as u128;
    let second = (0..starting)
        .map(|i| note(i, 40 + i as u8, 0, BAR))
        .collect();
    let project = dense_song(second);
    let mut renderer = Renderer::new(config(), Snapshot::from(&project), 128);
    renderer.controller.play().unwrap();
    renderer.render(BOUNDARY - 128);
    let before = renderer.take_status().dropped_note_events;
    renderer.render(128 + SIXTEENTH / 2);
    let dropped = renderer.take_status().dropped_note_events - before;
    assert_eq!(dropped, starting as u64 - VOICES as u64);
}

/// A note already under way that never got a voice, because more notes
/// started with it than a block may handle, starts when the track next
/// catches up: here, where the others all end.
#[test]
fn a_note_under_way_without_a_voice_starts_when_the_track_catches_up() {
    let crowd = MAX_NOTE_EVENTS_PER_BLOCK as u128 + 88;
    let mut notes: Vec<Note> = (0..crowd)
        .map(|i| note(i, 40 + (i % 20) as u8, 0, 480))
        .collect();
    // The last to start, so past the budget: dropped at the start.
    notes.push(note(crowd, 69, 0, 1920));
    let project = project(120.0, 1, &PLAIN_SINE, notes);
    for block_size in BLOCK_SIZES {
        let mut renderer = Renderer::new(config(), Snapshot::from(&project), block_size);
        renderer.controller.play().unwrap();
        renderer.render(12_000 + SIXTEENTH);
        let after = &renderer.samples()[12_000 + SETTLE..];
        let frequency = measure_frequency(after, 48_000);
        assert!(
            (frequency - 440.0).abs() < 1.0,
            "blocks of {block_size}: {frequency:.2} Hz"
        );
    }
}
