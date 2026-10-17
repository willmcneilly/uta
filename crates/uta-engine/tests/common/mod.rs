//! The projects the sound tests play, and the measurements they take from
//! the rendered waveform, which are in `uta-analysis`.

#![allow(dead_code)] // Each test binary uses a different subset.

use uta_core::time::{TICKS_PER_QUARTER, Ticks};
use uta_core::{
    Clip, ClipId, Command, CommandList, DrumParam, DrumSound, KitSettings, Note, NoteId,
    PlacedTrack, Project, ProjectId, Source, SynthParam, Track, TrackId,
};
use uuid::Uuid;

pub use uta_analysis::*;

/// The demo loop, built from its committed command list.
pub fn demo_loop() -> Project {
    let json = include_str!("../../../../examples/demo-loop.json");
    let list: CommandList = serde_json::from_str(json).expect("the demo loop parses");
    list.build().expect("the demo loop builds")
}

/// The demo song, built from its committed command list: three synth
/// tracks, each with its own sound, panned apart, and a new project's empty
/// drum track.
pub fn demo_song() -> Project {
    let json = include_str!("../../../../examples/demo-song.json");
    let list: CommandList = serde_json::from_str(json).expect("the demo song parses");
    list.build().expect("the demo song builds")
}

/// A note for [`project`]: its ID is worked out from `index`.
pub fn note(index: u128, pitch: u8, start: Ticks, length: Ticks) -> Note {
    Note {
        id: NoteId::from_uuid(Uuid::from_u128(1000 + index)),
        pitch,
        velocity: 100,
        start,
        length,
    }
}

/// A new project without its drum track: Synth 1 alone, with its empty
/// 4-bar clip, as a new project was before drum tracks. The tests built on
/// it are about the synth, and an empty drum track would change nothing
/// they hear.
pub fn synth_project() -> Project {
    let mut project = Project::with_id(ProjectId::from_uuid(Uuid::from_u128(1)));
    let drums = project
        .tracks()
        .iter()
        .filter(|track| matches!(track.source(), Source::Drums(_)))
        .map(Track::id)
        .collect();
    project
        .apply(&Command::RemoveTracks { tracks: drums })
        .expect("a new project has a drum track");
    project
}

/// A project at `bpm` with a loop of `bars`, the synth settings in `params`,
/// and `notes`, built through commands as the app would.
pub fn project(bpm: f32, bars: u32, params: &[SynthParam], notes: Vec<Note>) -> Project {
    let mut project = synth_project();
    let track = project.tracks()[0].id();
    let clip = project.tracks()[0].clips()[0].id();
    let mut commands = vec![Command::SetTempo { bpm }, Command::SetLoopLength { bars }];
    commands.extend(
        params
            .iter()
            .map(|&param| Command::SetSynthParam { track, param }),
    );
    if !notes.is_empty() {
        commands.push(Command::AddNotes { clip, notes });
    }
    for command in &commands {
        project.apply(command).expect("a valid test project");
    }
    project
}

/// The drum track [`drum_project`] adds.
pub fn drum_track() -> TrackId {
    TrackId::from_uuid(Uuid::from_u128(70))
}

/// A hit on a drum track: a sixteenth note at `start`, its ID worked out from
/// `index`.
pub fn hit(index: u128, pitch: u8, velocity: u8, start: Ticks) -> Note {
    Note {
        id: NoteId::from_uuid(Uuid::from_u128(5000 + index)),
        pitch,
        velocity,
        start,
        length: TICKS_PER_QUARTER / 4,
    }
}

/// The kick's note.
pub const KICK: u8 = 36;
/// The snare's note.
pub const SNARE: u8 = 38;
/// The clap's note.
pub const CLAP: u8 = 39;
/// The closed hat's note.
pub const CLOSED_HAT: u8 = 42;
/// The open hat's note.
pub const OPEN_HAT: u8 = 46;
/// The low tom's note.
pub const LOW_TOM: u8 = 45;
/// The high tom's note.
pub const HIGH_TOM: u8 = 50;
/// The cymbal's note.
pub const CYMBAL: u8 = 49;

/// The kit at its defaults, but with every sound's Level at 0 dB, so a
/// sound's own tests hear it at the kit's reference peak, whatever its
/// default Level does to balance the kit.
pub fn unity_kit() -> KitSettings {
    let mut kit = KitSettings::default();
    kit.kick.level_db = 0.0;
    kit.kick.tr909.level_db = 0.0;
    kit.snare.level_db = 0.0;
    kit.clap.level_db = 0.0;
    kit.closed_hat.level_db = 0.0;
    kit.open_hat.level_db = 0.0;
    kit.low_tom.level_db = 0.0;
    kit.high_tom.level_db = 0.0;
    kit.cymbal.level_db = 0.0;
    kit
}

