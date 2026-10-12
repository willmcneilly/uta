//! The Delay through the whole engine: added by a command, its state made
//! on the control side and handed to the audio thread, played by the real
//! processor, and ringing on after Stop and past the song's end. See
//! RFC-007, "How we'll verify it".

mod common;

use common::{
    PLAIN_SINE, demo_loop, level_at, max_difference, note, peak, project, rms, synth_project,
};
use uta_core::{
    Chain, Command, DelayParam, DelaySettings, Effect, EffectId, EffectSettings, PlacedEffect,
    Project,
};
use uta_engine::offline::{self, MAX_TAIL_SECONDS, Renderer};
use uta_engine::{EngineConfig, Snapshot};
use uuid::Uuid;

/// Below this, −120 dB, a sample is silence.
const SILENCE: f32 = 1e-6;

fn mono(sample_rate: u32) -> EngineConfig {
    EngineConfig {
        sample_rate,
        channels: 1,
    }
}

fn stereo() -> EngineConfig {
    EngineConfig {
        sample_rate: 48_000,
        channels: 2,
    }
}

fn effect_id(n: u128) -> EffectId {
    EffectId::from_uuid(Uuid::from_u128(5000 + n))
}

/// `project` with a Delay at the end of its first track's chain.
fn with_delay(project: Project, n: u128, time_seconds: f32, feedback: f32, mix: f32) -> Project {
    let track = project.tracks()[0].id();
    with_delay_on(project, Chain::Track(track), n, time_seconds, feedback, mix)
}

/// `project` with a Delay at the end of `chain`.
fn with_delay_on(
    mut project: Project,
    chain: Chain,
    n: u128,
    time_seconds: f32,
    feedback: f32,
    mix: f32,
) -> Project {
    let index = project.chain(chain).expect("the chain exists").len();
    let effect = Effect::new(
        effect_id(n),
        EffectSettings::Delay(DelaySettings::plain(time_seconds, feedback)),
    )
    .with_mix(mix);
    project
        .apply(&Command::AddEffects {
            effects: vec![PlacedEffect {
                chain,
                index,
                effect,
            }],
        })
        .expect("a valid Delay");
    project
}

/// A sine stab at the top of a 4-bar song: a 16th note of A4, which a
/// render plays once, with the loop off.
fn stab() -> Project {
    let mut project = project(120.0, 4, &PLAIN_SINE, vec![note(0, 69, 0, 240)]);
    project
        .apply(&Command::SetLoopEnabled { enabled: false })
        .unwrap();
    project
}

/// Plays `project` from the top for `seconds`, without stopping.
fn play(project: &Project, config: EngineConfig, seconds: f64) -> Vec<f32> {
    let mut renderer = Renderer::new(config, Snapshot::from(project), 128);
    renderer.controller.play().unwrap();
    renderer.render_seconds(seconds);
    renderer.into_samples()
}

/// With only the Delay's output (Mix 100%) and no feedback, the track is
/// itself, a time later: every sample lands exactly that many samples on, at
/// every rate.
#[test]
fn an_echo_comes_back_on_exactly_the_right_sample_at_each_rate() {
    for rate in [44_100, 48_000, 96_000] {
        let dry = play(&stab(), mono(rate), 0.6);
        let wet = play(&with_delay(stab(), 0, 0.25, 0.0, 1.0), mono(rate), 0.6);
        let at = rate as usize / 4;
        assert!(peak(&dry[..at]) > 0.01, "the stab plays at {rate} Hz");
        assert_eq!(
            peak(&wet[..at]),
            0.0,
            "nothing before the echo at {rate} Hz"
        );
        assert_eq!(
            max_difference(&wet[at..], &dry[..dry.len() - at]),
            0.0,
            "the echo is the stab {at} samples on, at {rate} Hz"
        );
    }
}

/// Each echo of the stab is Feedback times as loud as the one before.
#[test]
fn each_echo_is_feedback_times_the_one_before() {
    let rate = 48_000;
    let time = 0.2;
    let spacing = (f64::from(rate) * time) as usize;
    for feedback in [0.3, 0.6, 0.9] {
        let wet = play(
            &with_delay(stab(), 0, time as f32, feedback, 1.0),
            mono(rate),
            1.3,
        );
        // The stab is 125 ms long, so each echo's middle is 62.5 ms in.
        let middle = rate as usize / 16;
        let levels: Vec<f32> = (1..=5)
            .map(|n| level_at(&wet, n * spacing + middle, 480))
            .collect();
        for pair in levels.windows(2) {
            let ratio = pair[1] / pair[0];
            assert!(
                (ratio - feedback).abs() < 0.01 * feedback,
                "feedback {feedback}: echoes at {levels:?}"
            );
        }
    }
}

