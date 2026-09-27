//! Editing the loop while it plays: deleting, moving and re-pitching a note
//! as it sounds, changing the tempo and shortening the loop. Nothing sticks,
//! clicks, skips or doubles. See RFC-002, "The shared model", points 1, 6
//! and 7, and "Risks & unknowns".
//!
//! Each edit is made the way the app makes it: a command on the project, then
//! [`uta_engine::Controller::set_project`], which the audio thread swaps in at
//! the start of its next block. The edits are sent between whole blocks, so
//! the swap lands on a known sample.

mod common;

use common::*;
use uta_core::time::{TempoMap, Ticks};
use uta_core::{Command, Note, NoteId, Project, SynthParam, Waveform};
use uta_engine::offline::Renderer;
use uta_engine::{EngineConfig, NoteKey, Snapshot, VOICE_LEVEL, db_to_gain, velocity_to_gain};
use uuid::Uuid;

const RATE: u32 = 48_000;
const BLOCK_SIZES: [usize; 3] = [32, 128, 1024];
const BEAT: Ticks = 960;
const BAR: Ticks = 4 * BEAT;
/// A beat at 120 BPM, in samples.
const BEAT_SAMPLES: usize = 24_000;
/// A bar at 120 BPM, in samples.
const BAR_SAMPLES: usize = 4 * BEAT_SAMPLES;

/// A sine at full sustain, released over 5 ms: the "few milliseconds" a
/// note has to fall silent in once an edit releases it.
const RELEASE_SECONDS: f32 = 0.005;
const RELEASE_SAMPLES: usize = 240;
const SETTINGS: [SynthParam; 3] = [
    SynthParam::Waveform(Waveform::Sine),
    SynthParam::Sustain(1.0),
    SynthParam::ReleaseSeconds(RELEASE_SECONDS),
];

fn config() -> EngineConfig {
    EngineConfig {
        sample_rate: RATE,
        channels: 1,
    }
}

/// A renderer playing `project` from the start of its loop.
fn playing(project: &Project, block_size: usize) -> Renderer {
    let mut renderer = Renderer::new(config(), Snapshot::from(project), block_size);
    renderer.controller.play().unwrap();
    renderer
}

/// Applies `command` to the project and sends the engine the result, as the
/// app does.
fn edit(renderer: &mut Renderer, project: &mut Project, command: Command) {
    project.apply(&command).unwrap();
    renderer.controller.set_project(project).unwrap();
}

/// `at`, rounded down to a whole number of blocks, so an edit sent after
/// rendering that far lands on it exactly.
fn block_start(at: usize, block_size: usize) -> usize {
    at / block_size * block_size
}

/// Where each note starts: the sample before each sound that follows at
/// least 100 samples of silence. A sine note's first sample is exactly zero.
fn onsets(samples: &[f32]) -> Vec<usize> {
    let mut onsets = Vec::new();
    let mut silent = usize::MAX;
    for (i, &sample) in samples.iter().enumerate() {
        if sample == 0.0 {
            silent = silent.saturating_add(1);
        } else {
            if silent >= 100 {
                onsets.push(i.saturating_sub(1));
            }
            silent = 0;
        }
    }
    onsets
}

/// The steepest one sine voice at A4 moves in one sample, at velocity 100 and
/// the default master volume, while its envelope moves as fast as it does
/// here (the release's first, steepest step, or the 5 ms attack), plus 10%.
/// A note cut off, rather than released, jumps by ten times this.
fn click_limit(voices: usize) -> f32 {
    let level = VOICE_LEVEL * velocity_to_gain(100) * db_to_gain(Snapshot::DEFAULT_VOLUME_DB);
    let steepest_release = 1.0 - (0.001f32 / 1.001).powf(1.0 / RELEASE_SAMPLES as f32);
    let envelope = steepest_release.max(1.0 / 240.0);
    voices as f32 * level * (sine_max_step(440.0, RATE) + envelope) * 1.1
}

fn assert_no_clicks(samples: &[f32], voices: usize, what: &str) {
    let limit = click_limit(voices);
    let (jump, at) = max_jump(samples);
    assert!(
        jump <= limit,
        "{what}: jump of {jump} at sample {at}, limit {limit}"
    );
}

fn assert_silent(samples: &[f32], range: std::ops::Range<usize>, what: &str) {
    let loudest = peak(&samples[range.clone()]);
    assert!(
        loudest == 0.0,
        "{what}: sounding at {loudest} between samples {range:?}"
    );
}

