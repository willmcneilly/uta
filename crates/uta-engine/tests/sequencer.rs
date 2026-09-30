//! Playing the project's notes in a loop: every note on its exact sample,
//! whatever the tempo and block size, and the loop point handled without
//! doubled, cut-short or stuck notes. See RFC-002, "The shared model", point 5.
//!
//! Each note is compared sample for sample with the same note started
//! directly on a fresh engine, so an event a single sample early or late
//! fails.

mod common;

use common::*;
use uta_core::time::{TempoMap, Ticks};
use uta_core::{Command, Project};
use uta_engine::offline::Renderer;
use uta_engine::{EngineConfig, MAX_NOTE_EVENTS_PER_BLOCK, NoteKey, Snapshot};

const BLOCK_SIZES: [usize; 3] = [32, 128, 1024];
/// A bar of 4/4, in ticks.
const BAR: Ticks = 3840;

fn config(sample_rate: u32) -> EngineConfig {
    EngineConfig {
        sample_rate,
        channels: 1,
    }
}

/// The note `project` would play, started directly at sample 0 on a fresh,
/// stopped engine with the same settings: held for `hold` samples, then
/// released and rendered for `tail` more.
fn reference(project: &Project, sample_rate: u32, pitch: u8, hold: usize, tail: usize) -> Vec<f32> {
    let mut renderer = Renderer::new(config(sample_rate), Snapshot::from(project), 128);
    renderer
        .controller
        .note_on(0, NoteKey(u128::MAX), pitch, 100)
        .unwrap();
    renderer.render(hold);
    renderer.controller.note_off(0, NoteKey(u128::MAX)).unwrap();
    renderer.render(tail);
    renderer.into_samples()
}

/// Plays `project` from the start of its loop for `frames`.
fn play(project: &Project, sample_rate: u32, block_size: usize, frames: usize) -> Vec<f32> {
    let mut renderer = Renderer::new(config(sample_rate), Snapshot::from(project), block_size);
    renderer.controller.play().unwrap();
    renderer.render(frames);
    renderer.into_samples()
}

/// Asserts `samples` holds `expected` from `at` on, exactly.
fn assert_plays_at(samples: &[f32], at: usize, expected: &[f32], what: &str) {
    let difference = max_difference(&samples[at..at + expected.len()], expected);
    assert!(
        difference == 0.0,
        "{what}: differs from the reference by {difference} from sample {at}"
    );
}

#[test]
fn a_note_starts_on_its_exact_sample() {
    // An odd tick, so it lands between beats and rarely on a block boundary.
    const TICK: Ticks = 1234;
    for sample_rate in [44_100, 48_000, 96_000] {
        for bpm in [60.0, 97.0, 120.0, 174.2, 300.0] {
            let project = project(bpm, 1, &PLAIN_SINE, vec![note(0, 69, TICK, 480)]);
            let start = TempoMap::new(bpm).ticks_to_samples(TICK, sample_rate) as usize;
            let expected = reference(&project, sample_rate, 69, 2000, 0);
            for block_size in BLOCK_SIZES {
                let samples = play(&project, sample_rate, block_size, start + 2000);
                let what = format!("{bpm} BPM, {sample_rate} Hz, blocks of {block_size}");
                assert_eq!(first_sound(&samples), Some(start + 1), "{what}");
                assert_plays_at(&samples, start, &expected, &what);
            }
        }
    }
    // Checked by hand: at 120 BPM a quarter note is half a second.
    assert_eq!(TempoMap::new(120.0).ticks_to_samples(960, 48_000), 24_000);
}

/// A 1-bar loop at 120 BPM: 96,000 samples at 48 kHz.
const LOOP: usize = 96_000;
/// The last eighth note of the bar, in ticks and samples.
const LAST_EIGHTH: Ticks = BAR - 480;
const LAST_EIGHTH_SAMPLE: usize = LOOP - 12_000;
/// Enough for the 1 ms release to finish.
const TAIL: usize = 100;

