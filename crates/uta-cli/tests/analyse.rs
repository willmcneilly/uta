//! `uta analyse`, run as the built binary on WAVs these tests render with
//! `uta render`.

use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;
use uta_analysis::report::MEASUREMENTS;
use uta_engine::EngineConfig;
use uta_engine::offline::{read_wav, write_wav};

/// A project with one sine synth playing C3 for a beat, with the loop off.
const C3_SINE: &str = r#"{
  "project": "6f1c2a4e-0d8b-5c7a-9e3f-1a2b3c4d5e6f",
  "commands": [
    { "format": 6, "command": { "type": "set_tempo", "bpm": 120.0 } },
    { "format": 6, "command": { "type": "set_loop_enabled", "enabled": false } },
    { "format": 6, "command": { "type": "add_tracks", "tracks": [{
      "index": 1,
      "track": {
        "id": "7a2b3c4d-5e6f-5a1b-8c2d-3e4f5a6b7c8d",
        "name": "Sine",
        "source": { "synth": { "osc1_wave": "sine" } },
        "mixer": { "volume_db": 0.0, "pan": 0.0, "mute": false, "solo": false },
        "clips": [{
          "id": "8b3c4d5e-6f7a-5b2c-9d3e-4f5a6b7c8d9e",
          "start": 0,
          "length": 960,
          "notes": [{
            "id": "9c4d5e6f-7a8b-5c3d-8e4f-5a6b7c8d9e0f",
            "pitch": 48,
            "velocity": 100,
            "start": 0,
            "length": 960
          }]
        }]
      }
    }] } }
  ]
}"#;

/// [`C3_SINE`] with its note moved to `pitch`.
fn sine_at(pitch: u8) -> String {
    C3_SINE.replace("\"pitch\": 48", &format!("\"pitch\": {pitch}"))
}

/// A folder of its own for each test, in Cargo's scratch space.
fn folder(test: &str) -> PathBuf {
    let folder = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("analyse")
        .join(test);
    std::fs::create_dir_all(&folder).unwrap();
    folder
}

