//! Real-time safety: the real process path runs under `assert_no_alloc`, which
//! aborts the test binary if it allocates or frees. Built with
//! `RTSAN_ENABLE=1`, RealtimeSanitizer also checks `Processor::process`
//! (marked `#[nonblocking]`) for allocation, locks and system calls.
//!
//! Only the processor calls are wrapped: the control side allocates the
//! snapshots and frees the used ones, as it's meant to.
//!
//! The device callbacks from `live` are covered the same way: the error
//! callback with simulated device errors, and the data callback including
//! handing the processor back when its stream is dropped.

mod common;

use assert_no_alloc::{AllocDisabler, assert_no_alloc};
use common::{
    CLAP, CLOSED_HAT, CYMBAL, HIGH_TOM, KICK, LOW_TOM, OPEN_HAT, SNARE, demo_loop, demo_song,
    drum_project, drum_project_with_kit, drum_track, empty_drum_track, empty_sine_track, hit, note,
    project, replace_track, with_every_slot_taken, with_mixer,
};
use uta_core::{
    Clip, ClipId, ClipPosition, Command, DrumParam, DrumSound, KickModel, KitSettings, NoteId,
    PlacedClip, PlacedTrack, SynthParam, TrackId,
};
use uta_engine::live::{AudioCallback, DeviceError, ERROR_CAPACITY, ErrorCallback};
use uta_engine::offline::Renderer;
use uta_engine::{
    EngineConfig, FilterMode, LfoShape, MAX_NOTE_EVENTS_PER_BLOCK, MixerStrip, ModDestination,
    ModSlot, ModSource, NoteKey, Processor, STATUS_CAPACITY, Snapshot, SynthSettings,
    USED_SNAPSHOT_CAPACITY, VOICES, VoiceMode, Waveform,
};

#[global_allocator]
static ALLOCATOR: AllocDisabler = AllocDisabler;

const BLOCK: usize = 128;

fn stereo() -> EngineConfig {
    EngineConfig {
        sample_rate: 48_000,
        channels: 2,
    }
}

/// A renderer for the demo loop, so Play makes sound from the first block.
fn renderer() -> Renderer {
    rtsan_standalone::ensure_initialized();
    Renderer::new(stereo(), Snapshot::from(&demo_loop()), BLOCK)
}

/// Runs one block with allocation forbidden, as the audio thread would.
fn process_block(processor: &mut Processor, buffer: &mut [f32]) {
    assert_no_alloc(|| processor.process(buffer));
}

#[test]
fn play_stop_and_volume_do_not_allocate() {
    let mut renderer = renderer();
    let mut buffer = vec![0.0; BLOCK * 2];

    renderer.controller.play().unwrap();
    process_block(renderer.processor(), &mut buffer);
    renderer.controller.set_volume_db(-6.0).unwrap();
    for _ in 0..20 {
        process_block(renderer.processor(), &mut buffer);
    }
    renderer.controller.stop().unwrap();
    process_block(renderer.processor(), &mut buffer);

    let status = renderer.controller.poll();
    assert!(status.peak > 0.0, "the processor made no sound");
    assert!(!status.playing);
}

#[test]
fn snapshot_swap_does_not_allocate_and_returns_the_old_snapshot() {
    let mut renderer = renderer();
    let mut buffer = vec![0.0; BLOCK * 2];
    renderer.controller.play().unwrap();

    let mut new = Snapshot::from(&demo_loop());
    new.gain = 0.25;
    renderer.controller.set_snapshot(new.clone()).unwrap();
    process_block(renderer.processor(), &mut buffer);

    // The old snapshot came back to the control side, which frees it.
    assert_eq!(renderer.controller.free_used_snapshots(), 1);
    assert_eq!(renderer.controller.snapshot(), &new);
}

#[test]
fn many_swaps_in_one_block_do_not_allocate() {
    let mut renderer = renderer();
    let mut buffer = vec![0.0; BLOCK * 2];
    for i in 0..10 {
        renderer.controller.set_volume_db(-(i as f32)).unwrap();
    }
    process_block(renderer.processor(), &mut buffer);
    assert_eq!(renderer.controller.free_used_snapshots(), 10);
}

/// Notes starting and stopping, more notes than voices so voices are taken
/// over (including while already being taken over), and snapshot swaps that
/// change every synth setting while they sound (Poly to Mono and back, and
/// Mono's Legato and Glide) (the filter's own settings
/// on and off, so the voices move between sharing filter settings and
/// working out their own), with Play pressed now and
/// then to restart the free-running parts: none of it allocates or frees on
/// the audio thread.
#[test]
fn notes_take_overs_and_synth_changes_do_not_allocate() {
    let mut renderer = renderer();
    let mut buffer = vec![0.0; BLOCK * 2];
    let waveforms = [
        Waveform::Sine,
        Waveform::Triangle,
        Waveform::Saw,
        Waveform::Pulse,
    ];
    let filter_modes = [
        FilterMode::LowPass,
        FilterMode::BandPass,
        FilterMode::HighPass,
    ];
    let mut swaps = 0;
    for round in 0..8u8 {
        if round % 3 == 0 {
            renderer.controller.play().unwrap();
        } else if round % 3 == 2 {
            renderer.controller.stop().unwrap();
        }
        // Twice as many notes as voices, all in one block, then more.
        for i in 0..2 * VOICES as u8 {
            let key = NoteKey(u128::from(round) * 100 + u128::from(i));
            renderer
                .controller
                .note_on(0, key, 36 + i, 20 + i * 3)
                .unwrap();
        }
        process_block(renderer.processor(), &mut buffer);
        let step = f32::from(round) / 8.0;
        renderer
            .controller
            .set_synth_settings(
                0,
                SynthSettings {
                    osc1_wave: waveforms[usize::from(round) % 4],
                    osc1_octave: (round % 5) as i8 - 2,
                    osc1_width: 0.95 - 0.9 * step,
                    osc2_wave: waveforms[usize::from(round + 1) % 4],
                    osc2_octave: 2 - (round % 5) as i8,
                    osc2_pitch_semitones: 24.0 * step - 12.0,
                    // On, off and on again, crossfading.
                    osc2_sync: round % 3 != 1,
                    sub_octave: -1 - (round % 2) as i8,
                    osc1_level: 1.0 - step,
                    osc2_level: step,
                    sub_level: 0.5 * step,
                    noise_level: 1.0 - 0.5 * step,
                    filter_mode: filter_modes[usize::from(round) % 3],
                    cutoff_hz: 20_000.0 / f32::from(round + 1).powi(3),
                    resonance: f32::from(round % 3) / 2.0,
                    // On and off, so the voices move between sharing their
                    // filter settings and working out their own.
                    filter_env_octaves: [0.0, 6.0, -4.0, 0.0][usize::from(round) % 4],
                    filter_key: [0.0, 0.0, 1.0, 0.5][usize::from(round + 1) % 4],
                    // On and off with the LFO's other amounts, so the LFO
                    // runs and rests.
                    filter_lfo_octaves: [0.0, 3.0, -2.0, 0.0][usize::from(round + 2) % 4],
                    filter_attack_seconds: 0.002 * f32::from(round + 1),
                    filter_decay_seconds: 0.03,
                    filter_sustain: 1.0 - step,
                    filter_release_seconds: 0.02,
                    attack_seconds: 0.001 * f32::from(round + 1),
                    decay_seconds: 0.05,
                    sustain: step,
                    release_seconds: 0.01,
                    lfo_shape: LfoShape::ALL[usize::from(round) % 5],
                    lfo_rate_hz: 0.05 + 2.5 * f32::from(round),
                    lfo_sync: round % 4 < 2,
                    lfo_synced_seconds: 0.05 * f64::from(round + 1),
                    lfo_retrigger: round % 3 == 0,
                    lfo_vibrato_semitones: [0.0, 2.0, 0.5, 0.0][usize::from(round) % 4],
                    lfo_width: [0.45, 0.0, 0.2, 0.0][usize::from(round) % 4],
                    // Poly, then Mono for two rounds, gliding with and
                    // without Legato, then Poly again: more held notes than
                    // Mono remembers, and letting go returning to them.
                    voice_mode: if round % 4 == 1 || round % 4 == 2 {
                        VoiceMode::Mono
                    } else {
                        VoiceMode::Poly
                    },
                    legato: round % 4 == 2,
                    glide_seconds: 0.001 + 0.02 * step,
                    osc_retrigger: round % 2 == 0,
                    // Every source and every destination over the rounds,
                    // each slot changing its amount and its route while
                    // notes sound, and every slot empty in some.
                    mods: std::array::from_fn(|slot| {
                        let turn = usize::from(round) + 4 * slot;
                        ModSlot {
                            source: ModSource::ALL[turn % ModSource::ALL.len()],
                            destination: ModDestination::ALL
                                [(turn * 5 + slot) % ModDestination::ALL.len()],
                            amount: [1.0, -1.0, 0.5, 0.0][(turn + slot) % 4],
                        }
                    }),
                },
            )
            .unwrap();
        swaps += 1;
        for _ in 0..4 {
            process_block(renderer.processor(), &mut buffer);
        }
        for i in (0..2 * VOICES as u8).step_by(3) {
            let key = NoteKey(u128::from(round) * 100 + u128::from(i));
            renderer.controller.note_off(0, key).unwrap();
        }
        for _ in 0..4 {
            process_block(renderer.processor(), &mut buffer);
        }
        assert_eq!(renderer.controller.free_used_snapshots(), 1);
    }
    assert_eq!(swaps, 8);
    assert!(
        renderer.controller.poll().peak > 0.0,
        "the synth made no sound"
    );
}