/// Checks every pass of a render of the note that ends at the loop's end:
/// silent until the note starts, then exactly the note held to the loop's
/// end and released there.
fn assert_last_eighth_plays_once_per_pass(samples: &[f32], passes: usize, what: &str) {
    let project = project(120.0, 1, &PLAIN_SINE, vec![]);
    let expected = reference(&project, 48_000, 60, LOOP - LAST_EIGHTH_SAMPLE, TAIL);
    for pass in 0..passes {
        let start = pass * LOOP;
        // Silent from once the last pass's release is over until the note.
        assert_eq!(
            first_sound(&samples[start + TAIL..start + LAST_EIGHTH_SAMPLE]),
            None,
            "{what}: sound before the note in pass {}",
            pass + 1
        );
        assert_plays_at(
            samples,
            start + LAST_EIGHTH_SAMPLE,
            &expected,
            &format!("{what}, pass {}", pass + 1),
        );
    }
}

#[test]
fn a_note_at_the_loops_end_plays_once_per_pass_and_is_never_cut_short() {
    let project = project(120.0, 1, &PLAIN_SINE, vec![note(0, 60, LAST_EIGHTH, 480)]);
    for block_size in BLOCK_SIZES {
        let samples = play(&project, 48_000, block_size, 3 * LOOP + TAIL);
        assert_last_eighth_plays_once_per_pass(&samples, 3, &format!("blocks of {block_size}"));
    }
}

#[test]
fn a_note_crossing_the_loops_end_is_released_there() {
    // A quarter note starting on the last eighth: it would end half an
    // eighth into the next pass, but is released at the loop's end.
    let project = project(120.0, 1, &PLAIN_SINE, vec![note(0, 60, LAST_EIGHTH, 960)]);
    for block_size in BLOCK_SIZES {
        let samples = play(&project, 48_000, block_size, 3 * LOOP + TAIL);
        assert_last_eighth_plays_once_per_pass(&samples, 3, &format!("blocks of {block_size}"));
    }
}

#[test]
fn pass_1000_starts_on_the_exact_expected_sample() {
    // At 293 BPM a bar is 39,317.4 samples, rounded once to 39,317: the loop
    // is a whole number of samples, so passes never drift from it.
    let bpm = 293.0;
    let project = project(bpm, 1, &PLAIN_SINE, vec![note(0, 69, 0, 240)]);
    let loop_samples = TempoMap::new(bpm).ticks_to_samples(BAR, 48_000) as usize;
    assert_eq!(loop_samples, 39_317);
    let expected = reference(&project, 48_000, 69, 1000, 0);

    let pass_1000 = 999 * loop_samples;
    let mut renderer = Renderer::new(config(48_000), Snapshot::from(&project), 128);
    renderer.controller.play().unwrap();
    renderer.skip(pass_1000 - 1000);
    renderer.render(2000);
    let samples = renderer.samples();
    assert_eq!(
        first_sound(&samples[..1000]),
        None,
        "sound before pass 1000"
    );
    assert_plays_at(samples, 1000, &expected, "pass 1000");
}

#[test]
fn notes_in_the_loop_play_from_the_start_again_each_pass() {
    // Two notes and a chord: every pass renders exactly the same.
    let notes = vec![
        note(0, 48, 0, 960),
        note(1, 55, 960, 480),
        note(2, 60, 1920, 1920),
        note(3, 64, 1920, 1920),
    ];
    let project = project(120.0, 1, &PLAIN_SINE, notes);
    let samples = play(&project, 48_000, 128, 3 * LOOP);
    // Skip the first TAIL samples of each pass, where the last pass's
    // chord is still releasing (in the first pass there's nothing to
    // release).
    let pass = |n: usize| &samples[n * LOOP + TAIL..(n + 1) * LOOP];
    assert_eq!(max_difference(pass(0), pass(1)), 0.0);
    assert_eq!(max_difference(pass(1), pass(2)), 0.0);
}

