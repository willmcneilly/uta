//! Playing a song: from the play start to the song's end with the loop off,
//! or round the loop region with it on; Stop going back to the play start;
//! jumps; and notes already under way starting at once ("note chasing"). See
//! RFC-003, "Playing a song".
//!
//! As in the sequencer tests, each note is compared sample for sample with
//! the same note started directly on a fresh engine, so a note a single
//! sample early or late fails.

mod common;

use common::*;
use uta_core::time::Ticks;
use uta_core::{ClipPosition, Command, Note, Project};
use uta_engine::offline::Renderer;
use uta_engine::{EngineConfig, MAX_NOTE_EVENTS_PER_BLOCK, NoteKey, Snapshot, pitch_to_hz};

const BLOCK_SIZES: [usize; 3] = [32, 128, 1024];
/// A bar of 4/4, in ticks.
const BAR: Ticks = 3840;
/// A bar at 120 BPM and 48 kHz, in samples.
const BAR_SAMPLES: usize = 96_000;
/// Samples per tick at 120 BPM and 48 kHz.
const PER_TICK: usize = 25;
/// Enough for the 1 ms release to finish.
const TAIL: usize = 100;

fn config() -> EngineConfig {
    EngineConfig {
        sample_rate: 48_000,
        channels: 1,
    }
}

/// A song at 120 BPM: one track whose clip is `clip_bars` long and holds
/// `notes`, as plain sines, so the song ends a bar after the clip. The loop
/// region is `looped` (start bar from 0, and length), switched on, or off
/// with `None`.
fn song(clip_bars: u32, looped: Option<(u32, u32)>, notes: Vec<Note>) -> Project {
    let mut project = synth_project();
    let track = project.tracks()[0].id();
    let clip = project.tracks()[0].clips()[0].id();
    let mut commands = vec![Command::SetClips {
        clips: vec![ClipPosition {
            id: clip,
            track,
            start: 0,
            length: Ticks::from(clip_bars) * BAR,
        }],
    }];
    commands.extend(
        PLAIN_SINE
            .iter()
            .map(|&param| Command::SetSynthParam { track, param }),
    );
    match looped {
        Some((start_bar, bars)) => commands.push(Command::SetLoop { start_bar, bars }),
        None => commands.push(Command::SetLoopEnabled { enabled: false }),
    }
    if !notes.is_empty() {
        commands.push(Command::AddNotes { clip, notes });
    }
    for command in &commands {
        project.apply(command).expect("a valid test song");
    }
    project
}

fn renderer(project: &Project, block_size: usize) -> Renderer {
    Renderer::new(config(), Snapshot::from(project), block_size)
}

