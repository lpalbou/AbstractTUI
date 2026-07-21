//! Model loading facade: GLB bytes -> flattened, validated, render-ready
//! [`Model`]. Composes `glb::split` + `doc::Doc::parse` +
//! `extract::extract_primitive`, flattens the node hierarchy into world
//! transforms, and decodes GLB-embedded PNG textures (JPEG and external
//! URIs degrade with labeled warnings — the standalone engine ships no
//! JPEG decoder and never touches the filesystem here).
//!
//! Hostility contract: this is the single entry point REDTEAM's GLB
//! mutator battery drives (`testing::glb_mutate`). Every MustReject
//! mutant must produce a named `Error::Parse`; byte soup must never
//! panic. The campaign test lives at the bottom of this file.

use crate::base::{Error, Result};
use crate::gfx::bitmap::Bitmap;
use crate::three::animation::{Animation, NodePose};
use crate::three::doc::Doc;
use crate::three::extract::{extract_primitive, MeshData};
use crate::three::glb;
use crate::three::math::{Mat4, Vec3};
use crate::three::scene::Camera;

/// One flattened drawable: extracted mesh data + its world transform.
#[derive(Debug, Clone)]
pub struct MeshInstance {
    pub data: MeshData,
    pub world: Mat4,
    /// Source node in the rig (animated repose looks worlds up here);
    /// `None` for node-less placements (identity fallback).
    pub source_node: Option<usize>,
}

/// Render-ready material (v1: base color + optional decoded texture;
/// the rasterizer uses base color and vertex colors this cycle, the
/// texture is stored for the texturing pass).
#[derive(Debug, Clone)]
pub struct MaterialData {
    pub base_color: [f32; 4],
    pub texture: Option<Bitmap>,
    /// Minification mip chain BELOW `texture` (level 1 = half, ... to
    /// 1x1; ~1/3 extra memory), built once at load. The renderer picks
    /// a level per triangle by texel-per-pixel ratio — kills the
    /// shimmer on minified textures in motion (cycle 7). Empty when
    /// untextured.
    pub mips: Vec<Bitmap>,
    /// emissiveFactor — ADDS to the lit color (self-illumination).
    pub emissive: [f32; 3],
}

impl Default for MaterialData {
    fn default() -> Self {
        MaterialData {
            base_color: [1.0; 4],
            texture: None,
            mips: Vec::new(),
            emissive: [0.0; 3],
        }
    }
}

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

/// Load-time cost report (texture decode dominates on textured
/// models — a UI can show "loading" around `load_with_stats`).
#[derive(Debug, Clone, Default)]
pub struct LoadStats {
    pub total: std::time::Duration,
    pub texture_decode: std::time::Duration,
    /// Mip-chain build time (one-time, at load; part of the visible
    /// loading cost on textured models).
    pub mip_build: std::time::Duration,
    pub textures_decoded: usize,
    pub triangles: usize,
}

#[derive(Debug, Default)]
pub struct Model {
    pub instances: Vec<MeshInstance>,
    pub materials: Vec<MaterialData>,
    /// Node graph + animations; `None` for static models.
    pub rig: Option<Rig>,
    /// `#FALLBACK`-labeled degradations (undecodable textures etc.).
    pub warnings: Vec<String>,
}

/// Depth guard for the node walk: glTF hierarchies are trees; anything
/// deeper than this is either absurd or cyclic.
const MAX_NODE_DEPTH: usize = 256;

/// Triangle budget (labeled cap): 2M tris ≈ 8x the largest real asset
/// (x-wing 120k) and ~100 MB of extracted data — anything past it is
/// pathological for a terminal renderer and rejects BY NAME instead of
/// exhausting memory.
pub const MAX_TRIANGLES: usize = 2_000_000;

impl Model {
    /// Load a GLB byte buffer into a render-ready model.
    ///
    /// ```
    /// use abstracttui::three::{Model, Pose};
    /// // glb_bytes: any GLB source — disk, network, embed.
    /// # let glb_bytes = include_bytes!(concat!(
    /// #     env!("CARGO_MANIFEST_DIR"), "/src/three/fixtures/animated_bar.glb"));
    /// let model = Model::load(glb_bytes).unwrap();
    /// assert!(model.triangle_count() > 0);
    ///
    /// // Animations (if any) sample to per-instance transforms; time
    /// // clamps to the clip, so looping is `t % duration()`:
    /// let mut pose = Pose::default();
    /// for (i, anim) in model.animations().iter().enumerate() {
    ///     let t = 0.25_f32 % anim.duration().max(f32::EPSILON);
    ///     assert!(model.sample_pose_full(i, t, &mut pose));
    /// }
    /// ```
    pub fn load(bytes: &[u8]) -> Result<Model> {
        Model::load_with_stats(bytes).map(|(m, _)| m)
    }