#[test]
fn the_status_reports_the_playhead_in_ticks() {
    let project = project(120.0, 1, &PLAIN_SINE, vec![]);
    let mut renderer = Renderer::new(config(48_000), Snapshot::from(&project), 128);
    assert_eq!(renderer.controller.poll().playhead, 0);
    renderer.controller.play().unwrap();
    // A beat and a half: 36,000 samples, 1,440 ticks.
    renderer.render(36_000);
    assert_eq!(renderer.controller.poll().playhead, 1440);
    // Round the loop and an eighth note on.
    renderer.render(LOOP - 36_000 + 12_000);
    assert_eq!(renderer.controller.poll().playhead, 480);
    // Exactly at the loop's end, it reports the loop's start.
    renderer.render(LOOP - 12_000);
    assert_eq!(renderer.controller.poll().playhead, 0);
}

#[test]
fn stop_releases_the_notes_and_play_carries_on_from_the_playhead() {
    let project = project(120.0, 1, &PLAIN_SINE, vec![note(0, 60, 0, BAR)]);
    let mut renderer = Renderer::new(config(48_000), Snapshot::from(&project), 128);
    renderer.controller.play().unwrap();
    renderer.render(24_000);
    renderer.controller.stop().unwrap();
    renderer.render(24_000);
    let status = renderer.controller.poll();
    assert!(!status.playing);
    assert_eq!(status.playhead, 960, "the playhead stops where it is");
    // Released: silent once the 1 ms release is over.
    assert_eq!(first_sound(&renderer.samples()[24_000 + TAIL..]), None);

    // Play carries on from there. The note started before the playhead, so
    // it waits for the next pass.
    renderer.controller.play().unwrap();
    renderer.render(LOOP - 24_000 + 1000);
    let status = renderer.controller.poll();
    assert_eq!(status.playhead, 40, "1000 samples into the next pass");
    let samples = &renderer.samples()[48_000..];
    assert_eq!(first_sound(samples), Some(LOOP - 24_000 + 1));
}

#[test]
fn too_many_events_in_one_block_are_skipped_and_counted() {
    // More notes starting on the same sample than one block may handle.
    let extra = 88;
    let count = MAX_NOTE_EVENTS_PER_BLOCK + extra;
    let notes = (0..count as u128)
        .map(|i| note(i, 40 + (i % 60) as u8, 0, 480))
        .collect();
    let project = project(120.0, 1, &PLAIN_SINE, notes);
    let mut renderer = Renderer::new(config(48_000), Snapshot::from(&project), 128);
    renderer.controller.play().unwrap();
    renderer.render(128);
    assert_eq!(renderer.controller.poll().dropped_note_events, extra as u64);
    // The ends all fall in one block too.
    renderer.render(12_000);
    assert_eq!(
        renderer.controller.poll().dropped_note_events,
        2 * extra as u64
    );
}

#[test]
fn the_loop_follows_changes_to_the_project() {
    // An edit arrives as a new snapshot; the next pass plays it.
    let mut project = project(120.0, 1, &PLAIN_SINE, vec![]);
    let mut renderer = Renderer::new(config(48_000), Snapshot::from(&project), 128);
    renderer.controller.play().unwrap();
    renderer.render(LOOP / 2);
    assert_eq!(first_sound(renderer.samples()), None);

    let clip = project.tracks()[0].clips()[0].id();
    project
        .apply(&Command::AddNotes {
            clip,
            notes: vec![note(0, 69, 480, 480)],
        })
        .unwrap();
    renderer
        .controller
        .set_snapshot(Snapshot::from(&project))
        .unwrap();
    renderer.render(LOOP / 2 + 20_000);
    let expected = reference(&project, 48_000, 69, 1000, 0);
    assert_eq!(first_sound(renderer.samples()), Some(LOOP + 12_001));
    assert_plays_at(renderer.samples(), LOOP + 12_000, &expected, "the new note");
}
