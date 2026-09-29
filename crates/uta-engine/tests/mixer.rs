//! The mixer: a track's volume, pan and mute, the stereo master and its hard
//! clip, and how the mix reaches a device with one, two or more channels.
//! Rendered offline through the real processor and measured from the
//! waveform. See RFC-003, "The mixer in the track headers" and "The master
//! and headroom".
//!
//! No command sets a track's mixer strip yet, so these set it in the
//! snapshot directly.

mod common;

use common::*;
use uta_core::{Project, SynthParam};
use uta_engine::offline::Renderer;
use uta_engine::{
    EngineConfig, MixerStrip, Snapshot, VOICE_LEVEL, VOLUME_SMOOTHING_SECONDS, db_to_gain,
    pitch_to_hz,
};

const RATE: u32 = 48_000;
/// A 1-bar loop at 120 BPM, in samples.
const LOOP: usize = 96_000;

fn config(channels: usize) -> EngineConfig {
    EngineConfig {
        sample_rate: RATE,
        channels,
    }
}

/// A 1-bar loop of one A4 sine note filling the bar, at full velocity: a
/// steady tone at exactly [`VOICE_LEVEL`] times the master volume between
/// its attack and its release at the loop's end.
fn held_a4() -> Project {
    let a4 = uta_core::Note {
        velocity: 127,
        ..note(0, 69, 0, 3840)
    };
    project(120.0, 1, &PLAIN_SINE, vec![a4])
}

/// The held note with the master at 0 dB and the track's strip set to
/// `mixer`.
fn held_a4_through(mixer: MixerStrip) -> Snapshot {
    Snapshot {
        mixer,
        ..Snapshot::from(&held_a4()).with_volume_db(0.0)
    }
}

fn strip(volume_db: f32, pan: f32) -> MixerStrip {
    MixerStrip {
        volume_db,
        pan,
        mute: false,
    }
}

/// Splits interleaved stereo into its left and right.
fn sides(samples: &[f32]) -> [Vec<f32>; 2] {
    let (frames, rest) = samples.as_chunks::<2>();
    assert!(rest.is_empty(), "not whole stereo frames");
    [
        frames.iter().map(|frame| frame[0]).collect(),
        frames.iter().map(|frame| frame[1]).collect(),
    ]
}

/// The steady part of the held note through `mixer` in stereo, from 0.1 s
/// into the loop to 0.1 s before its end: its peak level on the left and
/// right.
fn steady_levels(mixer: MixerStrip) -> [f32; 2] {
    let mut renderer = Renderer::new(config(2), held_a4_through(mixer), 128);
    renderer.controller.play().unwrap();
    renderer.render(LOOP);
    sides(renderer.samples()).map(|side| peak(&side[LOOP / 20..LOOP - LOOP / 20]))
}

/// Whether `level` is within 0.1% of `expected`, or both are silent.
fn close(level: f32, expected: f32) -> bool {
    if expected == 0.0 {
        level == 0.0
    } else {
        (level / expected - 1.0).abs() < 1e-3
    }
}

/// The click limit for one sine voice at up to `level`: the steepest step
/// the sine takes, plus the steepest the 5 ms attack adds, plus 10%.
fn click_limit(level: f32) -> f32 {
    let attack = 0.005 * f64::from(RATE);
    (sine_max_step(pitch_to_hz(69), RATE) * level + level / attack as f32) * 1.1
}

#[test]
fn volume_changes_the_level_by_its_db() {
    for volume_db in [6.0, 3.0, 0.0, -6.0, -24.0] {
        let expected = VOICE_LEVEL * db_to_gain(volume_db);
        let [left, right] = steady_levels(strip(volume_db, 0.0));
        assert!(close(left, expected), "{volume_db} dB: left {left}");
        assert!(close(right, expected), "{volume_db} dB: right {right}");
    }
}

#[test]
fn volume_goes_up_to_6_db_and_no_further() {
    let loudest = steady_levels(strip(6.0, 0.0));
    assert_eq!(steady_levels(strip(12.0, 0.0)), loudest);
    // Well above the master's 0 dB ceiling.
    assert!(
        close(loudest[0], VOICE_LEVEL * db_to_gain(6.0)),
        "{loudest:?}"
    );
}

