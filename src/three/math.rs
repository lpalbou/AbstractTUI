//! Minimal 3D linear algebra for the software rasterizer. Column-major
//! `Mat4` deliberately matches BOTH glTF's `node.matrix` layout (a GLB
//! matrix loads as a straight 16-float copy) and the OpenGL convention
//! every reference formula (gluPerspective/gluLookAt) is written in, so
//! formulas transcribe 1:1 instead of being transposed by hand (the
//! classic source of "model renders inside-out" bugs).
//!
//! Conventions: right-handed, camera looks down −Z in view space, NDC
//! z ∈ [−1, 1] (GL-style; the rasterizer remaps to its z-buffer range).

/// 3-component f32 vector.
#[derive(Copy, Clone, Debug, Default, PartialEq)]
pub struct Vec3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Vec3 {
    pub const ZERO: Vec3 = Vec3::new(0.0, 0.0, 0.0);
    pub const X: Vec3 = Vec3::new(1.0, 0.0, 0.0);
    pub const Y: Vec3 = Vec3::new(0.0, 1.0, 0.0);
    pub const Z: Vec3 = Vec3::new(0.0, 0.0, 1.0);

    pub const fn new(x: f32, y: f32, z: f32) -> Vec3 {
        Vec3 { x, y, z }
    }

    pub const fn splat(v: f32) -> Vec3 {
        Vec3::new(v, v, v)
    }

    pub fn dot(self, o: Vec3) -> f32 {
        self.x * o.x + self.y * o.y + self.z * o.z
    }

    pub fn cross(self, o: Vec3) -> Vec3 {
        Vec3::new(
            self.y * o.z - self.z * o.y,
            self.z * o.x - self.x * o.z,
            self.x * o.y - self.y * o.x,
        )
    }

    pub fn length_sq(self) -> f32 {
        self.dot(self)
    }

    pub fn length(self) -> f32 {
        self.length_sq().sqrt()
    }

    /// Zero-safe normalize: a degenerate vector returns ZERO instead of
    /// NaN. The rasterizer treats zero normals as "unlit" — NaN would
    /// silently poison every downstream lerp and is invisible to
    /// property tests, which is the worse failure mode.
    pub fn normalize(self) -> Vec3 {
        let len = self.length();
        if len > f32::EPSILON {
            self * (1.0 / len)
        } else {
            Vec3::ZERO
        }
    }

    pub fn lerp(self, to: Vec3, t: f32) -> Vec3 {
        self + (to - self) * t
    }
}

impl std::ops::Add for Vec3 {
    type Output = Vec3;
    fn add(self, o: Vec3) -> Vec3 {
        Vec3::new(self.x + o.x, self.y + o.y, self.z + o.z)
    }
}

impl std::ops::Sub for Vec3 {
    type Output = Vec3;
    fn sub(self, o: Vec3) -> Vec3 {
        Vec3::new(self.x - o.x, self.y - o.y, self.z - o.z)
    }
}

impl std::ops::Mul<f32> for Vec3 {
    type Output = Vec3;
    fn mul(self, s: f32) -> Vec3 {
        Vec3::new(self.x * s, self.y * s, self.z * s)
    }
}

impl std::ops::Neg for Vec3 {
    type Output = Vec3;
    fn neg(self) -> Vec3 {
        Vec3::new(-self.x, -self.y, -self.z)
    }
}

/// 4-component f32 vector (homogeneous coordinates).
#[derive(Copy, Clone, Debug, Default, PartialEq)]
pub struct Vec4 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub w: f32,
}

impl Vec4 {
    pub const fn new(x: f32, y: f32, z: f32, w: f32) -> Vec4 {
        Vec4 { x, y, z, w }
    }

    pub const fn from_point(p: Vec3) -> Vec4 {
        Vec4::new(p.x, p.y, p.z, 1.0)
    }

    pub const fn from_dir(d: Vec3) -> Vec4 {
        Vec4::new(d.x, d.y, d.z, 0.0)
    }

    pub const fn xyz(self) -> Vec3 {
        Vec3::new(self.x, self.y, self.z)
    }

    /// Perspective divide; w == 0 (direction / degenerate clip result)
    /// returns the xyz as-is rather than dividing by zero.
    pub fn project(self) -> Vec3 {
        if self.w.abs() > f32::EPSILON {
            Vec3::new(self.x / self.w, self.y / self.w, self.z / self.w)
        } else {
            self.xyz()
        }
    }
}

/// Column-major 4x4 matrix: element (row r, col c) sits at `m[c*4+r]`,
/// exactly the order glTF's `node.matrix` array uses.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Mat4 {
    pub m: [f32; 16],
}