    /// Load + cost report: texture decode is the expensive part on
    /// textured models (~100 ms on the helmet's 2048² JPEG) — show a
    /// loading state around this call.
    pub fn load_with_stats(bytes: &[u8]) -> Result<(Model, LoadStats)> {
        let t_start = std::time::Instant::now();
        let mut stats = LoadStats::default();
        let chunks = glb::split(bytes)?;
        let doc = Doc::parse(chunks.json)?;
        let bin = chunks.bin;

        let mut warnings = Vec::new();

        // Materials (with GLB-embedded PNG texture decode). Severity
        // ruling (cycle-3, self-flagged in cycle 2): a MALFORMED
        // container (image view past the real BIN, corrupt PNG bytes)
        // REJECTS like any other corruption; an UNIMPLEMENTED feature
        // (JPEG, external uri) degrades with a labeled warning — the
        // file is fine, the engine is honest about its limits.
        let mut materials = Vec::with_capacity(doc.materials.len());
        for (mi, m) in doc.materials.iter().enumerate() {
            let mut out = MaterialData {
                base_color: m.base_color,
                texture: None,
                mips: Vec::new(),
                emissive: m.emissive,
            };
            if m.has_normal_texture {
                // Cycle-6 severity ruling: unsupported map = labeled
                // degradation (well-formed file, unimplemented feature).
                warnings.push(format!(
                    "#FALLBACK material {mi}: normal map ignored (no tangent pipeline)"
                ));
            }
            if let Some(tex_idx) = m.base_color_texture {
                let t0 = std::time::Instant::now();
                match decode_texture(&doc, tex_idx, bin)? {
                    TextureOutcome::Decoded(bmp) => {
                        stats.texture_decode += t0.elapsed();
                        stats.textures_decoded += 1;
                        let t1 = std::time::Instant::now();
                        out.mips = bmp.mip_chain();
                        stats.mip_build += t1.elapsed();
                        out.texture = Some(bmp);
                    }
                    TextureOutcome::Skipped(w) => {
                        warnings.push(format!("#FALLBACK material {mi}: {w}"))
                    }
                }
            }
            materials.push(out);
        }

        // Flatten the node hierarchy. Spec: nodes form a tree (a node
        // may be the child of at most one node); revisiting a node
        // means a cycle or a shared child — both malformed, both named.
        let mut placements: Vec<(usize, Mat4, Option<usize>, Option<usize>)> = Vec::new();
        if !doc.scene_roots.is_empty() {
            let mut visited = vec![false; doc.nodes.len()];
            // Explicit stack: (node index, parent world transform, depth).
            let mut stack: Vec<(usize, Mat4, usize)> = doc
                .scene_roots
                .iter()
                .rev()
                .map(|&r| (r, Mat4::IDENTITY, 0))
                .collect();
            while let Some((ni, parent, depth)) = stack.pop() {
                if depth > MAX_NODE_DEPTH {
                    return Err(Error::Parse(format!(
                        "gltf: node hierarchy deeper than {MAX_NODE_DEPTH} (cycle?)"
                    )));
                }
                let node = doc
                    .nodes
                    .get(ni)
                    .ok_or_else(|| Error::Parse(format!("gltf: node index {ni} out of range")))?;
                if visited[ni] {
                    return Err(Error::Parse(format!(
                        "gltf: node {ni} reachable twice (cycle or shared child)"
                    )));
                }
                visited[ni] = true;
                let local = match node.matrix {
                    Some(m) => Mat4::from_cols_array(m),
                    None => Mat4::from_trs(
                        Vec3::new(
                            node.translation[0],
                            node.translation[1],
                            node.translation[2],
                        ),
                        (
                            node.rotation[0],
                            node.rotation[1],
                            node.rotation[2],
                            node.rotation[3],
                        ),
                        Vec3::new(node.scale[0], node.scale[1], node.scale[2]),
                    ),
                };
                let world = parent.mul(&local);
                if let Some(mesh) = node.mesh {
                    placements.push((mesh, world, Some(ni), node.skin));
                }
                for &child in node.children.iter().rev() {
                    stack.push((child, world, depth + 1));
                }
            }
        } else if !doc.meshes.is_empty() {
            // No scene graph: instance every mesh at identity — a real
            // degradation worth labeling, not refusing (viewers do the
            // same; meshes are complete without nodes).
            warnings.push("#FALLBACK no scene/nodes; placing all meshes at identity".to_string());
            for mi in 0..doc.meshes.len() {
                placements.push((mi, Mat4::IDENTITY, None, None));
            }
        }

        // Extract each placed mesh's primitives. Shared meshes extract
        // once per placement (v1 simplicity; assets here have 1-3
        // nodes — dedup by mesh index is a cycle-3 memory win, noted).
        let mut instances = Vec::new();
        let mut instance_skins: Vec<Option<usize>> = Vec::new();
        let mut triangles = 0usize;
        for (mesh_idx, world, source_node, node_skin) in placements {
            let mesh = doc
                .meshes
                .get(mesh_idx)
                .ok_or_else(|| Error::Parse(format!("gltf: mesh index {mesh_idx} out of range")))?;
            for prim in &mesh.primitives {
                // Triangle budget from METADATA, before extraction
                // allocates: a hostile file can declare huge accessor
                // counts against buffers it never ships — memory must
                // stay bounded on the declaration alone.
                let declared = prim
                    .indices
                    .or(prim.position)
                    .and_then(|a| doc.accessors.get(a))
                    .map(|a| a.count / 3)
                    .unwrap_or(0);
                triangles = triangles.saturating_add(declared);
                if triangles > MAX_TRIANGLES {
                    return Err(Error::Parse(format!(
                        "gltf: triangle count exceeds the {MAX_TRIANGLES} budget \
                         (pathological input for a terminal renderer)"
                    )));
                }
                let mut data = extract_primitive(&doc, prim, bin)?;
                if let Some(mat) = data.material {
                    if mat >= materials.len() {
                        return Err(Error::Parse(format!(
                            "gltf: material index {mat} out of range ({})",
                            materials.len()
                        )));
                    }
                }
                // Skinned primitive sanity (needs the SKIN context, so
                // it lives here, not in extract): joint indices bound
                // by the joint list; weights finite, non-negative,
                // sum ~1 (renormalized with a label when off — real
                // exporters quantize; zero-sum is malformed).
                let skin = match (node_skin, &data.joints) {
                    (Some(s), Some(_)) => {
                        let joint_count = doc.skins[s].joints.len(); // validated
                        sanitize_skin_vertices(&mut data, joint_count, &mut warnings)?;
                        Some(s)
                    }
                    // Joints without a skin on the node: spec says the
                    // attributes are ignored; keep data, render rigid.
                    _ => None,
                };
                instance_skins.push(skin);
                instances.push(MeshInstance {
                    data,
                    world,
                    source_node,
                });
            }
        }

        if instances.is_empty() {
            return Err(Error::Parse("gltf: no drawable triangle primitives".into()));
        }
        stats.triangles = triangles;

        // Rig + animations (kept when the model animates OR skins:
        // static unskinned models carry no graph).
        let rig = if doc.animations.is_empty() && doc.skins.is_empty() {
            None
        } else {
            let (animations, anim_warnings) = crate::three::animation::build_animations(&doc, bin)?;
            warnings.extend(anim_warnings);
            let mut skins = Vec::with_capacity(doc.skins.len());
            for (si, s) in doc.skins.iter().enumerate() {
                let inverse_bind = match s.inverse_bind_matrices {
                    // Absent = identity per spec (bind pose == node pose).
                    None => vec![Mat4::IDENTITY; s.joints.len()],
                    Some(acc) => {
                        let what = format!("skin {si} inverseBindMatrices");
                        let mats = crate::three::extract::read_mat4_f32(&doc, acc, bin, &what)?;
                        if mats.len() < s.joints.len() {
                            return Err(Error::Parse(format!(
                                "gltf: skin {si} has {} joints but {} inverse bind matrices",
                                s.joints.len(),
                                mats.len()
                            )));
                        }
                        mats.into_iter()
                            .take(s.joints.len())
                            .map(Mat4::from_cols_array)
                            .collect()
                    }
                };
                skins.push(SkinData {
                    joints: s.joints.clone(),
                    inverse_bind,
                });
            }
            let nodes = doc
                .nodes
                .iter()
                .map(|n| RigNode {
                    rest: NodePose {
                        translation: Vec3::new(
                            n.translation[0],
                            n.translation[1],
                            n.translation[2],
                        ),
                        rotation: n.rotation,
                        scale: Vec3::new(n.scale[0], n.scale[1], n.scale[2]),
                    },
                    matrix: n.matrix.map(Mat4::from_cols_array),
                    children: n.children.clone(),
                })
                .collect();
            Some(Rig {
                nodes,
                roots: doc.scene_roots.clone(),
                animations,
                skins,
                instance_skins,
            })
        };

        stats.total = t_start.elapsed();
        Ok((
            Model {
                instances,
                materials,
                rig,
                warnings,
            },
            stats,
        ))
    }

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

