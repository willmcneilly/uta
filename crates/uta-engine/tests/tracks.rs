//! Several tracks: each plays through its own synth and its own slot, mixed
//! with its volume, pan, mute and solo, and tracks can be added, removed,
//! duplicated and reordered while the loop plays. Rendered offline through
//! the real processor and measured from the waveform and the status. See
//! RFC-003, "Tracks", "The mixer in the track headers" and "The shared model,
//! extended", points 4, 5, 7 and 8.

mod common;

use std::f32::consts::SQRT_2;

use common::*;
use uta_core::time::Ticks;
use uta_core::{
    Clip, ClipId, Command, MixerStrip, Note, NoteId, PlacedTrack, Project, ProjectId, Source,
    SynthSettings, Track, TrackId, Waveform,
};
use uta_engine::offline::Renderer;
use uta_engine::{
    EngineConfig, MAX_NOTE_EVENTS_PER_BLOCK, NoteError, NoteKey, Snapshot, TRACK_SLOTS,
    VOICE_LEVEL, VOLUME_SMOOTHING_SECONDS, db_to_gain, pitch_to_hz,
};
use uuid::Uuid;

const RATE: u32 = 48_000;
const BEAT: Ticks = 960;
const BAR: Ticks = 4 * BEAT;
/// A 1-bar loop at 120 BPM, in samples.
const LOOP: usize = 96_000;

fn config(channels: usize) -> EngineConfig {
    EngineConfig {
        sample_rate: RATE,
        channels,
    }
}

fn track_id(n: u128) -> TrackId {
    TrackId::from_uuid(Uuid::from_u128(10_000 + n))
}

fn clip_id(n: u128, clip: u128) -> ClipId {
    ClipId::from_uuid(Uuid::from_u128(20_000 + n * 100 + clip))
}

/// A sine that holds its level while the note is down: a 5 ms attack, full
/// sustain and a 50 ms release.
fn sine() -> SynthSettings {
    SynthSettings {
        waveform: Waveform::Sine,
        attack_seconds: 0.005,
        sustain: 1.0,
        release_seconds: 0.05,
        ..SynthSettings::default()
    }
}

fn strip(volume_db: f32, pan: f32) -> MixerStrip {
    MixerStrip {
        volume_db,
        pan,
        ..MixerStrip::default()
    }
}

/// Notes as (pitch, start, length), at full velocity, with IDs worked out
/// from track `n`, clip `clip` and their place in the list.
fn notes(n: u128, clip: u128, notes: &[(u8, Ticks, Ticks)]) -> Vec<Note> {
    notes
        .iter()
        .enumerate()
        .map(|(i, &(pitch, start, length))| Note {
            id: NoteId::from_uuid(Uuid::from_u128(
                1_000_000 + n * 10_000 + clip * 1_000 + i as u128,
            )),
            pitch,
            velocity: 127,
            start,
            length,
        })
        .collect()
}

/// Track `n`, "Synth n", with one clip over the 1-bar loop holding `played`.
fn track(n: u128, synth: SynthSettings, mixer: MixerStrip, played: &[(u8, Ticks, Ticks)]) -> Track {
    Track::new(track_id(n), format!("Synth {n}"), Source::Synth(synth))
        .with_mixer(mixer)
        .with_clips([Clip::new(clip_id(n, 0), 0, BAR).with_notes(notes(n, 0, played))])
}

/// A note held through the whole loop.
fn held(pitch: u8) -> [(u8, Ticks, Ticks); 1] {
    [(pitch, 0, BAR)]
}

/// Half-beat notes on every beat: each one's release ends long before the
/// next starts, so the track has one voice sounding at most.
fn beats(pitch: u8) -> Vec<(u8, Ticks, Ticks)> {
    (0..4).map(|beat| (pitch, beat * BEAT, BEAT / 2)).collect()
}