/// A project at `bpm` with a loop of `bars`, the master at 0 dB, and a drum
/// track, "Drums 1", under the empty synth track: its clip fills the loop
/// and holds `hits`, and its kit is [`unity_kit`] with the settings in
/// `params`. Built through commands, as the app would.
pub fn drum_project(
    bpm: f32,
    bars: u32,
    params: &[(DrumSound, DrumParam)],
    hits: Vec<Note>,
) -> Project {
    drum_project_with_kit(unity_kit(), bpm, bars, params, hits)
}

/// [`drum_project`], with the kit starting from `kit`.
pub fn drum_project_with_kit(
    kit: KitSettings,
    bpm: f32,
    bars: u32,
    params: &[(DrumSound, DrumParam)],
    hits: Vec<Note>,
) -> Project {
    let mut project = synth_project();
    let track = drum_track();
    let bar = project.transport().time_signature().ticks_per_bar();
    let clip = Clip::new(
        ClipId::from_uuid(Uuid::from_u128(71)),
        0,
        Ticks::from(bars) * bar,
    )
    .with_notes(hits);
    let mut commands = vec![
        Command::SetMasterVolume { volume_db: 0.0 },
        Command::SetTempo { bpm },
        Command::SetLoopLength { bars },
        Command::AddTracks {
            tracks: vec![PlacedTrack {
                index: 1,
                track: Track::new(track, "Drums 1", Source::Drums(kit)).with_clips([clip]),
            }],
        },
    ];
    commands.extend(params.iter().map(|&(sound, param)| Command::SetDrumParam {
        track,
        sound,
        param,
    }));
    for command in &commands {
        project.apply(command).expect("a valid test project");
    }
    project
}

/// Plays `project` from the top for `seconds`, in mono at 48 kHz, in blocks
/// of `block_size`.
pub fn render_drums(project: &Project, seconds: f64, block_size: usize) -> Vec<f32> {
    let config = uta_engine::EngineConfig {
        sample_rate: 48_000,
        channels: 1,
    };
    let mut renderer =
        uta_engine::offline::Renderer::new(config, uta_engine::Snapshot::from(project), block_size);
    renderer.controller.play().unwrap();
    renderer.render_seconds(seconds);
    renderer.into_samples()
}

/// One hit on `pitch` at the top of a bar at 60 BPM, so nothing else plays
/// for 4 s, with these kit settings, at this velocity, rendered for
/// `seconds` at 48 kHz.
pub fn one_hit(
    pitch: u8,
    params: &[(DrumSound, DrumParam)],
    velocity: u8,
    seconds: f64,
) -> Vec<f32> {
    let project = drum_project(60.0, 1, params, vec![hit(0, pitch, velocity, 0)]);
    render_drums(&project, seconds, 128)
}

/// How a drum setting change sounds, against the same render without it:
/// one hit on `pitch` at the top of a bar with the kit settings in `base`,
/// and `turn` applied at sample `at` (a whole number of 128-sample blocks).
/// Returns the change's difference: `with` minus `without`, sample by sample,
/// from `at` on. The kit's noise runs the same in both, so the difference is
/// the turn alone.
pub fn turn_difference(
    pitch: u8,
    base: &[(DrumSound, DrumParam)],
    turn: (DrumSound, DrumParam),
    at: usize,
    seconds: f64,
) -> Vec<f32> {
    assert_eq!(at % 128, 0, "turns land on a block boundary");
    let mut project = drum_project(60.0, 1, base, vec![hit(0, pitch, 127, 0)]);
    let without = render_drums(&project, seconds, 128);
    let config = uta_engine::EngineConfig {
        sample_rate: 48_000,
        channels: 1,
    };
    let mut renderer =
        uta_engine::offline::Renderer::new(config, uta_engine::Snapshot::from(&project), 128);
    renderer.controller.play().unwrap();
    renderer.render(at);
    project
        .apply(&Command::SetDrumParam {
            track: drum_track(),
            sound: turn.0,
            param: turn.1,
        })
        .unwrap();
    renderer.controller.set_project(&project).unwrap();
    renderer.render_seconds(seconds - at as f64 / 48_000.0);
    let with = renderer.into_samples();
    assert_eq!(&with[..at], &without[..at]);
    with[at..]
        .iter()
        .zip(&without[at..])
        .map(|(a, b)| a - b)
        .collect()
}

/// Fails unless a change glides in: in the first tenth of its glide, a
/// setting that glides has moved a tenth of the way, so the difference it
/// makes (see [`turn_difference`]) is still small. A setting that jumps
/// makes its whole difference at once. `glide` is the glide's length in
/// samples.
pub fn assert_glides(difference: &[f32], glide: usize, what: &str) {
    let early = peak(&difference[..glide / 10]);
    let full = peak(&difference[..2 * glide]);
    assert!(full > 1e-4, "{what}: the turn changed nothing");
    assert!(
        early < 0.3 * full,
        "{what}: {:.0}% of its difference in the first tenth of the glide",
        100.0 * early / full
    );
}

