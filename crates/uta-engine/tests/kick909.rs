//! The 909 kick, measured from its sound. See RFC-006, "How we'll verify
//! it", and UTA-56.
//!
//! Every threshold here was calibrated once against the reference behaviour
//! (Plaits' `SyntheticBassDrum`, which the kick is ported from, and the
//! research's 909 recipe) and says why it is what it is.

mod common;

use common::*;
use uta_core::time::{TICKS_PER_QUARTER, Ticks};
use uta_core::{Command, DrumParam, DrumSound, KickModel, Project};
use uta_engine::offline::Renderer;
use uta_engine::{EngineConfig, REFERENCE_PEAK, Snapshot};

const RATE: u32 = 48_000;
const BEAT: Ticks = TICKS_PER_QUARTER;

fn config() -> EngineConfig {
    EngineConfig {
        sample_rate: RATE,
        channels: 1,
    }
}

fn seconds(samples: f64) -> usize {
    (samples * f64::from(RATE)) as usize
}

fn kick(param: DrumParam) -> (DrumSound, DrumParam) {
    (DrumSound::Kick, param)
}

/// The 909 chosen, then `params` on it.
fn tr909(params: &[DrumParam]) -> Vec<(DrumSound, DrumParam)> {
    let mut all = vec![kick(DrumParam::Model(KickModel::Tr909))];
    all.extend(params.iter().map(|&param| kick(param)));
    all
}

/// One 909 kick with these settings, at this velocity, rendered for
/// `seconds`.
fn one_909(params: &[DrumParam], velocity: u8, seconds: f64) -> Vec<f32> {
    one_hit(KICK, &tr909(params), velocity, seconds)
}

/// The average pitch over `from..to` seconds.
fn pitch_between(samples: &[f32], from: f64, to: f64) -> f64 {
    let cycles: Vec<f64> = pitch_over_time(samples, RATE)
        .into_iter()
        .filter(|&(time, _)| (from..to).contains(&time))
        .map(|(_, hz)| hz)
        .collect();
    assert!(!cycles.is_empty(), "no cycles between {from} and {to} s");
    cycles.iter().sum::<f64>() / cycles.len() as f64
}

/// The pitch of the first whole cycle of the body. At a full accent the
/// click can make a short "cycle" of its own in the first 3 ms, far above
/// anything the body sweeps to (6 times Tune), so that's left out.
fn starting_pitch(samples: &[f32], tune_hz: f64) -> f64 {
    pitch_over_time(samples, RATE)
        .into_iter()
        .map(|(_, hz)| hz)
        .find(|&hz| hz < 6.0 * tune_hz)
        .expect("a cycle")
}

// Pitch.

/// The kick ends on its Tune and starts higher: the sweep. Measured, it
/// ends within 0.2% of Tune (the VCA's drive bends each cycle a little);
/// 1% is a sixth of a semitone. At the default Sweep the first whole cycle
/// is about twice Tune: well over the 1.5 times asserted.
#[test]
fn the_909_kick_ends_on_its_tune_and_starts_higher() {
    for tune in [45.0, 55.0, 62.0, 70.0] {
        for velocity in [100, 127] {
            let samples = one_909(&[DrumParam::TuneHz(tune)], velocity, 0.4);
            let tune = f64::from(tune);
            let end = pitch_between(&samples, 0.15, 0.3);
            assert!(
                (end / tune - 1.0).abs() < 0.01,
                "Tune {tune}, velocity {velocity}: ends at {end:.2} Hz"
            );
            let start = starting_pitch(&samples, tune);
            assert!(
                start > 1.5 * tune,
                "Tune {tune}, velocity {velocity}: starts at {start:.1} Hz"
            );
        }
    }
}

/// Sweep is how far the pitch drops at the start. Its first half deepens
/// the drop: with none the kick starts at Tune (within the 5% the first
/// cycle's rising envelope bends it), and every step starts it higher. Its
/// second half slows the drop, so at Sweep 1 the pitch is still well above
/// Tune 40 ms in, where at 0.5 it has nearly landed.
#[test]
fn sweep_widens_the_drop() {
    let start = |sweep: f32| starting_pitch(&one_909(&[DrumParam::Sweep(sweep)], 100, 0.3), 55.0);
    let none = start(0.0);
    assert!(
        (none / 55.0 - 1.0).abs() < 0.05,
        "Sweep 0 starts at {none:.1}"
    );
    let starts: Vec<f64> = [0.0, 0.1, 0.25, 0.4, 0.5].map(start).to_vec();
    for pair in starts.windows(2) {
        assert!(pair[1] > pair[0] * 1.1, "starts {starts:?}");
    }
    let at_40ms =
        |sweep: f32| pitch_between(&one_909(&[DrumParam::Sweep(sweep)], 100, 0.3), 0.03, 0.05);
    let (half, full) = (at_40ms(0.5), at_40ms(1.0));
    assert!(half < 55.0 * 1.15, "Sweep 0.5 at 40 ms: {half:.1}");
    assert!(full > 55.0 * 1.6, "Sweep 1 at 40 ms: {full:.1}");
}

