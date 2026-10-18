use super::*;
use uta_core::{DelaySettings, EffectSettings, KickModel, Waveform};

/// The song `text` builds.
fn build(text: &str) -> Project {
    compile(text).unwrap().build().unwrap()
}

/// Each note a track plays, in song time: (start, length, pitch, velocity).
fn notes(project: &Project, name: &str) -> Vec<(Ticks, Ticks, u8, u8)> {
    let track = project
        .tracks()
        .iter()
        .find(|track| track.name() == name)
        .unwrap_or_else(|| panic!("no track called {name}"));
    let mut notes: Vec<_> = track
        .clips()
        .iter()
        .flat_map(|clip| {
            clip.notes().map(|note| {
                (
                    clip.start() + note.start,
                    note.length,
                    note.pitch,
                    note.velocity,
                )
            })
        })
        .collect();
    notes.sort();
    notes
}

fn error(text: &str) -> SongError {
    compile(text).unwrap_err()
}

const HEADER: &str = "name = \"Test\"\ntempo = 120\n";

#[test]
fn a_pattern_becomes_the_right_notes() {
    let project = build(&format!(
        "{HEADER}
[[track]]
name = \"Keys\"
kind = \"synth\"
pattern.verse = {{ notes = \"C4 - . E4@90\", step = \"1/8\" }}

[[track]]
name = \"Drums\"
kind = \"drums\"
pattern.verse = {{ kick = \"x... X...\", closed_hat = \"..x90.\" }}

[[section]]
name = \"verse\"
bars = 1
"
    ));
    // Four eighths, twice round the bar: C4 held two of them, a rest, E4.
    assert_eq!(
        notes(&project, "Keys"),
        [
            (0, 960, 60, 100),
            (1440, 480, 64, 90),
            (1920, 960, 60, 100),
            (3360, 480, 64, 90),
        ]
    );
    // Sixteenths, each hit a sixteenth long: the kick's line is half a bar,
    // the hat's a quarter.
    let kick = |start, velocity| (start, 240, 36, velocity);
    let hat = |start| (start, 240, 42, 90);
    let mut expected = vec![
        kick(0, 100),
        kick(960, 127),
        kick(1920, 100),
        kick(2880, 127),
    ];
    expected.extend([480, 1440, 2400, 3360].map(hat));
    expected.sort();
    assert_eq!(notes(&project, "Drums"), expected);
}

#[test]
fn a_line_can_set_its_notes_length_and_velocity_and_play_with_others() {
    let project = build(&format!(
        "{HEADER}
[[track]]
name = \"Keys\"
kind = \"synth\"
pattern.verse = [
  {{ notes = \"A2 A2@105\", step = \"1/4\", length = 400, velocity = 80 }},
  {{ notes = \"A3+C4 -\", step = \"1/2\", length = \"1/8 dotted\" }},
]

[[section]]
name = \"verse\"
bars = 1
"
    ));
    assert_eq!(
        notes(&project, "Keys"),
        [
            (0, 400, 45, 80),
            (0, 720, 57, 100),
            (0, 720, 60, 100),
            (960, 400, 45, 105),
            (1920, 400, 45, 80),
            (2880, 400, 45, 105),
        ]
    );
}

#[test]
fn a_section_repeats_in_the_arrangement() {
    let text = "name = \"Test\"
tempo = 120
arrangement = [\"verse\", \"chorus\", \"verse\"]

[[track]]
name = \"Bass\"
kind = \"synth\"
pattern.verse = \"C2 . . .\"

[[section]]
name = \"verse\"
bars = 1

[[section]]
name = \"chorus\"
bars = 2
";
    let project = build(text);
    let bass = &project.tracks()[0];
    let clips: Vec<_> = bass
        .clips()
        .iter()
        .map(|clip| (clip.start(), clip.length()))
        .collect();
    // The verse at bar 1 and again at bar 4, after the 2-bar chorus, where
    // the bass doesn't play.
    assert_eq!(clips, [(0, 3840), (11_520, 3840)]);
    let pitches = |clip: &Clip| {
        let mut notes: Vec<_> = clip.notes().map(|note| (note.start, note.pitch)).collect();
        notes.sort();
        notes
    };
    assert_eq!(pitches(&bass.clips()[0]), pitches(&bass.clips()[1]));
    let ids = |clip: &Clip| clip.notes().map(|note| note.id).collect::<Vec<_>>();
    assert!(
        ids(&bass.clips()[0])
            .iter()
            .all(|id| !ids(&bass.clips()[1]).contains(id))
    );
    // The loop goes round the whole arrangement.
    assert_eq!(project.transport().loop_start(), 0);
    assert_eq!(project.transport().loop_length(), 4 * 3840);
    assert!(project.transport().loop_enabled());
}

#[test]
fn the_song_has_only_its_own_tracks() {
    let project = build(&format!(
        "{HEADER}
loop = false

[[track]]
name = \"Bass\"
kind = \"synth\"
mixer = {{ volume_db = -6, pan = -0.5 }}

[[track]]
name = \"Beat\"
kind = \"drums\"

[[section]]
name = \"verse\"
bars = 1
"
    ));
    let names: Vec<_> = project.tracks().iter().map(Track::name).collect();
    assert_eq!(names, ["Bass", "Beat"]);
    assert_eq!(
        *project.tracks()[0].mixer(),
        MixerStrip {
            volume_db: -6.0,
            pan: -0.5,
            ..MixerStrip::default()
        }
    );
    assert_eq!(project.transport().tempo_map().bpm(), 120.0);
    assert!(!project.transport().loop_enabled());
}

#[test]
fn a_factory_sound_and_settings_by_name_become_the_right_commands() {
    let list = compile(&format!(
        "{HEADER}
[[track]]
name = \"Bass\"
kind = \"synth\"
sound = \"Glide bass\"
settings = {{ cutoff_hz = 500, osc1_wave = \"pulse\" }}
effects = [{{ delay = {{ sync = true, division = \"1/8d\", feedback = 0.35 }}, mix = 0.2 }}]

[[track]]
name = \"Drums\"
kind = \"drums\"
settings.kick = {{ attack = 0.5, model = \"909\" }}

[[section]]
name = \"verse\"
bars = 1
"
    ))
    .unwrap();
    let project = list.build().unwrap();
    let bass = project.tracks()[0].id();
    let drums = project.tracks()[1].id();
    let delay = project.tracks()[0].effects()[0].id();
    let glide = FactorySound::find("glide_bass").unwrap();
    let wanted = [
        Command::SetSynthSettings {
            track: bass,
            settings: glide.settings,
        },
        Command::SetSynthParam {
            track: bass,
            param: SynthParam::CutoffHz(500.0),
        },
        Command::SetSynthParam {
            track: bass,
            param: SynthParam::Osc1Wave(Waveform::Pulse),
        },
        Command::SetDelayParam {
            effect: delay,
            param: DelayParam::Sync(true),
        },
        Command::SetDelayParam {
            effect: delay,
            param: DelayParam::Division(NoteDivision::DOTTED_EIGHTH),
        },
        Command::SetDelayParam {
            effect: delay,
            param: DelayParam::Feedback(0.35),
        },
        Command::SetEffectMix {
            effect: delay,
            mix: 0.2,
        },
        Command::SetDrumParam {
            track: drums,
            sound: DrumSound::Kick,
            param: DrumParam::Model(KickModel::Tr909),
        },
        Command::SetDrumParam {
            track: drums,
            sound: DrumSound::Kick,
            param: DrumParam::Attack(0.5),
        },
    ];
    for command in &wanted {
        assert!(list.commands.contains(command), "no {command:?}");
    }
    let place = |command: &Command| list.commands.iter().position(|c| c == command);
    assert!(
        place(&wanted[0]) < place(&wanted[1]),
        "the sound, then its settings"
    );
    // Attack is the 909's alone, and sorts before the model.
    assert!(
        place(&wanted[7]) < place(&wanted[8]),
        "the model, then the attack"
    );
    let Source::Synth(settings) = project.tracks()[0].source() else {
        panic!("a synth");
    };
    assert_eq!(settings.cutoff_hz, 500.0);
    assert_eq!(settings.glide_seconds, glide.settings.glide_seconds);
    let effect = &project.tracks()[0].effects()[0];
    let EffectSettings::Delay(delay) = effect.settings();
    assert_eq!(
        (delay.sync, delay.division, delay.feedback, effect.mix()),
        (true, NoteDivision::DOTTED_EIGHTH, 0.35, 0.2)
    );
    assert_ne!(*delay, DelaySettings::default());
}

#[test]
fn master_volume_and_effects() {
    let project = build(&format!(
        "{HEADER}
[master]
volume_db = -3
effects = [{{ delay = {{ time_seconds = 0.25 }}, enabled = false }}]

[[track]]
name = \"Bass\"
kind = \"synth\"

[[section]]
name = \"verse\"
bars = 1
"
    ));
    assert_eq!(project.master_volume_db(), -3.0);
    let delay = &project.master_effects()[0];
    assert!(!delay.enabled());
    let EffectSettings::Delay(settings) = delay.settings();
    assert_eq!(settings.time_seconds, 0.25);
}

#[test]
fn raw_commands_name_tracks_clips_and_effects() {
    let project = build(&format!(
        "{HEADER}
[[track]]
name = \"Bass\"
kind = \"synth\"
effects = [{{ delay = {{}} }}]
pattern.verse = \"C2\"

[[section]]
name = \"verse\"
bars = 1

[[command]]
type = \"set_effect_mix\"
effect = \"$effect:Bass:1\"
mix = 0.5

[[command]]
type = \"move_track\"
track = \"$track:Bass\"
index = 0

[[command]]
format = 14
type = \"set_clips\"
clips = [{{ id = \"$clip:Bass:1\", track = \"$track:Bass\", start = 3840, length = 3840 }}]
"
    ));
    let bass = &project.tracks()[0];
    assert_eq!(bass.effects()[0].mix(), 0.5);
    assert_eq!(bass.clips()[0].start(), 3840);
}

#[test]
fn compiling_twice_gives_the_same_ids() {
    let text = format!(
        "{HEADER}
[[track]]
name = \"Bass\"
kind = \"synth\"
effects = [{{ delay = {{}} }}]
pattern.verse = \"C2 E2\"

[[section]]
name = \"verse\"
bars = 1
"
    );
    let first = compile(&text).unwrap();
    assert_eq!(compile(&text).unwrap(), first);
    assert_eq!(
        first.build().unwrap(),
        compile(&text).unwrap().build().unwrap()
    );
    // Another song gets its own.
    let other = compile(&text.replace("\"Test\"", "\"Other\"")).unwrap();
    assert_ne!(other.project, first.project);
}

#[test]
fn an_unknown_sound_is_an_error_with_its_line() {
    let text = format!(
        "{HEADER}
[[track]]
name = \"Bass\"
kind = \"synth\"
sound = \"Wobble\"

[[section]]
name = \"verse\"
bars = 1
"
    );
    let error = error(&text);
    assert_eq!(error.line, 7, "{error}");
    assert!(
        error.message.contains("`Wobble` isn't a factory sound"),
        "{error}"
    );
}

#[test]
fn an_unknown_setting_is_an_error_with_its_line() {
    let synth = error(&format!(
        "{HEADER}
[[section]]
name = \"verse\"
bars = 1

[[track]]
name = \"Bass\"
kind = \"synth\"
settings = {{ cutof_hz = 500 }}
"
    ));
    assert_eq!(synth.line, 11, "{synth}");
    assert!(
        synth.message.contains("`cutof_hz` isn't a synth setting"),
        "{synth}"
    );

    let drum = error(&format!(
        "{HEADER}
[[section]]
name = \"verse\"
bars = 1

[[track]]
name = \"Beat\"
kind = \"drums\"
settings.kick = {{ snappy = 0.5 }}
"
    ));
    // The 808 kick has no Snappy: the command says so.
    assert_eq!(drum.line, 11, "{drum}");

    let delay = error(&format!(
        "{HEADER}
[[section]]
name = \"verse\"
bars = 1

[[track]]
name = \"Bass\"
kind = \"synth\"
effects = [
  {{ delay = {{ feedback = 0.3 }} }},
  {{ delay = {{ fedback = 0.3 }} }},
]
"
    ));
    assert_eq!(delay.line, 13, "{delay}");
    assert!(
        delay.message.contains("`fedback` isn't a Delay setting"),
        "{delay}"
    );
}

#[test]
fn a_value_out_of_range_is_an_error_on_its_line() {
    let error = error(&format!(
        "{HEADER}
[[section]]
name = \"verse\"
bars = 1

[[track]]
name = \"Bass\"
kind = \"synth\"
settings = {{ resonance = 0.2 }}
mixer = {{ volume_db = 20 }}
"
    ));
    assert_eq!(error.line, 12, "{error}");
    assert!(error.message.contains("out of range"), "{error}");
}

#[test]
fn an_unknown_note_is_an_error_with_its_line() {
    let synth = error(&format!(
        "{HEADER}
[[section]]
name = \"verse\"
bars = 2

[[track]]
name = \"Bass\"
kind = \"synth\"
pattern.verse = \"\"\"
C2 . E2 .
G2 . H2 .
\"\"\"
"
    ));
    assert_eq!(synth.line, 13, "{synth}");
    assert!(synth.message.contains("`H2` isn't a note"), "{synth}");

    let drum = error(&format!(
        "{HEADER}
[[section]]
name = \"verse\"
bars = 1

[[track]]
name = \"Beat\"
kind = \"drums\"
pattern.verse = {{ kick = \"x...\" }}
pattern.chorus = {{ snare = \"x...\" }}
"
    ));
    assert_eq!(drum.line, 12, "{drum}");
    assert!(
        drum.message.contains("no section called `chorus`"),
        "{drum}"
    );

    let row = error(&format!(
        "{HEADER}
[[section]]
name = \"verse\"
bars = 1

[[track]]
name = \"Beat\"
kind = \"drums\"

[track.pattern.verse]
kick = \"x...\"
snar = \"x...\"
"
    ));
    assert_eq!(row.line, 14, "{row}");
    assert!(row.message.contains("`snar` isn't part of a line"), "{row}");
}

#[test]
fn a_mistake_in_the_toml_is_an_error_with_its_line() {
    let error = error(&format!(
        "{HEADER}\n[[track]]\nname = \"Bass\"\nkind = \"piano\"\n"
    ));
    assert_eq!(error.line, 6, "{error}");
    let missing = super::compile("tempo = 120\n").unwrap_err();
    assert!(missing.message.contains("name"), "{missing}");
}

#[test]
fn a_line_longer_than_its_section_is_an_error() {
    let error = error(&format!(
        "{HEADER}
[[section]]
name = \"verse\"
bars = 1

[[track]]
name = \"Bass\"
kind = \"synth\"
pattern.verse = {{ notes = \"C2 C2 C2 C2 C2\", step = \"1/4\" }}
"
    ));
    assert_eq!(error.line, 11, "{error}");
    assert!(error.message.contains("longer than its section"), "{error}");
}

#[test]
fn a_mistake_after_a_line_continuation_is_on_its_line() {
    let error = error(&format!(
        "{HEADER}
[[section]]
name = \"verse\"
bars = 2

[[track]]
name = \"Bass\"
kind = \"synth\"
pattern.verse = \"\"\"C2 . E2 . \\
  G2 . H2 .\"\"\"
"
    ));
    assert_eq!(error.line, 12, "{error}");
    assert!(error.message.contains("`H2` isn't a note"), "{error}");
}

#[test]
fn an_empty_arrangement_is_an_error_on_its_line() {
    let error = error(
        "name = \"Test\"\ntempo = 120\narrangement = []\n\n[[section]]\nname = \"verse\"\nbars = 1\n",
    );
    assert_eq!(error.line, 3, "{error}");
    assert!(error.message.contains("at least one section"), "{error}");
}

#[test]
fn a_drum_pattern_has_no_length() {
    let error = error(&format!(
        "{HEADER}
[[section]]
name = \"verse\"
bars = 1

[[track]]
name = \"Beat\"
kind = \"drums\"
pattern.verse = {{ kick = \"x...\", length = \"1/4\" }}
"
    ));
    assert_eq!(error.line, 11, "{error}");
    assert!(
        error
            .message
            .contains("`length` isn't part of a line: it has step, velocity"),
        "{error}"
    );
}
