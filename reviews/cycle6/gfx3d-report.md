# GFX3D — Cycle 6 report

## Headline

glTF animation is real end-to-end: TRS node animation AND skeletal
skinning, parsed from GLB, sampled deterministically, rendered through
the vertex stage, playable in Viewport3D. 856 lib tests green, clippy
zero in my files, perf pins hold.

## 1. Animation shipped

**Parsing** (`three/doc.rs` + `validate.rs`): `animations` (channels +
samplers, interpolation string preserved), `skins` (joints,
inverseBindMatrices, skeleton), `node.skin`, and the JOINTS_0 /
WEIGHTS_0 primitive attributes. Parse-time index validation: sampler →
accessor, channel → sampler/node, skin → nodes/accessor, node.skin →
skins. Empty joint lists reject at parse.

**Sampling** (`three/animation.rs`): `Animation::sample(t, &mut
[NodePose])` — binary-search keyframe pair, LINEAR lerp / STEP hold,
clamped outside the range, duplicate-time hard cuts hold-left.
Rotations NLERP with shortest-path sign correction (documented
approximation of slerp; spec-permitted). `sample(t)` is PURE — REDTEAM's
`adv_gltf_anim` determinism suite (FNV over raw matrix bytes) passes.

**Interpolation coverage**: LINEAR + STEP evaluate. CUBICSPLINE
channels skip with `#FALLBACK` (tangent triplets — sampling them as
values would play garbage; the file loads, other channels play). Morph
`weights` channels skip with a label (no morph pipeline). Unknown
interpolations/paths, decreasing/non-finite times, count mismatches:
reject by name.

**Node re-evaluation** (`three/load.rs`): `Rig` = rest poses +
hierarchy + animations + skins. `Model::sample_pose_full(anim, t, &mut
Pose)` re-walks the hierarchy iteratively (same DFS shape as the load
flattener, depth-guarded) → per-instance worlds + per-skin joint
matrices. **Zero steady-state allocation**: `Pose` owns the sampling
scratch (rest-pose buffer, world array, DFS stack, joint matrix
vectors are cleared and refilled, never dropped) — pinned by a
capacity-stability test over 50 samples. `sample_pose` (rigid
convenience) delegates.

**Skinning** — shipped, not just the floor:
- Extraction: JOINTS_0 VEC4 u8/u16 (widened to u16), WEIGHTS_0 VEC4
  f32 / normalized u8/u16, both-or-neither, counts == POSITION.
  inverseBindMatrices MAT4 f32; ABSENT = identity (spec default).
- Load sanitation (needs skin context): joint index bound by the joint
  list where weight ≠ 0 (exporters pad unused slots with garbage —
  a zero-weight garbage index is fine, a weighted one rejects by
  name); weights finite + non-negative; zero sums reject; >1% drifted
  sums renormalize with one `#FALLBACK` per primitive.
- Vertex stage (`scene.rs`): joint matrices pre-multiplied into VIEW
  space once per instance; per-vertex `blend4` (≤4-joint weighted
  matrix sum) then ONE transform. Skinned vertices ignore the node
  world (glTF: skin overrides node transform). No pose → bind pose
  renders rigidly. Normals through the blended matrix (no
  inverse-transpose; exact under rigid motion, documented approx under
  non-uniform scale).
- Binding lives in `Rig::instance_skins` (parallel array), NOT a new
  `MeshInstance` field — applied the integrator's constructor-breakage
  lesson: `rg 'MeshInstance {' / 'Model {' / 'MeshData {'` before
  shaping, and `MeshData` gained `#[derive(Default)]` so future field
  adds don't break the 9 constructors again.

## 2. Which assets animate

None. Every GLB under `meshvault/frontend/testmodels/` and
`abstract3d/out/` was scanned (JSON chunk): all static. So:

- **Correctness proof**: synthetic fixtures built through the real GLB
  container (`glb_mutate::assemble`) — (a) a two-node TRS-animated
  triangle (LINEAR translation on the root, STEP rotation on the
  child; hierarchy propagation checked against hand-computed positions
  incl. the composed (2,6,0) case), (b) a 2-bone BENDING BAR with skin
  + IBM (`three/skin_tests.rs`): tip vertex (0.2,2,0) swings to
  (−1,1.2,0) at t=1, middle rung blends 50/50 to (0.1,1.1,0), bind
  pose reproduces authored positions exactly, and a raster test pins
  rest ≠ bent ≠ nondeterministic.
- **Scale proof**: 65k-tri skinned sphere, every vertex a non-trivial
  2-joint blend (perf below).