/// A 1-bar loop at 120 BPM, with the master at 0 dB and just `tracks`, in
/// order.
fn song(tracks: Vec<Track>) -> Project {
    let mut project = Project::with_id(ProjectId::from_uuid(Uuid::from_u128(1)));
    let first = project.tracks()[0].id();
    let tracks = tracks
        .into_iter()
        .enumerate()
        .map(|(index, track)| PlacedTrack { index, track })
        .collect();
    for command in [
        Command::RemoveTracks {
            tracks: vec![first],
        },
        Command::SetLoop {
            start_bar: 0,
            bars: 1,
        },
        Command::SetMasterVolume { volume_db: 0.0 },
        Command::AddTracks { tracks },
    ] {
        project.apply(&command).expect("a valid test song");
    }
    project
}

/// Applies `command` and sends the renderer's engine the result, as the app
/// does.
fn change(project: &mut Project, renderer: &mut Renderer, command: Command) {
    project.apply(&command).unwrap();
    renderer.controller.set_project(project).unwrap();
}

/// Splits interleaved stereo into its left and right.
fn sides(samples: &[f32]) -> [Vec<f32>; 2] {
    let (frames, rest) = samples.as_chunks::<2>();
    assert!(rest.is_empty(), "not whole stereo frames");
    [
        frames.iter().map(|frame| frame[0]).collect(),
        frames.iter().map(|frame| frame[1]).collect(),
    ]
}

/// Whether `level` is within 0.1% of `expected`.
fn close(level: f32, expected: f32) -> bool {
    (level / expected - 1.0).abs() < 1e-3
}

/// The steepest a sounding track moves in one sample: a full-velocity sine
/// voice at `pitch` through a gain of `gain`, at its tone and its attack, plus
/// 10%. A cut-off note jumps by far more.
fn step_limit(pitch: u8, gain: f32) -> f32 {
    let attack = 0.005 * f64::from(RATE);
    VOICE_LEVEL * gain * (sine_max_step(pitch_to_hz(pitch), RATE) + 1.0 / attack as f32) * 1.1
}

#[test]
fn each_track_plays_its_own_notes_through_its_own_mixer_strip() {
    // A4 hard left at 0 dB, E5 hard right at -6 dB.
    let project = song(vec![
        track(1, sine(), strip(0.0, -1.0), &held(69)),
        track(2, sine(), strip(-6.0, 1.0), &held(76)),
    ]);
    let mut renderer = Renderer::new(config(2), Snapshot::from(&project), 128);
    renderer.controller.play().unwrap();
    renderer.render(LOOP);
    let steady = LOOP / 20..LOOP - LOOP / 20;
    let [left, right] = sides(renderer.samples()).map(|side| side[steady.clone()].to_vec());

    // Hard to one side is +3 dB there.
    assert!(
        close(peak(&left), VOICE_LEVEL * SQRT_2),
        "left {}",
        peak(&left)
    );
    let right_level = VOICE_LEVEL * SQRT_2 * db_to_gain(-6.0);
    assert!(close(peak(&right), right_level), "right {}", peak(&right));
    for (side, pitch) in [(&left, 69), (&right, 76)] {
        let frequency = measure_frequency(side, RATE);
        let expected = pitch_to_hz(pitch);
        assert!(
            (frequency / expected - 1.0).abs() < 1e-4,
            "pitch {pitch}: {frequency} Hz"
        );
    }

    // Each slot's peak is its track's level before the master.
    let status = renderer.take_status();
    assert!(status.peak > 0.0);
    let slot = |n| renderer.controller.slot(track_id(n)).unwrap();
    assert!(close(status.track_peaks[slot(1)], VOICE_LEVEL * SQRT_2));
    assert!(close(status.track_peaks[slot(2)], right_level));
}