fn note_id(index: u128) -> NoteId {
    NoteId::from_uuid(Uuid::from_u128(1000 + index))
}

/// Guards the click limit: one note cut off rather than released goes over
/// it.
#[test]
fn the_click_limit_catches_a_note_cut_off() {
    let project = project(120.0, 1, &SETTINGS, vec![note(0, 69, 0, BEAT)]);
    let mut renderer = playing(&project, 128);
    renderer.render(10_000);
    let mut samples = renderer.into_samples();
    // Cut it off where it's loudest.
    let (at, _) = samples
        .iter()
        .enumerate()
        .max_by(|a, b| a.1.abs().total_cmp(&b.1.abs()))
        .unwrap();
    samples[at + 1..].fill(0.0);
    assert!(max_jump(&samples).0 > 5.0 * click_limit(1));
}

/// One note, A4 from beat 2 to the end of beat 3 of a 1-bar loop, edited
/// halfway through, on beat 3. Each edit either releases it within the
/// release time, then plays what the edit asks for, or (when the note still
/// covers the playhead) leaves it sounding untouched.
#[test]
fn deleting_moving_or_repitching_a_sounding_note_releases_it() {
    let original = note(0, 69, BEAT, 2 * BEAT);
    let moved = |pitch, start, length| Note {
        pitch,
        start,
        length,
        ..original
    };
    struct Case {
        what: &'static str,
        command: fn(uta_core::ClipId, Note) -> Command,
        after: Note,
        /// Where it starts again in this pass, if it does.
        next_onset: Option<usize>,
    }
    let set = |clip, note| Command::SetNotes {
        clip,
        notes: vec![note],
    };
    let cases = [
        Case {
            what: "deleted",
            command: |clip, note| Command::RemoveNotes {
                clip,
                notes: vec![note.id],
            },
            after: original,
            next_onset: None,
        },
        Case {
            what: "re-pitched",
            command: set,
            after: moved(72, BEAT, 2 * BEAT),
            next_onset: None,
        },
        Case {
            what: "moved later, past the playhead",
            command: set,
            after: moved(69, 3 * BEAT, BEAT / 2),
            next_onset: Some(3 * BEAT_SAMPLES),
        },
        Case {
            what: "moved earlier, before the playhead",
            command: set,
            after: moved(69, 0, BEAT),
            next_onset: None,
        },
        Case {
            what: "shortened to end before the playhead",
            command: set,
            after: moved(69, BEAT, BEAT / 2),
            next_onset: None,
        },
    ];
    for block_size in BLOCK_SIZES {
        for case in &cases {
            let what = format!("{}, blocks of {block_size}", case.what);
            let mut project = project(120.0, 1, &SETTINGS, vec![original]);
            let clip = project.tracks()[0].clips()[0].id();
            let mut renderer = playing(&project, block_size);
            let edit_at = block_start(2 * BEAT_SAMPLES + 5_000, block_size);
            renderer.render(edit_at);
            assert!(
                renderer.samples()[edit_at - 50..].iter().any(|&s| s != 0.0),
                "{what}: sounding before the edit"
            );
            edit(
                &mut renderer,
                &mut project,
                (case.command)(clip, case.after),
            );
            // The rest of this pass, and the next.
            renderer.render(2 * BAR_SAMPLES - edit_at);
            let samples = renderer.samples();

            let silent_from = edit_at + RELEASE_SAMPLES + 1;
            let silent_to = case.next_onset.unwrap_or(BAR_SAMPLES);
            assert_silent(samples, silent_from..silent_to, &what);
            let pass_two = onsets(&samples[BAR_SAMPLES..]);
            let expected: Vec<usize> = match case.what {
                "deleted" => vec![],
                _ => vec![
                    usize::try_from(TempoMap::new(120.0).ticks_to_samples(case.after.start, RATE))
                        .unwrap(),
                ],
            };
            assert_eq!(pass_two, expected, "{what}: the next pass");
            if let Some(onset) = case.next_onset {
                let this_pass: Vec<usize> = onsets(samples)
                    .into_iter()
                    .filter(|&at| (edit_at..BAR_SAMPLES).contains(&at))
                    .collect();
                assert_eq!(this_pass, [onset], "{what}: this pass");
            }
            assert_no_clicks(samples, 1, &what);
        }
    }
}