/// Fails if `samples` jump from one sample to the next by more than `limit`.
pub fn assert_no_click(samples: &[f32], limit: f32, what: &str) {
    let (jump, at) = max_jump(samples);
    assert!(
        jump <= limit,
        "{what}: jump of {jump} at sample {at}, limit {limit}"
    );
}

/// `project` filled up to [`Project::MAX_TRACKS`] with empty synth tracks,
/// so every slot is taken and a track added in place of a removed one has
/// to take the removed one's slot.
pub fn with_every_slot_taken(mut project: Project) -> Project {
    let missing = Project::MAX_TRACKS - project.tracks().len();
    let fill = (0..missing)
        .map(|n| PlacedTrack {
            index: project.tracks().len() + n,
            track: Track::new(
                TrackId::from_uuid(Uuid::from_u128(8000 + n as u128)),
                format!("Fill {n}"),
                Source::Synth(uta_core::SynthSettings::default()),
            ),
        })
        .collect();
    project
        .apply(&Command::AddTracks { tracks: fill })
        .expect("room for the fill");
    project
}

/// `project` with the track `old` removed and `new` added in its place in
/// the order, in one change, as deleting a track and adding another while
/// it still sounds would.
pub fn replace_track(project: &Project, old: TrackId, new: Track) -> Project {
    let index = project
        .tracks()
        .iter()
        .position(|track| track.id() == old)
        .expect("the old track is in the project");
    let mut replaced = project.clone();
    replaced
        .apply(&Command::RemoveTracks { tracks: vec![old] })
        .expect("the old track goes");
    replaced
        .apply(&Command::AddTracks {
            tracks: vec![PlacedTrack { index, track: new }],
        })
        .expect("the new track goes in");
    replaced
}

/// A drum track with no clips, at the defaults.
pub fn empty_drum_track(id: u128) -> Track {
    Track::new(
        TrackId::from_uuid(Uuid::from_u128(id)),
        "Drums 2",
        Source::Drums(KitSettings::default()),
    )
}

/// A synth track with no clips, playing a sine at full sustain.
pub fn empty_sine_track(id: u128) -> Track {
    let settings = uta_core::SynthSettings {
        osc1_wave: uta_core::Waveform::Sine,
        sustain: 1.0,
        ..uta_core::SynthSettings::default()
    };
    Track::new(
        TrackId::from_uuid(Uuid::from_u128(id)),
        "Synth 2",
        Source::Synth(settings),
    )
}

/// Synth settings for measuring exact sample positions: a sine at full
/// sustain, with the shortest release, so a note is silent 1 ms after it
/// ends.
pub const PLAIN_SINE: [SynthParam; 3] = [
    SynthParam::Osc1Wave(uta_core::Waveform::Sine),
    SynthParam::Sustain(1.0),
    SynthParam::ReleaseSeconds(0.001),
];

/// `snapshot` with its first track's mixer strip set to `mixer`.
pub fn with_mixer(
    mut snapshot: uta_engine::Snapshot,
    mixer: uta_engine::MixerStrip,
) -> uta_engine::Snapshot {
    snapshot.tracks_mut()[0].mixer = mixer;
    snapshot
}

/// `snapshot` with its first track's synth set to `synth`.
pub fn with_synth(
    mut snapshot: uta_engine::Snapshot,
    synth: uta_engine::SynthSettings,
) -> uta_engine::Snapshot {
    snapshot.tracks_mut()[0].sound = uta_engine::TrackSound::Synth(synth);
    snapshot
}

/// The app's stress notes (`stress::notes` in `uta-app`) for one bar of 4/4, seed
/// 0: the same pattern Develop → Add Stress Notes and the benchmark make.
/// The app gives each note a random ID; here they count up from `first_id`,
/// so every run plays the same song: notes on one sample sound in ID order.
pub fn stress_notes(first_id: u128) -> Vec<Note> {
    let sixteenth = TICKS_PER_QUARTER / 4;
    let steps = 16;
    let mut state: u64 = 1;
    let mut next = || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };
    (0..3_000)
        .map(|index| Note {
            id: NoteId::from_uuid(Uuid::from_u128(first_id + index)),
            pitch: 24 + (next() % 85) as u8,
            velocity: Note::MIN_VELOCITY
                + (next() % u64::from(Note::MAX_VELOCITY - Note::MIN_VELOCITY + 1)) as u8,
            start: (next() % steps) * sixteenth,
            length: (1 + next() % 16) * sixteenth,
        })
        .collect()
}