/// Two tracks playing the same note at the same time add up: centred, at 0 dB
/// and -6 dB, the mix is at the sum of their levels.
#[test]
fn tracks_add_up_in_the_mix() {
    let project = song(vec![
        track(1, sine(), strip(0.0, 0.0), &held(69)),
        track(2, sine(), strip(-6.0, 0.0), &held(69)),
    ]);
    let mut renderer = Renderer::new(config(1), Snapshot::from(&project), 128);
    renderer.controller.play().unwrap();
    renderer.render(LOOP);
    let level = peak(&renderer.samples()[LOOP / 20..LOOP - LOOP / 20]);
    let expected = VOICE_LEVEL * (1.0 + db_to_gain(-6.0));
    assert!(
        (level / expected - 1.0).abs() < 1e-4,
        "{level}, not {expected}"
    );
}

/// The same note on two tracks, one a sine and the other a square, panned
/// apart: each side has its own track's sound.
#[test]
fn each_track_plays_its_own_sound() {
    let square = SynthSettings {
        waveform: Waveform::Square,
        ..sine()
    };
    let project = song(vec![
        track(1, sine(), strip(0.0, -1.0), &held(57)),
        track(2, square, strip(0.0, 1.0), &held(57)),
    ]);
    let mut renderer = Renderer::new(config(2), Snapshot::from(&project), 128);
    renderer.controller.play().unwrap();
    renderer.render(LOOP);
    let [left, right] = sides(renderer.samples());
    // The third harmonic's level against the fundamental's: none in a sine,
    // about a third in a square.
    let third = |side: &[f32]| {
        let window = &side[LOOP / 4..LOOP / 4 + 16_384];
        let spectrum = spectrum(window);
        let near = |hz: f64| {
            let bin = (hz * 16_384.0 / f64::from(RATE)).round() as usize;
            spectrum[bin - 3..=bin + 3]
                .iter()
                .fold(0.0f64, |a, &b| a.max(b))
        };
        near(3.0 * pitch_to_hz(57)) / near(pitch_to_hz(57))
    };
    assert!(third(&left) < 0.001, "sine: {}", third(&left));
    assert!(
        (0.25..0.4).contains(&third(&right)),
        "square: {}",
        third(&right)
    );
}

/// A track plays if it isn't muted, and either no track is soloed or it is.
/// Mute wins, even on a soloed track.
#[test]
fn mute_and_solo_decide_which_tracks_play() {
    let mut project = song(vec![
        track(1, sine(), MixerStrip::default(), &held(60)),
        track(2, sine(), MixerStrip::default(), &held(64)),
        track(3, sine(), MixerStrip::default(), &held(67)),
    ]);
    let mut renderer = Renderer::new(config(2), Snapshot::from(&project), 128);
    renderer.controller.play().unwrap();
    renderer.render(4_096);

    let off = (false, false);
    let muted = (true, false);
    let soloed = (false, true);
    let both = (true, true);
    let cases = [
        ([off, off, off], [true, true, true]),
        ([off, soloed, off], [false, true, false]),
        ([soloed, soloed, off], [true, true, false]),
        ([off, off, muted], [true, true, false]),
        ([soloed, off, muted], [true, false, false]),
        ([off, both, off], [false, false, false]),
        ([soloed, both, off], [true, false, false]),
        ([off, off, off], [true, true, true]),
    ];
    for (strips, audible) in cases {
        for (n, (mute, solo)) in (1..).zip(strips) {
            let mixer = MixerStrip {
                mute,
                solo,
                ..MixerStrip::default()
            };
            change(
                &mut project,
                &mut renderer,
                Command::SetTrackMixer {
                    track: track_id(n),
                    mixer,
                },
            );
        }
        // After the glide.
        renderer.render(4_096);
        renderer.take_status();
        renderer.render(4_096);
        let status = renderer.take_status();
        for (n, audible) in (1..).zip(audible) {
            let slot = renderer.controller.slot(track_id(n)).unwrap();
            let level = status.track_peaks[slot];
            if audible {
                assert!(
                    close(level, VOICE_LEVEL),
                    "{strips:?}: track {n} at {level}"
                );
            } else {
                assert_eq!(level, 0.0, "{strips:?}: track {n}");
            }
        }
        let expected = audible.iter().filter(|&&a| a).count() as f32 * VOICE_LEVEL;
        assert!(
            status.peak <= expected * 1.001,
            "{strips:?}: {}",
            status.peak
        );
    }
}