/// The loop playing: notes starting and ending mid-block, the loop going
/// back to its start mid-block, more notes at once than voices so voices are
/// taken over, a jump that chases the notes under way, and edits arriving
/// as whole new snapshots, which switch it to Mono, with Glide and Legato,
/// and back. None of it
/// allocates or frees on the audio thread.
///
/// Each edit's snapshot is built afresh from the project, so it shares no
/// data with the one the processor is playing.
/// [`many_swaps_sharing_clip_data_do_not_allocate_or_free`] covers snapshots
/// that do.
#[test]
fn a_looping_render_with_take_overs_and_edits_does_not_allocate() {
    rtsan_standalone::ensure_initialized();
    // A 1-bar loop at 293 BPM, 39,317 samples, so it goes back to its start
    // mid-block. 24 notes of 3,000 ticks, 150 apart: up to 20 at once, so
    // voices are taken over, and ends cross the loop's end.
    let notes = (0..24u8)
        .map(|i| note(u128::from(i), 40 + i, u64::from(i) * 150, 3000))
        .collect();
    let mut project = project(293.0, 1, &[SynthParam::ReleaseSeconds(0.05)], notes);
    let loop_samples = 39_317;
    assert_eq!(
        Snapshot::from(&project).sequence.loop_samples(),
        0..loop_samples
    );
    assert_ne!(loop_samples % BLOCK as u64, 0);
    let track = project.tracks()[0].id();
    let mut renderer = Renderer::new(stereo(), Snapshot::from(&project), BLOCK);
    let mut buffer = vec![0.0; BLOCK * 2];
    renderer.controller.play().unwrap();

    let passes = 5;
    let blocks_per_pass = loop_samples as usize / BLOCK + 1;
    for pass in 0..passes {
        for block in 0..blocks_per_pass {
            process_block(renderer.processor(), &mut buffer);
            if block == blocks_per_pass / 4 {
                // A jump into the middle of the overlapping notes, which
                // chases a dozen of them: in Mono, into the held notes,
                // latest start first.
                renderer
                    .controller
                    .locate(1_700 + 100 * pass as u64)
                    .unwrap();
            }
            if block == blocks_per_pass / 2 {
                // An edit: the same notes, a new cutoff, in a new snapshot.
                // From the second pass the track is in Mono, gliding, then
                // with Legato, and in Poly again for the last: the next
                // pass's jump chases the notes into Mono's held notes.
                let voice = [
                    SynthParam::GlideSeconds(0.03),
                    SynthParam::VoiceMode(uta_core::VoiceMode::Mono),
                    SynthParam::Legato(true),
                    SynthParam::Legato(false),
                    SynthParam::VoiceMode(uta_core::VoiceMode::Poly),
                ][pass];
                for param in [SynthParam::CutoffHz(500.0 + 1000.0 * pass as f32), voice] {
                    project
                        .apply(&Command::SetSynthParam { track, param })
                        .unwrap();
                }
                renderer
                    .controller
                    .set_snapshot(Snapshot::from(&project))
                    .unwrap();
            }
            renderer.controller.poll();
        }
    }
    let status = renderer.controller.poll();
    assert!(status.playing);
    assert_eq!(status.dropped_note_events, 0);
    assert_eq!(
        status.position,
        (passes * blocks_per_pass * BLOCK) as u64,
        "played every block"
    );
}

/// Choosing factory sounds while a loop plays, as the Sound menu does: each
/// one a whole new set of settings in a new snapshot, mid-loop, while
/// chords and a line sound, every sound in turn, Poly and Mono, over and
/// over. The processor never allocates or frees.
#[test]
fn switching_factory_sounds_mid_loop_does_not_allocate() {
    rtsan_standalone::ensure_initialized();
    // A 1-bar loop at 120 BPM: a held chord and a line of 8ths over it.
    let chord = [48u8, 52, 55, 59]
        .into_iter()
        .enumerate()
        .map(|(i, pitch)| note(i as u128, pitch, 0, 3840));
    let line = (0..8u8).map(|i| note(10 + u128::from(i), 60 + 2 * i, u64::from(i) * 480, 600));
    let mut project = project(120.0, 1, &[], chord.chain(line).collect());
    let track = project.tracks()[0].id();
    let mut renderer = Renderer::new(stereo(), Snapshot::from(&project), BLOCK);
    let mut buffer = vec![0.0; BLOCK * 2];
    renderer.controller.play().unwrap();

    let sounds = uta_core::FactorySound::all();
    // Three times round the menu, a sound every 41 blocks (about 0.11 s),
    // so the switches land all over the loop.
    for sound in sounds.iter().cycle().take(3 * sounds.len()) {
        project
            .apply(&Command::SetSynthSettings {
                track,
                settings: sound.settings,
            })
            .unwrap();
        renderer
            .controller
            .set_snapshot(Snapshot::from(&project))
            .unwrap();
        for _ in 0..41 {
            process_block(renderer.processor(), &mut buffer);
        }
        assert_eq!(renderer.controller.free_used_snapshots(), 1);
    }
    let status = renderer.controller.poll();
    assert!(status.peak > 0.0, "the synth made no sound");
    assert_eq!(status.dropped_note_events, 0);
}