/// A note moved or resized so it still covers the playhead keeps sounding,
/// exactly as if it hadn't been touched, until its new end.
#[test]
fn a_note_that_still_covers_the_playhead_keeps_sounding() {
    // Beat 2 to the end of beat 3, then beat 1 to the end of beat 3: the
    // same end, a new start and length.
    let before = note(0, 69, BEAT, 2 * BEAT);
    let after = Note {
        start: 0,
        length: 3 * BEAT,
        ..before
    };
    let untouched = {
        let project = project(120.0, 1, &SETTINGS, vec![before]);
        let mut renderer = playing(&project, 128);
        renderer.render(BAR_SAMPLES);
        renderer.into_samples()
    };
    for block_size in BLOCK_SIZES {
        let mut project = project(120.0, 1, &SETTINGS, vec![before]);
        let clip = project.tracks()[0].clips()[0].id();
        let mut renderer = playing(&project, block_size);
        renderer.render(block_start(2 * BEAT_SAMPLES + 5_000, block_size));
        edit(
            &mut renderer,
            &mut project,
            Command::SetNotes {
                clip,
                notes: vec![after],
            },
        );
        renderer.render(BAR_SAMPLES - renderer.samples().len());
        assert_eq!(
            max_difference(renderer.samples(), &untouched),
            0.0,
            "blocks of {block_size}"
        );
    }
}

/// A note's velocity changing doesn't cut it short either.
#[test]
fn a_new_velocity_does_not_release_a_note() {
    let before = note(0, 69, 0, 2 * BEAT);
    let mut project = project(120.0, 1, &SETTINGS, vec![before]);
    let clip = project.tracks()[0].clips()[0].id();
    let mut renderer = playing(&project, 128);
    renderer.render(BEAT_SAMPLES);
    edit(
        &mut renderer,
        &mut project,
        Command::SetNotes {
            clip,
            notes: vec![Note {
                velocity: 20,
                ..before
            }],
        },
    );
    renderer.render(BEAT_SAMPLES - 1_000);
    let samples = renderer.samples();
    assert_eq!(onsets(samples), [0]);
    assert!(level_at(samples, 2 * BEAT_SAMPLES - 2_000, 200) > 0.01);
}

/// Stop releases every note, the sequencer's and live ones, and the output
/// is silent afterwards.
#[test]
fn stop_releases_every_note() {
    let chord = (0..4)
        .map(|i| note(i, 60 + 4 * i as u8, 0, 4 * BEAT))
        .collect();
    let project = project(120.0, 1, &SETTINGS, chord);
    for block_size in BLOCK_SIZES {
        let mut renderer = playing(&project, block_size);
        renderer
            .controller
            .note_on(NoteKey(u128::MAX), 81, 100)
            .unwrap();
        let stop_at = block_start(BEAT_SAMPLES, block_size);
        renderer.render(stop_at);
        renderer.controller.stop().unwrap();
        renderer.render(BEAT_SAMPLES);
        let samples = renderer.samples();
        assert!(level_at(samples, stop_at - 100, 100) > 0.0);
        assert_silent(
            samples,
            stop_at + RELEASE_SAMPLES + 1..samples.len(),
            &format!("blocks of {block_size}"),
        );
        assert!(!renderer.controller.poll().playing);
    }
}

/// Quarter notes, each an eighth long, on every beat of a 2-bar loop, each a
/// different pitch.
fn beats() -> Vec<Note> {
    (0..8)
        .map(|i| note(i, 60 + i as u8, u64::from(i as u32) * BEAT, BEAT / 2))
        .collect()
}

/// The onsets a 2-bar loop of [`beats`] should have when the tempo changes
/// from `from` to `to` BPM once `edit_at` samples have played in the first
/// pass, up to `end`: at the old tempo before the edit, then from the first
/// tick not yet reached, at the new tempo, looping as it goes.
fn expected_beats(from: f32, to: f32, edit_at: usize, end: usize) -> Vec<usize> {
    let (old, new) = (TempoMap::new(from), TempoMap::new(to));
    let at = |map: &TempoMap, ticks| usize::try_from(map.ticks_to_samples(ticks, RATE)).unwrap();
    let starts = || (0..8u64).map(|i| i * BEAT);
    let mut expected: Vec<usize> = starts()
        .map(|tick| at(&old, tick))
        .filter(|&sample| sample < edit_at)
        .collect();
    // The first tick the old tempo hadn't reached.
    let tick = (0..).find(|&tick| at(&old, tick) >= edit_at).unwrap();
    let playhead = at(&new, tick);
    let loop_samples = at(&new, 2 * BAR);
    let mut pass_start = edit_at as i64 - playhead as i64;
    while pass_start < end as i64 {
        for start in starts().map(|tick| at(&new, tick) as i64 + pass_start) {
            if start >= edit_at as i64 && start < end as i64 {
                expected.push(start as usize);
            }
        }
        pass_start += loop_samples as i64;
    }
    expected
}

