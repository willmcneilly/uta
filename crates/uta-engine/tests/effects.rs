//! The effect test kit: written once, against any effect, and run for every
//! kind there is, on a track's chain and on the master's. A new kind of
//! effect gets every test here by giving [`kit_effect`] the settings to
//! test it with. See RFC-007, "How we'll verify it", and "Patterns for
//! every effect after this one", point 8.
//!
//! Each kind's own sound has its own tests (the Delay's are in `delay.rs`):
//! these are about what the chain does for every effect.

mod common;

use common::{PLAIN_SINE, assert_no_click, demo_loop, note, peak, project, sine_max_step};
use uta_core::{
    Chain, Command, DelayMode, DelaySettings, Effect, EffectId, EffectKind, EffectSettings,
    PlacedEffect, Project, ReverbSettings,
};
use uta_engine::offline::Renderer;
use uta_engine::{EFFECT_FADE_SECONDS, EngineConfig, Snapshot};
use uuid::Uuid;

/// An effect of `kind` set to be clearly heard, with a tail that dies away
/// in a few seconds. Every kind must be here: the match fails to compile
/// until a new one is.
fn kit_effect(kind: EffectKind, n: u128) -> Effect {
    let id = EffectId::from_uuid(Uuid::from_u128(6000 + n));
    match kind {
        EffectKind::Delay => Effect::new(
            id,
            // Every part of the tape loop at work: ping-pong, both cuts,
            // Drive and Wobble.
            EffectSettings::Delay(DelaySettings {
                mode: DelayMode::PingPong,
                low_cut_hz: 150.0,
                high_cut_hz: 6000.0,
                drive: 0.5,
                wobble: 0.5,
                ..DelaySettings::plain(0.05, 0.6)
            }),
        )
        .with_mix(0.5),
        EffectKind::Reverb => Effect::new(
            id,
            // Every part at work: a predelay, a burst sent into the tail,
            // both heard, a tail of a second whose lows last longer and
            // highs shorter, both cuts, the crosses partway, and the drift
            // moving every line and lane it can.
            EffectSettings::Reverb(ReverbSettings {
                predelay_seconds: 0.02,
                decay_seconds: 1.0,
                early_size_seconds: 0.05,
                high_cut_hz: 8_000.0,
                low_cut_hz: 100.0,
                early_cross: 0.6,
                late_cross: 0.7,
                early_mod_rate_hz: 1.5,
                early_mod_depth: 0.8,
                late_mod_rate_hz: 0.8,
                late_mod_depth: 0.8,
                bass_mult: 1.5,
                bass_xover_hz: 300.0,
                high_mult: 0.6,
                high_xover_hz: 3_000.0,
                ..ReverbSettings::default()
            }),
        )
        .with_mix(0.5),
    }
}

/// The two kinds of place a chain sits: the first track's, and the
/// master's.
fn chains(project: &Project) -> [Chain; 2] {
    [Chain::Track(project.tracks()[0].id()), Chain::Master]
}

fn stereo() -> EngineConfig {
    EngineConfig {
        sample_rate: 48_000,
        channels: 2,
    }
}

/// `project` with `effect` added to `chain` at `index`.
fn with(mut project: Project, chain: Chain, index: usize, effect: Effect) -> Project {
    project
        .apply(&Command::AddEffects {
            effects: vec![PlacedEffect {
                chain,
                index,
                effect,
            }],
        })
        .expect("a valid effect");
    project
}

/// Plays `project` from the top for `seconds`, in blocks of 128.
fn play(project: &Project, seconds: f64) -> Vec<f32> {
    let mut renderer = Renderer::new(stereo(), Snapshot::from(project), 128);
    renderer.controller.play().unwrap();
    renderer.render_seconds(seconds);
    renderer.into_samples()
}

/// A held A3 sine filling the 4-bar loop: something steady to hear a
/// click against.
fn held_tone() -> Project {
    project(120.0, 4, &PLAIN_SINE, vec![note(0, 57, 0, 4 * 3840)])
}

/// The changes the kit makes while it plays, on `chain`: adding an effect,
/// adding a second after it, moving the second before the first,
/// switching the first off and on, removing the second, then the first.
fn changes(kind: EffectKind, chain: Chain) -> Vec<Command> {
    let (first, second) = (kit_effect(kind, 0), kit_effect(kind, 1));
    let add = |index, effect| Command::AddEffects {
        effects: vec![PlacedEffect {
            chain,
            index,
            effect,
        }],
    };
    let switch = |enabled| Command::SetEffectEnabled {
        effect: first.id(),
        enabled,
    };
    let remove = |effect: Effect| Command::RemoveEffects {
        effects: vec![effect.id()],
    };
    vec![
        add(0, first),
        add(1, second),
        Command::MoveEffect {
            effect: second.id(),
            index: 0,
        },
        switch(false),
        switch(true),
        remove(second),
        remove(first),
    ]
}

/// Plays `project` in blocks of `block`, making each of `changes` in turn
/// every `every` frames, then plays on for as long again.
fn play_changing(project: &Project, changes: &[Command], every: usize, block: usize) -> Vec<f32> {
    let mut project = project.clone();
    let mut renderer = Renderer::new(stereo(), Snapshot::from(&project), block);
    renderer.controller.play().unwrap();
    renderer.render(every);
    for command in changes {
        project.apply(command).unwrap();
        renderer.controller.set_project(&project).unwrap();
        renderer.render(every);
    }
    renderer.into_samples()
}