/// Mixer changes while a loud chord plays: volume, pan and mute glide, the
/// master clips, and the processor never allocates. In stereo, and in mono
/// and on four channels, where the mix is folded down or padded out.
#[test]
fn mixer_changes_and_clipping_do_not_allocate() {
    rtsan_standalone::ensure_initialized();
    // Twelve square notes at full velocity: past full scale at +6 dB.
    let notes = (0..12u8)
        .map(|i| uta_core::Note {
            velocity: 127,
            ..note(u128::from(i), 40 + i * 2, 0, 3840)
        })
        .collect();
    let project = project(
        120.0,
        1,
        &[SynthParam::Osc1Wave(uta_core::Waveform::Pulse)],
        notes,
    );
    let strips = [
        (6.0, 0.0, false),
        (-20.0, -1.0, false),
        (6.0, 1.0, false),
        (6.0, 0.5, true),
        (3.0, -0.3, false),
    ];
    for channels in [1, 2, 4] {
        let config = EngineConfig {
            channels,
            ..stereo()
        };
        let snapshot = Snapshot::from(&project).with_volume_db(0.0);
        let mut renderer = Renderer::new(config, snapshot.clone(), BLOCK);
        let mut buffer = vec![0.0; BLOCK * channels];
        renderer.controller.play().unwrap();
        for _ in 0..4 {
            for (volume_db, pan, mute) in strips {
                let mixer = MixerStrip {
                    volume_db,
                    pan,
                    mute,
                    solo: false,
                };
                renderer
                    .controller
                    .set_snapshot(with_mixer(snapshot.clone(), mixer))
                    .unwrap();
                for _ in 0..3 {
                    process_block(renderer.processor(), &mut buffer);
                }
                renderer.controller.poll();
            }
        }
        let status = renderer.controller.poll();
        assert!(status.clips > 0, "{channels} channels: nothing clipped");
        assert!(status.playing);
    }
}

/// Edits while the loop plays, each sent the way the app sends them
/// (`set_project`), so every snapshot shares the clip's notes with the one
/// before it unless the edit touched them. The swaps move the playhead
/// (tempo and loop length changes) and release notes that were moved away
/// from it, several swaps arrive in one block, and the processor never
/// allocates, or frees what it shares with the control side. See RFC-002,
/// "The shared model", point 7.
#[test]
fn many_swaps_sharing_clip_data_do_not_allocate_or_free() {
    rtsan_standalone::ensure_initialized();
    // Legato eighth notes, so notes are always sounding when an edit lands.
    let notes: Vec<_> = (0..16u8)
        .map(|i| note(u128::from(i), 48 + i, u64::from(i) * 480, 480))
        .collect();
    let mut project = project(120.0, 2, &[SynthParam::ReleaseSeconds(0.01)], notes.clone());
    let track = project.tracks()[0].id();
    let clip = project.tracks()[0].clips()[0].id();
    let mut renderer = Renderer::new(stereo(), Snapshot::from(&project), BLOCK);
    let mut buffer = vec![0.0; BLOCK * 2];
    renderer.controller.play().unwrap();

    let mut shared = 0;
    for step in 0..600u32 {
        let before =
            std::sync::Arc::clone(&renderer.controller.snapshot().tracks()[0].notes().clips()[0]);
        let edits = 1 + step as usize % 3;
        for edit in 0..edits {
            let n = step as usize * 3 + edit;
            let command = match n % 5 {
                0 => Command::SetTempo {
                    bpm: 90.0 + (n % 60) as f32,
                },
                1 => Command::SetSynthParam {
                    track,
                    param: SynthParam::CutoffHz(300.0 + (n % 40) as f32 * 100.0),
                },
                2 => {
                    renderer
                        .controller
                        .set_volume_db(-6.0 - (n % 12) as f32)
                        .unwrap();
                    continue;
                }
                // Moves notes, so the clip's notes are new.
                3 => Command::SetNotes {
                    clip,
                    notes: notes
                        .iter()
                        .map(|note| uta_core::Note {
                            start: (note.start + (n as u64 % 7) * 60) % 7680,
                            ..*note
                        })
                        .collect(),
                },
                _ => Command::SetLoopLength {
                    bars: 1 + (n / 5 % 2) as u32,
                },
            };
            project.apply(&command).unwrap();
            renderer.controller.set_project(&project).unwrap();
        }
        let after = &renderer.controller.snapshot().tracks()[0].notes().clips()[0];
        if std::sync::Arc::ptr_eq(&before, after) {
            shared += 1;
        }
        drop(before);
        process_block(renderer.processor(), &mut buffer);
        process_block(renderer.processor(), &mut buffer);
        renderer.controller.poll();
    }
    assert!(shared > 100, "only {shared} steps shared the clip's notes");
    renderer.controller.stop().unwrap();
    for _ in 0..10 {
        process_block(renderer.processor(), &mut buffer);
    }
    let status = renderer.controller.poll();
    assert!(!status.playing);
    assert_eq!(status.dropped_note_events, 0);
}

/// The demo song's three tracks playing while tracks are duplicated, added,
/// removed and reordered, clips are added, moved, moved between tracks and
/// removed, and mute, solo and the sound change, each sent the way the app
/// sends it (`set_project`), with live notes on a slot too. A removed track's
/// notes fade out in its slot, and new tracks take slots, all without the
/// processor allocating or freeing. See RFC-003, "The shared model,
/// extended", points 4, 5 and 7.
#[test]
fn changing_tracks_and_clips_while_several_play_does_not_allocate() {
    rtsan_standalone::ensure_initialized();
    let mut project = demo_song();
    let mut renderer = Renderer::new(stereo(), Snapshot::from(&project), BLOCK);
    let mut buffer = vec![0.0; BLOCK * 2];
    renderer.controller.play().unwrap();

    let first = project.tracks()[0].id();
    let mut added_clip: Option<ClipId> = None;
    for step in 0..480usize {
        let tracks: Vec<TrackId> = project.tracks().iter().map(|track| track.id()).collect();
        // Synth settings go to the synth tracks, not the empty drum track.
        let synths: Vec<TrackId> = project
            .tracks()
            .iter()
            .filter(|track| matches!(track.source(), uta_core::Source::Synth(_)))
            .map(|track| track.id())
            .collect();
        // Removing a track removes its clips.
        if added_clip.is_some_and(|id| project.clip(id).is_none()) {
            added_clip = None;
        }
        let command = match step % 9 {
            // Duplicate a track, up to 12 of them.
            0 if tracks.len() < 12 => {
                let source = project.tracks()[step / 9 % tracks.len()].clone();
                let copy = source.copy(
                    TrackId::random(),
                    project.next_track_name(source.source().kind()),
                    uta_core::EffectId::random,
                    ClipId::random,
                    NoteId::random,
                );
                Command::AddTracks {
                    tracks: vec![PlacedTrack {
                        index: step % tracks.len(),
                        track: copy,
                    }],
                }
            }
            // Remove one, never the first.
            0 | 6 if tracks.len() > 3 => Command::RemoveTracks {
                tracks: vec![*tracks.iter().rev().find(|&&id| id != first).unwrap()],
            },
            1 => Command::MoveTrack {
                track: tracks[step % tracks.len()],
                index: 0,
            },
            2 if added_clip.is_none() => {
                let id = ClipId::random();
                added_clip = Some(id);
                let notes = (0..8u64)
                    .map(|i| uta_core::Note {
                        id: NoteId::random(),
                        ..note(0, 60 + i as u8, i * 480, 400)
                    })
                    .collect::<Vec<_>>();
                Command::AddClips {
                    clips: vec![PlacedClip {
                        track: synths[1 % synths.len()],
                        clip: Clip::new(id, 960 * (step as u64 % 8), 3840).with_notes(notes),
                    }],
                }
            }
            // Move the added clip along, and to another synth track.
            3 | 4 if added_clip.is_some() => Command::SetClips {
                clips: vec![ClipPosition {
                    id: added_clip.unwrap(),
                    track: synths[step % synths.len()],
                    start: 480 * (step as u64 % 16),
                    length: 3840,
                }],
            },
            5 if added_clip.is_some() && step % 2 == 1 => Command::RemoveClips {
                clips: vec![added_clip.take().unwrap()],
            },
            7 => Command::SetTrackMixer {
                track: tracks[step % tracks.len()],
                mixer: uta_core::MixerStrip {
                    mute: step % 4 == 3,
                    solo: step % 5 == 2,
                    ..uta_core::MixerStrip::default()
                },
            },
            _ => Command::SetSynthParam {
                track: synths[step % synths.len()],
                param: SynthParam::CutoffHz(300.0 + (step % 40) as f32 * 100.0),
            },
        };
        project.apply(&command).unwrap();
        renderer.controller.set_project(&project).unwrap();
        if step % 7 == 0 {
            let slot = renderer.controller.slot(tracks[0]).unwrap();
            let key = NoteKey(step as u128);
            renderer.controller.note_on(slot, key, 72, 90).unwrap();
            process_block(renderer.processor(), &mut buffer);
            renderer.controller.note_off(slot, key).unwrap();
        }
        for _ in 0..3 {
            process_block(renderer.processor(), &mut buffer);
        }
        renderer.controller.poll();
    }
    renderer.controller.stop().unwrap();
    for _ in 0..40 {
        process_block(renderer.processor(), &mut buffer);
    }
    let status = renderer.controller.poll();
    assert!(!status.playing);
    assert_eq!(status.dropped_note_events, 0);
    assert!(status.snapshots >= 480, "{} swaps", status.snapshots);
}

