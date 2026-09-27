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
use common::{demo_loop, note, project};
use uta_core::{Command, SynthParam};
use uta_engine::live::{AudioCallback, DeviceError, ERROR_CAPACITY, ErrorCallback};
use uta_engine::offline::Renderer;
use uta_engine::{
    EngineConfig, MAX_NOTE_EVENTS_PER_BLOCK, NoteKey, Processor, STATUS_CAPACITY, Snapshot,
    SynthSettings, USED_SNAPSHOT_CAPACITY, VOICES, Waveform,
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

    let new = Snapshot {
        gain: 0.25,
        ..Snapshot::from(&demo_loop())
    };
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
/// change every synth setting while they sound: none of it allocates or
/// frees on the audio thread.
#[test]
fn notes_take_overs_and_synth_changes_do_not_allocate() {
    let mut renderer = renderer();
    let mut buffer = vec![0.0; BLOCK * 2];
    let waveforms = [
        Waveform::Sine,
        Waveform::Triangle,
        Waveform::Saw,
        Waveform::Square,
    ];
    let mut swaps = 0;
    for round in 0..8u8 {
        // Twice as many notes as voices, all in one block, then more.
        for i in 0..2 * VOICES as u8 {
            let key = NoteKey(u128::from(round) * 100 + u128::from(i));
            renderer
                .controller
                .note_on(key, 36 + i, 20 + i * 3)
                .unwrap();
        }
        process_block(renderer.processor(), &mut buffer);
        renderer
            .controller
            .set_synth_settings(SynthSettings {
                waveform: waveforms[usize::from(round) % 4],
                cutoff_hz: 20_000.0 / f32::from(round + 1).powi(3),
                resonance: f32::from(round % 3) / 2.0,
                attack_seconds: 0.001 * f32::from(round + 1),
                decay_seconds: 0.05,
                sustain: f32::from(round) / 8.0,
                release_seconds: 0.01,
            })
            .unwrap();
        swaps += 1;
        for _ in 0..4 {
            process_block(renderer.processor(), &mut buffer);
        }
        for i in (0..2 * VOICES as u8).step_by(3) {
            let key = NoteKey(u128::from(round) * 100 + u128::from(i));
            renderer.controller.note_off(key).unwrap();
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
/// taken over, and edits arriving as new snapshots that share the clips'
/// notes with the old ones. None of it allocates or frees on the audio
/// thread.
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
            if block == blocks_per_pass / 2 {
                // An edit: the same notes, a new cutoff. The clip's notes are
                // shared with the snapshot the processor is playing.
                project
                    .apply(&Command::SetSynthParam {
                        track,
                        param: SynthParam::CutoffHz(500.0 + 1000.0 * pass as f32),
                    })
                    .unwrap();
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

/// A block with more note events than it may handle skips the rest with a
/// search, not a loop over each one, and still doesn't allocate.
#[test]
fn too_many_note_events_do_not_allocate() {
    rtsan_standalone::ensure_initialized();
    let count = 2 * MAX_NOTE_EVENTS_PER_BLOCK;
    let notes = (0..count as u128)
        .map(|i| note(i, 40 + (i % 60) as u8, 0, 480))
        .collect();
    let project = project(120.0, 1, &[], notes);
    let mut renderer = Renderer::new(stereo(), Snapshot::from(&project), BLOCK);
    let mut buffer = vec![0.0; BLOCK * 2];
    renderer.controller.play().unwrap();
    for _ in 0..100 {
        process_block(renderer.processor(), &mut buffer);
    }
    assert_eq!(
        renderer.controller.poll().dropped_note_events,
        (2 * (count - MAX_NOTE_EVENTS_PER_BLOCK)) as u64
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
/// waiting, and the peak carries over to the next message that gets through.
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
