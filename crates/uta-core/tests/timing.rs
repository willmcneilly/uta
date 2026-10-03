//! What copy-on-write costs: editing one note in a 3,000-note clip whose
//! notes are shared, so the edit copies them first. See RFC-004, "Risks &
//! unknowns" (copy-on-write costs a copy). Reported, never gating, like the
//! engine's block timing. Ignored by default because debug timings mean
//! nothing. Run it with
//! `cargo test -p uta-core --release --test timing -- --ignored --nocapture`.
//! In CI it also writes the table to the job summary.

use std::io::Write;
use std::time::{Duration, Instant};

use uta_core::{Command, Note, NoteId, Project, ProjectId};
use uuid::Uuid;

/// As many notes as one clip of the check-7 song holds.
const NOTES: u32 = 3_000;
const EDITS: usize = 2_000;

/// A project whose first clip holds [`NOTES`] short notes spread over it.
fn project() -> Project {
    let mut project = Project::with_id(ProjectId::from_uuid(Uuid::from_u128(1)));
    let clip = project.tracks()[0].clips()[0].id();
    let length = project.tracks()[0].clips()[0].length();
    let notes = (0..NOTES)
        .map(|i| Note {
            id: NoteId::from_uuid(Uuid::from_u128(1_000 + u128::from(i))),
            pitch: 36 + (i % 48) as u8,
            velocity: 100,
            start: u64::from(i) * length / u64::from(NOTES),
            length: 60,
        })
        .collect();
    project.apply(&Command::AddNotes { clip, notes }).unwrap();
    project
}

/// Times [`EDITS`] edits that each move one note. With `shared`, a copy of
/// the project holds on to the clip's notes during each edit, as the
/// engine's snapshot does in the app, so the edit copies them.
fn time_edits(shared: bool) -> Vec<Duration> {
    let mut project = project();
    let clip = project.tracks()[0].clips()[0].id();
    let mut times = Vec::with_capacity(EDITS);
    for edit in 0..EDITS {
        let note = *project
            .clip(clip)
            .unwrap()
            .notes()
            .nth(edit % NOTES as usize)
            .unwrap();
        let command = Command::SetNotes {
            clip,
            notes: vec![Note {
                start: (note.start + 1) % 3840,
                ..note
            }],
        };
        let previous = shared.then(|| project.clone());
        let started = Instant::now();
        project.apply(&command).unwrap();
        times.push(started.elapsed());
        if let Some(previous) = &previous {
            assert!(!previous.tracks()[0].clips()[0].shares_notes(&project.tracks()[0].clips()[0]));
        }
        // Freed outside the timing.
        drop(previous);
    }
    times.sort_unstable();
    times
}

fn percentile(sorted: &[Duration], p: f64) -> Duration {
    sorted[((sorted.len() - 1) as f64 * p).round() as usize]
}

fn ms(duration: Duration) -> String {
    format!("{:.3} ms", duration.as_secs_f64() * 1000.0)
}

#[test]
#[ignore = "timing report: run in release with --ignored --nocapture"]
fn copy_on_write_timing_report() {
    let mut table = format!(
        "### Editing one note in a 3,000-note clip ({EDITS} edits)\n\n\
         | Notes | p50 | p99 | max |\n|---|---|---|---|\n"
    );
    for (name, shared) in [
        ("Shared, so copied first (as in the app)", true),
        ("Not shared, changed in place", false),
    ] {
        let times = time_edits(shared);
        table += &format!(
            "| {name} | {} | {} | {} |\n",
            ms(percentile(&times, 0.5)),
            ms(percentile(&times, 0.99)),
            ms(*times.last().unwrap()),
        );
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
