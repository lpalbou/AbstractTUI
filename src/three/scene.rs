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
use crate::three::raster::{clip_near, fill_triangle, ClipVertex, Framebuffer, RasterVertex};
use crate::three::texture::Wrap;

/// Orbit camera: spherical position around a target.
#[derive(Copy, Clone, Debug)]
pub struct Camera {
    pub target: Vec3,
    /// Radians around +Y; yaw 0 looks from +Z toward the target.
    pub yaw: f32,
    /// Radians above the horizon; clamped near ±90° (up-vector guard).
    pub pitch: f32,
    pub distance: f32,
    pub fov_y: f32,
    pub near: f32,
    pub far: f32,
}

impl Camera {
    pub fn orbit(target: Vec3, distance: f32, yaw: f32, pitch: f32) -> Camera {
        // Total over any float: non-finite distances (hostile bounds
        // arithmetic upstream) clamp to a sane default instead of
        // poisoning near/far, which `Mat4::perspective` asserts on.
        let distance = if distance.is_finite() {
            distance.max(1e-3)
        } else {
            1.0
        };
        Camera {
            target,
            yaw,
            pitch,
            distance,
            fov_y: std::f32::consts::FRAC_PI_4,
            near: (distance / 100.0).max(1e-3),
            far: distance * 100.0,
        }
    }

    /// Frame an AABB: distance chosen so the bounding sphere fits the
    /// vertical fov with ~15% margin. TOTAL over any bounds: per-axis
    /// finite bounds can still OVERFLOW the radius arithmetic
    /// (`f32::MAX - (-f32::MAX)` = inf — hostile-GLB coordinates,
    /// found by the cycle-7 mutator render pass); the radius clamps to
    /// a large finite value so near/far stay orderable and
    /// `perspective`'s assertion holds. Such a scene renders nothing
    /// visible (geometry is off past the far plane) — honest, not a
    /// panic.
    pub fn framing(min: Vec3, max: Vec3, yaw: f32, pitch: f32) -> Camera {
        let target = (min + max) * 0.5;
        let raw_radius = ((max - min) * 0.5).length();
        let radius = if raw_radius.is_finite() {
            raw_radius.max(1e-3)
        } else {
            1e18
        };
        let fov_y = std::f32::consts::FRAC_PI_4;
        let distance = radius / (fov_y * 0.5).sin() * 1.15;
        Camera {
            target,
            yaw,
            pitch,
            distance,
            fov_y,
            near: (distance - radius * 2.0).max(distance / 100.0),
            far: distance + radius * 4.0,
        }
    }

    pub fn eye(&self) -> Vec3 {
        // Hard pitch clamp: at ±90° the view direction parallels the
        // +Y up vector and look_at degenerates.
        let pitch = self.pitch.clamp(-1.55, 1.55);
        let (sp, cp) = pitch.sin_cos();
        let (sy, cy) = self.yaw.sin_cos();
        self.target + Vec3::new(cp * sy, sp, cp * cy) * self.distance
    }

    pub fn view(&self) -> Mat4 {
        Mat4::look_at(self.eye(), self.target, Vec3::Y)
    }

    pub fn projection(&self, aspect: f32) -> Mat4 {
        Mat4::perspective(self.fov_y, aspect.max(1e-3), self.near, self.far)
    }
}

/// Directional key light. `direction` is the direction the light
/// TRAVELS (surfaces facing against it are lit).
#[derive(Copy, Clone, Debug)]
pub struct Light {
    pub direction: Vec3,
    pub ambient: f32,
    pub diffuse: f32,
}

impl Default for Light {
    fn default() -> Self {
        Light {
            direction: Vec3::new(-0.4, -0.8, -0.45),
            ambient: 0.25,
            diffuse: 0.75,
        }
    }
}