impl Default for Mat4 {
    fn default() -> Mat4 {
        Mat4::IDENTITY
    }
}

impl Mat4 {
    pub const IDENTITY: Mat4 = Mat4 {
        m: [
            1.0, 0.0, 0.0, 0.0, //
            0.0, 1.0, 0.0, 0.0, //
            0.0, 0.0, 1.0, 0.0, //
            0.0, 0.0, 0.0, 1.0,
        ],
    };

    /// Adopt a glTF-order (column-major) array verbatim.
    /// Column-major array copy (symmetric with `from_cols_array`).
    pub const fn to_cols_array(&self) -> [f32; 16] {
        self.m
    }

    pub const fn from_cols_array(m: [f32; 16]) -> Mat4 {
        Mat4 { m }
    }

    #[inline]
    pub fn at(&self, row: usize, col: usize) -> f32 {
        self.m[col * 4 + row]
    }

    /// `self * rhs` (apply `rhs` first, then `self` — standard
    /// column-vector composition).
    #[must_use]
    pub fn mul(&self, rhs: &Mat4) -> Mat4 {
        let mut out = [0.0f32; 16];
        for c in 0..4 {
            for r in 0..4 {
                let mut acc = 0.0;
                for k in 0..4 {
                    acc += self.at(r, k) * rhs.at(k, c);
                }
                out[c * 4 + r] = acc;
            }
        }
        Mat4 { m: out }
    }

    pub fn mul_vec4(&self, v: Vec4) -> Vec4 {
        let m = &self.m;
        Vec4::new(
            m[0] * v.x + m[4] * v.y + m[8] * v.z + m[12] * v.w,
            m[1] * v.x + m[5] * v.y + m[9] * v.z + m[13] * v.w,
            m[2] * v.x + m[6] * v.y + m[10] * v.z + m[14] * v.w,
            m[3] * v.x + m[7] * v.y + m[11] * v.z + m[15] * v.w,
        )
    }

    /// Transform a point (w = 1) without perspective divide — model and
    /// view transforms are affine, the divide only matters after a
    /// projection matrix (use `mul_vec4(...).project()` there).
    pub fn transform_point(&self, p: Vec3) -> Vec3 {
        self.mul_vec4(Vec4::from_point(p)).xyz()
    }

    /// Transform a direction (w = 0): rotation/scale only, no
    /// translation. NOTE for cycle 2: correct *normal* transformation
    /// under non-uniform scale needs the inverse-transpose; this is the
    /// plain tangent-vector transform.
    pub fn transform_dir(&self, d: Vec3) -> Vec3 {
        self.mul_vec4(Vec4::from_dir(d)).xyz()
    }

    #[must_use]
    pub fn transpose(&self) -> Mat4 {
        let mut out = [0.0f32; 16];
        for c in 0..4 {
            for r in 0..4 {
                out[r * 4 + c] = self.m[c * 4 + r];
            }
        }
        Mat4 { m: out }
    }

    pub fn translate(t: Vec3) -> Mat4 {
        let mut m = Mat4::IDENTITY;
        m.m[12] = t.x;
        m.m[13] = t.y;
        m.m[14] = t.z;
        m
    }

    pub fn scale(s: Vec3) -> Mat4 {
        let mut m = Mat4::IDENTITY;
        m.m[0] = s.x;
        m.m[5] = s.y;
        m.m[10] = s.z;
        m
    }

    pub fn rotate_x(rad: f32) -> Mat4 {
        let (s, c) = rad.sin_cos();
        let mut m = Mat4::IDENTITY;
        m.m[5] = c;
        m.m[6] = s;
        m.m[9] = -s;
        m.m[10] = c;
        m
    }

    pub fn rotate_y(rad: f32) -> Mat4 {
        let (s, c) = rad.sin_cos();
        let mut m = Mat4::IDENTITY;
        m.m[0] = c;
        m.m[2] = -s;
        m.m[8] = s;
        m.m[10] = c;
        m
    }

    pub fn rotate_z(rad: f32) -> Mat4 {
        let (s, c) = rad.sin_cos();
        let mut m = Mat4::IDENTITY;
        m.m[0] = c;
        m.m[1] = s;
        m.m[4] = -s;
        m.m[5] = c;
        m
    }