/// Mute and solo glide like volume: toggling them while two held notes
/// sound never jumps faster than the notes and a glide can.
#[test]
fn mute_and_solo_changes_glide() {
    let mut project = song(vec![
        track(1, sine(), MixerStrip::default(), &held(69)),
        track(2, sine(), MixerStrip::default(), &held(76)),
    ]);
    let mut renderer = Renderer::new(config(1), Snapshot::from(&project), 128);
    renderer.controller.play().unwrap();
    renderer.render(4_800);
    let changes = [
        (1, false, true),
        (1, true, true),
        (1, false, false),
        (2, true, false),
        (2, false, false),
        (2, false, true),
        (2, false, false),
    ];
    // Each change lands mid-glide of the one before, then after the glide.
    for gap in [300, 2_000] {
        for (n, mute, solo) in changes {
            let mixer = MixerStrip {
                mute,
                solo,
                ..MixerStrip::default()
            };
            change(
                &mut project,
                &mut renderer,
                Command::SetTrackMixer {
                    track: track_id(n),
                    mixer,
                },
            );
            renderer.render(gap);
        }
    }
    renderer.render(LOOP / 10);

    let glide = VOLUME_SMOOTHING_SECONDS * f64::from(RATE);
    let limit = step_limit(69, 1.0) + step_limit(76, 1.0) + 2.0 * VOICE_LEVEL / glide as f32;
    let samples = renderer.samples();
    let (jump, at) = max_jump(samples);
    assert!(jump <= limit, "jump of {jump} at {at}, limit {limit}");
    // It did go quiet and come back.
    assert!(samples[4_800..].contains(&0.0));
    assert!(peak(&samples[samples.len() - 4_800..]) > VOICE_LEVEL * 1.5);
}

/// Renders the demo song round its loop twice. With `reorder`, the tracks are
/// moved around between blocks while it plays; either way, the same
/// snapshots are swapped in at the same moments.
fn demo_song_moving(reorder: bool) -> Vec<f32> {
    let mut project = demo_song();
    let ids: Vec<TrackId> = project.tracks().iter().map(Track::id).collect();
    let mut renderer = Renderer::new(config(2), Snapshot::from(&project), 128);
    renderer.controller.play().unwrap();
    for step in 0..40 {
        renderer.render(10_000);
        if reorder {
            let command = Command::MoveTrack {
                track: ids[step % 3],
                index: (step * 2) % 3,
            };
            change(&mut project, &mut renderer, command);
        } else {
            renderer.controller.set_project(&project).unwrap();
        }
    }
    if reorder {
        assert_ne!(
            project.tracks().iter().map(Track::id).collect::<Vec<_>>(),
            ids,
            "the order changed"
        );
    }
    renderer.controller.stop().unwrap();
    renderer.render_seconds(1.0);
    renderer.into_samples()
}

/// Tracks are mixed in slot order, and a track keeps its slot when it moves,
/// so reordering them changes nothing, down to the last bit.
#[test]
fn reordering_tracks_does_not_change_the_audio() {
    let moved = demo_song_moving(true);
    assert!(peak(&moved) > 0.1);
    assert!(
        moved == demo_song_moving(false),
        "reordering changed the audio"
    );
}

