//! Software 3D: minimal-but-correct GLB (glTF 2.0 binary) loading —
//! meshes, transforms, base-color materials — and a perspective
//! rasterizer with z-buffer, backface culling and lambert+ambient
//! shading, rendering into RGBA framebuffers presented via `gfx`
//! (mosaic cells or pixel protocols).
//!
//! OWNER: GFX3D. Hand-rolled JSON parser (no serde) keeps the engine
//! standalone; unsupported GLB extensions degrade loudly. Design notes
//! + research citations in `docs/design/gfx-three.md`.
//!
//! Pipeline: `glb::split` -> `doc::Doc::parse` -> `extract` (validated
//! typed arrays) -> `load::Model` (flattened instances + materials) ->
//! `scene::render` (near clip, backface cull, top-left edge fill,
//! z-buffer, lambert) into a `raster::Framebuffer`, whose bitmap the
//! gfx pipeline presents (mosaic or pixel protocols).

pub mod animation;
pub mod brandmark;
pub mod doc;
pub mod extract;
pub mod glb;
pub mod gltf_json;
pub mod load;
pub mod math;
pub mod primitives;
pub mod quick;
pub mod raster;
pub mod scene;
#[cfg(test)]
pub(crate) mod skin_tests;
pub mod texture;
pub(crate) mod validate;

#[cfg(test)]
mod e2e_tests;

pub use animation::{Animation, Interpolation, NodePose, Track, TrackValues};
pub use doc::{
    Accessor, AccessorType, BufferView, ComponentType, Doc, Material, Mesh, Node, Primitive,
};
pub use extract::MeshData;
pub use glb::GlbChunks;
pub use load::{
    load_glb, load_glb_with_stats, LoadStats, MaterialData, MeshInstance, Model, Pose, Rig,
    RigNode, SkinData,
};
pub use math::{Mat4, Vec3, Vec4};
pub use quick::{quick_view, quick_view_bytes, QuickView};
pub use raster::Framebuffer;
pub use scene::{render, Camera, Light, Scene, SceneRenderer};
pub use texture::{Filter, TextureSampler, Wrap};
