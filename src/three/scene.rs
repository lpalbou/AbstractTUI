//! Scene assembly + the vertex stage: orbit camera, directional light,
//! and `render(scene, framebuffer)` — model -> world -> view -> near
//! clip -> perspective -> viewport -> `raster::fill_triangle`.
//!
//! Lighting model (v1): lambert `ambient + diffuse * max(0, n·L)`,
//! evaluated per VERTEX when the mesh has normals (gouraud — cheap and
//! smooth at 160x96) and per FACE otherwise; vertex colors and the
//! material baseColorFactor modulate in linear space (glTF declares
//! both linear). All lighting happens in VIEW space: view transforms
//! are rigid so normals ride `transform_dir`; model matrices with
//! non-uniform scale would need the inverse-transpose (documented
//! cycle-3 gap — every current asset scales uniformly).
//!
//! Backface handling: glTF front faces are CCW in y-up; after the
//! y-flip to screen space they have NEGATIVE `orient2d` area, so the
//! canonicalization is: negative -> swap two vertices and fill,
//! positive -> cull (or, when `double_sided`, fill as-is).

use crate::base::Rgba;
use crate::three::load::{Model, Pose};
use crate::three::math::{Mat4, Vec3, Vec4};
use crate::three::raster::{clip_near, ClipVertex, Framebuffer, RasterVertex};
use crate::three::texture::Wrap;

// Camera + light fixtures, the per-triangle shading/winding helpers,
// and the test suite live in `#[path]` siblings (file-size split);
// `Camera`/`Light` re-export below — public paths unchanged.
#[path = "scene_camera.rs"]
mod camera;
pub use camera::{Camera, Light};

#[path = "scene_shading.rs"]
mod shading;
use shading::{blend4, emit_winding, flat_intensity, minmax3, mip_level, SkinAttrs};

pub struct Scene<'a> {
    pub model: &'a Model,
    pub camera: Camera,
    pub light: Light,
    pub background: Rgba,
    /// Rasterize back faces too (their lambert term goes ambient-dark).
    ///
    /// DEFAULTS DIFFER BY ENTRY (deliberate, documented): bare
    /// `Scene::new` starts `false` (culling ON — the cheap-and-correct
    /// choice for procedurally generated, consistently wound meshes),
    /// while the model-viewing entries — [`Viewport3D`] and
    /// [`QuickView::scene`] — set `true`, because real-world GLB
    /// exports are NOT consistently wound and holes read as bugs.
    /// Flip it explicitly when the other trade-off fits.
    ///
    /// [`Viewport3D`]: crate::widgets::Viewport3D
    /// [`QuickView::scene`]: crate::three::QuickView::scene
    pub double_sided: bool,
    /// Animated pose from [`Model::sample_pose_full`]: instance
    /// worlds + skin joint matrices. `None` = rest pose (skinned
    /// meshes draw their authored bind pose rigidly). Wrong-length
    /// data falls back per missing index rather than panicking.
    pub pose: Option<&'a Pose>,
}

impl<'a> Scene<'a> {
    pub fn new(model: &'a Model, camera: Camera) -> Scene<'a> {
        Scene {
            model,
            camera,
            light: Light::default(),
            background: Rgba::TRANSPARENT,
            double_sided: false,
            pose: None,
        }
    }
}

/// Reusable render scratch: per-vertex stage outputs live here and
/// persist across frames (grow-once — the cycle-4 perf wave's SoA
/// buffers). Hold one per long-lived viewport and call
/// [`SceneRenderer::render`]; the free [`render`] fn wraps a fresh one
/// for one-shot use.
#[derive(Default)]
pub struct SceneRenderer {
    corner_rgb: Vec<[f32; 3]>,
    /// Projected screen vertices — VALID ONLY where `in_front` is true
    /// (a vertex behind the near plane has no meaningful projection).
    screen: Vec<RasterVertex>,
    /// True: strictly in front of the near plane AND finite.
    in_front: Vec<bool>,
    /// View-space positions, kept for the near-clip slow path.
    view_pos: Vec<Vec3>,
    /// Skinning: this instance's joint matrices pre-multiplied into
    /// VIEW space (blend once per vertex, land directly in view
    /// coordinates — one matrix apply instead of two).
    skin_view: Vec<Mat4>,
}

impl SceneRenderer {
    pub fn new() -> SceneRenderer {
        SceneRenderer::default()
    }