/// Deleting a track mid-note releases its note, which fades out in its own
/// slot. The slot isn't handed to a new track until it has, and then it is.
#[test]
fn a_deleted_tracks_notes_release_and_its_slot_waits_for_them() {
    let slow_release = SynthSettings {
        release_seconds: 0.3,
        ..sine()
    };
    let mut project = song(vec![
        track(1, sine(), MixerStrip::default(), &held(69)),
        track(2, slow_release, MixerStrip::default(), &held(76)),
    ]);
    let mut renderer = Renderer::new(config(1), Snapshot::from(&project), 128);
    renderer.controller.play().unwrap();
    renderer.render_seconds(0.25);
    let slot = renderer.controller.slot(track_id(2)).unwrap();

    change(
        &mut project,
        &mut renderer,
        Command::RemoveTracks {
            tracks: vec![track_id(2)],
        },
    );
    assert_eq!(renderer.controller.slot(track_id(2)), None);
    // A track added straight away doesn't land on the fading note.
    let add = |n: u128, index: usize| Command::AddTracks {
        tracks: vec![PlacedTrack {
            index,
            track: track(n, sine(), MixerStrip::default(), &[]),
        }],
    };
    change(&mut project, &mut renderer, add(3, 1));
    let new_slot = renderer.controller.slot(track_id(3)).unwrap();
    assert_ne!(new_slot, slot);

    // The note releases: it's falling, not cut off or held.
    renderer.take_status();
    renderer.render(1_280);
    let fading = renderer.take_status();
    assert!(fading.sounding_slots & (1 << slot) != 0);
    let start = fading.track_peaks[slot];
    assert!(start > 0.0 && start < VOICE_LEVEL, "fading from {start}");
    renderer.render(4_800);
    let later = renderer.take_status().track_peaks[slot];
    assert!(later > 0.0 && later < start, "{later} after {start}");

    // Once it's silent the slot is free, and the next new track gets it.
    renderer.render_seconds(0.2);
    renderer.take_status();
    renderer.render(1_280);
    let status = renderer.take_status();
    assert_eq!(status.track_peaks[slot], 0.0);
    assert_eq!(status.sounding_slots & (1 << slot), 0);
    change(&mut project, &mut renderer, add(4, 2));
    assert_eq!(renderer.controller.slot(track_id(4)), Some(slot));

    // No click anywhere: the A4 keeps going, the E5 fades.
    let limit = step_limit(69, 1.0) + step_limit(76, 1.0);
    let (jump, at) = max_jump(renderer.samples());
    assert!(jump <= limit, "jump of {jump} at {at}, limit {limit}");
}

/// With all 32 slots taken, a new track added just after another is deleted
/// has to take the deleted track's slot while its note is still releasing.
/// That note fades out over the take-over time instead of carrying on in the
/// new track's slot, and without a click.
#[test]
fn a_slot_handed_on_while_it_still_sounds_fades_its_notes_out() {
    let slow_release = SynthSettings {
        release_seconds: 0.3,
        ..sine()
    };
    let mut tracks: Vec<Track> = (1..TRACK_SLOTS as u128)
        .map(|n| track(n, sine(), MixerStrip::default(), &[]))
        .collect();
    let last = TRACK_SLOTS as u128;
    tracks.push(track(last, slow_release, MixerStrip::default(), &held(76)));
    let mut project = song(tracks);
    let mut renderer = Renderer::new(config(1), Snapshot::from(&project), 128);
    renderer.controller.play().unwrap();
    renderer.render_seconds(0.25);
    let slot = renderer.controller.slot(track_id(last)).unwrap();

    change(
        &mut project,
        &mut renderer,
        Command::RemoveTracks {
            tracks: vec![track_id(last)],
        },
    );
    change(
        &mut project,
        &mut renderer,
        Command::AddTracks {
            tracks: vec![PlacedTrack {
                index: TRACK_SLOTS - 1,
                track: track(last + 1, sine(), MixerStrip::default(), &[]),
            }],
        },
    );
    // No other slot is left, so it gets the one still sounding.
    assert_eq!(renderer.controller.slot(track_id(last + 1)), Some(slot));

    // Its note fades out within the take-over time (5 ms, 240 samples),
    // long before its 0.3 s release would end.
    renderer.take_status();
    renderer.render(1_280);
    let status = renderer.take_status();
    assert_eq!(status.sounding_slots & (1 << slot), 0, "still sounding");
    let limit = step_limit(76, 1.0);
    let (jump, at) = max_jump(renderer.samples());
    assert!(jump <= limit, "jump of {jump} at {at}, limit {limit}");
}

