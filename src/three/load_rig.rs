//! The rig plane of a loaded model: node graph + skins ([`Rig`]),
//! reusable pose-sampling scratch ([`Pose`]), the `Model` sampling
//! methods, and the skinned-vertex hostility gate. `#[path]` sibling
//! of load.rs (file-size split) — the public types re-export from
//! `three::load` unchanged.
//!
//! OWNER: GFX3D.

use crate::base::{Error, Result};
use crate::three::animation::{Animation, NodePose};
use crate::three::extract::MeshData;
use crate::three::math::Mat4;

use super::{Model, MAX_NODE_DEPTH};

/// The node graph kept for animation: rest poses + hierarchy.
#[derive(Debug, Clone)]
pub struct RigNode {
    pub rest: NodePose,
    /// Matrix-form nodes (spec: never animated) use this verbatim.
    pub matrix: Option<Mat4>,
    pub children: Vec<usize>,
}

/// One skin: joint node indices + inverse bind matrices (identity
/// when the file omits them, per spec).
#[derive(Debug, Clone)]
pub struct SkinData {
    pub joints: Vec<usize>,
    pub inverse_bind: Vec<Mat4>,
}

#[derive(Debug, Clone, Default)]
pub struct Rig {
    pub nodes: Vec<RigNode>,
    pub roots: Vec<usize>,
    pub animations: Vec<Animation>,
    pub skins: Vec<SkinData>,
    /// Per-INSTANCE skin binding, parallel to `Model::instances`
    /// (kept here rather than as a `MeshInstance` field so adding
    /// skinning does not change the shape every `MeshInstance`
    /// constructor in the crate depends on). A skinned instance's
    /// vertices ignore its `world` when a pose provides joint
    /// matrices — glTF: the skin overrides the node transform.
    pub instance_skins: Vec<Option<usize>>,
}

/// A sampled animation pose: everything the renderer needs for one
/// frame. Produced by [`Model::sample_pose_full`]; plain data, pure in
/// `t`. Holds its own sampling scratch so a long-lived `Pose` makes
/// the per-frame re-evaluation allocation-free (grow-once, like
/// `SceneRenderer`).
#[derive(Debug, Clone, Default)]
pub struct Pose {
    /// Per-INSTANCE world transforms (rigid path).
    pub instance_worlds: Vec<Mat4>,
    /// Per-skin joint matrices: `world(joint) * inverse_bind` — a
    /// skinned vertex's world position is the weight-blend of these
    /// applied to the bind-space position (the mesh node's own
    /// transform is ignored, per spec).
    pub skin_joints: Vec<Vec<Mat4>>,
    // Sampling scratch (reused across frames; not part of the output).
    poses: Vec<NodePose>,
    worlds: Vec<Mat4>,
    stack: Vec<(usize, Mat4, usize)>,
}

impl Model {
    /// Animations on this model ([] for static models).
    pub fn animations(&self) -> &[Animation] {
        self.rig
            .as_ref()
            .map(|r| r.animations.as_slice())
            .unwrap_or(&[])
    }

    /// Per-NODE world matrices for animation `anim` at time `t`
    /// (clamped to the keyframe range), written into `pose`'s scratch.
    /// `false` when the model has no rig or the index is out of range.
    /// Pure in `t`; allocation-free once the scratch has grown.
    fn node_worlds_into(&self, anim: usize, t: f32, pose: &mut Pose) -> bool {
        let Some(rig) = self.rig.as_ref() else {
            return false;
        };
        let Some(animation) = rig.animations.get(anim) else {
            return false;
        };

        // Rest poses -> animated poses -> world walk.
        pose.poses.clear();
        pose.poses.extend(rig.nodes.iter().map(|n| n.rest));
        animation.sample(t, &mut pose.poses);
        pose.worlds.clear();
        pose.worlds.resize(rig.nodes.len(), Mat4::IDENTITY);
        // Iterative DFS mirroring the load walk (validated acyclic at
        // load; the depth guard here is belt only).
        pose.stack.clear();
        pose.stack
            .extend(rig.roots.iter().rev().map(|&r| (r, Mat4::IDENTITY, 0)));
        while let Some((ni, parent, depth)) = pose.stack.pop() {
            if depth > MAX_NODE_DEPTH {
                return false;
            }
            let node = &rig.nodes[ni];
            // Spec: matrix-form nodes are never animation targets.
            let local = node.matrix.unwrap_or_else(|| pose.poses[ni].matrix());
            let world = parent.mul(&local);
            pose.worlds[ni] = world;
            for &c in node.children.iter().rev() {
                pose.stack.push((c, world, depth + 1));
            }
        }
        true
    }

