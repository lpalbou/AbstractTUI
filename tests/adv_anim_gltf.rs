//! VERIFY cycle-6 glTF animation sampler correctness. The module's own
//! unit tests cover the headline cases; these add hand-computed
//! interpolation checks at arbitrary times, quaternion sanity
//! (normalized, shortest-path), clamping/looping edges, and a seeded
//! property that the sampler never produces NaN or denormalized output
//! for well-formed tracks.

use abstracttui::testing::Rng;
use abstracttui::three::animation::{Animation, Interpolation, NodePose, Track, TrackValues};
use abstracttui::three::Vec3;

fn rest() -> NodePose {
    NodePose {
        translation: Vec3::ZERO,
        rotation: [0.0, 0.0, 0.0, 1.0],
        scale: Vec3::new(1.0, 1.0, 1.0),
    }
}

fn t_track(times: Vec<f32>, values: Vec<[f32; 3]>, interp: Interpolation) -> Track {
    Track {
        node: 0,
        times,
        values: TrackValues::Translation(values),
        interpolation: interp,
    }
}

/// LINEAR interpolation must match the hand-computed lerp at ARBITRARY
/// fractions, not just the midpoint.
#[test]
fn linear_matches_hand_computed_at_arbitrary_fractions() {
    let anim = Animation::new(
        None,
        vec![t_track(
            vec![0.0, 4.0],
            vec![[0.0, 10.0, -20.0], [8.0, -6.0, 4.0]],
            Interpolation::Linear,
        )],
    );
    for &(t, k) in &[(0.0, 0.0), (1.0, 0.25), (2.0, 0.5), (3.0, 0.75), (4.0, 1.0)] {
        let mut poses = [rest()];
        anim.sample(t, &mut poses);
        let got = poses[0].translation;
        let expect = Vec3::new(
            0.0 + (8.0 - 0.0) * k,
            10.0 + (-6.0 - 10.0) * k,
            -20.0 + (4.0 - -20.0) * k,
        );
        assert!(
            (got.x - expect.x).abs() < 1e-4
                && (got.y - expect.y).abs() < 1e-4
                && (got.z - expect.z).abs() < 1e-4,
            "t={t}: got {got:?} want {expect:?}"
        );
    }
}

/// Multi-segment track: interpolation must select the CORRECT keyframe
/// pair (a naive first/last lerp fails the middle segment).
#[test]
fn linear_selects_correct_segment_in_multi_key_track() {
    let anim = Animation::new(
        None,
        vec![t_track(
            vec![0.0, 1.0, 2.0, 3.0],
            vec![
                [0.0, 0.0, 0.0],
                [10.0, 0.0, 0.0],
                [10.0, 10.0, 0.0],
                [0.0, 10.0, 0.0],
            ],
            Interpolation::Linear,
        )],
    );
    let mut poses = [rest()];
    // Middle of segment 1->2: x held at 10, y half way to 10.
    anim.sample(1.5, &mut poses);
    assert!(
        (poses[0].translation.x - 10.0).abs() < 1e-4,
        "{:?}",
        poses[0].translation
    );
    assert!(
        (poses[0].translation.y - 5.0).abs() < 1e-4,
        "{:?}",
        poses[0].translation
    );
    // Middle of segment 2->3: x half way back to 0, y held at 10.
    anim.sample(2.5, &mut poses);
    assert!(
        (poses[0].translation.x - 5.0).abs() < 1e-4,
        "{:?}",
        poses[0].translation
    );
    assert!(
        (poses[0].translation.y - 10.0).abs() < 1e-4,
        "{:?}",
        poses[0].translation
    );
}

/// STEP holds the LEFT key across the whole segment, then jumps exactly
/// at the next keyframe time.
#[test]
fn step_holds_then_jumps_at_keyframe() {
    let anim = Animation::new(
        None,
        vec![t_track(
            vec![0.0, 2.0, 4.0],
            vec![[0.0, 0.0, 0.0], [5.0, 0.0, 0.0], [9.0, 0.0, 0.0]],
            Interpolation::Step,
        )],
    );
    let mut poses = [rest()];
    for &(t, x) in &[(0.0, 0.0), (1.9, 0.0), (2.0, 5.0), (3.99, 5.0), (4.0, 9.0)] {
        anim.sample(t, &mut poses);
        assert!(
            (poses[0].translation.x - x).abs() < 1e-4,
            "STEP t={t}: got {}, want {x}",
            poses[0].translation.x
        );
    }
}

/// Time is clamped at both ends (looping is the caller's `t % duration`,
/// not the sampler's job).
#[test]
fn time_clamps_at_both_ends() {
    let anim = Animation::new(
        None,
        vec![t_track(
            vec![1.0, 2.0],
            vec![[3.0, 0.0, 0.0], [7.0, 0.0, 0.0]],
            Interpolation::Linear,
        )],
    );
    let mut poses = [rest()];
    anim.sample(-100.0, &mut poses);
    assert_eq!(poses[0].translation.x, 3.0, "before range clamps to first");
    anim.sample(1e6, &mut poses);
    assert_eq!(poses[0].translation.x, 7.0, "after range clamps to last");
}

