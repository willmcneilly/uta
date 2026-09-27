//! Block timing against the real-time deadline. Reported, never gating: shared
//! CI machines are too noisy to fail on. Ignored by default because debug
//! timings mean nothing. Run it with
//! `cargo test -p uta-engine --release --test timing -- --ignored --nocapture`.
//! In CI it also writes the table to the job summary.

use std::io::Write;
use std::time::{Duration, Instant};

use uta_engine::offline::Renderer;
use uta_engine::{EngineConfig, Snapshot};

const SAMPLE_RATE: u32 = 48_000;
const CHANNELS: usize = 2;
const BLOCKS: usize = 20_000;

struct Report {
    block_size: usize,
    deadline: Duration,
    p50: Duration,
    p99: Duration,
    max: Duration,
}

fn measure(block_size: usize) -> Report {
    let config = EngineConfig {
        sample_rate: SAMPLE_RATE,
        channels: CHANNELS,
    };
    let mut renderer = Renderer::new(config, Snapshot::default(), block_size);
    let mut buffer = vec![0.0; block_size * CHANNELS];
    let mut times = Vec::with_capacity(BLOCKS);
    renderer.controller.play().unwrap();
    for i in 0..BLOCKS {
        // Keep the smoothing busy, as a user dragging the volume would.
        if i % 64 == 0 {
            renderer
                .controller
                .set_volume_db(-(((i / 64) % 24) as f32))
                .unwrap();
        }
        let start = Instant::now();
        renderer.processor().process(&mut buffer);
        times.push(start.elapsed());
        renderer.controller.poll();
    }
    times.sort_unstable();
    let percentile = |p: f64| times[((times.len() - 1) as f64 * p).round() as usize];
    Report {
        block_size,
        deadline: Duration::from_secs_f64(block_size as f64 / f64::from(SAMPLE_RATE)),
        p50: percentile(0.5),
        p99: percentile(0.99),
        max: *times.last().unwrap(),
    }
}

fn micros(duration: Duration) -> String {
    format!("{:.2} µs", duration.as_secs_f64() * 1e6)
}

fn share(duration: Duration, deadline: Duration) -> String {
    format!(
        "{:.3}%",
        duration.as_secs_f64() / deadline.as_secs_f64() * 100.0
    )
}

#[test]
#[ignore = "timing report: run in release with --ignored --nocapture"]
fn report_block_timing() {
    let mut table = format!(
        "### Block timing ({SAMPLE_RATE} Hz, {CHANNELS} channels, {BLOCKS} blocks)\n\n\
         | Block | Deadline | p50 | p99 | Max | p99 of deadline |\n\
         |---|---|---|---|---|---|\n"
    );
    for block_size in [32, 128, 1024] {
        let r = measure(block_size);
        table += &format!(
            "| {} | {} | {} | {} | {} | {} |\n",
            r.block_size,
            micros(r.deadline),
            micros(r.p50),
            micros(r.p99),
            micros(r.max),
            share(r.p99, r.deadline),
        );
    }
    if cfg!(debug_assertions) {
        table += "\nDebug build: these numbers are not representative.\n";
    }
    println!("{table}");
    if let Some(path) = std::env::var_os("GITHUB_STEP_SUMMARY") {
        let mut summary = std::fs::OpenOptions::new()
            .append(true)
            .create(true)
            .open(path)
            .unwrap();
        writeln!(summary, "{table}").unwrap();
    }
}