    /// Rotation from a glTF-order quaternion (x, y, z, w), normalized
    /// internally because exporters routinely emit quaternions a few
    /// ULP off unit length and the error squares into the matrix.
    pub fn from_quat(x: f32, y: f32, z: f32, w: f32) -> Mat4 {
        let n = (x * x + y * y + z * z + w * w).sqrt();
        if n < f32::EPSILON {
            return Mat4::IDENTITY;
        }
        let (x, y, z, w) = (x / n, y / n, z / n, w / n);
        let (xx, yy, zz) = (x * x, y * y, z * z);
        let (xy, xz, yz) = (x * y, x * z, y * z);
        let (wx, wy, wz) = (w * x, w * y, w * z);
        Mat4::from_cols_array([
            1.0 - 2.0 * (yy + zz),
            2.0 * (xy + wz),
            2.0 * (xz - wy),
            0.0,
            2.0 * (xy - wz),
            1.0 - 2.0 * (xx + zz),
            2.0 * (yz + wx),
            0.0,
            2.0 * (xz + wy),
            2.0 * (yz - wx),
            1.0 - 2.0 * (xx + yy),
            0.0,
            0.0,
            0.0,
            0.0,
            1.0,
        ])
    }

    /// glTF node TRS composition: T * R * S (scale first, then rotate,
    /// then translate — the order the glTF spec mandates).
    pub fn from_trs(t: Vec3, r: (f32, f32, f32, f32), s: Vec3) -> Mat4 {
        Mat4::translate(t)
            .mul(&Mat4::from_quat(r.0, r.1, r.2, r.3))
            .mul(&Mat4::scale(s))
    }

    /// Right-handed perspective projection (gluPerspective): `fov_y` in
    /// radians, maps view-space z ∈ [−near, −far] to NDC z ∈ [−1, 1].
    pub fn perspective(fov_y: f32, aspect: f32, near: f32, far: f32) -> Mat4 {
        debug_assert!(near > 0.0 && far > near && aspect > 0.0);
        let f = 1.0 / (fov_y * 0.5).tan();
        let mut m = [0.0f32; 16];
        m[0] = f / aspect;
        m[5] = f;
        m[10] = (far + near) / (near - far);
        m[11] = -1.0;
        m[14] = 2.0 * far * near / (near - far);
        Mat4 { m }
    }

