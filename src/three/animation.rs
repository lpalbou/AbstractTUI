//! glTF node-TRS animation: keyframe tracks + a sampler that produces
//! per-node local transforms at time `t`.
//!
//! Scope (cycle 6): `translation`/`rotation`/`scale` channels with
//! LINEAR and STEP interpolation. CUBICSPLINE channels are SKIPPED
//! WITH A LABEL (their output accessors carry in-tangent/value/out-
//! tangent triplets — sampling them as plain values would play
//! garbage, and rejecting the whole file would kill channels that
//! play fine). Morph-target `weights` channels are skipped with a
//! label too (no morph pipeline).
//!
//! Rotation interpolation is NLERP with shortest-path sign correction
//! — normalized linear, not slerp: at cell-scale output and typical
//! keyframe densities the angular-velocity difference is invisible,
//! and nlerp is branch-free per component (the spec itself permits
//! implementations to approximate slerp).

use crate::three::math::{Mat4, Vec3};

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Interpolation {
    Linear,
    Step,
}

#[derive(Clone, Debug)]
pub enum TrackValues {
    Translation(Vec<[f32; 3]>),
    Rotation(Vec<[f32; 4]>),
    Scale(Vec<[f32; 3]>),
}

#[derive(Clone, Debug)]
pub struct Track {
    pub node: usize,
    /// Keyframe times, seconds, strictly non-decreasing (validated at
    /// build; a decreasing input accessor is a named load error).
    pub times: Vec<f32>,
    pub values: TrackValues,
    pub interpolation: Interpolation,
}

/// One node's local TRS pose (the sampler's output unit).
#[derive(Copy, Clone, Debug)]
pub struct NodePose {
    pub translation: Vec3,
    pub rotation: [f32; 4],
    pub scale: Vec3,
}

impl NodePose {
    pub fn matrix(&self) -> Mat4 {
        Mat4::from_trs(
            self.translation,
            (
                self.rotation[0],
                self.rotation[1],
                self.rotation[2],
                self.rotation[3],
            ),
            self.scale,
        )
    }
}

#[derive(Clone, Debug)]
pub struct Animation {
    pub name: Option<String>,
    pub tracks: Vec<Track>,
    duration: f32,
}

impl Animation {
    pub fn new(name: Option<String>, tracks: Vec<Track>) -> Animation {
        let duration = tracks
            .iter()
            .filter_map(|t| t.times.last().copied())
            .fold(0.0f32, f32::max);
        Animation {
            name,
            tracks,
            duration,
        }
    }

    /// Last keyframe time across all tracks (seconds).
    pub fn duration(&self) -> f32 {
        self.duration
    }

    /// Sample every track at `t` (seconds, CLAMPED to the keyframe
    /// range — looping is the caller's `t % duration`), overriding the
    /// animated fields of `poses[node]`. Untracked nodes keep their
    /// rest pose (the caller resets `poses` to rest first).
    pub fn sample(&self, t: f32, poses: &mut [NodePose]) {
        for track in &self.tracks {
            let Some(pose) = poses.get_mut(track.node) else {
                continue;
            };
            let (i0, i1, k) = locate(&track.times, t);
            let k = match track.interpolation {
                Interpolation::Step => 0.0,
                Interpolation::Linear => k,
            };
            match &track.values {
                TrackValues::Translation(v) => {
                    pose.translation = lerp3(v[i0], v[i1], k);
                }
                TrackValues::Scale(v) => {
                    pose.scale = lerp3(v[i0], v[i1], k);
                }
                TrackValues::Rotation(v) => {
                    pose.rotation = nlerp_quat(v[i0], v[i1], k);
                }
            }
        }
    }
}

/// Keyframe pair + blend factor for time `t`: clamps outside the
/// range; binary search inside it. NEVER panics on any float: a NaN
/// `t` clamps to the first keyframe (RT6-1 — reachable in practice
/// via `elapsed % duration` when every keyframe shares one time, so
/// duration is 0 and the modulo is NaN; `partition_point` would
/// return 0 and the pair index would underflow).
fn locate(times: &[f32], t: f32) -> (usize, usize, f32) {
    debug_assert!(!times.is_empty());
    // `!(t > x)` instead of `t <= x`: catches NaN in the same clamp
    // (the negation over a partial order is the point — see RT6-1).
    #[allow(clippy::neg_cmp_op_on_partial_ord)]
    if !(t > times[0]) || times.len() == 1 {
        return (0, 0, 0.0);
    }
    let last = times.len() - 1;
    if t >= times[last] {
        return (last, last, 0.0);
    }
    // partition_point: first index with times[i] > t; the pair is
    // (i-1, i).
    let i = times.partition_point(|&x| x <= t);
    let (i0, i1) = (i - 1, i);
    let span = times[i1] - times[i0];
    // Duplicate keyframe times are spec-legal (STEP-style hard cuts):
    // treat a zero span as "hold the left key".
    let k = if span > 0.0 {
        (t - times[i0]) / span
    } else {
        0.0
    };
    (i0, i1, k)
}