    /// Render the scene. Perf shape (measured on the 120k-tri x-wing,
    /// cycle 4): each vertex is transformed AND projected exactly once
    /// per instance (the cycle-3 code projected per triangle corner —
    /// 3x the work on shared vertices); triangles fully in front of
    /// the near plane take a fast path with no polygon clipping unless
    /// their bbox leaves the guard band; off-screen bboxes reject
    /// before any fill setup. Per-pixel work is allocation-free;
    /// steady state reallocates nothing.
    pub fn render(&mut self, scene: &Scene, fb: &mut Framebuffer) {
        fb.clear(scene.background);
        self.overlay(scene, fb);
    }

    /// Render WITHOUT clearing: composes into whatever the framebuffer
    /// already holds, sharing its depth buffer — a ground grid drawn
    /// first and the model overlaid z-test against each other
    /// correctly.
    pub fn overlay(&mut self, scene: &Scene, fb: &mut Framebuffer) {
        if fb.width() == 0 || fb.height() == 0 {
            return;
        }
        let aspect = fb.width() as f32 / fb.height() as f32;
        let view = scene.camera.view();
        let proj = scene.camera.projection(aspect);
        // The projection is ALWAYS Mat4::perspective's shape (the
        // camera builds it): only m[0], m[5], m[10], m[14] are nonzero
        // and w_clip = -z_view. The per-vertex projection below uses
        // the sparse terms directly — 4 mul + 1 madd + 1 reciprocal
        // instead of a full mul_vec4 (16 madd) + 3-divide project()
        // (cycle-7 vertex wave; the x-wing is vertex-bound).
        let (p00, p11, p22, p23) = (proj.m[0], proj.m[5], proj.m[10], proj.m[14]);
        // Pin the sparse shape the fast projection relies on: if the
        // camera ever grows a non-perspective projection, this fires
        // in debug instead of rendering silently wrong.
        debug_assert!(
            proj.m[11] == -1.0
                && [1, 2, 3, 4, 6, 7, 8, 9, 12, 13, 15]
                    .iter()
                    .all(|&k| proj.m[k] == 0.0),
            "projection no longer matches Mat4::perspective's sparse shape"
        );
        let near = scene.camera.near;
        // Direction TOWARD the light, in view space (view is rigid, so
        // transform_dir is exact for it).
        let to_light = view.transform_dir(-scene.light.direction).normalize();
        let (wpx, hpx) = (fb.width() as f32, fb.height() as f32);
        // Guard band: coordinates are bounded near the framebuffer so
        // the rasterizer's snap clamp (RT3-1) never distorts real
        // geometry; 4 fb-sizes + margin keeps almost every triangle on
        // the no-clip fast path.
        let band = (wpx.max(hpx) * 4.0) + 64.0;

        for (idx, inst) in scene.model.instances.iter().enumerate() {
            let data = &inst.data;
            let world = scene
                .pose
                .and_then(|p| p.instance_worlds.get(idx))
                .unwrap_or(&inst.world);
            let mv = view.mul(world);

            // Skinned instance with a sampled pose: joint matrices go
            // to view space once; vertices blend them per-vertex and
            // IGNORE `mv` (glTF: the skin overrides the node
            // transform). Without a pose, skinned meshes draw their
            // authored bind pose rigidly through `mv`.
            self.skin_view.clear();
            let skin_attrs: Option<SkinAttrs<'_>> = scene.pose.and_then(|p| {
                let s = scene.model.instance_skin(idx)?;
                let mats = p.skin_joints.get(s)?;
                let joints = data.joints.as_deref()?;
                let weights = data.weights.as_deref()?;
                self.skin_view.extend(mats.iter().map(|m| view.mul(m)));
                Some((joints, weights))
            });

