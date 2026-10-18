//! The demo songs, built into the app from `examples/demos/` and loaded from
//! Develop → Load Demo Song (RFC-009, "They load from the app's menu").

use uta_core::{Chain, Command, Effect, PlacedEffect, PlacedTrack, Project, Track};

/// A song file built into the app.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DemoSong {
    /// Its file's name in `examples/demos/`, without `.toml`: how the menu
    /// and the UI name it.
    pub id: &'static str,
    /// What the menu calls it.
    pub name: &'static str,
    /// The song file.
    pub text: &'static str,
}

/// A demo song from `examples/demos/<id>.toml`, called `name` in the menu.
macro_rules! demo {
    ($id:literal, $name:literal) => {
        DemoSong {
            id: $id,
            name: $name,
            text: include_str!(concat!("../../../examples/demos/", $id, ".toml")),
        }
    };
}

/// Every demo song, in the order the menu lists them. A new song needs its
/// file in `examples/demos/` and a line here.
// One song a line, so each new song is a line of its own.
#[rustfmt::skip]
pub const SONGS: &[DemoSong] = &[
    demo!("loop", "Loop"),
    demo!("beat", "Beat"),
    demo!("lofi", "Lo-fi"),
    demo!("house", "House"),
];

/// The demo song called `id`.
pub fn find(id: &str) -> Option<&'static DemoSong> {
    SONGS.iter().find(|song| song.id == id)
}

impl DemoSong {
    /// The project the song builds: compiled as `uta song` compiles it, then
    /// built.
    pub fn build(&self) -> Result<Project, String> {
        let list = uta_song::compile(self.text).map_err(|error| format!("{}: {error}", self.id))?;
        list.build()
            .map_err(|(index, error)| format!("{}: command {} failed: {error}", self.id, index + 1))
    }
}

/// The commands that turn `current` into `song`, everything but its ID: the
/// tracks and the master's chain go, and the song's tempo, master volume and
/// chain, tracks and loop come in. Applied as one undo step, they replace
/// the project cleanly, whatever was in it.
pub fn replacing(current: &Project, song: &Project) -> Vec<Command> {
    let mut commands = Vec::new();
    let tracks: Vec<_> = current.tracks().iter().map(Track::id).collect();
    if !tracks.is_empty() {
        commands.push(Command::RemoveTracks { tracks });
    }
    let effects: Vec<_> = current.master_effects().iter().map(Effect::id).collect();
    if !effects.is_empty() {
        commands.push(Command::RemoveEffects { effects });
    }
    let transport = song.transport();
    commands.push(Command::SetTempo {
        bpm: transport.tempo_map().bpm(),
    });
    commands.push(Command::SetMasterVolume {
        volume_db: song.master_volume_db(),
    });
    if !song.master_effects().is_empty() {
        let effects = song.master_effects().iter().enumerate();
        commands.push(Command::AddEffects {
            effects: effects
                .map(|(index, &effect)| PlacedEffect {
                    chain: Chain::Master,
                    index,
                    effect,
                })
                .collect(),
        });
    }
    if !song.tracks().is_empty() {
        let tracks = song.tracks().iter().cloned().enumerate();
        commands.push(Command::AddTracks {
            tracks: tracks
                .map(|(index, track)| PlacedTrack { index, track })
                .collect(),
        });
    }
    let bar = transport.time_signature().ticks_per_bar();
    commands.push(Command::SetLoop {
        start_bar: u32::try_from(transport.loop_start() / bar).unwrap_or(u32::MAX),
        bars: u32::try_from(transport.loop_length() / bar).unwrap_or(u32::MAX),
    });
    commands.push(Command::SetLoopEnabled {
        enabled: transport.loop_enabled(),
    });
    commands
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;
    use std::path::Path;

    #[test]
    fn every_song_in_examples_demos_is_in_the_menu_once() {
        let folder = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/demos");
        let files: BTreeSet<String> = std::fs::read_dir(folder)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .filter(|path| {
                path.extension()
                    .is_some_and(|extension| extension == "toml")
            })
            .map(|path| path.file_stem().unwrap().to_string_lossy().into_owned())
            .collect();
        let listed: BTreeSet<String> = SONGS.iter().map(|song| song.id.to_owned()).collect();
        assert_eq!(listed.len(), SONGS.len(), "a song is listed twice");
        assert_eq!(listed, files);
    }

    /// A song with what the demo songs don't have yet: a master chain, a
    /// track's chain, a loop that doesn't start at the top, and the loop off.
    const FULL: &str = r#"
name = "Full"
tempo = 96
loop = false
[master]
volume_db = -4
effects = [{ delay = { sync = true, division = "1/4" }, mix = 0.2 }]
[[track]]
name = "Lead"
kind = "synth"
sound = "lead"
effects = [{ delay = {}, mix = 0.1 }]
pattern.a = "C4 - E4 G4"
[[section]]
name = "a"
bars = 4
[[command]]
type = "set_loop"
start_bar = 1
bars = 2
"#;

    #[test]
    fn replacing_turns_any_project_into_the_song() {
        let song = uta_song::compile(FULL).unwrap().build().unwrap();
        let mut current = Project::new();
        current
            .apply(&Command::AddEffects {
                effects: vec![PlacedEffect {
                    chain: Chain::Master,
                    index: 0,
                    effect: Effect::new(
                        uta_core::EffectId::random(),
                        uta_core::EffectSettings::Delay(Default::default()),
                    ),
                }],
            })
            .unwrap();
        for _ in 0..2 {
            let commands = replacing(&current, &song);
            current = current.replay(&commands).unwrap();
            assert_eq!(current.tracks(), song.tracks());
            assert_eq!(current.transport(), song.transport());
            assert_eq!(current.master_volume_db(), song.master_volume_db());
            assert_eq!(current.master_effects(), song.master_effects());
            assert_ne!(current.id(), song.id(), "the project keeps its ID");
        }
    }

    #[test]
    fn every_demo_song_builds() {
        for song in SONGS {
            let project = song.build().unwrap();
            assert!(!project.tracks().is_empty(), "{}", song.id);
        }
    }
}