/// A song playing: the demo song with a long pad under it, with the loop
/// switched on and off and its region moved, so playback goes round it and
/// on to the song's end, jumps while playing, and pauses and carries on, so
/// notes are chased on Play, on every jump, on every wrap and on every
/// Continue. Meanwhile tracks are added, removed and reordered, and clips
/// added, moved and removed, each sent the way the app sends it. More notes
/// are under way at one jump than a block may start, so some are skipped.
/// None of it allocates or frees on the audio thread. See RFC-003, "Playing
/// a song".
#[test]
fn a_song_render_that_chases_jumps_and_wraps_while_editing_does_not_allocate() {
    rtsan_standalone::ensure_initialized();
    let mut project = demo_song();
    let first = project.tracks()[0].id();
    // A pad under the whole song, on the first track: 8 long chords, each
    // two bars, overlapping, so every jump and wrap lands in some.
    let pad = (0..24u64)
        .map(|i| uta_core::Note {
            id: NoteId::random(),
            ..note(0, 48 + (i % 12) as u8, (i / 3) * 1920, 7680)
        })
        .collect::<Vec<_>>();
    let pad_clip = ClipId::random();
    project
        .apply(&Command::AddClips {
            clips: vec![PlacedClip {
                track: first,
                clip: Clip::new(pad_clip, 0, 8 * 3840).with_notes(pad),
            }],
        })
        .unwrap();
    // And a clip of more long notes than a block may start at once.
    let crowd = (0..MAX_NOTE_EVENTS_PER_BLOCK as u64 + 20)
        .map(|i| uta_core::Note {
            id: NoteId::random(),
            ..note(0, 30 + (i % 60) as u8, i % 7, 3840)
        })
        .collect::<Vec<_>>();
    project
        .apply(&Command::AddClips {
            clips: vec![PlacedClip {
                track: project.tracks()[1].id(),
                clip: Clip::new(ClipId::random(), 12 * 3840, 3840).with_notes(crowd),
            }],
        })
        .unwrap();

    let mut renderer = Renderer::new(stereo(), Snapshot::from(&project), BLOCK);
    let mut buffer = vec![0.0; BLOCK * 2];
    renderer.controller.locate(3000).unwrap();
    renderer.controller.play().unwrap();

    let mut added_clip: Option<ClipId> = None;
    for step in 0..400usize {
        let tracks: Vec<TrackId> = project.tracks().iter().map(|track| track.id()).collect();
        // Synth settings go to the synth tracks, not the empty drum track.
        let synths: Vec<TrackId> = project
            .tracks()
            .iter()
            .filter(|track| matches!(track.source(), uta_core::Source::Synth(_)))
            .map(|track| track.id())
            .collect();
        if added_clip.is_some_and(|id| project.clip(id).is_none()) {
            added_clip = None;
        }
        let command = match step % 8 {
            0 => Command::SetLoop {
                start_bar: (step / 8 % 6) as u32,
                bars: 1 + (step / 16 % 3) as u32,
            },
            1 => Command::SetLoopEnabled {
                enabled: step % 3 != 0,
            },
            2 if tracks.len() < 10 => {
                let source = project.tracks()[step % tracks.len()].clone();
                Command::AddTracks {
                    tracks: vec![PlacedTrack {
                        index: step % tracks.len(),
                        track: source.copy(
                            TrackId::random(),
                            project.next_track_name(source.source().kind()),
                            uta_core::EffectId::random,
                            ClipId::random,
                            NoteId::random,
                        ),
                    }],
                }
            }
            2 | 5 if tracks.len() > 3 => Command::RemoveTracks {
                tracks: vec![*tracks.iter().rev().find(|&&id| id != first).unwrap()],
            },
            3 => Command::MoveTrack {
                track: tracks[step % tracks.len()],
                index: step % 3,
            },
            4 if added_clip.is_none() => {
                let id = ClipId::random();
                added_clip = Some(id);
                let notes = (0..6u64)
                    .map(|i| uta_core::Note {
                        id: NoteId::random(),
                        ..note(0, 60 + i as u8, i * 960, 2000)
                    })
                    .collect::<Vec<_>>();
                Command::AddClips {
                    clips: vec![PlacedClip {
                        track: synths[step % synths.len()],
                        clip: Clip::new(id, 3840 * (step as u64 % 10), 3840).with_notes(notes),
                    }],
                }
            }
            4 | 6 if added_clip.is_some() => Command::SetClips {
                clips: vec![ClipPosition {
                    id: added_clip.unwrap(),
                    track: synths[step % synths.len()],
                    start: 960 * (step as u64 % 40),
                    length: 3840,
                }],
            },
            7 if added_clip.is_some() => Command::RemoveClips {
                clips: vec![added_clip.take().unwrap()],
            },
            _ => Command::SetSynthParam {
                track: synths[step % synths.len()],
                param: SynthParam::CutoffHz(400.0 + (step % 30) as f32 * 100.0),
            },
        };
        project.apply(&command).unwrap();
        renderer.controller.set_project(&project).unwrap();
        match step % 5 {
            // Jump while playing, mid-pad; into the crowd now and then.
            0 => {
                let to = if step % 50 == 0 {
                    12 * 3840 + 500
                } else {
                    (step as u64 * 1237) % (10 * 3840)
                };
                renderer.controller.locate(to).unwrap();
            }
            // If the song's end stopped it, play again from the play start.
            1 if !renderer.controller.poll().playing => {
                renderer.controller.play().unwrap();
            }
            2 if step % 20 == 2 => {
                renderer.controller.stop().unwrap();
                renderer
                    .controller
                    .locate((step as u64 * 311) % (8 * 3840))
                    .unwrap();
                renderer.controller.play().unwrap();
            }
            // Pause now and then, mid-pad, and carry on from there a step
            // later, chasing what's under way.
            3 if step % 10 == 3 => renderer.controller.pause().unwrap(),
            4 if !renderer.controller.poll().playing => {
                renderer.controller.resume().unwrap();
            }
            _ => {}
        }
        for _ in 0..6 {
            process_block(renderer.processor(), &mut buffer);
        }
        renderer.controller.poll();
    }
    renderer.controller.stop().unwrap();
    for _ in 0..40 {
        process_block(renderer.processor(), &mut buffer);
    }
    let status = renderer.controller.poll();
    assert!(!status.playing);
    assert!(status.snapshots >= 400, "{} swaps", status.snapshots);
    assert!(
        status.dropped_note_events >= 20,
        "the crowd was chased past the limit: {} skipped",
        status.dropped_note_events
    );
}