/// Raising Tune raises the pitch it lands on, step by step.
#[test]
fn raising_tune_raises_the_pitch() {
    let pitches: Vec<f64> = [45.0, 50.0, 55.0, 62.0, 70.0]
        .map(|tune| pitch_between(&one_909(&[DrumParam::TuneHz(tune)], 100, 0.4), 0.15, 0.3))
        .to_vec();
    for pair in pitches.windows(2) {
        assert!(pair[1] > pair[0], "{pitches:?}");
    }
}

// The click and the tone.

/// The share of a hit's energy in its first 5 ms, against its first 50 ms:
/// how much of it is the click.
fn click_share(samples: &[f32]) -> f64 {
    let energy = |samples: &[f32]| samples.iter().map(|&s| f64::from(s).powi(2)).sum::<f64>();
    energy(&samples[..seconds(0.005)]) / energy(&samples[..seconds(0.05)])
}

/// Attack is how much click there is: it raises the click's share of the
/// energy in the first 5 ms at every step. Measured at a full accent, the
/// share goes from about 4% with no Attack to about 10% at full; the asserts
/// leave room either side. And the click is sharp: a step through a 5 kHz
/// low-pass, far steeper than anything the body (under 300 Hz) makes, so
/// the steepest step in the first 5 ms grows with Attack too. Measured, it's
/// about 20 times steeper at full than with none.
#[test]
fn attack_raises_the_clicks_share_of_the_first_5ms() {
    for velocity in [100, 127] {
        let hits: Vec<Vec<f32>> = [0.0, 0.25, 0.5, 0.75, 1.0]
            .map(|attack| one_909(&[DrumParam::Attack(attack)], velocity, 0.2))
            .to_vec();
        let shares: Vec<f64> = hits.iter().map(|samples| click_share(samples)).collect();
        for pair in shares.windows(2) {
            assert!(pair[1] > pair[0], "velocity {velocity}: shares {shares:?}");
        }
        assert!(
            shares[4] > 2.0 * shares[0],
            "velocity {velocity}: shares {shares:?}"
        );
        let steps: Vec<f32> = hits
            .iter()
            .map(|samples| max_jump(&samples[..seconds(0.005)]).0)
            .collect();
        for pair in steps.windows(2) {
            assert!(pair[1] > pair[0], "velocity {velocity}: steps {steps:?}");
        }
        assert!(
            steps[4] > 5.0 * steps[0],
            "velocity {velocity}: steps {steps:?}"
        );
    }
}

/// A harder hit is brighter, not just louder: the envelopes rise higher, so
/// it drives the VCA harder and has a louder click. The centroid doesn't
/// depend on level, so a louder copy of the soft hit would measure the
/// same. Measured over 1 s: about 400 Hz at a full accent against 320 Hz at
/// velocity 64, so at least 10% apart, as the 808 kick's test asks, and an
/// unaccented hit between them.
#[test]
fn velocity_127_is_brighter_than_64() {
    let centroid = |velocity| spectral_centroid(&one_909(&[], velocity, 1.0), RATE);
    let (soft, ordinary, accent) = (centroid(64), centroid(100), centroid(127));
    assert!(
        soft < ordinary && ordinary < accent,
        "{soft} {ordinary} {accent}"
    );
    assert!(accent > 1.1 * soft, "{soft:.0} Hz then {accent:.0} Hz");
    // And louder: velocity 100 is an ordinary hit, about 7 dB under a full
    // accent, as the 808 kick's is.
    let peak_of = |velocity| peak(&one_909(&[], velocity, 0.3));
    let db = 20.0 * f64::from(peak_of(100) / peak_of(127)).log10();
    assert!((-10.0..-5.0).contains(&db), "{db} dB");
}

