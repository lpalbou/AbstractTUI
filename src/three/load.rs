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
//! panic. The campaign test lives in load_tests.rs.

use crate::base::{Error, Result};
use crate::gfx::bitmap::Bitmap;
use crate::three::animation::NodePose;
use crate::three::doc::Doc;
use crate::three::extract::{extract_primitive, MeshData};
use crate::three::glb;
use crate::three::math::{Mat4, Vec3};
use crate::three::scene::Camera;

// The rig plane (node graph, skins, pose sampling, skinned-vertex
// sanitation) and the texture-decode severity split live in `#[path]`
// siblings — the file-size discipline; public types re-export below.
#[path = "load_rig.rs"]
mod rig;
use rig::sanitize_skin_vertices;
pub use rig::{Pose, Rig, RigNode, SkinData};

#[path = "load_texture.rs"]
mod texture_decode;
use texture_decode::{decode_texture, TextureOutcome};

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

#[cfg(test)]
#[path = "load_tests.rs"]
mod tests;