fn lerp3(a: [f32; 3], b: [f32; 3], k: f32) -> Vec3 {
    Vec3::new(
        a[0] + (b[0] - a[0]) * k,
        a[1] + (b[1] - a[1]) * k,
        a[2] + (b[2] - a[2]) * k,
    )
}

/// Normalized quaternion lerp along the SHORTEST path: negate one end
/// when the dot is negative (q and −q are the same rotation, but
/// lerping across the sign flip swings the long way around).
///
/// DETERMINISM AT THE DEGENERATE POINTS (RT6 risk, resolved):
/// - Rotations exactly 180° apart (quaternion dot 0 after the sign
///   correction) have two equal-length arcs; the TIE-BREAK is the
///   straight-line chord through the hemisphere of the LEFT key —
///   forced by the `dot < 0.0` negation rule (−0.0 does NOT negate),
///   so the same key pair always bends the same way. The blend norm
///   there is ≥ √0.5 for unit keys: never degenerate.
/// - A zero-norm BLEND is therefore only reachable with degenerate
///   INPUT keys (zero/denormal quaternions from a hostile file): the
///   deterministic resolution is the normalized left key, else the
///   identity rotation — always a unit quaternion out, never NaN.
fn nlerp_quat(a: [f32; 4], mut b: [f32; 4], k: f32) -> [f32; 4] {
    let dot = a[0] * b[0] + a[1] * b[1] + a[2] * b[2] + a[3] * b[3];
    if dot < 0.0 {
        for c in &mut b {
            *c = -*c;
        }
    }
    let mut out = [0.0f32; 4];
    for i in 0..4 {
        out[i] = a[i] + (b[i] - a[i]) * k;
    }
    normalize_or(out, a)
}

/// Normalize `q`; a degenerate norm falls back to normalized `left`,
/// then to identity. Non-finite norms (NaN-poisoned keys) take the
/// same fallback chain.
fn normalize_or(q: [f32; 4], left: [f32; 4]) -> [f32; 4] {
    let n = (q[0] * q[0] + q[1] * q[1] + q[2] * q[2] + q[3] * q[3]).sqrt();
    if n.is_finite() && n > 1e-6 {
        return [q[0] / n, q[1] / n, q[2] / n, q[3] / n];
    }
    let ln = (left[0] * left[0] + left[1] * left[1] + left[2] * left[2] + left[3] * left[3]).sqrt();
    if ln.is_finite() && ln > 1e-6 {
        return [left[0] / ln, left[1] / ln, left[2] / ln, left[3] / ln];
    }
    [0.0, 0.0, 0.0, 1.0]
}