/// Every sound's hits, fast repeats and flams, all eight sounds hitting at
/// once on every 16th from the default kit, closed hats choking open ones,
/// every control of each turned while they ring, the kick's model switched
/// while it rings, live hits, a jump and Play again (which restarts the
/// kit's noise and its metal, crossfading if the hats or the cymbal ring),
/// and more drum events in a block than the budget: none of it allocates or
/// frees on the audio thread.
#[test]
fn drum_hits_flams_and_control_turns_do_not_allocate() {
    rtsan_standalone::ensure_initialized();
    let beat = 960;
    let mut hits = Vec::new();
    let pitches = [
        KICK, SNARE, CLAP, CLOSED_HAT, OPEN_HAT, LOW_TOM, HIGH_TOM, CYMBAL,
    ];
    for (n, pitch) in pitches.into_iter().enumerate() {
        let first = 100 * n as u128;
        for i in 0..16u64 {
            // Each sound every 16th, with a flam 10 ms before each beat.
            let id = first + 2 * u128::from(i);
            hits.push(hit(id, pitch, 40 + i as u8 * 5, i * beat / 4));
            if i % 4 == 0 {
                hits.push(hit(id + 1, pitch, 64, (i * beat / 4).saturating_sub(19)));
            }
        }
    }
    // More hits on one sample than a block's budget of events.
    for i in 0..MAX_NOTE_EVENTS_PER_BLOCK as u128 {
        hits.push(hit(1000 + i, KICK, 100, beat / 4));
    }
    let mut project = drum_project_with_kit(KitSettings::default(), 120.0, 1, &[], hits);
    let mut renderer = Renderer::new(stereo(), Snapshot::from(&project), BLOCK);
    let mut buffer = vec![0.0; BLOCK * 2];
    let slot = renderer.controller.slot(drum_track()).unwrap();
    renderer.controller.play().unwrap();

    use DrumSound::{Clap, ClosedHat, Cymbal, HighTom, Kick, LowTom, OpenHat, Snare};
    let turns = [
        (Kick, DrumParam::TuneHz(80.0)),
        (Snare, DrumParam::TuneHz(260.0)),
        (Kick, DrumParam::Tone(1.0)),
        (Snare, DrumParam::Tone(0.04)),
        (Clap, DrumParam::Tone(2000.0)),
        (Kick, DrumParam::DecaySeconds(0.05)),
        (Clap, DrumParam::DecaySeconds(0.4)),
        (Snare, DrumParam::Snappy(1.0)),
        (Kick, DrumParam::LevelDb(-60.0)),
        (Snare, DrumParam::LevelDb(6.0)),
        (Clap, DrumParam::LevelDb(-60.0)),
        (Kick, DrumParam::TuneHz(40.0)),
        (Snare, DrumParam::TuneHz(140.0)),
        (Kick, DrumParam::Tone(0.0)),
        (Snare, DrumParam::Tone(0.4)),
        (Clap, DrumParam::Tone(700.0)),
        (Kick, DrumParam::DecaySeconds(0.8)),
        (Clap, DrumParam::DecaySeconds(0.05)),
        (Snare, DrumParam::Snappy(0.0)),
        (Kick, DrumParam::LevelDb(6.0)),
        (Snare, DrumParam::LevelDb(-60.0)),
        (Clap, DrumParam::LevelDb(6.0)),
        (ClosedHat, DrumParam::TuneHz(410.6)),
        (ClosedHat, DrumParam::Tone(12_000.0)),
        (ClosedHat, DrumParam::DecaySeconds(0.15)),
        (OpenHat, DrumParam::DecaySeconds(0.09)),
        (ClosedHat, DrumParam::LevelDb(-60.0)),
        (OpenHat, DrumParam::LevelDb(6.0)),
        (ClosedHat, DrumParam::TuneHz(102.65)),
        (ClosedHat, DrumParam::Tone(4000.0)),
        (ClosedHat, DrumParam::DecaySeconds(0.02)),
        (OpenHat, DrumParam::DecaySeconds(0.6)),
        (ClosedHat, DrumParam::LevelDb(6.0)),
        (OpenHat, DrumParam::LevelDb(-60.0)),
        (LowTom, DrumParam::TuneHz(100.0)),
        (HighTom, DrumParam::TuneHz(165.0)),
        (LowTom, DrumParam::DecaySeconds(0.1)),
        (HighTom, DrumParam::DecaySeconds(0.6)),
        (LowTom, DrumParam::LevelDb(-60.0)),
        (HighTom, DrumParam::LevelDb(6.0)),
        (LowTom, DrumParam::TuneHz(80.0)),
        (HighTom, DrumParam::TuneHz(220.0)),
        (LowTom, DrumParam::DecaySeconds(0.6)),
        (HighTom, DrumParam::DecaySeconds(0.1)),
        (LowTom, DrumParam::LevelDb(6.0)),
        (HighTom, DrumParam::LevelDb(-60.0)),
        (Cymbal, DrumParam::Tone(1.0)),
        (Cymbal, DrumParam::DecaySeconds(0.35)),
        (Cymbal, DrumParam::LevelDb(6.0)),
        (Cymbal, DrumParam::Tone(0.0)),
        (Cymbal, DrumParam::DecaySeconds(1.2)),
        (Cymbal, DrumParam::LevelDb(-60.0)),
        // The 909 kick, switched to while the 808 rings, and back.
        (Kick, DrumParam::Model(KickModel::Tr909)),
        (Kick, DrumParam::TuneHz(70.0)),
        (Kick, DrumParam::Sweep(1.0)),
        (Kick, DrumParam::Attack(1.0)),
        (Kick, DrumParam::DecaySeconds(1.5)),
        (Kick, DrumParam::LevelDb(-60.0)),
        (Kick, DrumParam::TuneHz(45.0)),
        (Kick, DrumParam::Sweep(0.0)),
        (Kick, DrumParam::Attack(0.0)),
        (Kick, DrumParam::DecaySeconds(0.1)),
        (Kick, DrumParam::LevelDb(6.0)),
        (Kick, DrumParam::Model(KickModel::Tr808)),
    ];
    for (round, &(sound, param)) in turns.iter().cycle().take(3 * turns.len()).enumerate() {
        project
            .apply(&Command::SetDrumParam {
                track: drum_track(),
                sound,
                param,
            })
            .unwrap();
        renderer.controller.set_project(&project).unwrap();
        renderer
            .controller
            .note_on(
                slot,
                NoteKey(round as u128),
                pitches[round % pitches.len()],
                127,
            )
            .unwrap();
        renderer
            .controller
            .note_off(slot, NoteKey(round as u128))
            .unwrap();
        if round % 10 == 9 {
            renderer.controller.locate(beat / 2).unwrap();
        }
        if round % 30 == 20 {
            renderer.controller.stop().unwrap();
            renderer.controller.play().unwrap();
        }
        for _ in 0..8 {
            process_block(renderer.processor(), &mut buffer);
        }
        renderer.controller.free_used_snapshots();
    }

    let status = renderer.controller.poll();
    assert!(status.peak > 0.0, "the kit made no sound");
    assert!(
        status.dropped_note_events > 0,
        "the budget was never reached: {status:?}"
    );
}