/// The default kick at full accent peaks at the kit's reference, as the
/// 808's does, so switching model keeps the level.
#[test]
fn a_full_accent_peaks_at_the_reference() {
    let peak = peak(&one_909(&[], 127, 0.5));
    assert!((peak - REFERENCE_PEAK).abs() < 0.01, "{peak}");
}

/// Decay follows the control: doubling it roughly doubles how long the
/// kick takes to die away by 40 dB. Measured at 0.25, 0.5 and 1 s, the
/// ratio is about 1.9 each time: the click, which doesn't change, makes the
/// peak the decay is measured from.
#[test]
fn doubling_decay_roughly_doubles_it() {
    let measured = |decay: f32| {
        let samples = one_909(&[DrumParam::DecaySeconds(decay)], 100, 3.0);
        decay_seconds(&samples, RATE, 40.0).expect("it dies away")
    };
    let times = [0.25, 0.5, 1.0].map(measured);
    for pair in times.windows(2) {
        let ratio = pair[1] / pair[0];
        assert!((1.6..2.4).contains(&ratio), "{times:?}");
    }
}

// No clicks.

/// A click is a step the sound couldn't make otherwise. The 909 kick's
/// steepest step is its own click, the Attack: a step rung through a 5 kHz
/// low-pass. Measured, it's at most 0.37 of the hit's peak, at full Attack
/// and full accent, and about 0.1 at the defaults. A hit that started at
/// its peak, or a restart cutting a ring off, steps by its whole level.
const HIT_STEP_LIMIT: f32 = 0.45;

/// The limit for anything that happens while a kick rings: the steepest
/// step a lone hit with the same settings makes, plus the ring's own
/// steepest step at `level` (a sine at the top of the sweep, 4.5 times
/// Tune), with 10% to spare.
fn ringing_limit(lone_hit_step: f32, tune_hz: f64, level: f32) -> f32 {
    (lone_hit_step + sine_max_step(tune_hz * 4.5, RATE) * level) * 1.1
}

/// The steepest step of a lone hit with these settings.
fn lone_step(params: &[DrumParam], velocity: u8) -> f32 {
    max_jump(&one_909(params, velocity, 1.0)).0
}

#[test]
fn a_hit_does_not_click() {
    use DrumParam as P;
    let cases: [(&str, Vec<DrumParam>); 9] = [
        ("defaults", vec![]),
        ("Attack 0", vec![P::Attack(0.0)]),
        ("Attack 1", vec![P::Attack(1.0)]),
        ("Sweep 0", vec![P::Sweep(0.0)]),
        ("Sweep 1", vec![P::Sweep(1.0)]),
        ("Tune 45", vec![P::TuneHz(45.0)]),
        (
            "Tune 70, Sweep 1, Attack 1",
            vec![P::TuneHz(70.0), P::Sweep(1.0), P::Attack(1.0)],
        ),
        ("Decay 0.1", vec![P::DecaySeconds(0.1)]),
        ("Level +6 dB", vec![P::LevelDb(6.0)]),
    ];
    for (what, params) in cases {
        for velocity in [1, 64, 100, 127] {
            let samples = one_909(&params, velocity, 0.5);
            let limit = peak(&samples) * HIT_STEP_LIMIT;
            assert_no_click(&samples, limit, &format!("{what}, velocity {velocity}"));
        }
    }
}

/// Guards the click limits: a ring cut off part-way, as a kick that
/// restarted its oscillator would, fails them.
#[test]
fn the_click_limits_catch_a_restart() {
    let mut samples = one_909(&[], 100, 0.5);
    let lone = max_jump(&samples).0;
    let level = peak(&samples);
    // Cut the ring off at its loudest point after the sweep.
    let ring = seconds(0.05)..seconds(0.08);
    let at = ring.start
        + samples[ring]
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.abs().total_cmp(&b.1.abs()))
            .unwrap()
            .0;
    samples[at + 1..].fill(0.0);
    assert!(max_jump(&samples).0 > level * HIT_STEP_LIMIT);
    assert!(max_jump(&samples).0 > ringing_limit(lone, 55.0, level));
}