            let material = data.material.and_then(|m| scene.model.materials.get(m));
            let base = material.map(|m| m.base_color).unwrap_or([1.0; 4]);
            let base_rgb = [base[0], base[1], base[2]];
            // Emissive ADDS after lighting (self-illumination). For the
            // gouraud path it folds into the vertex color; the flat
            // paths add it after the face-intensity multiply (adding it
            // before would wrongly scale it by the lambert term).
            let em = material.map(|m| m.emissive).unwrap_or([0.0; 3]);
            let em_flat = if data.normals.is_some() { [0.0; 3] } else { em };
            // Textured iff the material decoded a texture AND the mesh
            // has UVs (glTF wrap default REPEAT; per-sampler modes are
            // a material-system upgrade, not v1).
            let sampler = match (&data.uvs, material.and_then(|m| m.texture.as_ref())) {
                (Some(_), Some(bmp)) => {
                    crate::three::texture::TextureSampler::new(bmp, Wrap::Repeat, Wrap::Repeat)
                }
                _ => None,
            };
            // Mip context for per-triangle LOD (cycle 7): base texel
            // count + the chain. Empty chain = always level 0 (hand-
            // built models, or the loader was told not to).
            let mip_ctx: Option<(
                &crate::gfx::bitmap::Bitmap,
                &[crate::gfx::bitmap::Bitmap],
                f32,
            )> = match (&data.uvs, material) {
                (Some(_), Some(m)) => m
                    .texture
                    .as_ref()
                    .map(|bmp| (bmp, m.mips.as_slice(), (bmp.width() * bmp.height()) as f32)),
                _ => None,
            };
            let uvs = data.uvs.as_deref();
            let gouraud = data.normals.is_some();
            let n_verts = data.positions.len();

            // ---- vertex stage: ONE transform + shade + projection per
            // vertex per instance.
            self.view_pos.clear();
            self.corner_rgb.clear();
            self.screen.clear();
            self.in_front.clear();
            self.view_pos.reserve(n_verts);
            self.corner_rgb.reserve(n_verts);
            self.screen.reserve(n_verts);
            self.in_front.reserve(n_verts);

            for i in 0..n_verts {
                let p = data.positions[i];
                // Blended skin matrix (view space) or the rigid mv.
                // The blend is a plain weighted sum of matrices — exact
                // for the position; for normals it is the standard
                // approximation (no inverse-transpose), correct under
                // rotation+translation, slightly off under non-uniform
                // scale — documented, invisible at cell resolution.
                let blended;
                let xform: &Mat4 = match skin_attrs {
                    Some((joints, weights)) => {
                        blended = blend4(&self.skin_view, &joints[i], &weights[i]);
                        &blended
                    }
                    None => &mv,
                };
                let vp = xform.transform_point(Vec3::new(p[0], p[1], p[2]));
                self.view_pos.push(vp);

                let mut c = base_rgb;
                if let Some(vc) = &data.colors {
                    c = [c[0] * vc[i][0], c[1] * vc[i][1], c[2] * vc[i][2]];
                }
                if let Some(normals) = &data.normals {
                    let n = normals[i];
                    let nv = xform.transform_dir(Vec3::new(n[0], n[1], n[2])).normalize();
                    let intensity =
                        scene.light.ambient + scene.light.diffuse * nv.dot(to_light).max(0.0);
                    c = [
                        c[0] * intensity + em[0],
                        c[1] * intensity + em[1],
                        c[2] * intensity + em[2],
                    ];
                }
                self.corner_rgb.push(c);

                let finite = vp.x.is_finite() && vp.y.is_finite() && vp.z.is_finite();
                let front = finite && vp.z <= -near;
                self.in_front.push(front);
                if front {
                    // Sparse perspective apply (see the note at proj):
                    // w = -z_view >= near > 0, so inv_w is finite.
                    let inv_w = -1.0 / vp.z;
                    let ndc_x = p00 * vp.x * inv_w;
                    let ndc_y = p11 * vp.y * inv_w;
                    let ndc_z = (p22 * vp.z + p23) * inv_w;
                    let uv = uvs.map(|u| u[i]).unwrap_or([0.0, 0.0]);
                    self.screen.push(RasterVertex {
                        x: (ndc_x + 1.0) * 0.5 * wpx,
                        y: (1.0 - ndc_y) * 0.5 * hpx, // y flip
                        ndc_z,
                        rgb: c,
                        uw: uv[0] * inv_w,
                        vw: uv[1] * inv_w,
                        inv_w,
                    });
                } else {
                    self.screen
                        .push(RasterVertex::flat(0.0, 0.0, 0.0, [0.0; 3]));
                }
            }