/// A slot changing kind of track while it sounds, both ways, with every
/// slot taken so the new track has to take the old one's: a held synth
/// note's slot goes to a drum track that hits at once, and a ringing kick's
/// slot goes to a synth track that plays at once. Neither allocates or frees
/// on the audio thread.
#[test]
fn a_slot_changing_kind_of_track_does_not_allocate() {
    rtsan_standalone::ensure_initialized();
    let project = with_every_slot_taken(drum_project(120.0, 1, &[], vec![]));
    let synth = project.tracks()[0].id();
    let mut renderer = Renderer::new(stereo(), Snapshot::from(&project), BLOCK);
    let mut buffer = vec![0.0; BLOCK * 2];
    let slot = renderer.controller.slot(synth).unwrap();
    let blocks = |renderer: &mut Renderer, buffer: &mut [f32]| {
        for _ in 0..40 {
            process_block(renderer.processor(), buffer);
        }
        renderer.controller.free_used_snapshots();
    };

    // A held note on the synth.
    renderer
        .controller
        .note_on(slot, NoteKey(1), 45, 127)
        .unwrap();
    blocks(&mut renderer, &mut buffer);
    // Synth to drums, while the note sounds.
    let drums = replace_track(&project, synth, empty_drum_track(61));
    renderer.controller.set_project(&drums).unwrap();
    assert_eq!(renderer.controller.slot(drums.tracks()[0].id()), Some(slot));
    renderer
        .controller
        .note_on(slot, NoteKey(2), KICK, 127)
        .unwrap();
    blocks(&mut renderer, &mut buffer);
    assert!(renderer.controller.poll().sounding_slots & (1 << slot) != 0);
    // Drums to synth, while the kick rings.
    let synth_again = replace_track(&drums, drums.tracks()[0].id(), empty_sine_track(62));
    renderer.controller.set_project(&synth_again).unwrap();
    assert_eq!(
        renderer.controller.slot(synth_again.tracks()[0].id()),
        Some(slot)
    );
    renderer
        .controller
        .note_on(slot, NoteKey(3), 45, 127)
        .unwrap();
    blocks(&mut renderer, &mut buffer);

    let status = renderer.controller.poll();
    assert!(status.peak > 0.0);
}

/// A block with more note events than it may handle catches up on the rest
/// with searches, not a loop over each one, and still doesn't allocate,
/// with a snapshot swapped in every block. Every step of the catch-up runs:
/// - at the start, 1,044 notes start: 512 are handled, and the rest find no
///   free voice and are counted;
/// - an eighth in, 1,024 of them end and 8 more start: the ended notes are
///   released, the 8 start, and 8 of the 20 long notes still under way,
///   which never got a voice, start too.
#[test]
fn too_many_note_events_do_not_allocate() {
    rtsan_standalone::ensure_initialized();
    let short = 2 * MAX_NOTE_EVENTS_PER_BLOCK as u128;
    let long = 20;
    let starting = 8;
    let mut notes: Vec<_> = (0..short)
        .map(|i| note(i, 40 + (i % 60) as u8, 0, 480))
        .collect();
    notes.extend((short..short + long).map(|i| note(i, 40 + (i % 60) as u8, 0, 1920)));
    notes.extend(
        (short + long..short + long + starting).map(|i| note(i, 40 + (i % 60) as u8, 480, 480)),
    );
    let project = project(120.0, 1, &[], notes);
    let mut renderer = Renderer::new(stereo(), Snapshot::from(&project), BLOCK);
    let mut buffer = vec![0.0; BLOCK * 2];
    renderer.controller.play().unwrap();
    // Past the long notes' ends.
    for _ in 0..400 {
        renderer
            .controller
            .set_snapshot(Snapshot::from(&project))
            .unwrap();
        process_block(renderer.processor(), &mut buffer);
    }
    assert_eq!(
        renderer.controller.poll().dropped_note_events,
        (short + long) as u64 - MAX_NOTE_EVENTS_PER_BLOCK as u64
    );
}

/// When nobody collects the used snapshots, the processor must neither free
/// them nor block: it leaves further swaps queued until there's room.
#[test]
fn full_used_snapshot_queue_defers_swaps_without_freeing() {
    let mut renderer = renderer();
    let mut buffer = vec![0.0; BLOCK * 2];

    // One more swap than the used-snapshot queue can hold. `set_snapshot`
    // frees used snapshots first, so send them all before processing.
    let swaps = USED_SNAPSHOT_CAPACITY + 1;
    for i in 0..swaps {
        renderer
            .controller
            .set_volume_db(-(i as f32) / 10.0)
            .unwrap();
    }
    process_block(renderer.processor(), &mut buffer);
    process_block(renderer.processor(), &mut buffer);

    // Only a queue's worth came back; the last swap is still waiting.
    assert_eq!(
        renderer.controller.free_used_snapshots(),
        USED_SNAPSHOT_CAPACITY
    );
    process_block(renderer.processor(), &mut buffer);
    assert_eq!(renderer.controller.free_used_snapshots(), 1);
}

/// When nobody reads status, the processor drops status messages rather than
/// waiting, and the peak and the slowest block carry over to the next message
/// that gets through.
#[test]
fn full_status_queue_does_not_block_or_allocate() {
    let mut renderer = renderer();
    let mut buffer = vec![0.0; BLOCK * 2];
    renderer.controller.play().unwrap();
    for _ in 0..STATUS_CAPACITY * 2 {
        process_block(renderer.processor(), &mut buffer);
    }
    let status = renderer.controller.poll();
    assert!(status.playing);
    assert_eq!(status.position, (STATUS_CAPACITY * BLOCK) as u64);

    // With room again, the next message carries the latest position.
    process_block(renderer.processor(), &mut buffer);
    let status = renderer.controller.poll();
    assert_eq!(
        status.position,
        (STATUS_CAPACITY * 2 * BLOCK + BLOCK) as u64
    );
    assert!(status.peak > 0.0);
    assert!(status.slowest_block > 0.0);
}

#[test]
fn dropouts_are_reported_in_status() {
    let mut renderer = renderer();
    let mut buffer = vec![0.0; BLOCK * 2];
    assert_no_alloc(|| {
        renderer.processor().note_dropout();
        renderer.processor().note_dropout();
    });
    process_block(renderer.processor(), &mut buffer);
    assert_eq!(renderer.controller.poll().dropouts, 2);
}

/// Timing each block reads the clock on the audio thread, which must neither
/// wait nor allocate: through a snapshot swap, and a dropout reported the way
/// a device error reports it.
#[test]
fn the_slowest_block_is_measured_without_allocating() {
    let mut renderer = renderer();
    let (mut errors, mut codes) = ErrorCallback::new();
    let mut buffer = vec![0.0; BLOCK * 2];
    renderer.controller.play().unwrap();
    process_block(renderer.processor(), &mut buffer);
    let status = renderer.controller.poll();
    assert!(
        status.slowest_block > 0.0 && status.slowest_block.is_finite(),
        "{status:?}"
    );

    renderer.controller.set_volume_db(-6.0).unwrap();
    let error = device_error(cpal::ErrorKind::Xrun);
    assert_no_alloc(|| errors.report(error));
    assert_eq!(codes.pop(), Ok(DeviceError::Xrun));
    assert_no_alloc(|| renderer.processor().note_dropout());
    process_block(renderer.processor(), &mut buffer);
    let status = renderer.controller.poll();
    assert_eq!((status.snapshots, status.dropouts), (1, 1));
    assert!(status.slowest_block > 0.0, "{status:?}");

    // With no block since, there's nothing to report.
    assert_eq!(renderer.controller.poll().slowest_block, 0.0);
}

