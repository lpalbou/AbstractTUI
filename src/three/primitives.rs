//! Procedural mesh primitives: cube, UV sphere, plane — with normals
//! and UVs, wound CCW (glTF front-face convention) so they behave
//! exactly like loaded assets everywhere downstream. Consumers: widget
//! tests (deterministic geometry), the brandmark planes, examples.

use crate::three::extract::MeshData;
use crate::three::load::{MaterialData, MeshInstance, Model};
use crate::three::math::Mat4;

/// Axis-aligned cuboid centered at the origin, extents (w, h, d).
/// 24 vertices (per-face normals — a cube shaded with averaged corner
/// normals looks like a bad sphere), 12 triangles, per-face UVs.
pub fn cuboid(w: f32, h: f32, d: f32) -> MeshData {
    let (x, y, z) = (w * 0.5, h * 0.5, d * 0.5);
    // Each face: (normal, four corners CCW seen from outside).
    #[rustfmt::skip]
    let faces: [([f32; 3], [[f32; 3]; 4]); 6] = [
        ([0.0, 0.0, 1.0],  [[-x, -y,  z], [ x, -y,  z], [ x,  y,  z], [-x,  y,  z]]), // +Z
        ([0.0, 0.0, -1.0], [[ x, -y, -z], [-x, -y, -z], [-x,  y, -z], [ x,  y, -z]]), // -Z
        ([1.0, 0.0, 0.0],  [[ x, -y,  z], [ x, -y, -z], [ x,  y, -z], [ x,  y,  z]]), // +X
        ([-1.0, 0.0, 0.0], [[-x, -y, -z], [-x, -y,  z], [-x,  y,  z], [-x,  y, -z]]), // -X
        ([0.0, 1.0, 0.0],  [[-x,  y,  z], [ x,  y,  z], [ x,  y, -z], [-x,  y, -z]]), // +Y
        ([0.0, -1.0, 0.0], [[-x, -y, -z], [ x, -y, -z], [ x, -y,  z], [-x, -y,  z]]), // -Y
    ];
    let mut positions = Vec::with_capacity(24);
    let mut normals = Vec::with_capacity(24);
    let mut uvs = Vec::with_capacity(24);
    let mut indices = Vec::with_capacity(36);
    for (n, corners) in faces {
        let base = positions.len() as u32;
        for (k, c) in corners.iter().enumerate() {
            positions.push(*c);
            normals.push(n);
            uvs.push([[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]][k]);
        }
        // Two CCW triangles per face.
        indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }
    MeshData {
        positions,
        normals: Some(normals),
        uvs: Some(uvs),
        colors: None,
        indices,
        material: None,
        ..MeshData::default()
    }
}

/// Unit-ish cube (edge length `size`).
pub fn cube(size: f32) -> MeshData {
    cuboid(size, size, size)
}

/// UV sphere: `stacks` latitude bands (≥ 2), `slices` longitude
/// segments (≥ 3), radius `r`. Smooth normals (= normalized position),
/// equirectangular UVs. Poles are vertex rings with degenerate u — the
/// standard construction; seam vertices are duplicated so UVs do not
/// wrap backwards across the last slice.
pub fn uv_sphere(r: f32, stacks: u32, slices: u32) -> MeshData {
    let stacks = stacks.max(2);
    let slices = slices.max(3);
    let mut positions = Vec::new();
    let mut normals = Vec::new();
    let mut uvs = Vec::new();
    for st in 0..=stacks {
        // phi: 0 at the north pole, PI at the south.
        let phi = std::f32::consts::PI * st as f32 / stacks as f32;
        let (sp, cp) = phi.sin_cos();
        for sl in 0..=slices {
            let theta = std::f32::consts::TAU * sl as f32 / slices as f32;
            let (stheta, ctheta) = theta.sin_cos();
            let n = [sp * ctheta, cp, sp * stheta];
            positions.push([n[0] * r, n[1] * r, n[2] * r]);
            normals.push(n);
            uvs.push([sl as f32 / slices as f32, st as f32 / stacks as f32]);
        }
    }
    let ring = slices + 1;
    let mut indices = Vec::new();
    for st in 0..stacks {
        for sl in 0..slices {
            let a = st * ring + sl;
            let b = a + ring;
            // Quad (a, a+1, b+1, b) split CCW when viewed from OUTSIDE.
            // Skip the degenerate triangle at each pole (zero area).
            if st != 0 {
                indices.extend_from_slice(&[a, a + 1, b]);
            }
            if st != stacks - 1 {
                indices.extend_from_slice(&[a + 1, b + 1, b]);
            }
        }
    }
    MeshData {
        positions,
        normals: Some(normals),
        uvs: Some(uvs),
        colors: None,
        indices,
        material: None,
        ..MeshData::default()
    }
}