/// Switched off, or at Mix 0, an effect changes nothing: a busy loop
/// renders the same as with no effect at all, bit for bit.
#[test]
fn off_and_mix_0_match_no_effect_bit_for_bit() {
    let dry = play(&demo_loop(), 3.0);
    for kind in EffectKind::ALL {
        for chain in chains(&demo_loop()) {
            let off = kit_effect(kind, 0).with_enabled(false);
            let silent = kit_effect(kind, 1).with_mix(0.0);
            let both = with(with(demo_loop(), chain, 0, off), chain, 1, silent);
            assert!(play(&both, 3.0) == dry, "{kind:?} on {chain:?}");
            // Switched on at its Mix, it's heard.
            let heard = with(demo_loop(), chain, 0, kit_effect(kind, 0));
            assert!(play(&heard, 3.0) != dry, "{kind:?} on {chain:?} is heard");
        }
    }
}

/// Adding, moving, switching and removing effects while a tone plays
/// never jumps by more than the tone and its echoes do: each fades in and
/// out over 10 ms.
#[test]
fn adding_moving_switching_and_removing_while_playing_do_not_click() {
    let every = 24_000;
    for kind in EffectKind::ALL {
        for chain in chains(&held_tone()) {
            let samples = play_changing(&held_tone(), &changes(kind, chain), every, 128);
            // The tone with each effect is the same tone, a few times over
            // and delayed: at its loudest, it moves at most as fast as an
            // A3 sine that loud. The left and right are interleaved, so
            // each side is checked on its own.
            for side in 0..2 {
                let side: Vec<f32> = samples.iter().skip(side).step_by(2).copied().collect();
                let limit = 1.25 * sine_max_step(220.0, 48_000) * peak(&side);
                assert_no_click(&side, limit, &format!("{kind:?} on {chain:?}"));
            }
        }
    }
}

/// Blocks of 32, 128 and 1024 give the same audio, with effects added,
/// moved, switched and removed while a busy loop plays, each change on the
/// same sample: every fade ends on its own sample, whatever the block.
#[test]
fn every_block_size_gives_the_same_audio_through_every_change() {
    let every = 24 * 1024;
    for kind in EffectKind::ALL {
        for chain in chains(&demo_loop()) {
            let changes = changes(kind, chain);
            let reference = play_changing(&demo_loop(), &changes, every, 128);
            for block in [32, 1024] {
                assert!(
                    play_changing(&demo_loop(), &changes, every, block) == reference,
                    "{kind:?} on {chain:?}, blocks of {block}"
                );
            }
        }
    }
}

/// After Stop, an effect's tail rings out, then it sleeps: the status shows
/// its chain as silent, and from then on the output is silence. Nothing on
/// the way out is a slow "denormal" number.
#[test]
fn tails_ring_out_then_sleep() {
    let mut stab = project(120.0, 4, &PLAIN_SINE, vec![note(0, 69, 0, 240)]);
    stab.apply(&Command::SetLoopEnabled { enabled: false })
        .unwrap();
    for kind in EffectKind::ALL {
        for chain in chains(&stab) {
            let project = with(stab.clone(), chain, 0, kit_effect(kind, 0));
            let mut renderer = Renderer::new(stereo(), Snapshot::from(&project), 128);
            renderer.controller.play().unwrap();
            renderer.render_seconds(0.2);
            renderer.controller.stop().unwrap();
            let ringing = |renderer: &mut Renderer| {
                let status = renderer.take_status();
                status.ringing_chains != 0 || status.master_ringing
            };
            renderer.render(128);
            assert!(ringing(&mut renderer), "{kind:?} on {chain:?} rings");
            let mut seconds = 0.0;
            while ringing(&mut renderer) {
                assert!(seconds < 10.0, "{kind:?} on {chain:?} never sleeps");
                renderer.render(128);
                seconds += 128.0 / 48_000.0;
            }
            assert!(seconds > 0.5, "{kind:?} on {chain:?} rang for {seconds} s");
            let asleep = renderer.samples().len();
            renderer.render_seconds(1.0);
            let samples = renderer.samples();
            assert!(
                samples[asleep..].iter().all(|&sample| sample == 0.0),
                "{kind:?} on {chain:?} is silent once asleep"
            );
            assert!(
                samples
                    .iter()
                    .all(|&sample| sample == 0.0 || sample.is_normal()),
                "{kind:?} on {chain:?} made a denormal"
            );
        }
    }
}

/// Removing a track while its effects ring fades them out quickly, without
/// a click, and their states come back.
#[test]
fn removing_a_track_fades_its_tail_out_quickly() {
    for kind in EffectKind::ALL {
        let mut project = held_tone();
        let track = project.tracks()[0].id();
        project = with(project, Chain::Track(track), 0, kit_effect(kind, 0));
        let mut renderer = Renderer::new(stereo(), Snapshot::from(&project), 128);
        renderer.controller.play().unwrap();
        renderer.render_seconds(0.5);
        project
            .apply(&Command::RemoveTracks {
                tracks: vec![track],
            })
            .unwrap();
        renderer.controller.set_project(&project).unwrap();
        let removed = renderer.samples().len();
        renderer.render_seconds(0.5);
        let samples = renderer.samples();
        let fade = (EFFECT_FADE_SECONDS * 48_000.0) as usize * 2;
        assert!(
            samples[removed + fade..]
                .iter()
                .all(|&sample| sample == 0.0),
            "{kind:?}: silent once faded out"
        );
        let limit = 1.25 * sine_max_step(220.0, 48_000) * peak(&samples[..removed]);
        for side in 0..2 {
            let side: Vec<f32> = samples.iter().skip(side).step_by(2).copied().collect();
            assert_no_click(&side, limit, &format!("{kind:?}"));
        }
        assert_eq!(renderer.controller.effect_states_out(), 0, "{kind:?}");
    }
}