#[test]
fn odd_block_sizes_do_not_allocate() {
    let mut renderer = renderer();
    let mut buffer = vec![0.0; 1024 * 2];
    renderer.controller.play().unwrap();
    for frames in [1, 7, 32, 64, 128, 333, 1024] {
        process_block(renderer.processor(), &mut buffer[..frames * 2]);
    }
    assert_eq!(
        renderer.controller.poll().position,
        1 + 7 + 32 + 64 + 128 + 333 + 1024
    );
}

/// Simulated device errors, as cpal builds them on the paths that can reach
/// the audio thread: a kind and a static message, or no message.
fn device_error(kind: cpal::ErrorKind) -> cpal::Error {
    match kind {
        cpal::ErrorKind::Xrun => kind.into(),
        _ => cpal::Error::with_message(kind, "simulated device error"),
    }
}

#[test]
fn error_callback_only_queues_a_code() {
    rtsan_standalone::ensure_initialized();
    let (mut callback, mut codes) = ErrorCallback::new();
    let kinds = [
        cpal::ErrorKind::Xrun,
        cpal::ErrorKind::DeviceChanged,
        cpal::ErrorKind::StreamInvalidated,
        cpal::ErrorKind::DeviceNotAvailable,
        cpal::ErrorKind::BackendError,
    ];
    for kind in kinds {
        let error = device_error(kind);
        assert_no_alloc(|| callback.report(error));
    }

    let received: Vec<_> = std::iter::from_fn(|| codes.pop().ok()).collect();
    assert_eq!(
        received,
        [
            DeviceError::Xrun,
            DeviceError::Changed,
            DeviceError::Invalidated,
            DeviceError::Gone,
            DeviceError::Other,
        ]
    );
}

/// A burst of xruns fills the queue: the extra codes are lost, but the
/// callback still neither waits nor allocates.
#[test]
fn full_error_queue_does_not_block_or_allocate() {
    rtsan_standalone::ensure_initialized();
    let (mut callback, mut codes) = ErrorCallback::new();
    for _ in 0..ERROR_CAPACITY * 2 {
        let error = device_error(cpal::ErrorKind::Xrun);
        assert_no_alloc(|| callback.report(error));
    }
    assert_eq!(
        std::iter::from_fn(|| codes.pop().ok()).count(),
        ERROR_CAPACITY
    );
}

#[test]
fn audio_callback_plays_and_hands_the_processor_back_without_allocating() {
    rtsan_standalone::ensure_initialized();
    let config = EngineConfig {
        sample_rate: 48_000,
        channels: 2,
    };
    let (mut controller, processor) = uta_engine::engine(config, Snapshot::from(&demo_loop()));
    let (mut callback, mut home, handover) = AudioCallback::new(processor);
    let mut buffer = vec![0.0; BLOCK * 2];

    controller.play().unwrap();
    for _ in 0..10 {
        assert_no_alloc(|| callback.render(&mut buffer));
    }
    assert!(controller.poll().peak > 0.0, "the callback made no sound");

    // Fading out ahead of a rebuild.
    handover.start();
    for _ in 0..10 {
        assert_no_alloc(|| callback.render(&mut buffer));
    }
    assert!(buffer.iter().all(|&s| s == 0.0), "it didn't fade out");

    // What happens when cpal drops a stream, wherever it drops it.
    assert_no_alloc(|| drop(callback));
    let mut processor = home.pop().expect("the processor didn't come home");
    process_block(&mut processor, &mut buffer);
    assert_eq!(controller.poll().position, (21 * BLOCK) as u64);
}

/// The blocks an effect takes to fade out, and one more.
fn fade_blocks(sample_rate: u32) -> usize {
    (uta_engine::EFFECT_FADE_SECONDS * f64::from(sample_rate)) as usize / BLOCK + 2
}