/// XZ-plane rectangle centered at the origin, +Y normal, w x d.
pub fn plane(w: f32, d: f32) -> MeshData {
    let (x, z) = (w * 0.5, d * 0.5);
    MeshData {
        positions: vec![[-x, 0.0, z], [x, 0.0, z], [x, 0.0, -z], [-x, 0.0, -z]],
        normals: Some(vec![[0.0, 1.0, 0.0]; 4]),
        uvs: Some(vec![[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]]),
        colors: None,
        indices: vec![0, 1, 2, 0, 2, 3],
        material: None,
        ..MeshData::default()
    }
}

/// Wrap one mesh into a single-instance model with a solid base color —
/// exactly what the viewport widget and tests want for primitives.
pub fn model_of(mut mesh: MeshData, base_color: [f32; 4]) -> Model {
    mesh.material = Some(0);
    Model {
        instances: vec![MeshInstance {
            data: mesh,
            world: Mat4::IDENTITY,
            source_node: None,
        }],
        materials: vec![MaterialData {
            base_color,
            ..MaterialData::default()
        }],
        rig: None,
        warnings: Vec::new(),
    }
}

/// Ground-grid line mesh in the XZ plane at `y`: `2·(n+1)` thin quads
/// spanning `[-extent, extent]` at `step` spacing. Rendered as an
/// emissive-only model (lines read the same from every angle — a lit
/// grid would vanish edge-on to the light). ~4·(n+1) triangles; for
/// typical extents this is < 200 tris — free.
pub fn grid_lines(extent: f32, step: f32, y: f32, thickness: f32) -> MeshData {
    let step = step.max(1e-3);
    let half_t = (thickness * 0.5).max(1e-4);
    let n = ((extent / step).floor() as i32).max(0);
    let mut positions = Vec::new();
    let mut indices: Vec<u32> = Vec::new();
    let mut quad = |a: [f32; 3], b: [f32; 3], c: [f32; 3], d: [f32; 3]| {
        let base = positions.len() as u32;
        positions.extend_from_slice(&[a, b, c, d]);
        indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    };
    for i in -n..=n {
        let o = i as f32 * step;
        // Line parallel to X at z=o, and parallel to Z at x=o.
        quad(
            [-extent, y, o - half_t],
            [extent, y, o - half_t],
            [extent, y, o + half_t],
            [-extent, y, o + half_t],
        );
        quad(
            [o - half_t, y, -extent],
            [o - half_t, y, extent],
            [o + half_t, y, extent],
            [o + half_t, y, -extent],
        );
    }
    MeshData {
        positions,
        normals: None,
        uvs: None,
        colors: None,
        indices,
        material: Some(0),
        ..MeshData::default()
    }
}