/// Adding, duplicating, removing and reordering tracks while the loop plays,
/// each change in the middle of notes: no click, the duplicate plays in its
/// own slot, and after Stop nothing is left sounding.
#[test]
fn changing_tracks_while_playing_does_not_click_or_leave_notes_stuck() {
    let mut project = song(vec![
        track(1, sine(), MixerStrip::default(), &beats(60)),
        track(2, sine(), MixerStrip::default(), &beats(64)),
        track(3, sine(), MixerStrip::default(), &beats(67)),
    ]);
    let mut renderer = Renderer::new(config(1), Snapshot::from(&project), 128);
    renderer.controller.play().unwrap();

    let duplicate = project.track(track_id(1)).unwrap().copy(
        track_id(5),
        project.next_track_name(),
        ClipId::random,
        NoteId::random,
    );
    assert_eq!(duplicate.name(), "Synth 4");
    // Every change lands in the first half of a beat, while notes sound.
    let changes = [
        (
            0.1,
            Command::AddTracks {
                tracks: vec![PlacedTrack {
                    index: 1,
                    track: duplicate,
                }],
            },
        ),
        (
            0.62,
            Command::MoveTrack {
                track: track_id(3),
                index: 0,
            },
        ),
        (
            1.13,
            Command::RemoveTracks {
                tracks: vec![track_id(2)],
            },
        ),
        (
            1.6,
            Command::AddTracks {
                tracks: vec![PlacedTrack {
                    index: 0,
                    track: track(4, sine(), MixerStrip::default(), &beats(72)),
                }],
            },
        ),
        (
            2.07,
            Command::RemoveTracks {
                tracks: vec![track_id(5)],
            },
        ),
        (
            2.2,
            Command::MoveTrack {
                track: track_id(1),
                index: 2,
            },
        ),
    ];
    let mut duplicate_slot = None;
    for (at, command) in changes {
        let now = renderer.samples().len();
        renderer.render(renderer.frames_for(at) - now);
        change(&mut project, &mut renderer, command);
        if at == 0.1 {
            // The copy has its own slot, and plays from its next note.
            let slot = renderer.controller.slot(track_id(5)).unwrap();
            assert_ne!(Some(slot), renderer.controller.slot(track_id(1)));
            duplicate_slot = Some(slot);
            renderer.take_status();
            renderer.render_seconds(0.5);
            let level = renderer.take_status().track_peaks[slot];
            assert!(close(level, VOICE_LEVEL), "the duplicate plays at {level}");
        }
    }
    assert!(duplicate_slot.is_some());
    renderer.render_seconds(0.4);
    renderer.controller.stop().unwrap();
    renderer.render_seconds(0.2);

    // At most five tracks sounding at once.
    let limit: f32 = [60, 60, 64, 67, 72]
        .into_iter()
        .map(|pitch| step_limit(pitch, 1.0))
        .sum();
    let samples = renderer.samples();
    let (jump, at) = max_jump(samples);
    assert!(jump <= limit, "jump of {jump} at {at}, limit {limit}");
    // Everything has released: silence, and no slot sounding.
    assert_eq!(peak(&samples[samples.len() - 2_400..]), 0.0);
    assert_eq!(renderer.take_status().sounding_slots, 0);
}