/// Adding Delays while the song plays, a full chain at once on a track and
/// on the master and two on each other track; turning their settings and
/// Mix; moving them within their chains; switching them off and on;
/// removing some and adding others in the same block; removing a track with
/// its chain and undoing it; and moving to a new sample rate between
/// streams: each new effect's state arrives with its snapshot, nothing on
/// the audio thread allocates or frees, and every state handed over comes
/// back to be freed once it has faded out.
#[test]
fn changing_effects_on_tracks_and_the_master_does_not_allocate_and_every_state_comes_back() {
    use uta_core::{
        Chain, DelayMode, DelayParam, Effect, EffectId, EffectKind, MAX_EFFECTS, PlacedEffect,
    };

    rtsan_standalone::ensure_initialized();
    let mut project = demo_song();
    let mut renderer = Renderer::new(stereo(), Snapshot::from(&project), BLOCK);
    let mut buffer = vec![0.0; BLOCK * 2];
    renderer.controller.play().unwrap();

    let mut next = 0u128;
    let mut delay = || {
        next += 1;
        Effect::default_of(
            EffectId::from_uuid(uuid::Uuid::from_u128(7000 + next)),
            EffectKind::Delay,
        )
    };
    let chains = |project: &uta_core::Project| -> Vec<Chain> {
        project
            .tracks()
            .iter()
            .map(|track| Chain::Track(track.id()))
            .chain([Chain::Master])
            .collect()
    };
    let effects = |project: &uta_core::Project| -> Vec<EffectId> {
        chains(project)
            .into_iter()
            .flat_map(|chain| project.chain(chain).unwrap().to_vec())
            .map(|effect| effect.id())
            .collect()
    };
    let change = |project: &mut uta_core::Project, renderer: &mut Renderer, command| {
        let inverse = project.apply(&command).unwrap();
        renderer.controller.set_project(project).unwrap();
        inverse
    };

    for (round, rate) in [44_100, 96_000, 48_000].into_iter().enumerate() {
        let tracks: Vec<_> = project.tracks().iter().map(|track| track.id()).collect();
        // A full chain on the first track and the master, and two on each
        // of the other tracks, all in one snapshot.
        let mut placed = Vec::new();
        for (n, chain) in chains(&project).into_iter().enumerate() {
            let full = n == 0 || chain == Chain::Master;
            let count = if full { MAX_EFFECTS } else { 2 };
            placed.extend((0..count).map(|index| PlacedEffect {
                chain,
                index,
                effect: delay(),
            }));
        }
        change(
            &mut project,
            &mut renderer,
            Command::AddEffects { effects: placed },
        );
        assert_eq!(
            renderer.controller.effect_states_out(),
            effects(&project).len()
        );
        // Long enough at any rate to reach a note: a rate change silences
        // the notes playing.
        for _ in 0..80 {
            process_block(renderer.processor(), &mut buffer);
        }
        let status = renderer.controller.poll();
        assert!(
            status.peak > 0.0,
            "the processor made no sound in round {round}: {status:?}"
        );

        // Turn every Delay's settings and Mix, a snapshot each, so its
        // time, Feedback, Mode, cuts, Drive and Wobble all glide.
        for (n, effect) in effects(&project).into_iter().enumerate() {
            let f = n as f32 / 32.0;
            let set = |param| Command::SetDelayParam { effect, param };
            for command in [
                set(DelayParam::Sync(n % 2 == 0)),
                set(DelayParam::TimeSeconds(0.01 + f)),
                set(DelayParam::Feedback(f.min(1.1))),
                set(DelayParam::Mode(if n % 3 == 0 {
                    DelayMode::PingPong
                } else {
                    DelayMode::Stereo
                })),
                set(DelayParam::LowCutHz(20.0 + 100.0 * f)),
                set(DelayParam::HighCutHz(20_000.0 - 1000.0 * f)),
                set(DelayParam::Drive(f.min(1.0))),
                set(DelayParam::Wobble(1.0 - f.min(1.0))),
                Command::SetEffectMix {
                    effect,
                    mix: 1.0 - f.min(1.0),
                },
            ] {
                change(&mut project, &mut renderer, command);
            }
            process_block(renderer.processor(), &mut buffer);
        }
        // Stop and Play again while the echoes ring: the Delays' wobble
        // restarts, gliding from where it was. Only in the last round, as
        // Play goes back to the top, where the next round's blocks wouldn't
        // reach a note.
        if rate == 48_000 {
            renderer.controller.stop().unwrap();
            process_block(renderer.processor(), &mut buffer);
            assert!(renderer.controller.poll().ringing_chains != 0);
            renderer.controller.play().unwrap();
            process_block(renderer.processor(), &mut buffer);
            process_block(renderer.processor(), &mut buffer);
        }

        // Move the last effect in each chain to the front, and the first
        // to the back, while the moves before are still fading.
        for chain in chains(&project) {
            let ids: Vec<_> = project
                .chain(chain)
                .unwrap()
                .iter()
                .map(Effect::id)
                .collect();
            let last = ids.len() - 1;
            for (effect, index) in [(ids[last], 0), (ids[0], last)] {
                change(
                    &mut project,
                    &mut renderer,
                    Command::MoveEffect { effect, index },
                );
                process_block(renderer.processor(), &mut buffer);
            }
        }

        // Switch every other effect off, and on again before some have
        // faded out.
        let switched: Vec<_> = effects(&project).into_iter().step_by(2).collect();
        for &effect in &switched {
            change(
                &mut project,
                &mut renderer,
                Command::SetEffectEnabled {
                    effect,
                    enabled: false,
                },
            );
        }
        process_block(renderer.processor(), &mut buffer);
        for (n, &effect) in switched.iter().enumerate() {
            change(
                &mut project,
                &mut renderer,
                Command::SetEffectEnabled {
                    effect,
                    enabled: true,
                },
            );
            if n % 4 == 0 {
                process_block(renderer.processor(), &mut buffer);
            }
        }

        // Remove half of them and add new ones in their place, two
        // snapshots in one block.
        let all = effects(&project);
        let removed: Vec<_> = all.iter().copied().step_by(2).collect();
        change(
            &mut project,
            &mut renderer,
            Command::RemoveEffects { effects: removed },
        );
        let refill = chains(&project)
            .into_iter()
            .map(|chain| PlacedEffect {
                chain,
                index: 0,
                effect: delay(),
            })
            .collect();
        change(
            &mut project,
            &mut renderer,
            Command::AddEffects { effects: refill },
        );
        for _ in 0..10 {
            process_block(renderer.processor(), &mut buffer);
        }

        // Remove a track with its chain, and undo it: its effects fade
        // out, and come back with new states.
        let removal = Command::RemoveTracks {
            tracks: vec![tracks[round % tracks.len()]],
        };
        let undo = change(&mut project, &mut renderer, removal);
        process_block(renderer.processor(), &mut buffer);
        change(&mut project, &mut renderer, undo);
        for _ in 0..10 {
            process_block(renderer.processor(), &mut buffer);
        }
        renderer.controller.poll();
        assert_eq!(
            renderer.controller.effect_states_out(),
            effects(&project).len(),
            "round {round}"
        );

        // A new device's rate: every state is made again on the control
        // side, between streams, and the controller sends the snapshot
        // again, retimed.
        renderer.processor().prepare(rate, 2);
        for _ in 0..4 {
            process_block(renderer.processor(), &mut buffer);
            renderer.controller.poll();
        }
        assert_eq!(renderer.controller.poll().sample_rate, rate);
        assert_eq!(
            renderer.controller.effect_states_out(),
            effects(&project).len(),
            "round {round}"
        );

        // Then every effect goes, and every state comes back once it has
        // faded out.
        let all = effects(&project);
        change(
            &mut project,
            &mut renderer,
            Command::RemoveEffects { effects: all },
        );
        for _ in 0..fade_blocks(rate) {
            process_block(renderer.processor(), &mut buffer);
        }
        renderer.controller.poll();
        assert_eq!(renderer.controller.effect_states_out(), 0, "round {round}");
    }
}

/// When nobody collects the effect states sent back, the processor must
/// neither free them nor block. It leaves a swap queued until there's room
/// for every state the audio thread can hold. Ten swaps queue up, filling
/// every chain and emptying it five times. The ninth's new effects push out
/// effects still fading from before, and the fading ones go back as they
/// finish, which fills the queue; the tenth waits until it's emptied.
#[test]
fn full_used_state_queue_defers_swaps_without_freeing() {
    use uta_core::{
        Chain, Effect, EffectId, EffectKind, MAX_EFFECTS, PlacedEffect, Project, Source,
        SynthSettings, Track,
    };
    use uta_engine::{MAX_EFFECT_STATES, USED_STATE_CAPACITY};

    rtsan_standalone::ensure_initialized();
    // A low rate keeps each Delay's tape small, and its fade 10 samples.
    let config = EngineConfig {
        sample_rate: 1000,
        channels: 2,
    };
    let mut project = Project::new();
    let tracks = (project.tracks().len()..Project::MAX_TRACKS)
        .map(|index| PlacedTrack {
            index,
            track: Track::new(
                TrackId::random(),
                format!("Synth {index}"),
                Source::Synth(SynthSettings::default()),
            ),
        })
        .collect();
    project.apply(&Command::AddTracks { tracks }).unwrap();
    let mut renderer = Renderer::new(config, Snapshot::from(&project), BLOCK);
    let mut buffer = vec![0.0; BLOCK * 2];

    // Every track's chain and the master's, full.
    let chains: Vec<Chain> = project
        .tracks()
        .iter()
        .map(|track| Chain::Track(track.id()))
        .chain([Chain::Master])
        .collect();
    let full = chains.len() * MAX_EFFECTS;
    let fill = |project: &mut Project| {
        let effects = chains
            .iter()
            .flat_map(|&chain| {
                (0..MAX_EFFECTS).map(move |index| PlacedEffect {
                    chain,
                    index,
                    effect: Effect::default_of(EffectId::random(), EffectKind::Delay),
                })
            })
            .collect();
        project.apply(&Command::AddEffects { effects }).unwrap()
    };
    for _ in 0..5 {
        let empty = fill(&mut project);
        renderer.controller.set_project(&project).unwrap();
        project.apply(&empty).unwrap();
        renderer.controller.set_project(&project).unwrap();
    }
    assert_eq!(USED_STATE_CAPACITY, 2 * MAX_EFFECT_STATES);
    assert_eq!(MAX_EFFECT_STATES, 2 * full);
    assert_eq!(renderer.controller.effect_states_out(), 5 * full);

    // In each chain, the third, fourth and fifth fills each push out the
    // effects removed two swaps before, which haven't faded yet: three
    // fulls go back at once. The tenth swap would need room for every state
    // the audio thread can hold, so it waits. The fourth fill's effects
    // finish fading in this block, and go back too, which fills the queue.
    process_block(renderer.processor(), &mut buffer);
    assert_eq!(renderer.controller.free_used_snapshots(), 9);
    assert_eq!(renderer.controller.effect_states_out(), full);

    // With the queue emptied, the tenth swap goes through, and the fifth
    // fill's effects fade out and come back.
    process_block(renderer.processor(), &mut buffer);
    assert_eq!(renderer.controller.free_used_snapshots(), 1);
    assert_eq!(renderer.controller.effect_states_out(), 0);
}