/// The "−3 dB compensated" constant-power law: centred, unchanged in both
/// speakers; hard to one side, 3 dB up in that speaker and silent in the
/// other; in between, the same total power.
#[test]
fn pan_follows_the_compensated_constant_power_law() {
    let root_two = std::f32::consts::SQRT_2;
    let cases = [
        (0.0, [1.0, 1.0]),
        (-1.0, [root_two, 0.0]),
        (1.0, [0.0, root_two]),
        // √2·sin((1 ∓ pan)·π/4).
        (-0.5, [1.306_563, 0.541_196]),
        (0.25, [0.785_695, 1.175_876]),
    ];
    for (pan, [left_gain, right_gain]) in cases {
        let [left, right] = steady_levels(strip(0.0, pan));
        assert!(
            close(left, VOICE_LEVEL * left_gain),
            "pan {pan}: left {left}, expected {}",
            VOICE_LEVEL * left_gain
        );
        assert!(
            close(right, VOICE_LEVEL * right_gain),
            "pan {pan}: right {right}, expected {}",
            VOICE_LEVEL * right_gain
        );
        let power = (left / VOICE_LEVEL).powi(2) + (right / VOICE_LEVEL).powi(2);
        assert!((power - 2.0).abs() < 4e-3, "pan {pan}: power {power}");
    }
}

#[test]
fn mute_silences_the_track() {
    for pan in [-1.0, 0.0, 0.7] {
        let muted = MixerStrip {
            mute: true,
            ..strip(6.0, pan)
        };
        let mut renderer = Renderer::new(config(2), held_a4_through(muted), 128);
        renderer.controller.play().unwrap();
        renderer.render(LOOP);
        assert_eq!(peak(renderer.samples()), 0.0, "pan {pan}");
    }
}

/// Volume, pan and mute changes while the note plays, including reversals
/// mid-glide: nothing on either side jumps more than the note itself does at
/// the loudest the strip makes it.
#[test]
fn mixer_changes_glide_without_clicks() {
    let mut renderer = Renderer::new(config(2), held_a4_through(strip(0.0, 0.0)), 128);
    renderer.controller.play().unwrap();
    renderer.render(LOOP / 10);
    let changes = [
        strip(6.0, 0.0),
        strip(-40.0, 0.0),
        strip(6.0, -1.0),
        strip(6.0, 1.0),
        MixerStrip {
            mute: true,
            ..strip(6.0, 1.0)
        },
        strip(6.0, 1.0),
        strip(-60.0, -1.0),
        strip(6.0, 0.3),
        MixerStrip {
            mute: true,
            ..strip(0.0, 0.0)
        },
        strip(6.0, -1.0),
    ];
    // Each change lands mid-glide of the one before, then after the glide.
    for gap in [300, 2_000] {
        for mixer in changes {
            let snapshot = Snapshot {
                mixer,
                ..renderer.controller.snapshot().clone()
            };
            renderer.controller.set_snapshot(snapshot).unwrap();
            renderer.render(gap);
        }
    }
    renderer.render(LOOP / 10);

    // √2 for the pan, and +6 dB.
    let loudest = VOICE_LEVEL * std::f32::consts::SQRT_2 * db_to_gain(6.0);
    let limit = click_limit(loudest);
    let [left, right] = sides(renderer.samples());
    assert!(peak(&left) > VOICE_LEVEL * 2.5, "never got loud");
    for (name, side) in [("left", left), ("right", right)] {
        let (jump, at) = max_jump(&side);
        assert!(
            jump <= limit,
            "{name}: jump of {jump} at frame {at}, limit {limit}"
        );
    }
}

/// A change takes the smoothing time to arrive, and is part-way there
/// halfway through.
#[test]
fn a_pan_change_glides_to_its_new_level() {
    let mut renderer = Renderer::new(config(2), held_a4_through(strip(0.0, 0.0)), 128);
    renderer.controller.play().unwrap();
    renderer.render_seconds(0.2);
    let snapshot = Snapshot {
        mixer: strip(0.0, 1.0),
        ..renderer.controller.snapshot().clone()
    };
    renderer.controller.set_snapshot(snapshot).unwrap();
    renderer.render_seconds(0.2);
    let change = renderer.frames_for(0.2);
    let glide = renderer.frames_for(VOLUME_SMOOTHING_SECONDS);
    let [left, right] = sides(renderer.samples());

    // Halfway, each side is about halfway between its old and new gain.
    let mid = change + glide / 2;
    let left_mid = level_at(&left, mid, 120) / VOICE_LEVEL;
    let right_mid = level_at(&right, mid, 120) / VOICE_LEVEL;
    assert!(
        left_mid > 0.4 && left_mid < 0.6,
        "left mid-glide {left_mid}"
    );
    assert!(
        right_mid > 1.1 && right_mid < 1.3,
        "right mid-glide {right_mid}"
    );
    // After the glide it has arrived.
    assert_eq!(peak(&left[change + glide..]), 0.0);
    let right_after = peak(&right[change + glide..]);
    assert!(close(right_after, VOICE_LEVEL * std::f32::consts::SQRT_2));
}

