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

use assert_no_alloc::{AllocDisabler, assert_no_alloc};
use uta_engine::live::{AudioCallback, DeviceError, ERROR_CAPACITY, ErrorCallback};
use uta_engine::offline::Renderer;
use uta_engine::{EngineConfig, Processor, STATUS_CAPACITY, Snapshot, USED_SNAPSHOT_CAPACITY};

#[global_allocator]
static ALLOCATOR: AllocDisabler = AllocDisabler;

const BLOCK: usize = 128;

fn renderer() -> Renderer {
    rtsan_standalone::ensure_initialized();
    Renderer::new(
        EngineConfig {
            sample_rate: 48_000,
            channels: 2,
        },
        Snapshot::default(),
        BLOCK,
    )
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
        frequency_hz: 660.0,
        gain: 0.25,
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
    let (mut controller, processor) = uta_engine::engine(config, Snapshot::default());
    let (mut callback, mut home) = AudioCallback::new(processor);
    let mut buffer = vec![0.0; BLOCK * 2];

    controller.play().unwrap();
    for _ in 0..10 {
        assert_no_alloc(|| callback.render(&mut buffer));
    }
    assert!(controller.poll().peak > 0.0, "the callback made no sound");

    // What happens when cpal drops a stream, wherever it drops it.
    assert_no_alloc(|| drop(callback));
    let mut processor = home.pop().expect("the processor didn't come home");
    process_block(&mut processor, &mut buffer);
    assert_eq!(controller.poll().position, (11 * BLOCK) as u64);
}