/// Clips can overlap on a track, and both play: two clips with the same note
/// at the same time sound twice as loud as one.
#[test]
fn overlapping_clips_on_one_track_both_play() {
    let level = |clips: u128| {
        let track = Track::new(track_id(1), "Synth 1", Source::Synth(sine())).with_clips(
            (0..clips).map(|clip| {
                let start = clip as Ticks * BEAT / 4;
                Clip::new(clip_id(1, clip), start, BAR).with_notes(notes(
                    1,
                    clip,
                    &[(69, BEAT - start, BEAT)],
                ))
            }),
        );
        let mut renderer = Renderer::new(config(1), Snapshot::from(&song(vec![track])), 128);
        renderer.controller.play().unwrap();
        renderer.render(LOOP / 4 + LOOP / 8);
        peak(&renderer.samples()[LOOP / 4 + 1_000..])
    };
    let one = level(1);
    assert!(close(one, VOICE_LEVEL), "{one}");
    let two = level(2);
    assert!(close(two, 2.0 * VOICE_LEVEL), "{two}");
}

/// The limit on note events per block applies to each track: two tracks
/// that each start the most notes a block allows lose none, and one note
/// more on one of them is dropped: its start finds no free voice.
#[test]
fn the_limit_on_events_per_block_is_per_track() {
    let dropped = |extra: usize| {
        let chord = |n: u128, count: usize| {
            let played: Vec<_> = (0..count)
                .map(|i| (48 + (i % 24) as u8, 0, BEAT / 2))
                .collect();
            track(n, sine(), MixerStrip::default(), &played)
        };
        let project = song(vec![
            chord(1, MAX_NOTE_EVENTS_PER_BLOCK + extra),
            chord(2, MAX_NOTE_EVENTS_PER_BLOCK),
        ]);
        let mut renderer = Renderer::new(config(1), Snapshot::from(&project), 128);
        renderer.controller.play().unwrap();
        // The starts, then the ends, each all in one block.
        renderer.render(LOOP / 4);
        renderer.take_status().dropped_note_events
    };
    assert_eq!(dropped(0), 0);
    // The extra start. Its end is caught up on, not counted.
    assert_eq!(dropped(1), 1);
}

/// A live note plays on the track in its slot, through that track's mixer
/// strip, and nowhere if no track has the slot.
#[test]
fn live_notes_play_on_the_track_in_their_slot() {
    let project = song(vec![
        track(1, sine(), strip(0.0, -1.0), &[]),
        track(2, sine(), strip(0.0, 1.0), &[]),
    ]);
    let mut renderer = Renderer::new(config(2), Snapshot::from(&project), 128);
    let right = renderer.controller.slot(track_id(2)).unwrap();
    let key = NoteKey(1);
    renderer.controller.note_on(right, key, 69, 127).unwrap();
    renderer.render_seconds(0.1);
    let [left_side, right_side] = sides(renderer.samples());
    assert_eq!(peak(&left_side), 0.0);
    assert!(close(peak(&right_side), VOICE_LEVEL * SQRT_2));

    // The same key on another slot is another note: releasing it there
    // leaves this one sounding.
    renderer.controller.note_off(right + 1, key).unwrap();
    renderer.render_seconds(0.1);
    assert!(renderer.take_status().sounding_slots & (1 << right) != 0);
    renderer.controller.note_off(right, key).unwrap();
    renderer.render_seconds(0.1);
    assert_eq!(renderer.take_status().sounding_slots, 0);

    // No track in the slot: nothing sounds.
    let empty = (0..TRACK_SLOTS)
        .find(|&slot| Snapshot::from(&project).track_in(slot).is_none())
        .unwrap();
    renderer.controller.note_on(empty, key, 69, 127).unwrap();
    renderer.take_status();
    renderer.render_seconds(0.1);
    assert_eq!(renderer.take_status().peak, 0.0);
    assert_eq!(
        renderer.controller.note_on(TRACK_SLOTS, key, 69, 127),
        Err(NoteError::Slot(TRACK_SLOTS))
    );
}