    /// Bounds midpoint (world space); `None` for empty models.
    pub fn center(&self) -> Option<Vec3> {
        self.bounds().map(|(min, max)| (min + max) * 0.5)
    }

    /// A camera framing this model (yaw/pitch in radians). Empty
    /// models get a default orbit at unit distance — visible no-op
    /// rather than NaN.
    pub fn fit_camera(&self, yaw: f32, pitch: f32) -> Camera {
        match self.bounds() {
            Some((min, max)) => Camera::framing(min, max, yaw, pitch),
            None => Camera::orbit(Vec3::ZERO, 1.0, yaw, pitch),
        }
    }

    /// Smooth vertex normals for every instance that lacks normals
    /// (area-weighted; see `MeshData::compute_smooth_normals`). The
    /// per-face flat fallback remains the default when this is not
    /// called.
    pub fn ensure_smooth_normals(&mut self) {
        for inst in &mut self.instances {
            inst.data.compute_smooth_normals();
        }
    }

    /// Total triangles across instances.
    pub fn triangle_count(&self) -> usize {
        self.instances.iter().map(|i| i.data.triangle_count()).sum()
    }

    /// World-space AABB over all instances, skipping non-finite
    /// positions (hostile files can smuggle NaN through valid f32
    /// bits; the rasterizer skips those triangles, bounds skip those
    /// points). `None` when nothing finite exists.
    pub fn bounds(&self) -> Option<(Vec3, Vec3)> {
        let mut min = Vec3::splat(f32::INFINITY);
        let mut max = Vec3::splat(f32::NEG_INFINITY);
        let mut any = false;
        for inst in &self.instances {
            for p in &inst.data.positions {
                let w = inst.world.transform_point(Vec3::new(p[0], p[1], p[2]));
                if !(w.x.is_finite() && w.y.is_finite() && w.z.is_finite()) {
                    continue;
                }
                any = true;
                min = Vec3::new(min.x.min(w.x), min.y.min(w.y), min.z.min(w.z));
                max = Vec3::new(max.x.max(w.x), max.y.max(w.y), max.z.max(w.z));
            }
        }
        any.then_some((min, max))
    }
}