impl Light {
    /// Key light from spherical angles (viewer-friendly controls):
    /// `azimuth` radians around +Y (0 = light from +Z, matching yaw-0
    /// camera), `elevation` radians above the horizon. Ambient/diffuse
    /// keep the default balance; set them after if needed.
    pub fn from_angles(azimuth: f32, elevation: f32) -> Light {
        let (se, ce) = elevation.sin_cos();
        let (sa, ca) = azimuth.sin_cos();
        // The light POSITION direction is (ce·sa, se, ce·ca); the ray
        // TRAVELS the other way (Light.direction convention).
        Light {
            direction: Vec3::new(-ce * sa, -se, -ce * ca).normalize(),
            ..Light::default()
        }
    }
}

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

#[inline]
fn minmax3(a: f32, b: f32, c: f32) -> (f32, f32) {
    (a.min(b).min(c), a.max(b).max(c))
}

/// A skinned primitive's vertex attributes: (JOINTS_0, WEIGHTS_0).
type SkinAttrs<'a> = (&'a [[u16; 4]], &'a [[f32; 4]]);

/// Per-triangle mip level from the texels-per-pixel ratio: UV area
/// (in LEVEL-0 texels) over screen area, both doubled (the /2 cancels
/// in the ratio). Level k halves resolution per step, so texel density
/// shrinks 4x per level: level = floor(log2(tpp) / 2). tpp <= 1 means
/// magnification — always level 0 (bilinear handles it). Degenerate
/// screen triangles take the smallest mip (they cover ~no pixels; the
/// cheapest read is the right read).
#[allow(clippy::too_many_arguments)]
fn mip_level(
    a: (f32, f32),
    b: (f32, f32),
    c: (f32, f32),
    uv0: [f32; 2],
    uv1: [f32; 2],
    uv2: [f32; 2],
    texels: f32,
    max_level: usize,
) -> usize {
    let screen2 = ((b.0 - a.0) * (c.1 - a.1) - (b.1 - a.1) * (c.0 - a.0)).abs();
    if screen2 <= 1e-6 {
        return max_level;
    }
    let uv_area2 = ((uv1[0] - uv0[0]) * (uv2[1] - uv0[1]) - (uv1[1] - uv0[1]) * (uv2[0] - uv0[0]))
        .abs()
        * texels;
    // Negated on purpose: NaN UV area must land here too.
    #[allow(clippy::neg_cmp_op_on_partial_ord)]
    if !(uv_area2 > 0.0) {
        return 0; // zero/NaN UV area: nothing to minify
    }
    let tpp = uv_area2 / screen2;
    if tpp <= 1.0 {
        return 0;
    }
    ((tpp.log2() * 0.5) as usize).min(max_level)
}

/// Weighted blend of up to 4 joint matrices (linear blend skinning).
/// Zero-weight slots skip entirely — exporters pad unused slots with
/// arbitrary joint indices, so the index is only trusted where the
/// weight is nonzero (load sanitizes exactly that set; the `get` is
/// belt for hand-built models).
fn blend4(mats: &[Mat4], joints: &[u16; 4], weights: &[f32; 4]) -> Mat4 {
    let mut out = [0.0f32; 16];
    for k in 0..4 {
        let w = weights[k];
        if w == 0.0 {
            continue;
        }
        let Some(m) = mats.get(joints[k] as usize) else {
            continue;
        };
        for (o, s) in out.iter_mut().zip(m.m.iter()) {
            *o += s * w;
        }
    }
    Mat4::from_cols_array(out)
}

/// Face lambert term from view-space positions (flat-shading path).
#[inline]
fn flat_intensity(
    view_pos: &[Vec3],
    i0: usize,
    i1: usize,
    i2: usize,
    light: Light,
    to_light: Vec3,
) -> f32 {
    let (p0, p1, p2) = (view_pos[i0], view_pos[i1], view_pos[i2]);
    let n = (p1 - p0).cross(p2 - p0).normalize();
    light.ambient + light.diffuse * n.dot(to_light).max(0.0)
}