## 3. Hostility (REDTEAM surface)

New named rejections, all tested: out-of-range WEIGHTED joint index;
zero-sum / negative / NaN weights; IBM count < joints; JOINTS_0
without WEIGHTS_0; skin.joints → missing node; node.skin → missing
skin; empty joints; animation channel → missing node (parse);
CUBICSPLINE/weights = labeled skips; decreasing times. Triangle budget
(`MAX_TRIANGLES` 2M) now fires from ACCESSOR METADATA before
extraction allocates — a GLB declaring 2M+ tris against a 4-byte BIN
rejects without touching memory. Both REDTEAM anim suites
(`adv_gltf_anim`, `adv_anim_gltf`) pass; `adv_image` (KittyModel) still
green.

## 4. Viewport playback

`Viewport3D::animate(index, t)`: t is the app's clock signal; the
widget LOOPS it over the clip duration (`rem_euclid`). Play/pause/
speed = app-side signal policy (pause = stop advancing the signal,
speed = scale the delta) — same purity contract as `spin`, so two
builds with the same props paint the same cells. Unknown index /
static model = honest rest pose, no panic. DESIGN's viewer wires
`space=play` by toggling whether its elapsed-time signal advances;
`Model::animations()` tells it whether to offer the binding.
Playback test compares FULL CELLS (glyph+fg+bg) — halfblock paints
solid interiors as ' '+bg, a glyph-only diff is blind (found while
testing; worth knowing for any widget test).

## 5. Mosaic modes

All four modes (HalfBlock/Quadrant/Sextant/Braille) are exposed on
`Viewport3D::mode` + `Image`, and a new widget test renders the cube
through each: nonzero coverage per mode, pairwise-distinct output.
Sextant quality remains pinned by the cycle-4 goldens
(`mosaic_quality_tests.rs`).

## 6. Extras

- `gfx::decode_image(bytes)` / `sniff_format` (`gfx/decode.rs`):
  DESIGN's one-call entry, magic-routed (containers lie, bytes don't),
  unknown formats reject naming what does decode. GLB texture path now
  uses it.
- Emissive (`emissiveFactor`) adds after lighting on all shading
  paths; `normalTexture` presence degrades with a label.
- Smooth normal generation (`compute_smooth_normals`, area-weighted,
  degenerate/NaN-face safe) + `Model::ensure_smooth_normals`.
- `Mat4::to_cols_array` (REDTEAM's suite wanted it — symmetric with
  `from_cols_array`).
- Normal MAPPING skipped per the priority call (non-trivial: tangent
  pipeline); logged in follow-ups.

## 7. Perf (release, this box)

| bench | result |
|---|---|
| helmet 15,452 tris textured 160x96 | **3.11 ms** (pin ≤ 33 ms) |
| helmet plain | 3.42 ms |
| skinned sphere 65k tris: pose sample | **< 0.01 ms** |
| skinned sphere: render | **8.55 ms** |
| same mesh rigid: render | 7.14 ms (skinning delta ≈ +20% worst-case) |
| x-wing 120k tris textured | 17.98 ms (report-only) |

New pin: `perf_three_animated_160x96` (sample ≤ 10 ms, skinned render
≤ 120 ms — regression-catching ceilings, not noise-sensitive).

## 8. Clippy / green

Clippy zero across src/gfx, src/three, widgets/image, widgets/
viewport3d, boot/brandmark3d (fixed this cycle: `+ 0` no-op, useless
`vec!`, `drop` of non-Drop, `% 3 == 0` → `is_multiple_of`, two
type-complexity aliases, byte-grouping sites). Remaining clippy noise
is in `tests/adv_*` (REDTEAM's). `cargo test --lib`: **856 passed / 0
failed**; `cargo test --no-run` compiles the full tree (drift check
done at my end as ordered).

## 9. Risks for REDTEAM

1. `blend4` trusts load-time sanitation for weighted joint indices;
   hand-BUILT `Model`s (no load) only get the `mats.get()` belt —
   attack the direct-construction path.
2. NLERP near-antipodal keyframes (dot ≈ −1) hold-left at the exact
   degenerate midpoint — a 180°-apart keyframe pair sampled at k=0.5.
3. The metadata triangle budget counts indices/positions accessor
   `count` — a mode≠4 primitive rejects later; probe interleavings of
   budget vs mode vs sparse rejections for error-order surprises.

## 10. Requests

None new. The cycle-3 post-present overlay ask (protocol images
through widgets) stays open with REACT/RENDER.