/// The note `project` would play, started directly at sample 0 on a fresh,
/// stopped engine: held for `hold` samples, then released and rendered for
/// `tail` more.
fn reference(project: &Project, pitch: u8, hold: usize, tail: usize) -> Vec<f32> {
    let mut renderer = renderer(project, 128);
    renderer
        .controller
        .note_on(0, NoteKey(u128::MAX), pitch, 100)
        .unwrap();
    renderer.render(hold);
    renderer.controller.note_off(0, NoteKey(u128::MAX)).unwrap();
    renderer.render(tail);
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

fn samples_at(ticks: Ticks) -> usize {
    ticks as usize * PER_TICK
}

#[test]
fn with_the_loop_off_playback_stops_at_the_songs_end_and_goes_back_to_the_play_start() {
    // A 2-bar clip, so the song ends at bar 3. A note in the clip's last
    // beat, so the song carries on for a bar after the last note.
    let project = song(2, None, vec![note(0, 69, 2 * BAR - 960, 960)]);
    let song_end = 3 * BAR_SAMPLES;
    for block_size in BLOCK_SIZES {
        let what = format!("blocks of {block_size}");
        let mut renderer = renderer(&project, block_size);
        renderer.controller.play().unwrap();
        renderer.render(song_end + 1000);
        let status = renderer.controller.poll();
        assert!(!status.playing, "{what}: stopped at the song's end");
        assert_eq!(status.playhead, 0, "{what}: back at the play start");
        assert_eq!(
            status.position, song_end as u64,
            "{what}: played up to the song's end, to the sample"
        );
        let note_at = samples_at(2 * BAR - 960);
        assert_eq!(first_sound(renderer.samples()), Some(note_at + 1), "{what}");
        let expected = reference(&project, 69, samples_at(960), TAIL);
        assert_plays_at(renderer.samples(), note_at, &expected, &what);
        assert_eq!(
            first_sound(&renderer.samples()[note_at + expected.len()..]),
            None
        );

        // From a play start at bar 2, it plays the rest, stops at the end
        // and goes back to bar 2.
        renderer.controller.locate(BAR).unwrap();
        renderer.controller.play().unwrap();
        renderer.render(song_end);
        let status = renderer.controller.poll();
        assert!(!status.playing, "{what}");
        assert_eq!(status.playhead, BAR, "{what}: back at bar 2");
        assert_eq!(
            status.position,
            (song_end + song_end - BAR_SAMPLES) as u64,
            "{what}: played bar 2 to the end"
        );
    }
}

#[test]
fn stop_goes_back_to_where_play_was_last_pressed() {
    let project = song(4, None, vec![]);
    let mut renderer = renderer(&project, 128);
    // Moving the play start while stopped moves the playhead there.
    renderer.controller.locate(BAR + 960).unwrap();
    renderer.render(128);
    assert_eq!(renderer.controller.poll().playhead, BAR + 960);

    renderer.controller.play().unwrap();
    renderer.render(BAR_SAMPLES / 2);
    assert_eq!(renderer.controller.poll().playhead, BAR + 960 + BAR / 2);
    // A jump while playing doesn't move where Stop goes back to.
    renderer.controller.locate(3 * BAR).unwrap();
    renderer.render(PER_TICK * 40);
    let status = renderer.controller.poll();
    assert!(status.playing);
    assert_eq!(status.playhead, 3 * BAR + 40);
    assert_eq!(status.play_start, BAR + 960, "reported as it was");
    renderer.controller.stop().unwrap();
    renderer.render(128);
    let status = renderer.controller.poll();
    assert!(!status.playing);
    assert_eq!(status.playhead, BAR + 960, "back where Play was pressed");

    // And Play starts from there again.
    renderer.controller.play().unwrap();
    renderer.render(PER_TICK * 10);
    assert_eq!(renderer.controller.poll().playhead, BAR + 960 + 10);
}

/// Pause stops where the playhead is, and Continue carries on from that
/// exact sample: a note after it starts as long after Continue as it was
/// after the pause, whatever the block size. The play start doesn't move.
#[test]
fn continue_carries_on_from_the_exact_sample_where_it_paused() {
    // An odd tick, rarely on a block boundary.
    const TICK: Ticks = 2 * BAR + 1234;
    let project = song(4, None, vec![note(0, 69, TICK, 480)]);
    let expected = reference(&project, 69, samples_at(480), TAIL);
    for block_size in BLOCK_SIZES {
        let what = format!("blocks of {block_size}");
        let mut renderer = renderer(&project, block_size);
        renderer.controller.locate(BAR).unwrap();
        renderer.controller.play().unwrap();
        // Some way into the silence before the note, not on a tick.
        renderer.render(block_size * 7);
        let paused_at = samples_at(BAR) + renderer.samples().len();
        renderer.controller.pause().unwrap();
        renderer.render(BAR_SAMPLES / 4);
        let status = renderer.controller.poll();
        assert!(!status.playing, "{what}: paused");
        // Reported to the nearest tick.
        let ticks = ((paused_at + PER_TICK / 2) / PER_TICK) as Ticks;
        assert_eq!(status.playhead, ticks, "{what}: the playhead stays put");
        assert_eq!(first_sound(renderer.samples()), None, "{what}");

        let resumed = renderer.samples().len();
        renderer.controller.resume().unwrap();
        renderer.render(samples_at(TICK) - paused_at + expected.len() + 1000);
        let at = resumed + samples_at(TICK) - paused_at;
        let samples = renderer.samples();
        assert_eq!(first_sound(samples), Some(at + 1), "{what}");
        assert_plays_at(samples, at, &expected, &what);

        // Stop still goes back to where Play was pressed.
        renderer.controller.stop().unwrap();
        renderer.render(block_size);
        assert_eq!(renderer.controller.poll().playhead, BAR, "{what}");
    }
}

/// Pausing, then Play, plays from the play start, not from the pause; and
/// Continue after Stop does the same. Moving the play start while paused
/// forgets the pause.
#[test]
fn play_and_stop_ignore_where_it_paused() {
    let project = song(4, None, vec![]);
    let mut renderer = renderer(&project, 128);
    renderer.controller.locate(BAR).unwrap();
    renderer.controller.play().unwrap();
    renderer.render(PER_TICK * 400);
    renderer.controller.pause().unwrap();
    renderer.render(128);
    assert_eq!(renderer.controller.poll().playhead, BAR + 400);

    renderer.controller.play().unwrap();
    renderer.render(PER_TICK * 10);
    assert_eq!(renderer.controller.poll().playhead, BAR + 10, "Play");

    renderer.controller.pause().unwrap();
    renderer.controller.stop().unwrap();
    renderer.controller.resume().unwrap();
    renderer.render(PER_TICK * 10);
    assert_eq!(
        renderer.controller.poll().playhead,
        BAR + 10,
        "Continue after Stop"
    );

    renderer.controller.pause().unwrap();
    renderer.controller.locate(3 * BAR).unwrap();
    renderer.controller.resume().unwrap();
    renderer.render(PER_TICK * 10);
    assert_eq!(
        renderer.controller.poll().playhead,
        3 * BAR + 10,
        "Continue after moving the play start"
    );

    // Pause while stopped, and Continue while playing, do nothing.
    renderer.controller.stop().unwrap();
    renderer.controller.pause().unwrap();
    renderer.render(128);
    assert_eq!(renderer.controller.poll().playhead, 3 * BAR);
    renderer.controller.play().unwrap();
    renderer.render(PER_TICK * 10);
    renderer.controller.resume().unwrap();
    renderer.render(PER_TICK * 10);
    assert_eq!(renderer.controller.poll().playhead, 3 * BAR + 20);
}

/// An edit while paused keeps the playhead where it paused, even one that
/// changes the tempo: it stays on the same tick.
#[test]
fn an_edit_while_paused_keeps_the_playhead_where_it_paused() {
    let mut project = song(4, None, vec![]);
    let mut renderer = renderer(&project, 128);
    renderer.controller.play().unwrap();
    renderer.render(PER_TICK * 1000);
    renderer.controller.pause().unwrap();
    renderer.render(128);
    assert_eq!(renderer.controller.poll().playhead, 1000);

    project
        .apply(&Command::SetLoopEnabled { enabled: true })
        .unwrap();
    renderer.controller.set_project(&project).unwrap();
    renderer.render(128);
    assert_eq!(renderer.controller.poll().playhead, 1000);

    project.apply(&Command::SetTempo { bpm: 100.0 }).unwrap();
    renderer.controller.set_project(&project).unwrap();
    renderer.render(128);
    assert_eq!(renderer.controller.poll().playhead, 1000, "a new tempo");
}

/// Continue in the middle of a note starts it at once, as Play does.
#[test]
fn continuing_in_the_middle_of_a_note_starts_it_at_once() {
    let project = song(4, None, vec![pad()]);
    for block_size in BLOCK_SIZES {
        let what = format!("Continue, blocks of {block_size}");
        let mut renderer = renderer(&project, block_size);
        renderer.controller.locate(3 * BAR).unwrap();
        renderer.controller.play().unwrap();
        renderer.controller.locate(BAR + 1234).unwrap();
        renderer.controller.pause().unwrap();
        renderer.render(block_size * 3);
        assert_eq!(first_sound(renderer.samples()), None, "{what}");
        let resumed = renderer.samples().len();
        renderer.controller.resume().unwrap();
        renderer.render(BAR_SAMPLES * 3 / 2);
        let hold = samples_at(2 * BAR - (BAR + 1234));
        assert_chased(renderer.samples(), &project, resumed, hold, &what);
    }
}

/// A jump while playing lands on the exact sample: a note starting where it
/// lands starts on the first sample of the next block, whatever the block
/// size and wherever the playhead was.
#[test]
fn a_jump_while_playing_lands_on_the_exact_sample() {
    // An odd tick, rarely on a block boundary.
    const TICK: Ticks = 2 * BAR + 1234;
    let project = song(4, None, vec![note(0, 69, TICK, 480)]);
    let expected = reference(&project, 69, samples_at(480), TAIL);
    for block_size in BLOCK_SIZES {
        let what = format!("blocks of {block_size}");
        let mut renderer = renderer(&project, block_size);
        renderer.controller.play().unwrap();
        // Some way into the silence before the note, not on a tick.
        renderer.render(block_size * 7);
        let jump_at = renderer.samples().len();
        renderer.controller.locate(TICK).unwrap();
        renderer.render(expected.len() + 1000);
        let samples = renderer.samples();
        assert_eq!(first_sound(samples), Some(jump_at + 1), "{what}");
        assert_plays_at(samples, jump_at, &expected, &what);
    }
}

/// A 2-bar A3 from bar 1 to bar 3, as a pad would hold, for chasing.
fn pad() -> Note {
    note(0, 57, 0, 2 * BAR)
}

/// Checks that a chased A3 from [`pad`] starts at `at` and plays exactly as
/// a note started there directly, held for `hold` samples, at its pitch,
/// then stops: silent for the half bar after.
fn assert_chased(samples: &[f32], project: &Project, at: usize, hold: usize, what: &str) {
    assert_eq!(
        first_sound(&samples[at..]),
        Some(1),
        "{what}: starts at once"
    );
    let expected = reference(project, 57, hold, TAIL);
    assert_plays_at(samples, at, &expected, what);
    let steady = &samples[at + 2400..at + hold - 2400];
    let frequency = measure_frequency(steady, 48_000);
    assert!(
        (frequency - pitch_to_hz(57)).abs() < 0.01,
        "{what}: {frequency} Hz"
    );
    let after = at + expected.len();
    assert_eq!(
        first_sound(&samples[after..after + BAR_SAMPLES / 2]),
        None,
        "{what}: ends where it would have ended"
    );
}

#[test]
fn starting_in_the_middle_of_a_note_starts_it_at_once() {
    let project = song(4, None, vec![pad()]);
    for block_size in BLOCK_SIZES {
        let mut renderer = renderer(&project, block_size);
        // Bar 2, beat 2: 1.25 bars into the pad.
        let start = BAR + 960;
        renderer.controller.locate(start).unwrap();
        renderer.controller.play().unwrap();
        renderer.render(BAR_SAMPLES * 3 / 2);
        let hold = samples_at(2 * BAR - start);
        let what = format!("Play, blocks of {block_size}");
        assert_chased(renderer.samples(), &project, 0, hold, &what);
    }
}

#[test]
fn jumping_into_the_middle_of_a_note_starts_it_at_once() {
    let project = song(4, None, vec![pad()]);
    for block_size in BLOCK_SIZES {
        let mut renderer = renderer(&project, block_size);
        // Start after the pad, in silence, then jump into its middle.
        renderer.controller.locate(3 * BAR).unwrap();
        renderer.controller.play().unwrap();
        renderer.render(block_size * 5);
        let jump_at = renderer.samples().len();
        assert_eq!(first_sound(renderer.samples()), None);
        let to = BAR + 1234;
        renderer.controller.locate(to).unwrap();
        renderer.render(BAR_SAMPLES * 3 / 2);
        let hold = samples_at(2 * BAR - to);
        let what = format!("jump, blocks of {block_size}");
        assert_chased(renderer.samples(), &project, jump_at, hold, &what);
    }
}

#[test]
fn going_round_the_loop_in_the_middle_of_a_note_starts_it_at_once() {
    // The loop is bar 2 to bar 4; the pad runs from bar 1 into it and ends
    // in its middle, at bar 3.
    let project = song(4, Some((1, 2)), vec![pad()]);
    let loop_end = 3 * BAR_SAMPLES;
    for block_size in BLOCK_SIZES {
        let what = format!("wrap, blocks of {block_size}");
        let mut renderer = renderer(&project, block_size);
        renderer.controller.play().unwrap();
        renderer.render(loop_end + 4 * BAR_SAMPLES);
        let samples = renderer.samples();
        // From the top: it plays into the loop, and the pad plays whole.
        let whole = reference(&project, 57, 2 * BAR_SAMPLES, TAIL);
        assert_plays_at(samples, 0, &whole, &format!("{what}, first pass"));
        assert_eq!(first_sound(&samples[whole.len()..loop_end]), None, "{what}");
        // At the loop's end it goes back to bar 2, in the pad's middle, and
        // the pad starts again at once, ending at bar 3 as before, twice.
        for pass in 0..2 {
            let at = loop_end + pass * 2 * BAR_SAMPLES;
            let what = format!("{what}, pass {}", pass + 2);
            assert_chased(samples, &project, at, BAR_SAMPLES, &what);
        }
    }
}

/// A chased note plays its attack from scratch: it rises from silence
/// without a jump bigger than the tone and its 5 ms attack allow, with the
/// note at full level.
#[test]
fn a_chased_note_starts_from_its_attack_without_a_click() {
    let loud = Note {
        velocity: 127,
        ..pad()
    };
    let project = song(4, None, vec![loud]);
    let snapshot = Snapshot::from(&project).with_volume_db(0.0);
    let mut renderer = Renderer::new(config(), snapshot, 128);
    renderer.controller.locate(BAR + 1234).unwrap();
    renderer.controller.play().unwrap();
    renderer.render(BAR_SAMPLES / 4);
    let samples = renderer.samples();
    let level = uta_engine::VOICE_LEVEL * uta_engine::velocity_to_gain(127);
    let attack = level / (0.005 * 48_000.0);
    let limit = (sine_max_step(pitch_to_hz(57), 48_000) * level + attack) * 1.1;
    let (jump, at) = max_jump(samples);
    assert!(
        jump <= limit,
        "jump of {jump} at sample {at}, limit {limit}"
    );
    // It starts from silence and reaches full level after the attack.
    assert!(samples[1].abs() < attack * 2.0);
    assert!(level_at(samples, 4800, 480) > 0.99 * level);
}

/// An edit that moves a note under the playhead doesn't start it: it waits
/// for its next pass. See RFC-003, "Playing a song".
#[test]
fn a_note_moved_under_the_playhead_waits_for_its_next_pass() {
    // A 1-bar loop, empty at first.
    let mut project = song(1, Some((0, 1)), vec![]);
    let mut renderer = renderer(&project, 128);
    renderer.controller.play().unwrap();
    renderer.render(BAR_SAMPLES / 2);

    // A note over the playhead, from beat 1 to beat 4.
    let clip = project.tracks()[0].clips()[0].id();
    project
        .apply(&Command::AddNotes {
            clip,
            notes: vec![note(0, 57, 960, 1920)],
        })
        .unwrap();
    renderer.controller.set_project(&project).unwrap();
    renderer.render(BAR_SAMPLES * 3 / 2);
    let start = BAR_SAMPLES + samples_at(960);
    assert_eq!(
        first_sound(renderer.samples()),
        Some(start + 1),
        "silent until the note's start on the next pass"
    );
    let expected = reference(&project, 57, samples_at(1920), TAIL);
    assert_plays_at(renderer.samples(), start, &expected, "the moved note");
}

#[test]
fn with_the_loop_on_starting_before_it_plays_into_it_and_goes_round() {
    // A note at the top, and one at the start of the loop, bar 3 to bar 4.
    let project = song(
        4,
        Some((2, 1)),
        vec![note(0, 69, 0, 480), note(1, 69, 2 * BAR, 480)],
    );
    let mut renderer = renderer(&project, 128);
    renderer.controller.play().unwrap();
    renderer.render(6 * BAR_SAMPLES);
    let samples = renderer.samples();
    let expected = reference(&project, 69, samples_at(480), TAIL);
    // The top once, then the loop's note on every pass round bar 3.
    let mut onsets = vec![0];
    onsets.extend((0..4).map(|pass| (2 + pass) * BAR_SAMPLES));
    for &at in &onsets {
        assert_plays_at(samples, at, &expected, &format!("note at {at}"));
    }
    // Nothing else: silent between them.
    for pair in onsets.windows(2) {
        let quiet = &samples[pair[0] + expected.len()..pair[1]];
        assert_eq!(
            first_sound(quiet),
            None,
            "between {} and {}",
            pair[0],
            pair[1]
        );
    }
    assert!(renderer.controller.poll().playing, "round the loop forever");
}

#[test]
fn with_the_loop_on_starting_after_it_plays_on_to_the_songs_end() {
    // The loop is bar 1; the song ends at bar 5.
    let project = song(4, Some((0, 1)), vec![note(0, 69, 3 * BAR, 480)]);
    let mut renderer = renderer(&project, 128);
    renderer.controller.locate(2 * BAR).unwrap();
    renderer.controller.play().unwrap();
    renderer.render(4 * BAR_SAMPLES);
    let status = renderer.controller.poll();
    assert!(!status.playing, "stopped at the song's end");
    assert_eq!(status.playhead, 2 * BAR, "back at the play start");
    assert_eq!(status.position, 3 * BAR_SAMPLES as u64, "bar 3 to bar 5");
    let expected = reference(&project, 69, samples_at(480), TAIL);
    assert_eq!(first_sound(renderer.samples()), Some(BAR_SAMPLES + 1));
    assert_plays_at(renderer.samples(), BAR_SAMPLES, &expected, "bar 4");
}

/// Switching the loop on while playing before it heads into it; switching it
/// off lets playback carry on to the song's end; and shortening it under the
/// playhead goes round it straight away.
#[test]
fn the_loop_switch_and_region_follow_the_project_while_playing() {
    let mut project = song(4, None, vec![]);
    let mut renderer = renderer(&project, 128);
    renderer.controller.play().unwrap();
    renderer.render(BAR_SAMPLES / 2);

    // On, over bar 1 to bar 2: it goes round at bar 2.
    project
        .apply(&Command::SetLoop {
            start_bar: 0,
            bars: 1,
        })
        .unwrap();
    project
        .apply(&Command::SetLoopEnabled { enabled: true })
        .unwrap();
    renderer.controller.set_project(&project).unwrap();
    renderer.render(BAR_SAMPLES);
    assert_eq!(renderer.controller.poll().playhead, BAR / 2);

    // Off again: on past bar 2, to the song's end at bar 5.
    project
        .apply(&Command::SetLoopEnabled { enabled: false })
        .unwrap();
    renderer.controller.set_project(&project).unwrap();
    renderer.render(BAR_SAMPLES);
    assert_eq!(renderer.controller.poll().playhead, BAR + BAR / 2);
    renderer.render(4 * BAR_SAMPLES);
    assert!(!renderer.controller.poll().playing);

    // Looping over 2 bars, then shortened to 1 bar with the playhead in the
    // second: it goes round at once.
    project
        .apply(&Command::SetLoop {
            start_bar: 0,
            bars: 2,
        })
        .unwrap();
    project
        .apply(&Command::SetLoopEnabled { enabled: true })
        .unwrap();
    renderer.controller.set_project(&project).unwrap();
    renderer.controller.play().unwrap();
    renderer.render(BAR_SAMPLES + 1000);
    project
        .apply(&Command::SetLoop {
            start_bar: 0,
            bars: 1,
        })
        .unwrap();
    renderer.controller.set_project(&project).unwrap();
    renderer.render(PER_TICK * 10);
    let status = renderer.controller.poll();
    assert!(status.playing);
    assert_eq!(status.playhead, 10, "back at the loop's start");
}

/// Chased notes count towards the track's events per block: past the limit
/// they're skipped and counted, like any other note event.
#[test]
fn too_many_chased_notes_are_skipped_and_counted() {
    let extra = 40;
    let count = MAX_NOTE_EVENTS_PER_BLOCK + extra;
    let notes = (0..count as u128)
        .map(|i| note(i, 40 + (i % 60) as u8, 0, 2 * BAR))
        .collect();
    let project = song(4, None, notes);
    let mut renderer = renderer(&project, 128);
    renderer.controller.locate(BAR).unwrap();
    renderer.controller.play().unwrap();
    renderer.render(128);
    assert_eq!(renderer.controller.poll().dropped_note_events, extra as u64);
}

/// Chased notes share the budget with the ordinary events in the same
/// block: 500 chased, and 20 notes starting where playback starts, is 8 over
/// the limit.
#[test]
fn chased_notes_and_events_in_the_same_block_share_the_budget() {
    let chased = 500u128;
    let starting = 20u128;
    let mut notes: Vec<Note> = (0..chased)
        .map(|i| note(i, 40 + (i % 60) as u8, 0, 2 * BAR))
        .collect();
    notes.extend((0..starting).map(|i| note(chased + i, 40 + (i % 60) as u8, BAR, 480)));
    let project = song(4, None, notes);
    let mut renderer = renderer(&project, 128);
    renderer.controller.locate(BAR).unwrap();
    renderer.controller.play().unwrap();
    renderer.render(128);
    let over = (chased + starting) as u64 - MAX_NOTE_EVENTS_PER_BLOCK as u64;
    assert_eq!(renderer.controller.poll().dropped_note_events, over);
}

/// Going round the loop doesn't play the notes that start on the loop's end:
/// they're outside the loop. So they neither sound nor take from the
/// budget, however many there are.
#[test]
fn notes_starting_on_the_loops_end_do_not_play_at_the_wrap() {
    // The loop is bar 1; more notes than a block may handle start on bar 2.
    let count = MAX_NOTE_EVENTS_PER_BLOCK as u128 + 10;
    let notes = (0..count)
        .map(|i| note(i, 40 + (i % 60) as u8, BAR, 480))
        .collect();
    let project = song(2, Some((0, 1)), notes);
    let mut renderer = renderer(&project, 128);
    renderer.controller.play().unwrap();
    renderer.render(3 * BAR_SAMPLES);
    assert_eq!(renderer.controller.poll().dropped_note_events, 0);
    assert_eq!(first_sound(renderer.samples()), None);
}

/// Where a song with the loop off ends, and where with it on it goes round,
/// don't depend on the block size: the same audio in blocks of 32, 128 and
/// 1024, with chasing, a jump and a wrap.
#[test]
fn block_size_does_not_change_song_playback() {
    let project = song(
        4,
        Some((1, 2)),
        vec![
            pad(),
            note(1, 64, BAR + 480, 3000),
            note(2, 67, 3 * BAR, 960),
        ],
    );
    let session = |block_size| {
        let mut renderer = renderer(&project, block_size);
        renderer.controller.locate(480).unwrap();
        renderer.controller.play().unwrap();
        renderer.render(1024 * 300);
        renderer.controller.locate(BAR + 1000).unwrap();
        renderer.render(1024 * 400);
        renderer.controller.stop().unwrap();
        renderer.render(1024 * 10);
        renderer.into_samples()
    };
    let reference = session(32);
    assert!(peak(&reference) > 0.01, "it plays");
    for block_size in [128, 1024] {
        assert_eq!(
            max_difference(&session(block_size), &reference),
            0.0,
            "blocks of {block_size}"
        );
    }
}

#[test]
fn locating_past_the_end_of_time_is_clamped() {
    let project = song(1, None, vec![]);
    let mut renderer = renderer(&project, 128);
    renderer.controller.locate(Ticks::MAX).unwrap();
    renderer.render(128);
    assert_eq!(
        renderer.controller.poll().playhead,
        uta_core::time::MAX_TICKS
    );
    // Play there reaches the song's end at once, and goes back.
    renderer.controller.play().unwrap();
    renderer.render(128);
    assert!(!renderer.controller.poll().playing);
    assert_eq!(first_sound(renderer.samples()), None);
}
