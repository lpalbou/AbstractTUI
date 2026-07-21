//! The five-line 3D hello: load a GLB, get a framed camera and a
//! default light, render.
//!
//! ```no_run
//! use abstracttui::three::{self, Framebuffer, SceneRenderer};
//!
//! let view = three::quick_view("model.glb")?;
//! let mut fb = Framebuffer::new(160, 96);
//! SceneRenderer::new().render(&view.scene(), &mut fb);
//! # Ok::<(), abstracttui::base::Error>(())
//! ```
//!
//! `QuickView` owns the model and remembers a camera + light you can
//! adjust before building the borrowed [`Scene`]; the scene itself
//! stays the real API — quick_view only removes the setup boilerplate
//! (bounds → framing camera → light), it hides nothing.
//!
//! OWNER: GFX3D.

use crate::base::Result;
use crate::three::load::{self, LoadStats, Model};
use crate::three::scene::{Camera, Light, Scene};

/// A loaded model plus ready-to-render view state. Fields are public
/// on purpose: adjust the camera/light freely between frames, then
/// call [`QuickView::scene`].
#[derive(Debug)]
pub struct QuickView {
    pub model: Model,
    pub camera: Camera,
    pub light: Light,
    /// Load-time cost report (texture decode dominates on textured
    /// models — show "loading" around [`quick_view`]).
    pub stats: LoadStats,
}

impl QuickView {
    /// The scene for the current camera/light. Backface culling stays
    /// OFF (real-world exports are not consistently wound); flip
    /// `double_sided` on the returned scene to cull.
    pub fn scene(&self) -> Scene<'_> {
        let mut scene = Scene::new(&self.model, self.camera);
        scene.light = self.light;
        scene.double_sided = true;
        scene
    }

    /// Re-frame the camera on the model's bounds at new orbit angles
    /// (radians) — the "reset camera" a viewer needs.
    pub fn look_from(&mut self, yaw: f32, pitch: f32) {
        self.camera = self.model.fit_camera(yaw, pitch);
    }
}

/// Load a GLB from disk with a framed camera and default lighting —
/// the docs' 3D hello. Equivalent to [`load::load_glb_with_stats`] +
/// [`Model::fit_camera`] + [`Light::default`]; rejections and labeled
/// degradations are exactly the loader's (see the support matrix in
/// [`crate::three`] docs).
pub fn quick_view(path: impl AsRef<std::path::Path>) -> Result<QuickView> {
    let (model, stats) = load::load_glb_with_stats(path)?;
    let camera = model.fit_camera(0.6, 0.35);
    Ok(QuickView {
        model,
        camera,
        light: Light::default(),
        stats,
    })
}

/// [`quick_view`] over in-memory GLB bytes (network fetches, embedded
/// assets).
pub fn quick_view_bytes(bytes: &[u8]) -> Result<QuickView> {
    let (model, stats) = Model::load_with_stats(bytes)?;
    let camera = model.fit_camera(0.6, 0.35);
    Ok(QuickView {
        model,
        camera,
        light: Light::default(),
        stats,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::glb_mutate;
    use crate::three::raster::Framebuffer;
    use crate::three::scene::SceneRenderer;

    #[test]
    fn quick_view_renders_in_five_lines() {
        let view = quick_view_bytes(&glb_mutate::minimal_glb()).unwrap();
        let mut fb = Framebuffer::new(64, 48);
        SceneRenderer::new().render(&view.scene(), &mut fb);
        assert!(fb.coverage() > 0.0, "framed camera must show the model");
        assert!(view.stats.total > std::time::Duration::ZERO);
    }

    #[test]
    fn quick_view_path_errors_name_the_file() {
        let err = quick_view("/nonexistent/nope.glb").unwrap_err();
        assert!(err.to_string().contains("nope.glb"), "{err}");
    }

    #[test]
    fn look_from_reframes() {
        let mut view = quick_view_bytes(&glb_mutate::minimal_glb()).unwrap();
        let before = view.camera.yaw;
        view.look_from(before + 1.0, 0.1);
        assert!((view.camera.yaw - (before + 1.0)).abs() < 1e-6);
    }
}