/// At Mix 0 you hear the sound unchanged: a busy loop with a Delay at
/// Mix 0, however much feedback, is the same render as with no Delay, bit
/// for bit.
#[test]
fn mix_0_matches_no_effect_bit_for_bit() {
    let dry = play(&demo_loop(), stereo(), 3.0);
    let silent = with_delay(
        with_delay(demo_loop(), 0, 0.125, 0.9, 0.0),
        1,
        0.003,
        1.0,
        0.0,
    );
    assert_eq!(play(&silent, stereo(), 3.0), dry);
    // At Mix 30%, it isn't.
    let heard = with_delay(demo_loop(), 0, 0.125, 0.9, 0.3);
    assert_ne!(play(&heard, stereo(), 3.0), dry);
}

/// Blocks of 32, 128 and 1024 give the same audio through Delays, one
/// with its echoes coming round and one a few samples long.
#[test]
fn every_block_size_gives_the_same_audio() {
    let project = with_delay(
        with_delay(demo_loop(), 0, 0.125, 0.7, 0.5),
        1,
        0.0031,
        0.5,
        0.3,
    );
    let render = |block| {
        let mut renderer = Renderer::new(stereo(), Snapshot::from(&project), block);
        renderer.controller.play().unwrap();
        renderer.render_seconds(3.0);
        renderer.into_samples()
    };
    let reference = render(128);
    for block in [32, 1024] {
        assert!(render(block) == reference, "blocks of {block}");
    }
}

/// A Mix halfway keeps both the sound and its echo at about −3 dB, so the
/// blend doesn't dip: equal-power.
#[test]
fn halfway_both_the_sound_and_its_echo_are_at_about_minus_3_db() {
    let rate = 48_000;
    let dry = play(&stab(), mono(rate), 0.6);
    let half = play(&with_delay(stab(), 0, 0.25, 0.0, 0.5), mono(rate), 0.6);
    let middle = rate as usize / 16;
    let gain = |at| level_at(&half, at, 480) / level_at(&dry, middle, 480);
    let expected = std::f32::consts::FRAC_1_SQRT_2;
    assert!(
        (gain(middle) - expected).abs() < 0.01,
        "the stab: {}",
        gain(middle)
    );
    assert!(
        (gain(rate as usize / 4 + middle) - expected).abs() < 0.01,
        "its echo: {}",
        gain(rate as usize / 4 + middle)
    );
}

/// Turning the Mix or the time while it plays glides: no sample jumps by
/// more than the tone itself does.
#[test]
fn turning_the_mix_and_time_while_it_plays_does_not_click() {
    let held = project(120.0, 4, &PLAIN_SINE, vec![note(0, 57, 0, 4 * 3840)]);
    let mut project = with_delay(held, 0, 0.1, 0.5, 0.0);
    let mut renderer = Renderer::new(mono(48_000), Snapshot::from(&project), 128);
    renderer.controller.play().unwrap();
    renderer.render_seconds(0.5);
    for command in [
        Command::SetEffectMix {
            effect: effect_id(0),
            mix: 1.0,
        },
        Command::SetDelayParam {
            effect: effect_id(0),
            param: DelayParam::TimeSeconds(0.11),
        },
        Command::SetEffectMix {
            effect: effect_id(0),
            mix: 0.4,
        },
    ] {
        project.apply(&command).unwrap();
        renderer.controller.set_project(&project).unwrap();
        renderer.render_seconds(0.3);
    }
    // Two echoes of an A3 sine and the sine itself, each at most at the
    // tone's own steepest.
    let limit = 3.0 * common::sine_max_step(220.0, 48_000) * peak(renderer.samples());
    common::assert_no_click(renderer.samples(), limit, "turning the Delay");
}

/// After Stop, the track keeps sounding until the echoes die away, and the
/// status says its chain is ringing until then.
#[test]
fn after_stop_the_echoes_ring_out_then_the_chain_is_quiet() {
    let project = with_delay(stab(), 0, 0.25, 0.5, 0.5);
    let mut renderer = Renderer::new(mono(48_000), Snapshot::from(&project), 128);
    renderer.controller.play().unwrap();
    renderer.render_seconds(0.2);
    renderer.controller.stop().unwrap();
    renderer.render(128);
    assert!(!renderer.take_status().playing);
    // The stab has ended; its echoes go on at 0.25 s, 0.5 s, 0.75 s...
    renderer.render_seconds(1.0);
    let status = renderer.take_status();
    assert_eq!(status.ringing_chains, 1, "slot 0's chain is ringing");
    assert_eq!(status.sounding_slots, 0, "the synth itself has stopped");
    let samples = renderer.samples();
    let middle = 48_000 / 16;
    for n in 1..=4 {
        assert!(
            level_at(samples, n * 12_000 + middle, 480) > 1e-3,
            "echo {n} after Stop"
        );
    }
    // 0.5 to the 17th is below −120 dB: about 4.5 s of echoes.
    renderer.render_seconds(5.0);
    assert_eq!(renderer.take_status().ringing_chains, 0);
    let end = renderer.samples().len();
    assert!(peak(&renderer.samples()[end - 48_000..]) < SILENCE);
}

