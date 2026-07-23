//! The scene fixtures: orbit camera + directional key light.
//! `#[path]` sibling of scene.rs (file-size split) — both types
//! re-export from `three::scene` unchanged.
//!
//! OWNER: GFX3D.

use crate::three::math::{Mat4, Vec3};

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