/// A mono device gets the average of the left and right, so a centred track
/// sounds as it did before the mix was stereo, and a panned one keeps half
/// its level on the far side's average.
#[test]
fn a_mono_device_gets_the_average_of_left_and_right() {
    for mixer in [strip(0.0, 0.0), strip(3.0, -0.6), strip(-6.0, 1.0)] {
        let snapshot = Snapshot {
            mixer,
            ..Snapshot::from(&demo_loop())
        };
        let render = |channels| {
            let mut renderer = Renderer::new(config(channels), snapshot.clone(), 128);
            renderer.controller.play().unwrap();
            renderer.render_seconds(1.0);
            renderer.into_samples()
        };
        let mono = render(1);
        let [left, right] = sides(&render(2));
        assert!(peak(&mono) > 0.01, "{mixer:?}: silent");
        for ((&mono, &left), &right) in mono.iter().zip(&left).zip(&right) {
            assert_eq!(mono, (left + right) * 0.5, "{mixer:?}");
        }
    }
}

#[test]
fn channels_past_the_second_are_silent() {
    let snapshot = Snapshot {
        mixer: strip(0.0, -0.4),
        ..Snapshot::from(&demo_loop())
    };
    let render = |channels| {
        let mut renderer = Renderer::new(config(channels), snapshot.clone(), 128);
        renderer.controller.play().unwrap();
        renderer.render_seconds(1.0);
        renderer.into_samples()
    };
    let stereo = render(2);
    let four = render(4);
    assert_eq!(four.len(), stereo.len() * 2);
    for (frame, expected) in four
        .as_chunks::<4>()
        .0
        .iter()
        .zip(stereo.as_chunks::<2>().0)
    {
        assert_eq!(frame[..2], *expected);
        assert_eq!(frame[2..], [0.0, 0.0]);
    }
    assert!(peak(&stereo) > 0.01);
}

/// Sixteen loud square notes at once, with the track at +6 dB and the master
/// at 0 dB, add up to several times full scale. The hard clip keeps every
/// sample on every channel within it, and counts what it cut off.
#[test]
fn loud_notes_never_pass_full_scale_and_are_counted() {
    let chord = (0..16)
        .map(|i| uta_core::Note {
            velocity: 127,
            ..note(i, 36 + i as u8 * 3, 0, 3840)
        })
        .collect();
    let loud = project(
        120.0,
        1,
        &[SynthParam::Waveform(uta_core::Waveform::Square)],
        chord,
    );
    let snapshot = Snapshot {
        mixer: strip(6.0, 0.5),
        ..Snapshot::from(&loud).with_volume_db(0.0)
    };
    for channels in [1, 2] {
        let mut renderer = Renderer::new(config(channels), snapshot.clone(), 128);
        renderer.controller.play().unwrap();
        renderer.render(LOOP / 4);
        let halfway = renderer.controller.poll().clips;
        renderer.render(LOOP / 4);
        let status = renderer.controller.poll();

        let samples = renderer.samples();
        assert_eq!(peak(samples), 1.0, "{channels} channels");
        assert!(samples.iter().all(|s| s.abs() <= 1.0));
        assert!(halfway > 1_000, "{channels} channels: {halfway} clips");
        assert!(status.clips > halfway, "the count doesn't keep running");
    }
}

/// A mix within full scale never touches the clip: the demo loop at the
/// default master volume, and the held note at the loudest a single voice
/// gets, +6 dB and panned hard.
#[test]
fn a_quiet_mix_is_never_clipped() {
    let snapshots = [
        Snapshot::from(&demo_loop()),
        held_a4_through(strip(6.0, -1.0)),
    ];
    for snapshot in snapshots {
        let mut renderer = Renderer::new(config(2), snapshot, 128);
        renderer.controller.play().unwrap();
        renderer.render(LOOP);
        let status = renderer.controller.poll();
        assert_eq!(status.clips, 0);
        assert!(peak(renderer.samples()) < 1.0);
    }
}