/// The tempo changes mid-note and between notes, faster and slower. Every
/// note after the change lands where its bar and beat fall at the new tempo,
/// counted on from where the playhead was: none skipped, none doubled, and
/// the note sounding at the change carries on.
#[test]
fn a_tempo_change_keeps_the_bar_and_beat() {
    for (from, to) in [(120.0, 90.0), (120.0, 157.3), (97.0, 300.0), (60.0, 20.0)] {
        let beat = f64::from(RATE) * 60.0 / f64::from(from);
        // Mid-note on beat 3, and between beats 6 and 7, of the first pass.
        for edit_beat in [2.3, 5.7] {
            for block_size in BLOCK_SIZES {
                let what = format!(
                    "{from} to {to} BPM at beat {}, blocks of {block_size}",
                    edit_beat + 1.0
                );
                let mut project = project(from, 2, &SETTINGS, beats());
                let mut renderer = playing(&project, block_size);
                let edit_at = block_start((edit_beat * beat) as usize, block_size);
                renderer.render(edit_at);
                edit(&mut renderer, &mut project, Command::SetTempo { bpm: to });
                // Two passes at the new tempo.
                let new_loop = TempoMap::new(to).ticks_to_samples(2 * BAR, RATE) as usize;
                let end = edit_at + 2 * new_loop;
                renderer.render(end - edit_at);
                let samples = renderer.samples();

                assert_eq!(
                    onsets(samples),
                    expected_beats(from, to, edit_at, end),
                    "{what}"
                );
                assert_no_clicks(samples, 1, &what);
                // Every note ends on time: silent from half a beat (and the
                // release) after each onset until the next.
                let note = TempoMap::new(to).ticks_to_samples(BEAT / 2, RATE) as usize;
                let found = onsets(samples);
                for pair in found.windows(2).filter(|pair| pair[0] >= edit_at) {
                    assert_silent(
                        samples,
                        pair[0] + note + RELEASE_SAMPLES + 1..pair[1],
                        &what,
                    );
                }
            }
        }
    }
}

/// The playhead's bar and beat, as the status reports it, is the same just
/// before and just after a tempo change.
#[test]
fn the_reported_position_keeps_its_bar_and_beat_across_a_tempo_change() {
    let mut project = project(120.0, 2, &SETTINGS, beats());
    let mut renderer = playing(&project, 128);
    // Beat 3 of bar 1, exactly: 1,920 ticks.
    renderer.render(2 * BEAT_SAMPLES);
    assert_eq!(renderer.controller.poll().playhead, 2 * BEAT);
    edit(&mut renderer, &mut project, Command::SetTempo { bpm: 60.0 });
    // A block later at half the tempo: 128 samples are 2.56 ticks at 60 BPM.
    renderer.render(128);
    assert_eq!(renderer.controller.poll().playhead, 2 * BEAT + 3);
}

/// A 4-bar loop is shortened to 1 bar while the playhead is in bar 3, with
/// bar 3's note sounding. That note is released, and the loop carries on
/// from its start.
#[test]
fn shortening_the_loop_past_the_playhead_carries_on_from_the_loop_start() {
    // One note in each bar, an eighth note in, a beat long.
    let notes: Vec<Note> = (0..4)
        .map(|bar| {
            note(
                bar,
                60 + bar as u8,
                u64::from(bar as u32) * BAR + BEAT / 2,
                BEAT,
            )
        })
        .collect();
    for block_size in BLOCK_SIZES {
        let what = format!("blocks of {block_size}");
        let mut project = project(120.0, 4, &SETTINGS, notes.clone());
        let mut renderer = playing(&project, block_size);
        let edit_at = block_start(2 * BAR_SAMPLES + BEAT_SAMPLES / 2 + 5_000, block_size);
        renderer.render(edit_at);
        assert!(level_at(renderer.samples(), edit_at - 100, 100) > 0.0);
        edit(
            &mut renderer,
            &mut project,
            Command::SetLoopLength { bars: 1 },
        );
        renderer.render(2 * BAR_SAMPLES);
        let samples = renderer.samples();

        let eighth = BEAT_SAMPLES / 2;
        assert_silent(
            samples,
            edit_at + RELEASE_SAMPLES + 1..edit_at + eighth,
            &what,
        );
        assert_eq!(
            onsets(samples),
            [
                eighth,
                BAR_SAMPLES + eighth,
                2 * BAR_SAMPLES + eighth,
                // From the loop's start, then round the 1-bar loop.
                edit_at + eighth,
                edit_at + BAR_SAMPLES + eighth,
            ],
            "{what}"
        );
        assert_no_clicks(samples, 1, &what);
        let status = renderer.controller.poll();
        assert!(status.playhead < BAR, "{what}: inside the 1-bar loop");
    }
}

