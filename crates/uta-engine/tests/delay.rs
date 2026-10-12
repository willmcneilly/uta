//! The Delay through the whole engine: added by a command, its state made
//! on the control side and handed to the audio thread, played by the real
//! processor, and ringing on after Stop and past the song's end. See
//! RFC-007, "How we'll verify it".

mod common;

use common::{
    PLAIN_SINE, demo_loop, level_at, max_difference, measure_frequency, note, peak, project, rms,
    share_above, spectral_centroid, synth_project,
};
use uta_core::time::NoteDivision;
use uta_core::{
    Chain, Command, DelayMode, DelayParam, DelaySettings, Effect, EffectId, EffectSettings,
    PlacedEffect, Project, SynthParam, Waveform,
};
use uta_engine::offline::{self, MAX_TAIL_SECONDS, Renderer};
use uta_engine::{EngineConfig, Snapshot, WOBBLE_REACH_SECONDS};
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

/// `project` with a plain Delay at the end of `chain`.
fn with_delay_on(
    project: Project,
    chain: Chain,
    n: u128,
    time_seconds: f32,
    feedback: f32,
    mix: f32,
) -> Project {
    let settings = DelaySettings::plain(time_seconds, feedback);
    with_settings_on(project, chain, n, settings, mix)
}

/// `project` with a Delay with `settings` at the end of its first track's
/// chain.
fn with_settings(project: Project, n: u128, settings: DelaySettings, mix: f32) -> Project {
    let track = project.tracks()[0].id();
    with_settings_on(project, Chain::Track(track), n, settings, mix)
}

/// `project` with a Delay with `settings` at the end of `chain`.
fn with_settings_on(
    mut project: Project,
    chain: Chain,
    n: u128,
    settings: DelaySettings,
    mix: f32,
) -> Project {
    let index = project.chain(chain).expect("the chain exists").len();
    let effect = Effect::new(effect_id(n), EffectSettings::Delay(settings)).with_mix(mix);
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
    stab_at(120.0)
}

/// [`stab`] at `bpm`.
fn stab_at(bpm: f32) -> Project {
    once(project(bpm, 4, &PLAIN_SINE, vec![note(0, 69, 0, 240)]))
}

