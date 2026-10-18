//! `uta song`, and `uta render --song`, run as the built binary on the demo
//! songs.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use uta_core::CommandList;
use uta_engine::offline::read_wav;

fn beat() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/demos/beat.toml")
}

/// A folder of its own for each test, in Cargo's scratch space.
fn folder(test: &str) -> PathBuf {
    let folder = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("song")
        .join(test);
    std::fs::create_dir_all(&folder).unwrap();
    folder
}

fn uta(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_uta"))
        .args(args)
        .output()
        .expect("uta runs")
}

/// What `uta` printed. It must succeed.
fn stdout(args: &[&str]) -> String {
    let output = uta(args);
    assert!(
        output.status.success(),
        "uta {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("uta prints text")
}

/// The command list `uta-song` compiles the beat to.
fn compiled() -> CommandList {
    uta_song::compile(&std::fs::read_to_string(beat()).unwrap()).unwrap()
}

#[test]
fn song_prints_the_command_list_or_writes_it_with_out() {
    let printed: CommandList =
        serde_json::from_str(&stdout(&["song", beat().to_str().unwrap()])).expect("a command list");
    assert_eq!(printed, compiled());

    let out = folder("out").join("beat.json");
    let printed = stdout(&[
        "song",
        beat().to_str().unwrap(),
        "--out",
        out.to_str().unwrap(),
    ]);
    assert!(printed.is_empty(), "{printed}");
    let written: CommandList =
        serde_json::from_str(&std::fs::read_to_string(&out).unwrap()).unwrap();
    assert_eq!(written, compiled());

    // The written list renders as the song does.
    let folder = folder("out");
    let from_song = folder.join("song.wav");
    let from_list = folder.join("list.wav");
    stdout(&[
        "render",
        from_song.to_str().unwrap(),
        "--song",
        beat().to_str().unwrap(),
    ]);
    stdout(&[
        "render",
        from_list.to_str().unwrap(),
        "--commands",
        out.to_str().unwrap(),
    ]);
    let (_, song) = read_wav(&from_song).unwrap();
    let (_, list) = read_wav(&from_list).unwrap();
    assert!(song.iter().any(|&sample| sample != 0.0), "the beat plays");
    assert_eq!(song, list);
}

#[test]
fn a_mistake_in_a_song_says_its_file_and_line() {
    let song = folder("mistake").join("wrong.toml");
    std::fs::write(
        &song,
        "name = \"Wrong\"\ntempo = 120\n\n[[track]]\nname = \"Keys\"\nkind = \"synth\"\nsound = \"Kazoo\"\n\n[[section]]\nname = \"verse\"\nbars = 1\n",
    )
    .unwrap();
    for args in [
        vec!["song", song.to_str().unwrap()],
        vec!["render", "unused.wav", "--song", song.to_str().unwrap()],
    ] {
        let output = uta(&args);
        assert!(!output.status.success());
        let error = String::from_utf8_lossy(&output.stderr);
        assert!(
            error.contains("wrong.toml: line 7: `Kazoo` isn't a factory sound"),
            "{error}"
        );
    }
}

#[test]
fn song_and_commands_cant_both_be_given() {
    let output = uta(&[
        "render",
        "unused.wav",
        "--song",
        "a.toml",
        "--commands",
        "a.json",
    ]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("cannot be used with"));
    let output = uta(&["play", "--song", "a.toml", "--commands", "a.json"]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("cannot be used with"));
}