/// `uta render` runs past the song's end until every chain is silent: the
/// render with a Delay is longer, ends in silence, and isn't cut off.
#[test]
fn a_render_runs_on_until_the_echoes_die_away() {
    let seconds = 0.5;
    let dry = offline::render_song(stereo(), Snapshot::from(&stab()), seconds);
    let wet = offline::render_song(
        stereo(),
        Snapshot::from(&with_delay(stab(), 0, 0.3, 0.6, 0.5)),
        seconds,
    );
    assert!(!dry.cut_off && !wet.cut_off);
    let length = |samples: &[f32]| samples.len() as f64 / 2.0 / 48_000.0;
    // 0.6 to the 25th is below −120 dB: about 7.5 s of echoes after the
    // stab, with its release.
    assert!(length(&dry.samples) < 0.6, "{}", length(&dry.samples));
    let tail = length(&wet.samples) - seconds;
    assert!((6.5..9.0).contains(&tail), "a tail of {tail} s");
    // The last echo on the tape above −120 dB is about a time before the
    // end: quieter still by the time it's out, after the Mix and the master.
    let end = wet.samples.len();
    assert!(peak(&wet.samples[end - 2 * 4800..]) < SILENCE);
    assert!(peak(&wet.samples[end - 2 * 48_000..]) > 1e-8);
}

/// A render runs on for the master's echoes too: with a Delay only on the
/// master, it's as long as with one on the track. From the first review.
#[test]
fn a_render_runs_on_until_the_masters_echoes_die_away() {
    let seconds = 0.5;
    let wet = offline::render_song(
        stereo(),
        Snapshot::from(&with_delay_on(stab(), Chain::Master, 0, 0.3, 0.6, 0.5)),
        seconds,
    );
    assert!(!wet.cut_off);
    let tail = wet.samples.len() as f64 / 2.0 / 48_000.0 - seconds;
    assert!((6.5..9.0).contains(&tail), "a tail of {tail} s");
    let end = wet.samples.len();
    assert!(peak(&wet.samples[end - 2 * 48_000..]) > 1e-8);
}

/// A device switch makes every Delay's state again for the new rate, in
/// `Processor::prepare`, on a track and on the master. Made at 48 kHz, a
/// Delay's tape holds under 3 s at 96 kHz, and its time would still be
/// counted at 48 kHz: after the switch, a 5 s echo comes back exactly 5 s
/// later at 96 kHz. From the first review.
#[test]
fn a_device_switch_makes_every_delay_again_for_the_new_rate() {
    let render = |project: &Project| {
        let mut renderer = Renderer::new(mono(48_000), Snapshot::from(project), 128);
        // What the supervisor does between streams.
        renderer.processor().prepare(96_000, 1);
        // The controller hears of the new rate and sends the snapshot
        // again, retimed, and the output fades back in.
        renderer.render(2048);
        let start = renderer.samples().len();
        renderer.controller.play().unwrap();
        renderer.render(96_000 * 5 + 48_000);
        renderer.samples()[start..].to_vec()
    };
    let dry = render(&stab());
    let at = 5 * 96_000;
    assert!(peak(&dry[..at]) > 0.01, "the stab plays");
    for chain in [Chain::Track(stab().tracks()[0].id()), Chain::Master] {
        let wet = render(&with_delay_on(stab(), chain, 0, 5.0, 0.0, 1.0));
        assert_eq!(
            peak(&wet[..at]),
            0.0,
            "nothing before the echo on {chain:?}"
        );
        assert_eq!(
            max_difference(&wet[at..], &dry[..dry.len() - at]),
            0.0,
            "the echo is the stab 5 s on at 96 kHz, on {chain:?}"
        );
    }
}

/// Echoes that never fade stop the render at the limit, which says so.
#[test]
fn a_render_stops_at_the_limit_when_the_echoes_never_fade() {
    let song = offline::render_song(
        mono(48_000),
        Snapshot::from(&with_delay(stab(), 0, 0.5, 1.0, 0.5)),
        0.5,
    );
    assert!(song.cut_off);
    let tail = song.samples.len() as f64 / 48_000.0 - 0.5;
    assert!(
        (tail - MAX_TAIL_SECONDS).abs() < 0.01,
        "a tail of {tail} s, not {MAX_TAIL_SECONDS}"
    );
    // Still echoing at the end, as loud as the first: the cuts, wide open,
    // take only the very lowest and highest of the stab's sound each time
    // round, and leave its tone.
    let end = song.samples.len();
    let first = rms(&song.samples[24_000..48_000]);
    let last = rms(&song.samples[end - 24_000..]);
    assert!((last - first).abs() < 0.02 * first, "{first} then {last}");
}

/// A project with no effects renders exactly as long as it always has:
/// the tail is only the release.
#[test]
fn without_effects_a_render_has_only_the_release() {
    let project = synth_project();
    let song = offline::render_song(stereo(), Snapshot::from(&project), 1.0);
    assert!(!song.cut_off);
    assert_eq!(song.samples.len(), 2 * 48_000 + 2 * 480);
}