/// Lengthening the loop leaves the playhead where it is.
#[test]
fn lengthening_the_loop_carries_on_where_it_was() {
    let mut project = project(120.0, 1, &SETTINGS, beats()[..4].to_vec());
    let mut renderer = playing(&project, 128);
    let edit_at = block_start(3 * BEAT_SAMPLES + 1_000, 128);
    renderer.render(edit_at);
    edit(
        &mut renderer,
        &mut project,
        Command::SetLoopLength { bars: 2 },
    );
    renderer.render(250_000 - edit_at);
    // Beats 1 to 4 of bar 1, then a silent bar 2, then bar 1 again.
    let beat = |n: usize| n * BEAT_SAMPLES;
    assert_eq!(
        onsets(renderer.samples()),
        [
            beat(0),
            beat(1),
            beat(2),
            beat(3),
            beat(8),
            beat(9),
            beat(10)
        ]
    );
}

/// Many edits in a row while a busy loop plays, the way a drag sends them:
/// notes moved and re-pitched under the playhead, the tempo nudged, and the
/// loop lengthened and shortened. No clicks, no stuck notes.
#[test]
fn a_drag_of_edits_while_playing_does_not_click_or_stick() {
    let notes: Vec<Note> = (0..16)
        .map(|i| {
            note(
                i,
                57 + (i % 5) as u8 * 3,
                u64::from(i as u32) * BEAT / 2,
                700,
            )
        })
        .collect();
    let mut project = project(120.0, 2, &SETTINGS, notes.clone());
    let clip = project.tracks()[0].clips()[0].id();
    let mut renderer = playing(&project, 64);
    for step in 0..400u64 {
        renderer.render(256);
        let command = match step % 4 {
            0 => Command::SetNotes {
                clip,
                notes: notes
                    .iter()
                    .map(|note| Note {
                        start: (note.start + step * 7) % (2 * BAR),
                        pitch: note.pitch + (step % 3) as u8,
                        ..*note
                    })
                    .collect(),
            },
            1 => Command::SetTempo {
                bpm: 100.0 + (step % 40) as f32,
            },
            2 => Command::SetLoopLength {
                bars: 1 + (step / 4 % 2) as u32,
            },
            _ => Command::RemoveNotes {
                clip,
                notes: vec![note_id(u128::from(step % 16))],
            },
        };
        // Removing a note that's already gone is refused; that's fine.
        if project.apply(&command).is_ok() {
            renderer.controller.set_project(&project).unwrap();
        }
        if step % 4 == 3 && project.tracks()[0].clips()[0].notes().count() < 16 {
            let missing: Vec<Note> = notes
                .iter()
                .filter(|n| !project.tracks()[0].clips()[0].notes().any(|m| m.id == n.id))
                .copied()
                .collect();
            edit(
                &mut renderer,
                &mut project,
                Command::AddNotes {
                    clip,
                    notes: missing,
                },
            );
        }
    }
    // End the drag by removing every note while the loop still plays. Only
    // the swap can release the notes sounding now: a voice left holding
    // one would sound on.
    // Render on (a block at a time, for at most a second) until notes are
    // sounding.
    for _ in 0..RATE as usize / 64 {
        renderer.render(64);
        let samples = renderer.samples();
        if samples[samples.len() - 64..].iter().all(|&s| s != 0.0) {
            break;
        }
    }
    let removed_at = renderer.samples().len();
    assert!(
        level_at(renderer.samples(), removed_at - 32, 64) > 0.0,
        "notes sounding before they're removed"
    );
    let every_note = project.tracks()[0].clips()[0]
        .notes()
        .map(|n| n.id)
        .collect();
    edit(
        &mut renderer,
        &mut project,
        Command::RemoveNotes {
            clip,
            notes: every_note,
        },
    );
    renderer.render(RATE as usize / 10);
    assert!(renderer.controller.poll().playing);
    let samples = renderer.samples();
    // Up to 5 notes overlap.
    assert_no_clicks(samples, 5, "the drag");
    assert_silent(
        samples,
        removed_at + RELEASE_SAMPLES + 1..samples.len(),
        "after removing every note",
    );
}