/// Winding canonicalization + fill: glTF front faces (CCW in y-up)
/// land NEGATIVE in y-down screen space — swap to the rasterizer's
/// positive-area convention; positive input is a back face (filled
/// only when double-sided).
#[inline]
fn emit_winding(
    fb: &mut Framebuffer,
    a: RasterVertex,
    b: RasterVertex,
    c: RasterVertex,
    double_sided: bool,
    tex: Option<&crate::three::texture::TextureSampler<'_>>,
) {
    let signed = (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x);
    if signed < 0.0 {
        fill_triangle(fb, &[a, c, b], tex);
    } else if signed > 0.0 && double_sided {
        fill_triangle(fb, &[a, b, c], tex);
    }
    // signed == 0: degenerate, skip.
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::three::extract::MeshData;
    use crate::three::load::{MaterialData, MeshInstance};

    /// Hand-built model: helper for synthetic scenes.
    fn model_of(tris: Vec<([f32; 9], [f32; 4])>) -> Model {
        // Each entry: 3 positions (xyz xyz xyz) + a base color.
        let mut model = Model::default();
        for (pos, color) in tris {
            let positions = vec![
                [pos[0], pos[1], pos[2]],
                [pos[3], pos[4], pos[5]],
                [pos[6], pos[7], pos[8]],
            ];
            let mat_idx = model.materials.len();
            model.materials.push(MaterialData {
                base_color: color,
                ..MaterialData::default()
            });
            model.instances.push(MeshInstance {
                data: MeshData {
                    positions,
                    normals: None,
                    uvs: None,
                    colors: None,
                    indices: vec![0, 1, 2],
                    material: Some(mat_idx),
                    ..MeshData::default()
                },
                world: Mat4::IDENTITY,
                source_node: None,
            });
        }
        model
    }

    /// CCW-from-camera triangle helper (camera on +Z looking at −Z):
    /// counter-clockwise in y-up right-handed space.
    fn tri_at(z: f32, half: f32) -> [f32; 9] {
        [-half, -half, z, half, -half, z, 0.0, half, z]
    }

    #[test]
    fn mip_level_picks_by_texel_density() {
        let uv = ([0.0, 0.0], [1.0, 0.0], [0.0, 1.0]);
        // 256x256 texels squeezed onto a 16px triangle: tpp = 65536/2
        // over 128... concretely: screen2 = 16*16 = 256 (2x area of an
        // 8x16 right triangle... keep it simple: uv_area2 = 1 * 65536,
        // screen2 = 256 -> tpp = 256 -> level = floor(8/2) = 4.
        let lvl = mip_level(
            (0.0, 0.0),
            (16.0, 0.0),
            (0.0, 16.0),
            uv.0,
            uv.1,
            uv.2,
            65536.0,
            8,
        );
        assert_eq!(lvl, 4);
        // Magnification (few texels over many pixels): level 0.
        let lvl = mip_level(
            (0.0, 0.0),
            (100.0, 0.0),
            (0.0, 100.0),
            uv.0,
            uv.1,
            uv.2,
            64.0,
            8,
        );
        assert_eq!(lvl, 0);
        // Degenerate screen triangle: cheapest (last) level.
        let lvl = mip_level(
            (5.0, 5.0),
            (5.0, 5.0),
            (5.0, 5.0),
            uv.0,
            uv.1,
            uv.2,
            65536.0,
            8,
        );
        assert_eq!(lvl, 8);
        // Clamp to the chain length.
        let lvl = mip_level(
            (0.0, 0.0),
            (2.0, 0.0),
            (0.0, 2.0),
            uv.0,
            uv.1,
            uv.2,
            16_777_216.0,
            3,
        );
        assert_eq!(lvl, 3);
        // NaN UVs: level 0, no panic.
        let lvl = mip_level(
            (0.0, 0.0),
            (16.0, 0.0),
            (0.0, 16.0),
            [f32::NAN, 0.0],
            uv.1,
            uv.2,
            65536.0,
            8,
        );
        assert_eq!(lvl, 0);
    }

    #[test]
    fn mips_average_minified_checkerboards() {
        use crate::gfx::bitmap::Bitmap;
        use crate::three::extract::MeshData;
        use crate::three::load::{MaterialData, MeshInstance, Model};

        // A 1-texel checker (128x128) on a quad rendered FAR AWAY
        // (~12px): without mips, bilinear reads isolated texels —
        // extreme blacks/whites survive; with mips the selected level
        // is a box average — mid-gray. This is the shimmer mechanism:
        // per-frame extremes flip with sub-texel camera motion.
        let checker = Bitmap::from_fn(128, 128, |x, y| {
            if (x + y) % 2 == 0 {
                Rgba::rgb(255, 255, 255)
            } else {
                Rgba::rgb(0, 0, 0)
            }
        });
        let quad = MeshData {
            positions: vec![
                [-1.0, -1.0, 0.0],
                [1.0, -1.0, 0.0],
                [1.0, 1.0, 0.0],
                [-1.0, 1.0, 0.0],
            ],
            normals: None,
            uvs: Some(vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]]),
            colors: None,
            indices: vec![0, 1, 2, 0, 2, 3],
            material: Some(0),
            ..MeshData::default()
        };
        let build = |mips: bool| {
            let mut mat = MaterialData {
                texture: Some(checker.clone()),
                // Ambient-only white light response: isolate sampling.
                base_color: [1.0; 4],
                ..MaterialData::default()
            };
            if mips {
                mat.mips = checker.mip_chain();
            }
            Model {
                instances: vec![MeshInstance {
                    data: quad.clone(),
                    world: Mat4::IDENTITY,
                    source_node: None,
                }],
                materials: vec![mat],
                rig: None,
                warnings: Vec::new(),
            }
        };
        let render_spread = |model: &Model| -> (u8, u8) {
            let mut fb = Framebuffer::new(48, 48);
            let mut scene = Scene::new(model, Camera::orbit(Vec3::ZERO, 12.0, 0.0, 0.0));
            scene.double_sided = true;
            scene.light = Light {
                direction: Vec3::new(0.0, 0.0, -1.0),
                ambient: 1.0,
                diffuse: 0.0,
            };
            render(&scene, &mut fb);
            let mut min = 255u8;
            let mut max = 0u8;
            for p in fb.bitmap().pixels() {
                if p.a > 0 {
                    min = min.min(p.r);
                    max = max.max(p.r);
                }
            }
            (min, max)
        };
        let (min_raw, max_raw) = render_spread(&build(false));
        let (min_mip, max_mip) = render_spread(&build(true));
        let spread_raw = max_raw - min_raw;
        let spread_mip = max_mip - min_mip;
        assert!(
            spread_mip < spread_raw / 2,
            "mips must collapse the minified checker toward its mean \
             (raw spread {spread_raw}, mip spread {spread_mip})"
        );
    }

    #[test]
    fn camera_is_total_over_hostile_bounds() {
        // Overflow radius: per-axis finite bounds whose span is inf
        // (the exact shape the mutator render pass caught panicking
        // inside Mat4::perspective's near/far assertion).
        let cam = Camera::framing(Vec3::splat(f32::MIN), Vec3::splat(f32::MAX), 0.3, 0.2);
        assert!(cam.near > 0.0 && cam.far > cam.near, "{cam:?}");
        let _ = cam.projection(1.0); // must not assert
                                     // Point bounds (radius 0) and a non-finite orbit distance.
        let cam = Camera::framing(Vec3::splat(2.0), Vec3::splat(2.0), 0.0, 0.0);
        assert!(cam.near > 0.0 && cam.far > cam.near);
        let _ = cam.projection(1.0);
        let cam = Camera::orbit(Vec3::ZERO, f32::INFINITY, 0.0, 0.0);
        assert!(cam.near > 0.0 && cam.far > cam.near && cam.distance.is_finite());
        let _ = cam.projection(1.0);
    }

    #[test]
    fn degenerate_geometry_renders_nothing_and_never_panics() {
        use crate::three::extract::MeshData;
        // NaN vertices, zero-area triangles (collinear + repeated
        // index), an all-NaN triangle, and an empty-normal mesh in one
        // model: the renderer must skip them all quietly.
        let mesh = MeshData {
            positions: vec![
                [f32::NAN, 0.0, 0.0], // NaN vertex
                [1.0, 0.0, 0.0],
                [2.0, 0.0, 0.0], // collinear with 1 and 3
                [3.0, 0.0, 0.0],
                [0.0, 1.0, -0.5],
            ],
            normals: None,
            uvs: None,
            colors: None,
            indices: vec![
                0, 1, 2, // NaN corner
                1, 2, 3, // zero area (collinear)
                4, 4, 4, // repeated index (degenerate)
                0, 0, 0, // repeated NaN
            ],
            material: None,
            ..MeshData::default()
        };
        let model = crate::three::primitives::model_of(mesh, [1.0; 4]);
        let mut fb = Framebuffer::new(32, 32);
        let mut scene = Scene::new(&model, Camera::orbit(Vec3::ZERO, 3.0, 0.3, 0.2));
        scene.double_sided = true;
        render(&scene, &mut fb);
        // The collinear and repeated-index triangles have zero area,
        // the NaN ones are skipped: nothing may paint.
        assert_eq!(fb.coverage(), 0.0, "degenerate geometry painted pixels");

        // Same mesh through smooth-normal generation: NaN faces must
        // not poison the accumulation, and rendering stays safe.
        let mut model = model;
        model.ensure_smooth_normals();
        render(
            &Scene::new(&model, Camera::orbit(Vec3::ZERO, 3.0, 0.3, 0.2)),
            &mut fb,
        );
        assert_eq!(fb.coverage(), 0.0);
    }

    #[test]
    fn camera_orbit_and_framing() {
        let cam = Camera::orbit(Vec3::ZERO, 5.0, 0.0, 0.0);
        let eye = cam.eye();
        assert!((eye.z - 5.0).abs() < 1e-5 && eye.x.abs() < 1e-6);
        // Framing puts the whole box inside the frustum: distance must
        // exceed the bounding radius.
        let cam = Camera::framing(Vec3::splat(-1.0), Vec3::splat(1.0), 0.3, 0.2);
        assert!(cam.distance > (Vec3::splat(1.0) - Vec3::ZERO).length());
        // Extreme pitch stays finite (up-vector guard).
        let cam = Camera::orbit(Vec3::ZERO, 3.0, 0.0, 10.0);
        let v = cam.view();
        assert!(v.m.iter().all(|f| f.is_finite()));
    }

    #[test]
    fn scene_depth_ordering_through_full_pipeline() {
        // Near green triangle at z=1, far red at z=-1 (camera at +5Z
        // looking toward origin): green must win the overlap.
        let model = model_of(vec![
            (tri_at(-1.0, 2.0), [1.0, 0.0, 0.0, 1.0]),
            (tri_at(1.0, 1.0), [0.0, 1.0, 0.0, 1.0]),
        ]);
        let scene = Scene::new(&model, Camera::orbit(Vec3::ZERO, 5.0, 0.0, 0.0));
        let mut fb = Framebuffer::new(64, 64);
        render(&scene, &mut fb);
        assert!(fb.coverage() > 0.05, "coverage {}", fb.coverage());
        // Center: both triangles overlap; green is nearer.
        let center = fb.bitmap().get(32, 36).unwrap();
        assert!(
            center.g > center.r,
            "near triangle must occlude: {center:?}"
        );
        // Outside the small green tri but inside the big red one.
        let outer = fb.bitmap().get(10, 50).unwrap();
        assert!(
            outer.r > outer.g,
            "far triangle visible at edges: {outer:?}"
        );
    }

    #[test]
    fn backface_cull_and_double_sided() {
        // Same triangle wound to face AWAY from the camera.
        let mut back = tri_at(0.0, 1.0);
        back.swap(0, 3); // swap first two vertices' x
        back.swap(1, 4);
        back.swap(2, 5);
        let model = model_of(vec![(back, [1.0, 1.0, 1.0, 1.0])]);
        let mut scene = Scene::new(&model, Camera::orbit(Vec3::ZERO, 5.0, 0.0, 0.0));
        let mut fb = Framebuffer::new(32, 32);
        render(&scene, &mut fb);
        assert_eq!(fb.coverage(), 0.0, "backface must cull");
        scene.double_sided = true;
        render(&scene, &mut fb);
        assert!(fb.coverage() > 0.05, "double_sided renders it");
    }

    #[test]
    fn camera_inside_geometry_clips_instead_of_exploding() {
        // A triangle BEHIND the near plane straddling the camera: near
        // clip must produce stable output (no NaN, no full-screen
        // garbage from a w<=0 projection).
        let model = model_of(vec![(tri_at(4.99, 50.0), [1.0, 1.0, 1.0, 1.0])]);
        let scene = Scene::new(&model, Camera::orbit(Vec3::ZERO, 5.0, 0.0, 0.0));
        let mut fb = Framebuffer::new(32, 32);
        render(&scene, &mut fb); // camera at z=5, near ~0.05: triangle at z=4.99 is 0.01 in front
                                 // The triangle is huge and hugs the near plane: it either
                                 // clips away or fills sanely — the assert is "no NaN depths".
        for y in 0..32 {
            for x in 0..32 {
                let d = fb.depth_at(x, y).unwrap();
                assert!(!d.is_nan(), "NaN depth at {x},{y}");
            }
        }
    }

    #[test]
    fn gouraud_uses_vertex_normals() {
        // One triangle with normals tilted toward/away from the light:
        // the lit corner must be brighter than the unlit one.
        let mut model = model_of(vec![(tri_at(0.0, 2.0), [1.0, 1.0, 1.0, 1.0])]);
        model.instances[0].data.normals = Some(vec![
            [0.0, 0.0, 1.0], // toward camera/light
            [1.0, 0.0, 0.0], // sideways
            [0.0, 0.0, 1.0],
        ]);
        let mut scene = Scene::new(&model, Camera::orbit(Vec3::ZERO, 5.0, 0.0, 0.0));
        scene.light = Light {
            direction: Vec3::new(0.0, 0.0, -1.0),
            ambient: 0.2,
            diffuse: 0.8,
        };
        let mut fb = Framebuffer::new(48, 48);
        render(&scene, &mut fb);
        // Corner near vertex 0 (bottom-left) vs corner near vertex 1
        // (bottom-right): v0 normal faces the light, v1 is sideways.
        let lit = fb.bitmap().get(12, 40).unwrap();
        let dim = fb.bitmap().get(36, 40).unwrap();
        assert!(
            lit.r as i32 > dim.r as i32 + 30,
            "gouraud gradient missing: lit {lit:?} dim {dim:?}"
        );
    }

    #[test]
    fn render_is_deterministic() {
        let model = model_of(vec![(tri_at(0.0, 1.5), [0.9, 0.5, 0.2, 1.0])]);
        let scene = Scene::new(&model, Camera::orbit(Vec3::ZERO, 4.0, 0.4, 0.3));
        let mut a = Framebuffer::new(40, 30);
        let mut b = Framebuffer::new(40, 30);
        render(&scene, &mut a);
        render(&scene, &mut b);
        assert_eq!(a.bitmap(), b.bitmap());
    }
}