/// Runs `uta` with `args`, and returns what it printed. It must succeed.
fn uta(args: &[&str]) -> String {
    let output = Command::new(env!("CARGO_BIN_EXE_uta"))
        .args(args)
        .output()
        .expect("uta runs");
    assert!(
        output.status.success(),
        "uta {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("uta prints text")
}

/// Renders the C3 sine to `sine.wav` in `folder`, with `uta render`.
fn render_sine(folder: &Path) -> String {
    render(folder, "sine", C3_SINE)
}

/// Renders a command list to `<name>.wav` in `folder`, with `uta render`.
fn render(folder: &Path, name: &str, commands_json: &str) -> String {
    let commands = folder.join(format!("{name}.json"));
    std::fs::write(&commands, commands_json).unwrap();
    let wav = folder.join(format!("{name}.wav"));
    uta(&[
        "render",
        wav.to_str().unwrap(),
        "--commands",
        commands.to_str().unwrap(),
    ]);
    wav.to_str().unwrap().to_string()
}

/// A copy of `wav` with every sample at half the level (6 dB down).
fn quieter_copy(wav: &str) -> String {
    let (spec, samples) = read_wav(Path::new(wav)).unwrap();
    let quieter: Vec<f32> = samples.iter().map(|s| s * 0.5).collect();
    let path = wav.replace(".wav", "-quieter.wav");
    let config = EngineConfig {
        channels: usize::from(spec.channels),
        sample_rate: spec.sample_rate,
    };
    write_wav(Path::new(&path), config, &quieter).unwrap();
    path
}

fn json(args: &[&str]) -> Value {
    serde_json::from_str(&uta(args)).expect("--json prints JSON")
}

/// Checks a report has the documented shape: its fields, then a number or
/// `null` for every measurement, under its key, in the table's order.
fn assert_report_shape(report: &Value) {
    let fields: Vec<&str> = report
        .as_object()
        .expect("a report is an object")
        .keys()
        .map(String::as_str)
        .collect();
    let mut expected = vec![
        "above_hz",
        "channels",
        "file",
        "measurements",
        "note",
        "sample_rate",
        "seconds",
    ];
    let mut sorted = fields.clone();
    sorted.sort_unstable();
    expected.sort_unstable();
    assert_eq!(sorted, expected);
    assert!(report["file"].is_string());
    assert_eq!(report["sample_rate"], 48_000);
    assert_eq!(report["channels"], 2);
    assert!(report["seconds"].as_f64().unwrap() > 0.5);
    assert!(report["above_hz"].is_number());
    let measurements = report["measurements"].as_object().unwrap();
    let keys: Vec<&str> = measurements.keys().map(String::as_str).collect();
    let mut table: Vec<&str> = MEASUREMENTS.iter().map(|m| m.key).collect();
    let mut sorted_keys = keys.clone();
    sorted_keys.sort_unstable();
    table.sort_unstable();
    assert_eq!(sorted_keys, table, "a value for every measurement");
    for (key, value) in measurements {
        assert!(value.is_number() || value.is_null(), "{key} is {value}");
    }
}

#[test]
fn the_report_has_a_line_for_every_measurement() {
    let wav = render_sine(&folder("every_measurement"));
    let text = uta(&["analyse", &wav]);
    let lines: Vec<&str> = text.lines().collect();
    assert!(lines[0].starts_with(&wav), "{}", lines[0]);
    assert_eq!(lines.len(), 1 + MEASUREMENTS.len(), "{text}");
    for (measurement, line) in MEASUREMENTS.iter().zip(&lines[1..]) {
        let label = measurement
            .label
            .replace("{above}", "5000")
            .replace("{note}", "C3");
        assert!(line.starts_with(&label), "{line:?} is {label}");
        assert!(
            line.trim_end().ends_with(measurement.unit),
            "{line:?} in {}",
            measurement.unit
        );
        // A sine that starts, holds and is released has every measurement.
        assert!(!line.contains(" - "), "{line:?} has no value");
    }
}

#[test]
fn the_json_report_has_the_documented_shape() {
    let wav = render_sine(&folder("json"));
    let printed = uta(&["analyse", &wav, "--json"]);
    let report: Value = serde_json::from_str(&printed).unwrap();
    assert_report_shape(&report);
    let measurements = report["measurements"].as_object().unwrap();
    assert!(
        measurements.values().all(Value::is_number),
        "a sine has every measurement: {measurements:?}"
    );
    // Its order is the table's, so the text and the data read alike.
    let at: Vec<usize> = MEASUREMENTS
        .iter()
        .map(|m| printed.find(&format!("\"{}\"", m.key)).unwrap())
        .collect();
    assert!(at.is_sorted(), "in the table's order: {printed}");
}

#[test]
fn pitch_names_the_intended_note_and_how_far_off_it_is() {
    let wav = render_sine(&folder("pitch"));
    let on = json(&["analyse", &wav, "--json", "--pitch", "C3"]);
    assert_eq!(on["note"], "C3");
    let cents = on["measurements"]["pitch_cents"].as_f64().unwrap();
    assert!(cents.abs() < 2.0, "C3 reads {cents} cents off C3");
    let hz = on["measurements"]["pitch_hz"].as_f64().unwrap();
    assert!((hz - 130.81).abs() < 0.2, "C3 reads {hz} Hz");

    // Told it's meant to be a D3, it's 200 cents flat.
    let off = json(&["analyse", &wav, "--json", "--pitch", "D3"]);
    let cents = off["measurements"]["pitch_cents"].as_f64().unwrap();
    assert!((cents + 200.0).abs() < 2.0, "C3 reads {cents} cents off D3");
    assert!(uta(&["analyse", &wav, "--pitch", "D3"]).contains("pitch off D3"));
}

#[test]
fn a_render_against_itself_shows_no_difference() {
    let wav = render_sine(&folder("against_itself"));
    let both = json(&["analyse", &wav, "--against", &wav, "--json"]);
    assert_report_shape(&both["this"]);
    assert_report_shape(&both["against"]);
    let difference = both["difference"].as_object().unwrap();
    assert_eq!(difference.len(), MEASUREMENTS.len());
    for (key, value) in difference {
        assert_eq!(value.as_f64(), Some(0.0), "{key} differs by {value}");
    }

    let text = uta(&["analyse", &wav, "--against", &wav]);
    let header = text.lines().nth(3).unwrap();
    assert!(header.trim_end().ends_with("difference"), "{header}");
    for line in text.lines().skip(4) {
        let columns: Vec<&str> = line.split_whitespace().collect();
        let unit = MEASUREMENTS
            .iter()
            .find(|m| line.trim_end().ends_with(m.unit))
            .unwrap()
            .unit;
        let difference = columns[columns.len() - 1 - unit.split(' ').count()];
        assert_eq!(difference, "0", "{line}");
    }
}

#[test]
fn a_quieter_copy_differs_only_in_level() {
    let wav = render_sine(&folder("against_quieter"));
    let quieter = quieter_copy(&wav);
    let both = json(&["analyse", &wav, "--against", &quieter, "--json"]);
    let level = ["peak_dbfs", "rms_dbfs", "loudness_db"];
    let six_db = 20.0 * 2f64.log10();
    for (key, value) in both["difference"].as_object().unwrap() {
        let difference = value.as_f64().unwrap();
        if level.contains(&key.as_str()) {
            assert!(
                (difference - six_db).abs() < 1e-3,
                "{key} differs by {difference}"
            );
        } else {
            assert!(difference.abs() < 1e-6, "{key} differs by {difference}");
        }
    }

    let text = uta(&["analyse", &wav, "--against", &quieter]);
    let line = |label: &str| {
        text.lines()
            .find(|line| line.starts_with(label))
            .unwrap()
            .split_whitespace()
            .collect::<Vec<_>>()
    };
    assert_eq!(line("peak")[3], "+6.0", "{text}");
    assert_eq!(line("crest factor")[4], "0", "{text}");
}

#[test]
fn above_sets_the_frequency_the_share_is_measured_above() {
    let wav = render_sine(&folder("above"));
    // A C3 sine (131 Hz) is nearly all above 100 Hz, and nearly none of it
    // is above 1 kHz.
    let low = json(&["analyse", &wav, "--json", "--above", "100"]);
    assert_eq!(low["above_hz"], 100.0);
    let share = low["measurements"]["share_above_percent"].as_f64().unwrap();
    assert!(share > 90.0, "{share}% above 100 Hz");
    let high = json(&["analyse", &wav, "--json", "--above", "1000"]);
    let share = high["measurements"]["share_above_percent"]
        .as_f64()
        .unwrap();
    assert!(share < 1.0, "{share}% above 1 kHz");
    assert!(uta(&["analyse", &wav, "--above", "1000"]).contains("share above 1000 Hz"));
}

#[test]
fn an_integer_wav_reads_as_its_float_original() {
    let wav = render_sine(&folder("integer"));
    let (spec, samples) = read_wav(Path::new(&wav)).unwrap();
    let integer = wav.replace(".wav", "-24-bit.wav");
    let mut writer = hound::WavWriter::create(
        &integer,
        hound::WavSpec {
            bits_per_sample: 24,
            sample_format: hound::SampleFormat::Int,
            ..spec
        },
    )
    .unwrap();
    for sample in samples {
        writer
            .write_sample((f64::from(sample) * f64::from(1 << 23)).round() as i32)
            .unwrap();
    }
    writer.finalize().unwrap();
    let both = json(&["analyse", &integer, "--against", &wav, "--json"]);
    for key in ["peak_dbfs", "rms_dbfs", "pitch_hz"] {
        let difference = both["difference"][key].as_f64().unwrap();
        assert!(difference.abs() < 1e-3, "{key} differs by {difference}");
    }
}

#[test]
fn against_measures_both_pitches_from_the_first_ones_note() {
    let folder = folder("against_note");
    let c3 = render_sine(&folder);
    let d3 = render(&folder, "d3-sine", &sine_at(50));
    let both = json(&["analyse", &c3, "--against", &d3, "--json"]);
    assert_eq!(both["this"]["note"], "C3");
    assert_eq!(both["against"]["note"], "C3");
    let cents = both["against"]["measurements"]["pitch_cents"]
        .as_f64()
        .unwrap();
    assert!((cents - 200.0).abs() < 2.0, "D3 reads {cents} cents off C3");
}

#[test]
fn a_wav_that_isnt_there_is_an_error() {
    let output = Command::new(env!("CARGO_BIN_EXE_uta"))
        .args(["analyse", "no-such-file.wav"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let error = String::from_utf8_lossy(&output.stderr);
    assert!(error.contains("couldn't read no-such-file.wav"), "{error}");
}