/// Fast repeats of the same sound, at the longest Decay so they pile up:
/// 16ths, then 32nds, then a roll of 64ths at 120 BPM. Each hit adds to the
/// ring, and the oscillator carries on through it, so none clicks.
#[test]
fn fast_repeats_do_not_click() {
    let params = [DrumParam::DecaySeconds(1.5)];
    let mut hits = Vec::new();
    for (bar, step) in [BEAT / 4, BEAT / 8, BEAT / 16].into_iter().enumerate() {
        for t in (0..4 * BEAT).step_by(step as usize) {
            hits.push(hit(
                hits.len() as u128,
                KICK,
                100,
                bar as u64 * 4 * BEAT + t,
            ));
        }
    }
    let samples = render_drums(&drum_project(120.0, 3, &tr909(&params), hits), 6.5, 128);
    let limit = ringing_limit(lone_step(&params, 100), 55.0, peak(&samples));
    assert_no_click(&samples, limit, "fast repeats");
    // The envelopes add up to a ceiling rather than without end, so a roll
    // stays clear of full scale.
    assert!(peak(&samples) < 0.9, "{}", peak(&samples));
}

/// Flams: a soft grace hit, then an accent, 40 ms down to 1 ms apart.
#[test]
fn flams_do_not_click() {
    let mut hits = Vec::new();
    // 1 tick is about 0.52 ms at 120 BPM.
    for (i, gap) in [77, 38, 19, 10, 2].into_iter().enumerate() {
        let start = (i as u64 + 1) * 2 * BEAT;
        hits.push(hit(2 * i as u128, KICK, 64, start - gap));
        hits.push(hit(2 * i as u128 + 1, KICK, 127, start));
    }
    let samples = render_drums(&drum_project(120.0, 3, &tr909(&[]), hits), 6.0, 128);
    let limit = ringing_limit(lone_step(&[], 127), 55.0, peak(&samples));
    assert_no_click(&samples, limit, "flams");
}

/// Plays `project` with `turns` applied one after another every 90 ms while
/// it plays, and returns what it played.
fn with_turns(mut project: Project, turns: &[(DrumSound, DrumParam)]) -> Vec<f32> {
    let mut renderer = Renderer::new(config(), Snapshot::from(&project), 128);
    renderer.controller.play().unwrap();
    renderer.render(seconds(0.05));
    for &(sound, param) in turns {
        project
            .apply(&Command::SetDrumParam {
                track: drum_track(),
                sound,
                param,
            })
            .unwrap();
        renderer.controller.set_project(&project).unwrap();
        renderer.render(seconds(0.09));
    }
    renderer.into_samples()
}

/// Every control swept between its limits, faster than it can glide, while
/// a long kick rings. They all glide, so nothing clicks.
#[test]
fn turning_the_controls_while_it_rings_does_not_click() {
    use DrumParam as P;
    let sweeps: [(&str, [DrumParam; 2]); 5] = [
        ("Tune", [P::TuneHz(70.0), P::TuneHz(45.0)]),
        ("Sweep", [P::Sweep(1.0), P::Sweep(0.0)]),
        ("Attack", [P::Attack(1.0), P::Attack(0.0)]),
        ("Decay", [P::DecaySeconds(0.1), P::DecaySeconds(1.5)]),
        ("Level", [P::LevelDb(6.0), P::LevelDb(-60.0)]),
    ];
    let base = [P::DecaySeconds(1.5), P::Attack(1.0), P::Sweep(1.0)];
    let step = lone_step(&base, 127);
    for (what, [one, other]) in sweeps {
        // Kicks every beat, so the turns land on hits as well as rings.
        let hits = (0..8).map(|i| hit(i, KICK, 127, i as u64 * BEAT)).collect();
        let project = drum_project(120.0, 2, &tr909(&base), hits);
        let turns: Vec<_> = (0..40)
            .map(|turn| kick(if turn % 2 == 0 { one } else { other }))
            .collect();
        let samples = with_turns(project, &turns);
        // Level +6 dB doubles everything, and Tune 70 is the highest ring.
        let limit = ringing_limit(step * 2.0, 70.0, peak(&samples));
        assert_no_click(&samples, limit, what);
    }
}

/// Every control glides in when it's turned while the kick rings, rather
/// than jumping: compared with the same render without the turn, the turn
/// makes little of its difference in the first tenth of the 20 ms glide.
/// Sweep is turned as the hit lands, while it sweeps. Attack shapes only the
/// first few milliseconds of a hit, the click, too short to see a 20 ms
/// glide in; turning it never clicks (above).
#[test]
fn every_control_glides_in() {
    use DrumParam as P;
    let glide = seconds(uta_engine::DRUM_SMOOTHING_SECONDS);
    for (what, param, at) in [
        ("Tune", P::TuneHz(70.0), 12 * 128),
        ("Decay", P::DecaySeconds(0.1), 12 * 128),
        ("Level", P::LevelDb(6.0), 12 * 128),
        ("Sweep", P::Sweep(1.0), 0),
    ] {
        let base = tr909(&[P::DecaySeconds(1.5), P::Sweep(0.5)]);
        let difference = turn_difference(KICK, &base, kick(param), at, 0.3);
        assert_glides(&difference, glide, what);
    }
}