            // ---- triangle stage.
            let tex = sampler.as_ref();
            for tri in data.indices.chunks_exact(3) {
                let (i0, i1, i2) = (tri[0] as usize, tri[1] as usize, tri[2] as usize);
                let fronts = (self.in_front[i0], self.in_front[i1], self.in_front[i2]);

                if fronts == (true, true, true) {
                    // FAST PATH: no near clipping needed. REJECTS RUN
                    // FIRST (cycle-7 hoist): flat shading's per-face
                    // cross+sqrt is the priciest per-triangle setup on
                    // normal-less meshes (the x-wing class), so
                    // off-screen and beyond-far triangles must not pay
                    // it. Output-identical — rejects never depended on
                    // shading.
                    let a0 = self.screen[i0];
                    let b0 = self.screen[i1];
                    let c0 = self.screen[i2];
                    // Beyond-far reject (all NDC z past 1).
                    if a0.ndc_z > 1.0 && b0.ndc_z > 1.0 && c0.ndc_z > 1.0 {
                        continue;
                    }
                    // Screen bbox reject: entirely off-framebuffer.
                    let (min_x, max_x) = minmax3(a0.x, b0.x, c0.x);
                    let (min_y, max_y) = minmax3(a0.y, b0.y, c0.y);
                    if max_x < 0.0 || min_x >= wpx || max_y < 0.0 || min_y >= hpx {
                        continue;
                    }
                    // Per-triangle mip pick: texels-per-pixel ratio
                    // from screen area vs UV area (level-0 texels).
                    // The slow (near-clip) path keeps level 0 — those
                    // triangles graze the camera, where level 0 is
                    // right anyway.
                    let mip_sampler;
                    let tri_tex = match (&mip_ctx, uvs) {
                        (Some((base, mips, texels)), Some(uv)) if !mips.is_empty() => {
                            let level = mip_level(
                                (a0.x, a0.y),
                                (b0.x, b0.y),
                                (c0.x, c0.y),
                                uv[i0],
                                uv[i1],
                                uv[i2],
                                *texels,
                                mips.len(),
                            );
                            let bmp = if level == 0 { *base } else { &mips[level - 1] };
                            mip_sampler = crate::three::texture::TextureSampler::new(
                                bmp,
                                Wrap::Repeat,
                                Wrap::Repeat,
                            );
                            mip_sampler.as_ref()
                        }
                        _ => tex,
                    };
                    let (mut a, mut b, mut c) = (a0, b0, c0);
                    if !gouraud {
                        let fi = flat_intensity(&self.view_pos, i0, i1, i2, scene.light, to_light);
                        for v in [&mut a, &mut b, &mut c] {
                            v.rgb = [
                                v.rgb[0] * fi + em_flat[0],
                                v.rgb[1] * fi + em_flat[1],
                                v.rgb[2] * fi + em_flat[2],
                            ];
                        }
                    }
                    if min_x >= -band
                        && max_x <= wpx + band
                        && min_y >= -band
                        && max_y <= hpx + band
                    {
                        emit_winding(fb, a, b, c, scene.double_sided, tri_tex);
                    } else {
                        // Rare: huge on-screen-crossing triangle — bound
                        // its coordinates exactly via the guard clip.
                        let mut clipped = [a; 12];
                        let n = crate::three::raster::clip_screen_rect(
                            &[a, b, c],
                            wpx,
                            hpx,
                            band,
                            &mut clipped,
                        );
                        for k in 1..n.saturating_sub(1) {
                            emit_winding(
                                fb,
                                clipped[0],
                                clipped[k],
                                clipped[k + 1],
                                scene.double_sided,
                                tri_tex,
                            );
                        }
                    }
                    continue;
                }

                // SLOW PATH: at least one vertex behind the near plane
                // (or non-finite). All behind: skip. Mixed: view-space
                // near clip, then project the (≤4-vertex) polygon.
                if fronts == (false, false, false) {
                    continue;
                }
                let (p0, p1, p2) = (self.view_pos[i0], self.view_pos[i1], self.view_pos[i2]);
                if !(p0.x.is_finite() && p1.x.is_finite() && p2.x.is_finite()) {
                    continue;
                }
                let fi = if gouraud {
                    1.0
                } else {
                    flat_intensity(&self.view_pos, i0, i1, i2, scene.light, to_light)
                };
                let corner = |i: usize| -> [f32; 3] {
                    let c = self.corner_rgb[i];
                    // em_flat is zero on the gouraud path (fi == 1 and
                    // corner_rgb already carries emissive).
                    [
                        c[0] * fi + em_flat[0],
                        c[1] * fi + em_flat[1],
                        c[2] * fi + em_flat[2],
                    ]
                };
                let uv_of = |i: usize| uvs.map(|u| u[i]).unwrap_or([0.0, 0.0]);
                let tri_clip = [
                    ClipVertex {
                        pos: [p0.x, p0.y, p0.z],
                        rgb: corner(i0),
                        uv: uv_of(i0),
                    },
                    ClipVertex {
                        pos: [p1.x, p1.y, p1.z],
                        rgb: corner(i1),
                        uv: uv_of(i1),
                    },
                    ClipVertex {
                        pos: [p2.x, p2.y, p2.z],
                        rgb: corner(i2),
                        uv: uv_of(i2),
                    },
                ];
                let mut poly = [tri_clip[0]; 4];
                let n = clip_near(&tri_clip, near, &mut poly);
                if n < 3 {
                    continue;
                }
                let mut screen = [RasterVertex::flat(0.0, 0.0, 0.0, [0.0; 3]); 4];
                let mut all_beyond_far = true;
                for (k, cv) in poly[..n].iter().enumerate() {
                    let clip = proj.mul_vec4(Vec4::new(cv.pos[0], cv.pos[1], cv.pos[2], 1.0));
                    let inv_w = 1.0 / clip.w;
                    let ndc = clip.project();
                    all_beyond_far &= ndc.z > 1.0;
                    screen[k] = RasterVertex {
                        x: (ndc.x + 1.0) * 0.5 * wpx,
                        y: (1.0 - ndc.y) * 0.5 * hpx,
                        ndc_z: ndc.z,
                        rgb: cv.rgb,
                        uw: cv.uv[0] * inv_w,
                        vw: cv.uv[1] * inv_w,
                        inv_w,
                    };
                }
                if all_beyond_far {
                    continue;
                }
                // Near-clipped polygons can still stretch far on screen
                // (glancing geometry): always bound them exactly.
                let mut clipped = [screen[0]; 12];
                let m = crate::three::raster::clip_screen_rect(
                    &screen[..n],
                    wpx,
                    hpx,
                    band,
                    &mut clipped,
                );
                for k in 1..m.saturating_sub(1) {
                    emit_winding(
                        fb,
                        clipped[0],
                        clipped[k],
                        clipped[k + 1],
                        scene.double_sided,
                        tex,
                    );
                }
            }
        }
    }
}

/// One-shot render (fresh scratch; fine outside frame loops — hold a
/// [`SceneRenderer`] to reuse buffers across frames).
///
/// ```
/// use abstracttui::three::{self, Framebuffer, Scene};
///
/// let model = three::primitives::model_of(
///     three::primitives::cube(1.0),
///     [0.9, 0.5, 0.2, 1.0], // base color RGBA, linear
/// );
/// let camera = model.fit_camera(0.6, 0.35); // yaw, pitch (radians)
/// let mut fb = Framebuffer::new(80, 48);
/// three::render(&Scene::new(&model, camera), &mut fb);
/// assert!(fb.coverage() > 0.0);
/// ```
///
/// (Cycle-3 note: the old free-floating `srgb_to_linear` moved to
/// `three::texture::srgb8_to_linear`, where it earns its keep — texel
/// decode is the one place sRGB→linear conversion happens; factors and
/// vertex colors are declared linear by glTF and are never converted.)
pub fn render(scene: &Scene, fb: &mut Framebuffer) {
    SceneRenderer::new().render(scene, fb)
}

#[cfg(test)]
#[path = "scene_tests.rs"]
mod tests;