    /// Per-INSTANCE world transforms for animation `anim` at time `t`
    /// seconds (clamped to the keyframe range — loop with
    /// `t % animations()[i].duration()`). Returns false (out untouched)
    /// when the model has no rig or the index is out of range. Rigid
    /// only — skinned instances need [`Model::sample_pose_full`];
    /// convenience shape for tests and one-shot callers.
    pub fn sample_pose(&self, anim: usize, t: f32, out: &mut Vec<Mat4>) -> bool {
        let mut pose = Pose::default();
        if !self.sample_pose_full(anim, t, &mut pose) {
            return false;
        }
        out.clear();
        out.append(&mut pose.instance_worlds);
        true
    }

    /// Full pose sample: instance worlds + per-skin joint matrices
    /// (`world(joint) * inverse_bind`). Reuses `pose`'s allocations
    /// across frames (the per-frame playback path — zero steady-state
    /// allocation). Returns false (pose untouched) when the model has
    /// no rig or the index is out of range.
    pub fn sample_pose_full(&self, anim: usize, t: f32, pose: &mut Pose) -> bool {
        if !self.node_worlds_into(anim, t, pose) {
            return false;
        }
        let rig = self.rig.as_ref().expect("node_worlds_into implies rig");
        // Split the borrows: worlds is read-only input from here on.
        let Pose {
            instance_worlds,
            skin_joints,
            worlds,
            ..
        } = pose;

        instance_worlds.clear();
        for inst in &self.instances {
            instance_worlds.push(match inst.source_node {
                Some(n) => worlds[n],
                None => inst.world,
            });
        }

        // Reuse inner joint-matrix vectors: clear + refill each
        // (dropping them would realloc every frame).
        skin_joints.resize(rig.skins.len(), Vec::new());
        skin_joints.truncate(rig.skins.len());
        for (skin, out) in rig.skins.iter().zip(skin_joints.iter_mut()) {
            out.clear();
            out.extend(
                skin.joints
                    .iter()
                    .zip(&skin.inverse_bind)
                    .map(|(&j, ibm)| worlds[j].mul(ibm)),
            );
        }
        true
    }

    /// The skin bound to instance `i`, if any (rig-side parallel
    /// array; see `Rig::instance_skins`).
    pub fn instance_skin(&self, i: usize) -> Option<usize> {
        self.rig
            .as_ref()
            .and_then(|r| r.instance_skins.get(i).copied().flatten())
    }
}

/// Skinned-vertex hostility gate (REDTEAM surface): joint indices must
/// address the skin's joint list; weights must be finite, non-negative,
/// and sum to ~1. Sub-1% drift renormalizes with ONE label per
/// primitive (real exporters quantize weights); zero/negative/NaN sums
/// reject by name. Weights of unused joint slots are welcome to be 0.
pub(super) fn sanitize_skin_vertices(
    data: &mut MeshData,
    joint_count: usize,
    warnings: &mut Vec<String>,
) -> Result<()> {
    let (Some(joints), Some(weights)) = (&data.joints, &mut data.weights) else {
        return Ok(()); // unskinned primitive on a skinned node: rigid
    };
    let mut renormalized = false;
    for (vi, (j, w)) in joints.iter().zip(weights.iter_mut()).enumerate() {
        for (slot, &ji) in j.iter().enumerate() {
            // A joint index only matters where its weight is nonzero:
            // exporters routinely pad unused slots with 0/garbage.
            if w[slot] != 0.0 && ji as usize >= joint_count {
                return Err(Error::Parse(format!(
                    "gltf: vertex {vi} joint index {ji} out of range ({joint_count} joints)"
                )));
            }
        }
        if w.iter().any(|c| !c.is_finite() || *c < 0.0) {
            return Err(Error::Parse(format!(
                "gltf: vertex {vi} has non-finite or negative skin weights"
            )));
        }
        let sum: f32 = w.iter().sum();
        if sum <= 1e-6 {
            return Err(Error::Parse(format!(
                "gltf: vertex {vi} skin weights sum to zero"
            )));
        }
        if (sum - 1.0).abs() > 0.01 {
            for c in w.iter_mut() {
                *c /= sum;
            }
            renormalized = true;
        }
    }
    if renormalized {
        warnings.push("#FALLBACK skin weights renormalized (sums drifted past 1%)".to_string());
    }
    Ok(())
}