/// Build runtime animations from a parsed document + BIN chunk.
/// Returns `(animations, warnings)`; unsupported interpolation is a
/// NAMED error (CUBICSPLINE output triplets would play garbage as
/// values), unsupported channel paths (`weights`) skip with a label.
pub(crate) fn build_animations(
    doc: &crate::three::doc::Doc,
    bin: Option<&[u8]>,
) -> crate::base::Result<(Vec<Animation>, Vec<String>)> {
    use crate::base::Error;
    use crate::three::extract::{read_scalar_f32, read_vec3_f32, read_vec4_f32};

    let mut out = Vec::with_capacity(doc.animations.len());
    let mut warnings = Vec::new();
    for (ai, def) in doc.animations.iter().enumerate() {
        let mut tracks = Vec::new();
        for ch in &def.channels {
            let sampler = &def.samplers[ch.sampler]; // validated at parse
            let interpolation = match sampler.interpolation.as_str() {
                "LINEAR" => Interpolation::Linear,
                "STEP" => Interpolation::Step,
                "CUBICSPLINE" => {
                    // Labeled skip, not rejection: the output accessor
                    // carries in-tangent/value/out-tangent TRIPLETS —
                    // sampling them as plain values would play garbage,
                    // and rejecting the whole file would kill every
                    // OTHER channel that plays fine. The channel is
                    // dropped loudly; the rest of the animation runs.
                    warnings.push(format!(
                        "#FALLBACK animation {ai}: CUBICSPLINE channel on node {} skipped \
                         (tangent triplets unsupported)",
                        ch.target_node
                    ));
                    continue;
                }
                other => {
                    return Err(Error::Parse(format!(
                        "gltf: animation {ai}: unknown interpolation {other:?}"
                    )))
                }
            };
            let what = format!("animation {ai} times");
            let times = read_scalar_f32(doc, sampler.input, bin, &what)?;
            if times.windows(2).any(|w| w[1] < w[0]) {
                return Err(Error::Parse(format!(
                    "gltf: animation {ai}: keyframe times decrease"
                )));
            }
            if times.iter().any(|t| !t.is_finite()) {
                return Err(Error::Parse(format!(
                    "gltf: animation {ai}: non-finite keyframe time"
                )));
            }
            let what = format!("animation {ai} values");
            let values = match ch.target_path.as_str() {
                "translation" => {
                    TrackValues::Translation(read_vec3_f32(doc, sampler.output, bin, &what)?)
                }
                "scale" => TrackValues::Scale(read_vec3_f32(doc, sampler.output, bin, &what)?),
                "rotation" => {
                    TrackValues::Rotation(read_vec4_f32(doc, sampler.output, bin, &what)?)
                }
                "weights" => {
                    warnings.push(format!(
                        "#FALLBACK animation {ai}: morph-target weights channel skipped (no morph pipeline)"
                    ));
                    continue;
                }
                other => {
                    return Err(Error::Parse(format!(
                        "gltf: animation {ai}: unknown channel path {other:?}"
                    )))
                }
            };
            let n_values = match &values {
                TrackValues::Translation(v) | TrackValues::Scale(v) => v.len(),
                TrackValues::Rotation(v) => v.len(),
            };
            if n_values != times.len() || times.is_empty() {
                return Err(Error::Parse(format!(
                    "gltf: animation {ai}: {} keyframe times vs {n_values} values",
                    times.len()
                )));
            }
            tracks.push(Track {
                node: ch.target_node,
                times,
                values,
                interpolation,
            });
        }
        out.push(Animation::new(def.name.clone(), tracks));
    }
    Ok((out, warnings))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track_t(times: Vec<f32>, values: Vec<[f32; 3]>, interp: Interpolation) -> Track {
        Track {
            node: 0,
            times,
            values: TrackValues::Translation(values),
            interpolation: interp,
        }
    }

    fn rest() -> NodePose {
        NodePose {
            translation: Vec3::ZERO,
            rotation: [0.0, 0.0, 0.0, 1.0],
            scale: Vec3::new(1.0, 1.0, 1.0),
        }
    }

    #[test]
    fn linear_interpolates_and_clamps() {
        let anim = Animation::new(
            None,
            vec![track_t(
                vec![1.0, 3.0],
                vec![[0.0, 0.0, 0.0], [10.0, -4.0, 2.0]],
                Interpolation::Linear,
            )],
        );
        assert_eq!(anim.duration(), 3.0);
        let mut poses = [rest()];
        anim.sample(2.0, &mut poses); // midpoint
        assert_eq!(poses[0].translation, Vec3::new(5.0, -2.0, 1.0));
        anim.sample(0.0, &mut poses); // before range: clamp to first
        assert_eq!(poses[0].translation, Vec3::ZERO);
        anim.sample(99.0, &mut poses); // after range: clamp to last
        assert_eq!(poses[0].translation, Vec3::new(10.0, -4.0, 2.0));
    }

    #[test]
    fn step_holds_until_the_next_key() {
        let anim = Animation::new(
            None,
            vec![track_t(
                vec![0.0, 1.0],
                vec![[0.0, 0.0, 0.0], [8.0, 0.0, 0.0]],
                Interpolation::Step,
            )],
        );
        let mut poses = [rest()];
        anim.sample(0.999, &mut poses);
        assert_eq!(poses[0].translation, Vec3::ZERO, "STEP holds the left key");
        anim.sample(1.0, &mut poses);
        assert_eq!(poses[0].translation, Vec3::new(8.0, 0.0, 0.0));
    }

    #[test]
    fn rotation_nlerp_shortest_path() {
        // Identity -> 90° about Z, sampled halfway = 45° about Z.
        let s = std::f32::consts::FRAC_1_SQRT_2;
        let track = Track {
            node: 0,
            times: vec![0.0, 1.0],
            values: TrackValues::Rotation(vec![[0.0, 0.0, 0.0, 1.0], [0.0, 0.0, s, s]]),
            interpolation: Interpolation::Linear,
        };
        let anim = Animation::new(None, vec![track]);
        let mut poses = [rest()];
        anim.sample(0.5, &mut poses);
        let q = poses[0].rotation;
        let expected = (std::f32::consts::FRAC_PI_8).sin(); // sin(22.5°)
        assert!((q[2] - expected).abs() < 1e-3, "{q:?}");
        // Sign-flipped end key must interpolate the SAME way (q == -q).
        let track = Track {
            node: 0,
            times: vec![0.0, 1.0],
            values: TrackValues::Rotation(vec![[0.0, 0.0, 0.0, 1.0], [0.0, 0.0, -s, -s]]),
            interpolation: Interpolation::Linear,
        };
        let anim2 = Animation::new(None, vec![track]);
        let mut poses2 = [rest()];
        anim2.sample(0.5, &mut poses2);
        let q2 = poses2[0].rotation;
        assert!(
            (q2[2].abs() - expected).abs() < 1e-3,
            "shortest path: {q2:?}"
        );
    }

    #[test]
    fn degenerate_rotation_keys_resolve_deterministically() {
        // Zero-norm keys (hostile file): unit identity out, no NaN.
        let track = Track {
            node: 0,
            times: vec![0.0, 1.0],
            values: TrackValues::Rotation(vec![[0.0; 4], [0.0; 4]]),
            interpolation: Interpolation::Linear,
        };
        let anim = Animation::new(None, vec![track]);
        let mut poses = [rest()];
        anim.sample(0.5, &mut poses);
        assert_eq!(poses[0].rotation, [0.0, 0.0, 0.0, 1.0]);

        // Exactly-180°-apart ROTATIONS (quat dot 0): the midpoint is a
        // 90° rotation on the left key's side — deterministic, unit,
        // and identical across repeated samples.
        let track = Track {
            node: 0,
            times: vec![0.0, 1.0],
            // identity -> 180° about Z: q = (0,0,1,0); dot with
            // identity = 0 — the equal-arcs tie.
            values: TrackValues::Rotation(vec![[0.0, 0.0, 0.0, 1.0], [0.0, 0.0, 1.0, 0.0]]),
            interpolation: Interpolation::Linear,
        };
        let anim = Animation::new(None, vec![track]);
        let mut a = [rest()];
        anim.sample(0.5, &mut a);
        let q = a[0].rotation;
        let norm = (q.iter().map(|c| c * c).sum::<f32>()).sqrt();
        assert!((norm - 1.0).abs() < 1e-5, "unit at the tie: {q:?}");
        let s = std::f32::consts::FRAC_1_SQRT_2;
        // Chord midpoint normalized = 90° about +Z: (0, 0, s, s).
        assert!((q[2] - s).abs() < 1e-4 && (q[3] - s).abs() < 1e-4, "{q:?}");
        let mut b = [rest()];
        anim.sample(0.5, &mut b);
        assert_eq!(a[0].rotation, b[0].rotation, "tie-break is stable");

        // NaN-poisoned key: falls back down the chain, never NaN out.
        let track = Track {
            node: 0,
            times: vec![0.0, 1.0],
            values: TrackValues::Rotation(vec![[f32::NAN; 4], [0.0, 0.0, 1.0, 0.0]]),
            interpolation: Interpolation::Linear,
        };
        let anim = Animation::new(None, vec![track]);
        let mut poses = [rest()];
        anim.sample(0.5, &mut poses);
        assert!(poses[0].rotation.iter().all(|c| c.is_finite()));
    }

    #[test]
    fn nan_time_clamps_to_first_key() {
        // RT6-1 twin at the unit level (the acceptance test lives in
        // tests/adv_anim_gltf.rs): NaN t = clamp to first, no panic.
        let anim = Animation::new(
            None,
            vec![track_t(
                vec![1.0, 2.0],
                vec![[3.0, 0.0, 0.0], [7.0, 0.0, 0.0]],
                Interpolation::Linear,
            )],
        );
        let mut poses = [rest()];
        anim.sample(f32::NAN, &mut poses);
        assert_eq!(poses[0].translation.x, 3.0);
    }

    #[test]
    fn duplicate_times_hold_left() {
        let anim = Animation::new(
            None,
            vec![track_t(
                vec![0.0, 1.0, 1.0, 2.0],
                vec![[0.0; 3], [1.0, 0.0, 0.0], [5.0, 0.0, 0.0], [6.0, 0.0, 0.0]],
                Interpolation::Linear,
            )],
        );
        let mut poses = [rest()];
        // At exactly t=1.0 partition lands after both duplicates: the
        // hard cut has happened.
        anim.sample(1.0, &mut poses);
        assert_eq!(poses[0].translation.x, 5.0);
        anim.sample(1.5, &mut poses);
        assert_eq!(poses[0].translation.x, 5.5);
    }

    #[test]
    fn untracked_nodes_keep_rest_pose() {
        let anim = Animation::new(
            None,
            vec![track_t(
                vec![0.0],
                vec![[9.0, 9.0, 9.0]],
                Interpolation::Linear,
            )],
        );
        let mut poses = [rest(), rest()];
        anim.sample(0.5, &mut poses);
        assert_eq!(poses[0].translation.x, 9.0);
        assert_eq!(poses[1].translation, Vec3::ZERO, "node 1 untouched");
    }
}