/// `project` with its loop off, so a render plays it once.
fn once(mut project: Project) -> Project {
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
    assert!((last - first).abs() < 1e-3 * first, "{first} then {last}");
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

/// The Delay at its plainest, synced: the division at the tempo, with no
/// feedback.
fn synced(division: &str) -> DelaySettings {
    DelaySettings {
        sync: true,
        division: NoteDivision::from_value(division).unwrap(),
        ..DelaySettings::plain(1.0, 0.0)
    }
}

/// Synced, an echo comes back exactly a division later, whatever the
/// tempo: a quarter at 120 BPM is 24,000 samples at 48 kHz, a quarter
/// triplet at 100 BPM 19,200, and a dotted eighth at 90 BPM 24,000.
#[test]
fn a_synced_echo_comes_back_on_time_at_each_tempo() {
    for (bpm, division, at) in [
        (120.0, "1/4", 24_000),
        (100.0, "1/4t", 19_200),
        (90.0, "1/8d", 24_000),
    ] {
        let dry = play(&stab_at(bpm), mono(48_000), 1.0);
        let wet = play(
            &with_settings(stab_at(bpm), 0, synced(division), 1.0),
            mono(48_000),
            1.0,
        );
        assert!(peak(&dry[..at]) > 0.01, "the stab plays at {bpm} BPM");
        assert_eq!(peak(&wet[..at]), 0.0, "nothing before {division} at {bpm}");
        assert_eq!(
            max_difference(&wet[at..], &dry[..dry.len() - at]),
            0.0,
            "{division} at {bpm} BPM is {at} samples"
        );
    }
}

/// Renders `project` with and without a Delay of `settings` at Mix 100%,
/// playing for `before` seconds, then applying `change` to both (or only
/// the wet, for a change to the Delay) and playing on for `after`. Returns
/// the dry render and the wet.
fn dry_and_wet_through(
    project: &Project,
    settings: DelaySettings,
    change: &Command,
    before: f64,
    after: f64,
) -> (Vec<f32>, Vec<f32>) {
    let render = |mut project: Project| {
        let mut renderer = Renderer::new(mono(48_000), Snapshot::from(&project), 128);
        renderer.controller.play().unwrap();
        renderer.render_seconds(before);
        // Without the Delay, a change to it has nothing to change.
        if project.apply(change).is_err() {
            assert!(matches!(change, Command::SetDelayParam { .. }));
        }
        renderer.controller.set_project(&project).unwrap();
        renderer.render_seconds(after);
        renderer.into_samples()
    };
    let wet = with_settings(project.clone(), 0, settings, 1.0);
    (render(project.clone()), render(wet))
}

/// An A3 sine held for the whole 4-bar song at 120 BPM.
fn held() -> Project {
    once(project(
        120.0,
        4,
        &PLAIN_SINE,
        vec![note(0, 57, 0, 4 * 3840)],
    ))
}

/// Turning the time while echoes are on their way bends them, as tape
/// does: the playback head glides from 0.1 s to 0.25 s, so the held tone's
/// echo drops in pitch while it moves, and nothing jumps by more than the
/// tone itself can. Once the glide is over, the echo is the tone exactly
/// 0.25 s later.
#[test]
fn changing_the_time_bends_the_echoes_then_lands_on_the_new_time() {
    let (dry, wet) = dry_and_wet_through(
        &held(),
        DelaySettings::plain(0.1, 0.0),
        &Command::SetDelayParam {
            effect: effect_id(0),
            param: DelayParam::TimeSeconds(0.25),
        },
        0.5,
        3.0,
    );
    // The tone at its steepest, played at most half as fast again.
    let limit = 1.5 * common::sine_max_step(220.0, 48_000) * peak(&dry) * 1.01;
    common::assert_no_click(&wet, limit, "changing the time");
    // A tenth of a second into the glide, the echo plays slower: lower.
    let bending = measure_frequency(&wet[28_800..28_800 + 2400], 48_000);
    assert!(bending < 200.0, "the echo bends to {bending} Hz");
    let at = 12_000;
    let landed = 2 * 48_000;
    assert_eq!(
        max_difference(&wet[landed..], &dry[landed - at..dry.len() - at]),
        0.0,
        "the echo lands exactly 0.25 s on"
    );
}

/// A tempo change moves a synced Delay's time as turning it does: the
/// echoes of the held tone drift to the new time without a jump, and land
/// exactly on it. A quarter at 120 BPM is 0.5 s, and at 100 BPM 0.6 s.
#[test]
fn a_tempo_change_glides_a_synced_delay_to_its_new_time() {
    let (dry, wet) = dry_and_wet_through(
        &held(),
        synced("1/4"),
        &Command::SetTempo { bpm: 100.0 },
        1.0,
        4.0,
    );
    let limit = 1.5 * common::sine_max_step(220.0, 48_000) * peak(&dry) * 1.01;
    common::assert_no_click(&wet, limit, "changing the tempo");
    let at = 28_800;
    let landed = 3 * 48_000;
    assert_eq!(
        max_difference(&wet[landed..], &dry[landed - at..dry.len() - at]),
        0.0,
        "the echo lands exactly 0.6 s on"
    );
}

/// A saw stab, bright with every harmonic: a 16th note of `pitch` at the
/// top of the song, played once.
fn saw_stab(pitch: u8) -> Project {
    let params = [
        SynthParam::Osc1Wave(Waveform::Saw),
        SynthParam::CutoffHz(20_000.0),
        SynthParam::Sustain(1.0),
        SynthParam::ReleaseSeconds(0.001),
    ];
    once(project(120.0, 4, &params, vec![note(0, pitch, 0, 240)]))
}

/// Each of the first five echoes of a stab, 0.25 s apart: the 0.125 s
/// each lasts.
fn five_echoes(samples: &[f32]) -> Vec<&[f32]> {
    (1..=5)
        .map(|n| &samples[n * 12_000..n * 12_000 + 6000])
        .collect()
}

/// The High cut is inside the loop: each echo has been through it once
/// more than the last, so each is darker, measured as its spectral
/// centroid. With the cut wide open, they stay as bright as the first.
#[test]
fn with_the_high_cut_on_each_echo_is_darker_than_the_last() {
    let centroids = |high_cut_hz| {
        let settings = DelaySettings {
            high_cut_hz,
            ..DelaySettings::plain(0.25, 0.7)
        };
        let wet = play(
            &with_settings(saw_stab(57), 0, settings, 1.0),
            mono(48_000),
            1.5,
        );
        five_echoes(&wet)
            .into_iter()
            .map(|echo| spectral_centroid(echo, 48_000))
            .collect::<Vec<_>>()
    };
    let dark = centroids(2000.0);
    for pair in dark.windows(2) {
        assert!(pair[1] < pair[0], "centroids {dark:?}");
    }
    assert!(dark[4] < 0.25 * dark[0], "centroids {dark:?}");
    let open = centroids(20_000.0);
    for centroid in &open {
        assert!(
            (centroid / open[0] - 1.0).abs() < 0.05,
            "wide open, centroids {open:?}"
        );
    }
}

/// The Low cut is inside the loop too: each echo of a low saw has less of
/// its bass than the last, measured as the share of it below 300 Hz, which
/// falls to a fraction each time round.
#[test]
fn with_the_low_cut_on_each_echo_is_thinner_than_the_last() {
    let settings = DelaySettings {
        low_cut_hz: 400.0,
        ..DelaySettings::plain(0.25, 0.7)
    };
    let wet = play(
        &with_settings(saw_stab(45), 0, settings, 1.0),
        mono(48_000),
        1.5,
    );
    let lows: Vec<f64> = five_echoes(&wet)
        .into_iter()
        .map(|echo| 1.0 - share_above(echo, 48_000, 300.0))
        .collect();
    for pair in lows.windows(2) {
        assert!(pair[1] < 0.5 * pair[0], "shares below 300 Hz {lows:?}");
    }
}

/// At 110% feedback with full Drive, the echoes of a loud stab swell until
/// the saturation holds them, then sit at a steady howl well below full
/// scale: the track and the master at 0 dB, and only the Delay heard.
#[test]
fn at_110_percent_with_full_drive_the_echoes_settle_below_full_scale() {
    let mut project = saw_stab(57);
    project
        .apply(&Command::SetMasterVolume { volume_db: 0.0 })
        .unwrap();
    let settings = DelaySettings {
        drive: 1.0,
        ..DelaySettings::plain(0.25, 1.1)
    };
    let wet = play(
        &with_settings(project, 0, settings, 1.0),
        mono(48_000),
        30.0,
    );
    assert!(peak(&wet) < 0.5, "peak {}", peak(&wet));
    // Each of the last ten seconds is about as loud as the others: it
    // neither grows nor dies away.
    let seconds: Vec<f64> = (20..30)
        .map(|n| rms(&wet[n * 48_000..(n + 1) * 48_000]))
        .collect();
    let mean = seconds.iter().sum::<f64>() / seconds.len() as f64;
    assert!(mean > 0.01, "still howling: {seconds:?}");
    for second in &seconds {
        assert!((second / mean - 1.0).abs() < 0.05, "{seconds:?}");
    }
}

/// Ping-pong records both sides onto the left tape and crosses each echo
/// to the other side: a mono stab's echoes go left, right, left, right,
/// with the other side silent each time, though the stab itself is on
/// both.
#[test]
fn ping_pong_bounces_a_mono_sounds_echoes_left_and_right() {
    let settings = DelaySettings {
        mode: DelayMode::PingPong,
        ..DelaySettings::plain(0.25, 0.6)
    };
    let wet = play(&with_settings(stab(), 0, settings, 1.0), stereo(), 1.5);
    let side =
        |offset: usize| -> Vec<f32> { wet.iter().skip(offset).step_by(2).copied().collect() };
    let (left, right) = (side(0), side(1));
    for (n, (left, right)) in five_echoes(&left)
        .into_iter()
        .zip(five_echoes(&right))
        .enumerate()
    {
        let (heard, other) = if n % 2 == 0 {
            (left, right)
        } else {
            (right, left)
        };
        assert!(peak(heard) > 1e-3, "echo {} is heard", n + 1);
        assert!(
            peak(other) < 1e-3 * peak(heard),
            "echo {}: {} on the other side, against {}",
            n + 1,
            peak(other),
            peak(heard)
        );
    }
}

/// Plays `project` from the top for `seconds`, stops and lets it ring out
/// for `rest`, then plays it from the top again for `seconds`. Returns
/// both plays.
fn play_twice(project: &Project, seconds: f64, rest: f64) -> (Vec<f32>, Vec<f32>) {
    let mut renderer = Renderer::new(stereo(), Snapshot::from(project), 128);
    renderer.controller.play().unwrap();
    renderer.render_seconds(seconds);
    let first = renderer.samples().to_vec();
    renderer.controller.stop().unwrap();
    renderer.render_seconds(rest);
    let start = renderer.samples().len();
    renderer.controller.play().unwrap();
    renderer.render_seconds(seconds);
    (first, renderer.samples()[start..].to_vec())
}

/// Wobble is deterministic, and restarts on Play: the demo loop through a
/// wobbling Delay renders the same twice, and playing it from the top a
/// second time, once the echoes have died away, sounds exactly as the
/// first time did.
#[test]
fn wobble_renders_the_same_every_time_and_restarts_on_play() {
    let settings = DelaySettings {
        wobble: 1.0,
        ..DelaySettings::plain(0.3, 0.3)
    };
    let project = with_settings(demo_loop(), 0, settings, 0.5);
    assert_eq!(play(&project, stereo(), 3.0), play(&project, stereo(), 3.0));
    let (first, again) = play_twice(&once(project), 3.0, 8.0);
    assert!(first == again, "the second play differs");
}

/// Noise stabs every eighth note at 120 BPM, each a short burst whose
/// echo is easy to find: noise matches itself at only one place.
fn noise_stabs() -> Project {
    let params = [
        SynthParam::Osc1Level(0.0),
        SynthParam::NoiseLevel(1.0),
        SynthParam::CutoffHz(20_000.0),
        SynthParam::AttackSeconds(0.001),
        SynthParam::DecaySeconds(0.02),
        SynthParam::Sustain(0.0),
        SynthParam::ReleaseSeconds(0.001),
    ];
    let notes = (0..32).map(|n| note(n, 60, n as u64 * 480, 120)).collect();
    once(project(120.0, 4, &params, notes))
}

/// How far each noise stab's echo is from where it would be with no
/// Wobble, in samples: the lag at which the echo best matches the stab,
/// within 300 samples of 0.1 s.
fn echo_shifts(dry: &[f32], wet: &[f32]) -> Vec<i64> {
    let time = 4800i64;
    (0..32)
        .map(|n| {
            let start = n * 12_000;
            let stab = &dry[start..start + 1200];
            let matches = |lag: i64| -> f64 {
                let from = (start as i64 + time + lag) as usize;
                stab.iter()
                    .zip(&wet[from..from + stab.len()])
                    .map(|(a, b)| f64::from(*a) * f64::from(*b))
                    .sum()
            };
            (-300..=300)
                .max_by(|&a, &b| matches(a).total_cmp(&matches(b)))
                .unwrap()
        })
        .collect()
}

/// Wobble sways the playback head by about the amount set: at full, the
/// echoes of stabs over 8 seconds land up to its reach (2.7 ms, 130
/// samples) early or late, and at half, up to half that. At 0 every echo
/// is exactly on time.
#[test]
fn wobble_moves_the_echoes_by_about_the_amount_set() {
    let dry = play(&noise_stabs(), mono(48_000), 8.5);
    let reach = WOBBLE_REACH_SECONDS * 48_000.0;
    for wobble in [0.0, 0.5, 1.0] {
        let settings = DelaySettings {
            wobble,
            ..DelaySettings::plain(0.1, 0.0)
        };
        let wet = play(
            &with_settings(noise_stabs(), 0, settings, 1.0),
            mono(48_000),
            8.5,
        );
        let shifts = echo_shifts(&dry, &wet);
        let furthest = shifts.iter().map(|shift| shift.abs()).max().unwrap() as f64;
        let set = reach * f64::from(wobble);
        if wobble == 0.0 {
            assert!(shifts.iter().all(|&shift| shift == 0), "{shifts:?}");
        } else {
            assert!(
                (0.7 * set..=set + 1.0).contains(&furthest),
                "Wobble {wobble}: up to {furthest} samples, set {set}: {shifts:?}"
            );
        }
    }
}

/// Every setting glides while echoes ring, and Play restarts the wobble
/// without a jump: a held A3 through a Delay at 60% Feedback, with only
/// its echoes heard, has Mode, both cuts, Drive and Wobble turned in turn,
/// then Stop and Play pressed while the echoes ring, and no sample jumps by
/// more than its echoes could, played a little sharp by the Wobble.
#[test]
fn turning_every_setting_and_playing_again_while_it_rings_does_not_click() {
    // Released over 50 ms, so Stop doesn't cut the tone off with a jump
    // of its own.
    let soft = [
        SynthParam::Osc1Wave(Waveform::Sine),
        SynthParam::Sustain(1.0),
        SynthParam::ReleaseSeconds(0.05),
    ];
    let held = once(project(120.0, 4, &soft, vec![note(0, 57, 0, 4 * 3840)]));
    let dry = play(&held, mono(48_000), 0.5);
    let project = with_settings(held, 0, DelaySettings::plain(0.1, 0.6), 1.0);
    let mut renderer = Renderer::new(stereo(), Snapshot::from(&project), 128);
    renderer.controller.play().unwrap();
    // A second and a quarter of a cycle, so each change lands where the
    // tone is at its peak, not where it crosses zero and a jump would be
    // smallest.
    renderer.render(48_000 + 55);
    let mut project = project;
    let set = |param| Command::SetDelayParam {
        effect: effect_id(0),
        param,
    };
    for command in [
        set(DelayParam::Mode(DelayMode::PingPong)),
        set(DelayParam::LowCutHz(400.0)),
        set(DelayParam::HighCutHz(2000.0)),
        set(DelayParam::Drive(1.0)),
        set(DelayParam::Wobble(1.0)),
        set(DelayParam::Mode(DelayMode::Stereo)),
    ] {
        project.apply(&command).unwrap();
        renderer.controller.set_project(&project).unwrap();
        renderer.render_seconds(0.3);
    }
    renderer.controller.stop().unwrap();
    renderer.render_seconds(0.05);
    assert_eq!(renderer.take_status().ringing_chains, 1, "the echoes ring");
    renderer.controller.play().unwrap();
    renderer.render_seconds(0.5);
    // The echoes add up to at most the tone over (1 - Feedback), on one
    // side √2 louder in ping-pong, and the steepest each can be is the
    // tone's own, played sharp: by up to 2% by full Wobble, and 5.4% more
    // while the playback head glides back from where the old sway had it
    // (2.7 ms over 50 ms) after Play.
    let limit =
        common::sine_max_step(220.0 * 1.08, 48_000) * peak(&dry) * std::f32::consts::SQRT_2 / 0.4;
    for side in [0, 1] {
        let samples: Vec<f32> = renderer
            .samples()
            .iter()
            .skip(side)
            .step_by(2)
            .copied()
            .collect();
        common::assert_no_click(&samples, limit, "turning the tape loop");
    }
}