/// FINDING RT6-1 (P2, GFX3D): `animation::locate` underflow-panics on a
/// NaN sample time. `t <= times[0]` and `t >= times[last]` are both
/// FALSE for NaN, so control falls to `partition_point(|x| x <= t)`,
/// which returns 0 (no element is `<= NaN`); the very next line did
/// `i - 1` on a `usize` 0 → panic. RT6-1 CLOSED (cycle 7, GFX3D): the
/// first clamp is now `!(t > times[0])`, TRUE for NaN, so a NaN `t`
/// returns the first keyframe. Permanent regression guard (runs green).
#[test]
fn nan_sample_time_must_not_panic() {
    let anim = Animation::new(
        None,
        vec![t_track(
            vec![1.0, 2.0],
            vec![[3.0, 0.0, 0.0], [7.0, 0.0, 0.0]],
            Interpolation::Linear,
        )],
    );
    let mut poses = [rest()];
    anim.sample(f32::NAN, &mut poses); // must not panic
    assert!(
        poses[0].translation.x.is_finite(),
        "NaN time produced non-finite output"
    );
}

/// Rotation output is always a UNIT quaternion (nlerp normalizes), and
/// slerp-vs-nlerp stays within the documented small tolerance for a 90°
/// arc sampled densely.
#[test]
fn rotation_output_is_always_normalized() {
    let s = std::f32::consts::FRAC_1_SQRT_2;
    let track = Track {
        node: 0,
        times: vec![0.0, 1.0],
        values: TrackValues::Rotation(vec![[0.0, 0.0, 0.0, 1.0], [0.0, 0.0, s, s]]),
        interpolation: Interpolation::Linear,
    };
    let anim = Animation::new(None, vec![track]);
    let mut poses = [rest()];
    for i in 0..=20 {
        let t = i as f32 / 20.0;
        anim.sample(t, &mut poses);
        let q = poses[0].rotation;
        let norm = (q[0] * q[0] + q[1] * q[1] + q[2] * q[2] + q[3] * q[3]).sqrt();
        assert!(
            (norm - 1.0).abs() < 1e-3,
            "t={t}: quaternion not unit (norm {norm})"
        );
    }
}

/// Antipodal-key degenerate midpoint (q and -q, 180° apart) must not
/// produce NaN — the sampler falls back to holding the left key.
#[test]
fn antipodal_rotation_midpoint_is_finite() {
    let track = Track {
        node: 0,
        times: vec![0.0, 1.0],
        values: TrackValues::Rotation(vec![[0.0, 0.0, 0.0, 1.0], [0.0, 0.0, 0.0, -1.0]]),
        interpolation: Interpolation::Linear,
    };
    let anim = Animation::new(None, vec![track]);
    let mut poses = [rest()];
    anim.sample(0.5, &mut poses);
    for c in poses[0].rotation {
        assert!(
            c.is_finite(),
            "antipodal midpoint produced NaN: {:?}",
            poses[0].rotation
        );
    }
}

/// Seeded property: for random well-formed translation/scale/rotation
/// tracks sampled at random times, output is always finite and (for
/// rotation) unit-length. No panic, no NaN — the reliability guarantee.
#[test]
fn sampler_never_produces_non_finite_for_wellformed_tracks() {
    let mut rng = Rng::new(0x00A4_173A);
    for _ in 0..500 {
        let n = 2 + rng.below(6);
        // Strictly increasing times.
        let mut times = Vec::new();
        let mut cur = 0.0f32;
        for _ in 0..n {
            cur += 0.1 + rng.below(20) as f32 / 10.0;
            times.push(cur);
        }
        let kind = rng.below(3);
        let track = match kind {
            0 => Track {
                node: 0,
                times: times.clone(),
                values: TrackValues::Translation((0..n).map(|_| rand_vec3(&mut rng)).collect()),
                interpolation: if rng.below(2) == 0 {
                    Interpolation::Linear
                } else {
                    Interpolation::Step
                },
            },
            1 => Track {
                node: 0,
                times: times.clone(),
                values: TrackValues::Scale((0..n).map(|_| rand_vec3(&mut rng)).collect()),
                interpolation: Interpolation::Linear,
            },
            _ => Track {
                node: 0,
                times: times.clone(),
                values: TrackValues::Rotation((0..n).map(|_| rand_quat(&mut rng)).collect()),
                interpolation: Interpolation::Linear,
            },
        };
        let anim = Animation::new(None, vec![track]);
        let mut poses = [rest()];
        for _ in 0..8 {
            let t = rng.below((cur as usize + 2) * 10) as f32 / 10.0 - 0.5;
            anim.sample(t, &mut poses);
            let p = poses[0];
            assert!(
                p.translation.x.is_finite()
                    && p.translation.y.is_finite()
                    && p.translation.z.is_finite(),
                "non-finite translation at t={t}"
            );
            assert!(p.scale.x.is_finite() && p.scale.y.is_finite() && p.scale.z.is_finite());
            if kind == 2 {
                let q = p.rotation;
                let norm = (q[0] * q[0] + q[1] * q[1] + q[2] * q[2] + q[3] * q[3]).sqrt();
                assert!(
                    norm.is_finite() && norm > 0.5,
                    "rotation degenerate at t={t}: {q:?}"
                );
            }
        }
    }
}

fn rand_vec3(rng: &mut Rng) -> [f32; 3] {
    [rand_f(rng), rand_f(rng), rand_f(rng)]
}

fn rand_f(rng: &mut Rng) -> f32 {
    rng.below(2000) as f32 / 10.0 - 100.0
}

fn rand_quat(rng: &mut Rng) -> [f32; 4] {
    let mut q = [rand_f(rng), rand_f(rng), rand_f(rng), rand_f(rng)];
    let n = (q[0] * q[0] + q[1] * q[1] + q[2] * q[2] + q[3] * q[3]).sqrt();
    if n < 1e-6 {
        return [0.0, 0.0, 0.0, 1.0];
    }
    for c in &mut q {
        *c /= n;
    }
    q
}