/// Skinned-vertex hostility gate (REDTEAM surface): joint indices must
/// address the skin's joint list; weights must be finite, non-negative,
/// and sum to ~1. Sub-1% drift renormalizes with ONE label per
/// primitive (real exporters quantize weights); zero/negative/NaN sums
/// reject by name. Weights of unused joint slots are welcome to be 0.
fn sanitize_skin_vertices(
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

/// Load a GLB file into a render-ready model in one line.
pub fn load_glb(path: impl AsRef<std::path::Path>) -> Result<Model> {
    load_glb_with_stats(path).map(|(m, _)| m)
}

/// `load_glb` + the decode cost report (show "loading" around this —
/// textured models spend ~100 ms in JPEG decode).
pub fn load_glb_with_stats(path: impl AsRef<std::path::Path>) -> Result<(Model, LoadStats)> {
    let path = path.as_ref();
    let bytes = std::fs::read(path)
        .map_err(|e| Error::Parse(format!("glb: cannot read {}: {e}", path.display())))?;
    Model::load_with_stats(&bytes)
}

/// Texture decode result: decoded, or skipped for a LABELED reason
/// (unimplemented format / external source). Malformed containers are
/// hard `Err`s — see the severity ruling at the call site.
enum TextureOutcome {
    Decoded(Bitmap),
    Skipped(String),
}

fn decode_texture(doc: &Doc, tex_idx: usize, bin: Option<&[u8]>) -> Result<TextureOutcome> {
    // Index validity is guaranteed by Doc::parse (RT2-2); the gets stay
    // defensive for hand-built docs.
    let tex = doc
        .textures
        .get(tex_idx)
        .ok_or_else(|| Error::Parse(format!("gltf: texture {tex_idx} out of range")))?;
    let Some(src) = tex.source else {
        return Ok(TextureOutcome::Skipped(format!(
            "texture {tex_idx} has no source image; using baseColorFactor"
        )));
    };
    let image = doc
        .images
        .get(src)
        .ok_or_else(|| Error::Parse(format!("gltf: image {src} out of range")))?;

    if let Some(uri) = &image.uri {
        return Ok(TextureOutcome::Skipped(format!(
            "external image uri {uri:?} not fetched (standalone engine); using baseColorFactor"
        )));
    }
    // Unsupported DECLARED formats skip with a label before touching
    // bytes; for png/jpeg/undeclared the MAGIC decides via the unified
    // `gfx::decode_image` entry (containers lie, bytes don't).
    match image.mime_type.as_deref() {
        Some("image/png") | Some("image/jpeg") | None => {}
        Some(other) => {
            return Ok(TextureOutcome::Skipped(format!(
                "{other} texture not decoded (PNG/JPEG only); using baseColorFactor"
            )))
        }
    }
    let Some(bv_idx) = image.buffer_view else {
        return Ok(TextureOutcome::Skipped(format!(
            "image {src} has neither bufferView nor uri; using baseColorFactor"
        )));
    };
    let view = doc
        .buffer_views
        .get(bv_idx)
        .ok_or_else(|| Error::Parse(format!("gltf: image bufferView {bv_idx} out of range")))?;
    if view.buffer != 0 {
        return Ok(TextureOutcome::Skipped(format!(
            "image buffer {} is external; using baseColorFactor",
            view.buffer
        )));
    }
    let bin = bin.ok_or_else(|| {
        Error::Parse("gltf: image references BIN but the GLB has no BIN chunk".into())
    })?;
    // Range vs the REAL BIN: a lie here is container corruption, not a
    // missing feature — reject (upgraded from a cycle-2 warning).
    let start = view.byte_offset as u64;
    let end = start
        .checked_add(view.byte_length as u64)
        .ok_or_else(|| Error::Parse("gltf: image bufferView range overflows".into()))?;
    if end > bin.len() as u64 {
        return Err(Error::Parse(format!(
            "gltf: image bufferView runs past BIN ({} bytes)",
            bin.len()
        )));
    }
    let data = &bin[start as usize..end as usize];
    let bmp = crate::gfx::decode_image(data)
        .map_err(|e| Error::Parse(format!("gltf: embedded texture corrupt: {e}")))?;
    Ok(TextureOutcome::Decoded(bmp))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::glb_mutate::{self, Expect};

    /// REDTEAM's battery, driven through the single load entry point
    /// (RT1-8 tests-first contract). MustLoad => full pipeline Ok;
    /// MustReject => Err (the panic hook proves "named error, no
    /// panic"); NoPanic => any Result.
    #[test]
    fn glb_mutator_campaign() {
        let battery = glb_mutate::mutants(0xC0FFEE, 300);
        let mut rejected = 0usize;
        // Cycle-7 hostile pass: anything that LOADS also RENDERS —
        // load tolerance without render tolerance is half a defense
        // (degenerate geometry must draw nothing, never panic).
        let mut fb = crate::three::raster::Framebuffer::new(24, 24);
        let mut renderer = crate::three::scene::SceneRenderer::new();
        let mut render_survivor = |model: &Model, name: &str| {
            let camera = model.fit_camera(0.4, 0.3);
            let mut scene = crate::three::scene::Scene::new(model, camera);
            scene.double_sided = true;
            renderer.render(&scene, &mut fb);
            // No assertion on coverage: hostile geometry may honestly
            // paint nothing. Reaching here without panic is the test.
            crate::testing::bench::sink(fb.coverage());
            let _ = name;
        };
        for m in &battery {
            let result = Model::load(&m.bytes);
            match m.expect {
                Expect::MustLoad => {
                    let model = result.unwrap_or_else(|e| panic!("{} must load: {e}", m.name));
                    assert!(model.triangle_count() > 0, "{}: no triangles", m.name);
                    render_survivor(&model, &m.name);
                }
                Expect::MustReject => {
                    let err = match result {
                        Err(e) => e,
                        Ok(_) => panic!("{} must reject", m.name),
                    };
                    // Named rejection: the message must say something
                    // beyond a bare word (all our errors are prefixed).
                    let msg = err.to_string();
                    assert!(msg.len() > 12, "{}: unnamed rejection {msg:?}", m.name);
                    rejected += 1;
                }
                Expect::NoPanic => {
                    // Reaching here without panic is the assert; if the
                    // soup happened to load, it must render safely too.
                    if let Ok(model) = result {
                        render_survivor(&model, &m.name);
                    }
                }
            }
        }
        assert!(rejected >= 30, "battery shrank? {rejected} rejects");
    }

    /// Synthetic animated GLB (no asset in the sibling repos animates —
    /// verified by scanning every *.glb JSON chunk): a two-node
    /// hierarchy where the ROOT translates (LINEAR, 3 keys) and the
    /// mesh-bearing CHILD rotates 90° about Z (STEP, 2 keys). Exercises
    /// parse -> validate -> track build -> pose sample -> hierarchy
    /// propagation in one fixture.
    fn animated_glb() -> (String, Vec<u8>) {
        let mut bin = Vec::new();
        // positions: unit triangle @0 (36 bytes)
        for p in [[0.0f32, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]] {
            for c in p {
                bin.extend_from_slice(&c.to_le_bytes());
            }
        }
        // indices u16 @36 (6 bytes) + 2 pad -> 44
        for i in [0u16, 1, 2] {
            bin.extend_from_slice(&i.to_le_bytes());
        }
        bin.extend_from_slice(&[0, 0]);
        // times @44 (12)
        for t in [0.0f32, 1.0, 2.0] {
            bin.extend_from_slice(&t.to_le_bytes());
        }
        // translations @56 (36)
        for p in [[0.0f32, 0.0, 0.0], [2.0, 0.0, 0.0], [2.0, 4.0, 0.0]] {
            for c in p {
                bin.extend_from_slice(&c.to_le_bytes());
            }
        }
        // rotation times @92 (8)
        for t in [0.0f32, 1.0] {
            bin.extend_from_slice(&t.to_le_bytes());
        }
        // rotation quats @100 (32): identity, then 90° about Z
        let s = std::f32::consts::FRAC_1_SQRT_2;
        for q in [[0.0f32, 0.0, 0.0, 1.0], [0.0, 0.0, s, s]] {
            for c in q {
                bin.extend_from_slice(&c.to_le_bytes());
            }
        }
        assert_eq!(bin.len(), 132);
        let json = r#"{
          "asset": {"version": "2.0"},
          "buffers": [{"byteLength": 132}],
          "bufferViews": [
            {"buffer":0,"byteOffset":0,"byteLength":36},
            {"buffer":0,"byteOffset":36,"byteLength":6},
            {"buffer":0,"byteOffset":44,"byteLength":12},
            {"buffer":0,"byteOffset":56,"byteLength":36},
            {"buffer":0,"byteOffset":92,"byteLength":8},
            {"buffer":0,"byteOffset":100,"byteLength":32}
          ],
          "accessors": [
            {"bufferView":0,"componentType":5126,"count":3,"type":"VEC3","min":[0,0,0],"max":[1,1,0]},
            {"bufferView":1,"componentType":5123,"count":3,"type":"SCALAR"},
            {"bufferView":2,"componentType":5126,"count":3,"type":"SCALAR"},
            {"bufferView":3,"componentType":5126,"count":3,"type":"VEC3"},
            {"bufferView":4,"componentType":5126,"count":2,"type":"SCALAR"},
            {"bufferView":5,"componentType":5126,"count":2,"type":"VEC4"}
          ],
          "meshes": [{"primitives":[{"attributes":{"POSITION":0},"indices":1}]}],
          "nodes": [
            {"children":[1],"name":"root"},
            {"mesh":0,"translation":[0,1,0],"name":"child"}
          ],
          "scenes": [{"nodes":[0]}],
          "scene": 0,
          "animations": [{
            "name": "move",
            "samplers": [
              {"input":2,"output":3,"interpolation":"LINEAR"},
              {"input":4,"output":5,"interpolation":"STEP"}
            ],
            "channels": [
              {"sampler":0,"target":{"node":0,"path":"translation"}},
              {"sampler":1,"target":{"node":1,"path":"rotation"}}
            ]
          }]
        }"#;
        (json.to_string(), bin)
    }

    #[test]
    fn animated_glb_samples_through_the_hierarchy() {
        let (json, bin) = animated_glb();
        let model = Model::load(&glb_mutate::assemble(json.as_bytes(), Some(&bin))).unwrap();
        assert_eq!(model.animations().len(), 1);
        let anim = &model.animations()[0];
        assert_eq!(anim.name.as_deref(), Some("move"));
        assert_eq!(anim.duration(), 2.0);
        assert_eq!(anim.tracks.len(), 2);

        // Rest pose: instance world = child local translate(0,1,0).
        let inst = &model.instances[0];
        assert_eq!(inst.source_node, Some(1));
        let rest = inst.world.transform_point(Vec3::ZERO);
        assert_eq!((rest.x, rest.y, rest.z), (0.0, 1.0, 0.0));

        let mut pose = Vec::new();
        // t=0.5: root translation lerps to (1,0,0); STEP rotation still
        // identity. Origin -> (1,1,0).
        assert!(model.sample_pose(0, 0.5, &mut pose));
        assert_eq!(pose.len(), model.instances.len());
        let p = pose[0].transform_point(Vec3::ZERO);
        assert!(
            (p.x - 1.0).abs() < 1e-5 && (p.y - 1.0).abs() < 1e-5,
            "{p:?}"
        );

        // t=2.0: root at (2,4,0); child = T(0,1,0)·R90z, so child-local
        // (1,0,0) -> R90z -> (0,1,0) -> +(0,1,0) -> +(2,4,0) = (2,6,0).
        assert!(model.sample_pose(0, 2.0, &mut pose));
        let px = pose[0].transform_point(Vec3::new(1.0, 0.0, 0.0));
        assert!(
            (px.x - 2.0).abs() < 1e-4 && (px.y - 6.0).abs() < 1e-4 && px.z.abs() < 1e-4,
            "rotated+translated: {px:?}"
        );

        // Out-of-range animation index: refused, out untouched.
        assert!(!model.sample_pose(7, 0.0, &mut pose));

        // Static models refuse pose sampling.
        let cube =
            crate::three::primitives::model_of(crate::three::primitives::cube(1.0), [1.0; 4]);
        assert!(!cube.sample_pose(0, 0.0, &mut Vec::new()));
    }

    #[test]
    fn cubicspline_skips_with_label_and_weights_labeled() {
        // CUBICSPLINE: the CHANNEL drops loudly, the file still loads
        // and the remaining channels play (cycle-6 ruling: label, not
        // whole-file rejection).
        let (json, bin) = animated_glb();
        let cubic = json.replace(
            "\"interpolation\":\"LINEAR\"",
            "\"interpolation\":\"CUBICSPLINE\"",
        );
        let model = Model::load(&glb_mutate::assemble(cubic.as_bytes(), Some(&bin))).unwrap();
        assert_eq!(
            model.animations()[0].tracks.len(),
            1,
            "rotation channel survives"
        );
        assert!(
            model
                .warnings
                .iter()
                .any(|w| w.contains("#FALLBACK") && w.contains("CUBICSPLINE")),
            "{:?}",
            model.warnings
        );

        // weights channels skip with a label (path checked before the
        // output accessor shape, so the VEC4 output is never read).
        let weights = json.replace("\"path\":\"rotation\"", "\"path\":\"weights\"");
        let model = Model::load(&glb_mutate::assemble(weights.as_bytes(), Some(&bin))).unwrap();
        assert_eq!(
            model.animations()[0].tracks.len(),
            1,
            "weights track skipped"
        );
        assert!(
            model
                .warnings
                .iter()
                .any(|w| w.contains("#FALLBACK") && w.contains("weights")),
            "{:?}",
            model.warnings
        );

        // Decreasing keyframe times: named rejection.
        let (json, mut bin) = animated_glb();
        bin[44..48].copy_from_slice(&9.0f32.to_le_bytes()); // times[0] = 9 > times[1]
        let err = Model::load(&glb_mutate::assemble(json.as_bytes(), Some(&bin))).unwrap_err();
        assert!(err.to_string().contains("decrease"), "{err}");

        // Animation channel pointing at a missing node: parse-level
        // named rejection (validate.rs).
        let (json, bin) = animated_glb();
        let bad = json.replace(
            "{\"node\":0,\"path\":\"translation\"}",
            "{\"node\":9,\"path\":\"translation\"}",
        );
        let err = Model::load(&glb_mutate::assemble(bad.as_bytes(), Some(&bin))).unwrap_err();
        assert!(err.to_string().contains("node"), "{err}");
    }

    #[test]
    fn triangle_budget_rejects_on_declaration_alone() {
        // Declares 2M+ triangles via the index accessor count while
        // shipping a 4-byte BIN. The DECLARED metadata is internally
        // consistent (accessor fits its view, view fits the declared
        // buffer length) so parse-time validation passes — only the
        // real BIN is a lie, and the budget must fire BEFORE
        // extraction ever compares against it (bounded memory on
        // hostile declarations).
        let index_count = (MAX_TRIANGLES + 1) * 3;
        let index_bytes = index_count * 4;
        let json = format!(
            r#"{{
              "asset": {{"version": "2.0"}},
              "buffers": [{{"byteLength": {total}}}],
              "bufferViews": [
                {{"buffer":0,"byteOffset":0,"byteLength":36}},
                {{"buffer":0,"byteOffset":36,"byteLength":{index_bytes}}}
              ],
              "accessors": [
                {{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3"}},
                {{"bufferView":1,"componentType":5125,"count":{index_count},"type":"SCALAR"}}
              ],
              "meshes": [{{"primitives":[{{"attributes":{{"POSITION":0}},"indices":1}}]}}],
              "nodes": [{{"mesh":0}}],
              "scenes": [{{"nodes":[0]}}],
              "scene": 0
            }}"#,
            total = 36 + index_bytes,
        );
        let err =
            Model::load(&glb_mutate::assemble(json.as_bytes(), Some(&[0, 0, 0, 0]))).unwrap_err();
        assert!(err.to_string().contains("triangle count exceeds"), "{err}");
    }

    #[test]
    fn load_stats_report_decode_cost() {
        let (model, stats) = Model::load_with_stats(&glb_mutate::minimal_glb()).unwrap();
        assert_eq!(stats.triangles, model.triangle_count());
        assert!(stats.total > std::time::Duration::ZERO);
        assert_eq!(stats.textures_decoded, 0);
    }

    #[test]
    fn smooth_normal_generation_is_area_weighted_and_optional() {
        // Two coplanar triangles sharing an edge: every generated
        // normal must be the plane normal exactly (coplanar faces
        // cannot disagree).
        let mut mesh = MeshData {
            positions: vec![
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [0.0, 0.0, -1.0],
                [1.0, 0.0, -1.0],
            ],
            normals: None,
            uvs: None,
            colors: None,
            indices: vec![0, 1, 2, 2, 1, 3],
            material: None,
            ..MeshData::default()
        };
        mesh.compute_smooth_normals();
        let normals = mesh.normals.as_ref().unwrap();
        for n in normals {
            assert!(
                (n[1] - 1.0).abs() < 1e-6,
                "flat ground plane normal +Y: {n:?}"
            );
        }
        // Existing normals are never overwritten.
        let sentinel = vec![[0.0, 0.0, 1.0]; 4];
        mesh.normals = Some(sentinel.clone());
        mesh.compute_smooth_normals();
        assert_eq!(mesh.normals.as_ref().unwrap(), &sentinel);

        // Degenerate triangle (repeated index) contributes nothing and
        // does not poison neighbors.
        let mut degen = MeshData {
            positions: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
            normals: None,
            uvs: None,
            colors: None,
            indices: vec![0, 1, 2, 0, 0, 1],
            material: None,
            ..MeshData::default()
        };
        degen.compute_smooth_normals();
        let n = degen.normals.as_ref().unwrap()[0];
        assert!((n[2] - 1.0).abs() < 1e-6, "{n:?}");
    }

    #[test]
    fn emissive_and_normal_map_metadata_load() {
        // Patch the animated fixture's mesh with a material carrying
        // emissive + normalTexture: emissive lands in MaterialData, the
        // normal map degrades with a label (no tangent pipeline).
        let (mut anim_json, bin) = animated_glb();
        anim_json = anim_json.replace(
            r#""meshes": [{"primitives":[{"attributes":{"POSITION":0},"indices":1}]}],"#,
            r#""meshes": [{"primitives":[{"attributes":{"POSITION":0},"indices":1,"material":0}]}],
          "materials": [{"emissiveFactor":[0.5,0.25,0.125],"normalTexture":{"index":0}}],"#,
        );
        let model = Model::load(&glb_mutate::assemble(anim_json.as_bytes(), Some(&bin))).unwrap();
        assert_eq!(model.materials[0].emissive, [0.5, 0.25, 0.125]);
        assert!(
            model
                .warnings
                .iter()
                .any(|w| w.contains("#FALLBACK") && w.contains("normal map")),
            "{:?}",
            model.warnings
        );
    }

    #[test]
    fn load_glb_convenience_and_fit_camera() {
        // Round-trip through a temp file: the 3-line app path.
        let dir = std::env::temp_dir().join("abstracttui_load_glb_test");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("minimal.glb");
        std::fs::write(&path, glb_mutate::minimal_glb()).unwrap();
        let (model, stats) = load_glb_with_stats(&path).unwrap();
        assert!(model.triangle_count() > 0);
        assert!(stats.total > std::time::Duration::ZERO);
        let model2 = load_glb(&path).unwrap();
        assert_eq!(model2.triangle_count(), model.triangle_count());
        std::fs::remove_file(&path).ok();

        // Missing file: named error naming the path.
        let err = load_glb(dir.join("nope.glb")).unwrap_err();
        assert!(err.to_string().contains("nope.glb"), "{err}");

        // fit_camera frames the bounds; center is the AABB midpoint.
        let c = model.center().unwrap();
        assert_eq!((c.x, c.y), (0.5, 0.5));
        let cam = model.fit_camera(0.3, 0.2);
        assert!(cam.distance.is_finite() && cam.distance > 0.0);
        // Empty model: visible no-op camera, no NaN.
        let empty = Model::default();
        let cam = empty.fit_camera(0.0, 0.0);
        assert!(cam.distance == 1.0 && cam.eye().z.is_finite());
    }

    #[test]
    fn minimal_glb_loads_with_geometry() {
        let model = Model::load(&glb_mutate::minimal_glb()).unwrap();
        assert_eq!(model.triangle_count(), 1);
        assert_eq!(model.instances.len(), 1);
        let (min, max) = model.bounds().unwrap();
        assert_eq!((min.x, min.y, min.z), (0.0, 0.0, 0.0));
        assert_eq!((max.x, max.y, max.z), (1.0, 1.0, 0.0));
        assert!(model.warnings.is_empty(), "{:?}", model.warnings);
    }

    /// Real sibling-repo assets (guarded; skip silently elsewhere).
    #[test]
    fn real_assets_load_end_to_end() {
        let cases: [(&str, usize); 3] = [
            // (path, expected minimum triangle count)
            (
                "/Users/albou/tmp/abstractframework/meshvault/frontend/testmodels/helmet.glb",
                10_000,
            ),
            (
                "/Users/albou/tmp/abstractframework/meshvault/frontend/testmodels/machine.glb",
                10,
            ),
            (
                "/Users/albou/tmp/abstractframework/abstract3d/out/x-wing/scene.glb",
                50_000,
            ),
        ];
        for (path, min_tris) in cases {
            let p = std::path::Path::new(path);
            if !p.exists() {
                continue;
            }
            let bytes = std::fs::read(p).unwrap();
            let model = Model::load(&bytes).unwrap_or_else(|e| panic!("{path}: {e}"));
            assert!(
                model.triangle_count() >= min_tris,
                "{path}: {} triangles",
                model.triangle_count()
            );
            // Transforms finite, bounds sane (non-degenerate, not absurd).
            let (min, max) = model
                .bounds()
                .unwrap_or_else(|| panic!("{path}: no finite bounds"));
            for v in [min, max] {
                assert!(
                    v.x.is_finite() && v.y.is_finite() && v.z.is_finite(),
                    "{path}"
                );
            }
            let extent = max - min;
            assert!(
                extent.x > 0.0 && extent.y > 0.0,
                "{path}: flat bounds {min:?}..{max:?}"
            );
            assert!(extent.length() < 1e6, "{path}: absurd extent {extent:?}");

            // Texture expectations (cycle 5): helmet's baseColorTexture
            // is JPEG and now DECODES (the labeled fallback is gone);
            // x-wing's is PNG.
            if path.contains("helmet") || path.contains("x-wing") {
                assert!(
                    !model.warnings.iter().any(|w| w.contains("jpeg")),
                    "{path}: jpeg fallback should be gone: {:?}",
                    model.warnings
                );
                let tex = model.materials.iter().find_map(|m| m.texture.as_ref());
                let t = tex.unwrap_or_else(|| panic!("{path}: baseColorTexture should decode"));
                assert!(t.width() > 0 && t.height() > 0);
            }
        }
    }

    #[test]
    fn compressed_assets_still_reject_via_load() {
        for path in [
            "/Users/albou/tmp/abstractframework/meshvault/frontend/testmodels/helmet_draco.glb",
            "/Users/albou/tmp/abstractframework/meshvault/frontend/testmodels/helmet_meshopt.glb",
        ] {
            let p = std::path::Path::new(path);
            if !p.exists() {
                continue;
            }
            let bytes = std::fs::read(p).unwrap();
            assert!(Model::load(&bytes).is_err(), "{path} must reject");
        }
    }
}