/// A grid model colored by `rgb` (emissive: ignores lighting), sized
/// for viewer aesthetics. Render it FIRST, then overlay the model —
/// the shared depth buffer composes them.
pub fn grid_model(extent: f32, step: f32, y: f32, rgb: [f32; 3]) -> Model {
    let mesh = grid_lines(extent, step, y, step * 0.02);
    Model {
        instances: vec![MeshInstance {
            data: mesh,
            world: Mat4::IDENTITY,
            source_node: None,
        }],
        materials: vec![MaterialData {
            // Black base + emissive = flat unlit line color.
            base_color: [0.0, 0.0, 0.0, 1.0],
            emissive: rgb,
            ..MaterialData::default()
        }],
        rig: None,
        warnings: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::three::math::Vec3;

    fn assert_valid(mesh: &MeshData) {
        assert!(!mesh.indices.is_empty() && mesh.indices.len().is_multiple_of(3));
        let n = mesh.positions.len();
        for &i in &mesh.indices {
            assert!((i as usize) < n, "index {i} out of {n}");
        }
        assert_eq!(mesh.normals.as_ref().unwrap().len(), n);
        assert_eq!(mesh.uvs.as_ref().unwrap().len(), n);
        for nn in mesh.normals.as_ref().unwrap() {
            let len = (nn[0] * nn[0] + nn[1] * nn[1] + nn[2] * nn[2]).sqrt();
            assert!((len - 1.0).abs() < 1e-4, "unnormalized normal {nn:?}");
        }
    }

    #[test]
    fn cube_shape() {
        let c = cube(2.0);
        assert_valid(&c);
        assert_eq!(c.positions.len(), 24);
        assert_eq!(c.triangle_count(), 12);
        // All corners at ±1.
        for p in &c.positions {
            for v in p {
                assert!((v.abs() - 1.0).abs() < 1e-6);
            }
        }
    }

    #[test]
    fn cube_winding_is_ccw_outward() {
        // For every triangle, the geometric normal must point AWAY from
        // the origin (dot with the centroid > 0) — that is what CCW-
        // from-outside means for a convex solid around the origin.
        let c = cube(2.0);
        for tri in c.indices.chunks_exact(3) {
            let p = |i: usize| {
                let v = c.positions[tri[i] as usize];
                Vec3::new(v[0], v[1], v[2])
            };
            let (a, b, cc) = (p(0), p(1), p(2));
            let n = (b - a).cross(cc - a);
            let centroid = (a + b + cc) * (1.0 / 3.0);
            assert!(n.dot(centroid) > 0.0, "inward-facing triangle {tri:?}");
        }
    }

    #[test]
    fn sphere_shape_and_radius() {
        let s = uv_sphere(2.0, 8, 12);
        assert_valid(&s);
        assert!(s.triangle_count() > 100);
        for p in &s.positions {
            let r = (p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).sqrt();
            assert!((r - 2.0).abs() < 1e-4, "off-sphere vertex {p:?}");
        }
        // Winding: outward normals (same convexity argument as cube).
        for tri in s.indices.chunks_exact(3) {
            let p = |i: usize| {
                let v = s.positions[tri[i] as usize];
                Vec3::new(v[0], v[1], v[2])
            };
            let (a, b, c) = (p(0), p(1), p(2));
            let n = (b - a).cross(c - a);
            let centroid = (a + b + c) * (1.0 / 3.0);
            assert!(n.dot(centroid) > -1e-4, "inward sphere triangle {tri:?}");
        }
        // Clamped params still valid.
        assert_valid(&uv_sphere(1.0, 1, 2));
    }

    #[test]
    fn plane_shape() {
        let p = plane(4.0, 2.0);
        assert_valid(&p);
        assert_eq!(p.triangle_count(), 2);
    }

    #[test]
    fn model_of_bounds_and_render() {
        let model = model_of(cube(2.0), [1.0, 0.2, 0.2, 1.0]);
        let (min, max) = model.bounds().unwrap();
        assert_eq!((min.x, max.x), (-1.0, 1.0));
        assert_eq!(model.triangle_count(), 12);
        // End-to-end sanity: the cube renders with visible coverage.
        let cam = crate::three::scene::Camera::framing(min, max, 0.7, 0.5);
        let scene = crate::three::scene::Scene::new(&model, cam);
        let mut fb = crate::three::raster::Framebuffer::new(64, 48);
        crate::three::scene::render(&scene, &mut fb);
        assert!(fb.coverage() > 0.08, "coverage {}", fb.coverage());
    }
}