    /// Right-handed view matrix (gluLookAt): camera at `eye`, looking
    /// at `center`, `up` roughly up. Maps eye → origin and the view
    /// direction → −Z.
    pub fn look_at(eye: Vec3, center: Vec3, up: Vec3) -> Mat4 {
        let f = (center - eye).normalize();
        let s = f.cross(up).normalize();
        let u = s.cross(f);
        Mat4::from_cols_array([
            s.x,
            u.x,
            -f.x,
            0.0,
            s.y,
            u.y,
            -f.y,
            0.0,
            s.z,
            u.z,
            -f.z,
            0.0,
            -s.dot(eye),
            -u.dot(eye),
            f.dot(eye),
            1.0,
        ])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EPS: f32 = 1e-5;

    fn assert_vec3(a: Vec3, b: Vec3) {
        assert!(
            (a.x - b.x).abs() < EPS && (a.y - b.y).abs() < EPS && (a.z - b.z).abs() < EPS,
            "{a:?} != {b:?}"
        );
    }

    #[test]
    fn vec3_basics() {
        assert_eq!(Vec3::X.dot(Vec3::Y), 0.0);
        assert_vec3(Vec3::X.cross(Vec3::Y), Vec3::Z);
        assert_vec3(Vec3::Y.cross(Vec3::Z), Vec3::X);
        assert_vec3(
            Vec3::new(3.0, 0.0, 4.0).normalize(),
            Vec3::new(0.6, 0.0, 0.8),
        );
        assert_vec3(Vec3::ZERO.normalize(), Vec3::ZERO); // zero-safe
        assert_vec3(Vec3::X.lerp(Vec3::Y, 0.5), Vec3::new(0.5, 0.5, 0.0));
    }

    #[test]
    fn mat4_identity_and_mul() {
        let p = Vec3::new(1.0, 2.0, 3.0);
        assert_vec3(Mat4::IDENTITY.transform_point(p), p);
        // translate ∘ scale: scale applies first (column-vector order).
        let m = Mat4::translate(Vec3::new(1.0, 2.0, 3.0)).mul(&Mat4::scale(Vec3::splat(2.0)));
        assert_vec3(
            m.transform_point(Vec3::new(1.0, 1.0, 1.0)),
            Vec3::new(3.0, 4.0, 5.0),
        );
        // transpose of transpose is identity of the op.
        assert_eq!(m.transpose().transpose(), m);
    }

    #[test]
    fn rotations_quarter_turn() {
        let half_pi = std::f32::consts::FRAC_PI_2;
        assert_vec3(Mat4::rotate_z(half_pi).transform_point(Vec3::X), Vec3::Y);
        assert_vec3(Mat4::rotate_x(half_pi).transform_point(Vec3::Y), Vec3::Z);
        assert_vec3(Mat4::rotate_y(half_pi).transform_point(Vec3::Z), Vec3::X);
        // Directions ignore translation.
        let m = Mat4::translate(Vec3::splat(9.0)).mul(&Mat4::rotate_z(half_pi));
        assert_vec3(m.transform_dir(Vec3::X), Vec3::Y);
    }

    #[test]
    fn quat_matches_axis_rotation() {
        // 90° about Z as a quaternion: (0, 0, sin 45°, cos 45°).
        let s = std::f32::consts::FRAC_1_SQRT_2;
        let q = Mat4::from_quat(0.0, 0.0, s, s);
        assert_vec3(q.transform_point(Vec3::X), Vec3::Y);
        // Non-normalized input is normalized internally.
        let q2 = Mat4::from_quat(0.0, 0.0, 2.0 * s, 2.0 * s);
        assert_vec3(q2.transform_point(Vec3::X), Vec3::Y);
        // Identity quaternion.
        assert_eq!(Mat4::from_quat(0.0, 0.0, 0.0, 1.0), Mat4::IDENTITY);
    }

    #[test]
    fn trs_composition_order() {
        // Scale 2, rotate 90° about Z, translate +X: point (1,0,0)
        // -> scale (2,0,0) -> rotate (0,2,0) -> translate (1,2,0).
        let s = std::f32::consts::FRAC_1_SQRT_2;
        let m = Mat4::from_trs(Vec3::X, (0.0, 0.0, s, s), Vec3::splat(2.0));
        assert_vec3(m.transform_point(Vec3::X), Vec3::new(1.0, 2.0, 0.0));
    }

    #[test]
    fn perspective_known_values() {
        // fov 90°, aspect 1, near 1, far 3: f = 1/tan(45°) = 1.
        let p = Mat4::perspective(std::f32::consts::FRAC_PI_2, 1.0, 1.0, 3.0);
        assert!((p.at(0, 0) - 1.0).abs() < EPS);
        assert!((p.at(1, 1) - 1.0).abs() < EPS);
        assert!((p.at(2, 2) - -2.0).abs() < EPS); // (f+n)/(n−f) = 4/−2
        assert!((p.at(2, 3) - -3.0).abs() < EPS); // 2fn/(n−f) = 6/−2
        assert!((p.at(3, 2) - -1.0).abs() < EPS);
        assert_eq!(p.at(3, 3), 0.0);

        // Near plane (z = −1) maps to NDC z = −1, far (z = −3) to +1.
        let near_pt = p
            .mul_vec4(Vec4::from_point(Vec3::new(0.0, 0.0, -1.0)))
            .project();
        assert!((near_pt.z + 1.0).abs() < EPS, "near -> {near_pt:?}");
        let far_pt = p
            .mul_vec4(Vec4::from_point(Vec3::new(0.0, 0.0, -3.0)))
            .project();
        assert!((far_pt.z - 1.0).abs() < EPS, "far -> {far_pt:?}");

        // A point at 45° from the axis at the near plane lands on the
        // NDC edge (|x| = 1) at fov 90.
        let edge = p
            .mul_vec4(Vec4::from_point(Vec3::new(1.0, 0.0, -1.0)))
            .project();
        assert!((edge.x - 1.0).abs() < EPS);
    }

    #[test]
    fn look_at_known_values() {
        // Camera at +5Z looking at origin: eye maps to origin, the
        // target to (0, 0, −5), and world-up stays +Y.
        let v = Mat4::look_at(Vec3::new(0.0, 0.0, 5.0), Vec3::ZERO, Vec3::Y);
        assert_vec3(v.transform_point(Vec3::new(0.0, 0.0, 5.0)), Vec3::ZERO);
        assert_vec3(v.transform_point(Vec3::ZERO), Vec3::new(0.0, 0.0, -5.0));
        assert_vec3(v.transform_point(Vec3::new(0.0, 1.0, 5.0)), Vec3::Y);
        // +X in world stays +X in view (right-handed, no flip).
        assert_vec3(v.transform_dir(Vec3::X), Vec3::X);

        // Off-axis camera: the eye always maps to the origin.
        let v2 = Mat4::look_at(Vec3::new(3.0, 4.0, 5.0), Vec3::new(1.0, 1.0, 1.0), Vec3::Y);
        assert_vec3(v2.transform_point(Vec3::new(3.0, 4.0, 5.0)), Vec3::ZERO);
        // The look target sits on the −Z axis at its true distance.
        let d = (Vec3::new(3.0, 4.0, 5.0) - Vec3::new(1.0, 1.0, 1.0)).length();
        let t = v2.transform_point(Vec3::new(1.0, 1.0, 1.0));
        assert_vec3(t, Vec3::new(0.0, 0.0, -d));
    }
}