// The model.

/// Switching the model while a kick rings doesn't cut it off or click: the
/// kick that was ringing rings on to its end, and the next hit plays the new
/// model. Both ways round.
#[test]
fn switching_the_model_while_a_kick_rings_does_not_click() {
    for (from, to) in [
        (KickModel::Tr808, KickModel::Tr909),
        (KickModel::Tr909, KickModel::Tr808),
    ] {
        let base = [
            kick(DrumParam::Model(from)),
            kick(DrumParam::DecaySeconds(0.8)),
        ];
        // 32 ms into the ring: switching changes nothing that's sounding.
        let difference = turn_difference(KICK, &base, kick(DrumParam::Model(to)), 12 * 128, 0.5);
        assert!(
            difference.iter().all(|&d| d == 0.0),
            "{from:?} to {to:?} changed the ring"
        );
        // Switching back and forth while a beat plays: every hit lands on a
        // ring of one model or the other, and none clicks.
        let hits = (0..8).map(|i| hit(i, KICK, 127, i as u64 * BEAT)).collect();
        let project = drum_project(120.0, 2, &base, hits);
        let turns: Vec<_> = (0..40)
            .map(|turn| kick(DrumParam::Model(if turn % 2 == 0 { to } else { from })))
            .collect();
        let samples = with_turns(project, &turns);
        let step = lone_step(&[DrumParam::DecaySeconds(0.8)], 127)
            .max(max_jump(&one_hit(KICK, &base, 127, 1.0)).0);
        let limit = ringing_limit(step, 55.0, peak(&samples));
        assert_no_click(&samples, limit, &format!("{from:?} and {to:?}"));
    }
}

/// Each model keeps its own settings: set the 909's, switch to the 808 and
/// back, and it plays exactly as before. And the 808's sound doesn't change
/// for the 909's settings being there.
#[test]
fn each_model_plays_its_own_settings() {
    let tuned = [DrumParam::TuneHz(62.0), DrumParam::Sweep(0.8)];
    let direct = one_909(&tuned, 100, 0.5);
    let mut there_and_back = tr909(&tuned);
    there_and_back.push(kick(DrumParam::Model(KickModel::Tr808)));
    there_and_back.push(kick(DrumParam::Model(KickModel::Tr909)));
    assert_eq!(one_hit(KICK, &there_and_back, 100, 0.5), direct);
    let mut tr808 = tr909(&tuned);
    tr808.push(kick(DrumParam::Model(KickModel::Tr808)));
    assert_eq!(
        one_hit(KICK, &tr808, 100, 0.5),
        one_hit(KICK, &[], 100, 0.5)
    );
    assert_ne!(one_hit(KICK, &tr808, 100, 0.5), direct);
}

// Determinism.

/// A beat with everything in it: accents, soft hits, a repeat, a flam, and
/// the snare and hats over it.
fn beat() -> Project {
    let pattern = [
        (KICK, 0, 127),
        (CLOSED_HAT, BEAT / 2, 80),
        (SNARE, BEAT, 100),
        (KICK, BEAT + BEAT / 2, 70),
        (KICK, 2 * BEAT - 10, 64),
        (KICK, 2 * BEAT, 127),
        (KICK, 2 * BEAT + BEAT / 4, 90),
        (SNARE, 3 * BEAT, 100),
        (KICK, 3 * BEAT + BEAT / 2, 100),
    ];
    let hits = pattern
        .iter()
        .enumerate()
        .map(|(i, &(pitch, start, velocity))| hit(i as u128, pitch, velocity, start))
        .collect();
    drum_project(120.0, 1, &tr909(&[DrumParam::Sweep(0.6)]), hits)
}

#[test]
fn renders_are_identical_twice_and_at_any_block_size() {
    let project = beat();
    let reference = render_drums(&project, 4.5, 128);
    assert!(peak(&reference) > 0.1);
    assert_eq!(
        render_drums(&project, 4.5, 128),
        reference,
        "rendered twice"
    );
    for block_size in [32, 1024] {
        assert_eq!(
            render_drums(&project, 4.5, block_size),
            reference,
            "block size {block_size}"
        );
    }
}
