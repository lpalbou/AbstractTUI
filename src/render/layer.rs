//! Layer: one z-ordered compositor input with geometry, blend mode,
//! color transform and an optional cell shader.
//!
//! Per-cell contribution pipeline (order is contractual, documented in
//! docs/design/render.md §2.2b):
//!
//! ```text
//! src cell -> shader(x, y, shader_t, cell) -> color transform
//!          -> opacity (alpha scale) -> blend (Normal | Additive)
//! ```
//!
//! The shader runs FIRST (it adjusts content); the fixed-function
//! transform and fade then apply to the shader's output, so a fading
//! layer fades whatever its shader produced. Every stage is per-cell,
//! allocation-free and deterministic; all defaults are identity (no
//! shader, `ColorTransform::None`, opacity 1, `Blend::Normal`) and the
//! identity path is byte-identical to the ungraded compositor
//! (test-pinned).

use crate::base::{Rect, Rgba};

use super::cell::Cell;
use super::surface::Surface;

/// How a layer's contribution combines with what is below it.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub enum Blend {
    /// Source-over: opaque content replaces, translucent content veils.
    #[default]
    Normal,
    /// Light accumulation: the contribution's premultiplied color adds
    /// (saturating) onto what is below — black adds nothing, white burns
    /// to full. For glow trails, particles, scanline highlights. A cell
    /// still holds ONE glyph, so an additive glyph replaces the glyph
    /// slot while its light adds; a glyph-less additive cell brightens
    /// both the background and any glyph ink underneath.
    Additive,
}

/// Fixed-function per-layer color grade, applied to every contributed
/// cell's fg/bg/ul after the shader. One transform at a time — fades and
/// disable-states need exactly one, and composing several belongs in a
/// [`CellShader`], not in stacked fixed-function state.
///
/// "Terminal default" colors (alpha 0) pass through every transform
/// untouched: there is no known RGB to grade, and minting one would
/// replace "whatever the theme ground is" with a guess.
#[derive(Copy, Clone, Debug, Default, PartialEq)]
pub enum ColorTransform {
    /// Identity (the default): colors pass through untouched.
    #[default]
    None,
    /// Multiply channels by `f` (clamped 0..=1). 0 = black, 1 = identity.
    /// The trail-decay primitive (DESIGN: ×0.72 per 100 ms).
    Dim(f32),
    /// Lerp channels toward `color` by `strength` (clamped 0..=1).
    /// 0 = identity, 1 = flat `color`. The wash/flash primitive.
    Tint(Rgba, f32),
    /// Lerp toward the WCAG-luma gray by `strength`. 1 = fully grayscale.
    /// The disabled/background-de-emphasis primitive.
    Grayscale(f32),
}

impl ColorTransform {
    fn is_identity(&self) -> bool {
        match *self {
            ColorTransform::None => true,
            ColorTransform::Dim(f) => f >= 1.0,
            ColorTransform::Tint(_, s) | ColorTransform::Grayscale(s) => s <= 0.0,
        }
    }

    /// Applies the transform to one color channel triple (alpha kept —
    /// transparency is opacity's job, color grading never edits coverage).
    fn apply(&self, c: Rgba) -> Rgba {
        if c.is_transparent() {
            return c;
        }
        match *self {
            ColorTransform::None => c,
            ColorTransform::Dim(f) => {
                let f = f.clamp(0.0, 1.0);
                let mul = |v: u8| (v as f32 * f).round() as u8;
                Rgba::new(mul(c.r), mul(c.g), mul(c.b), c.a)
            }
            ColorTransform::Tint(color, strength) => {
                let s = strength.clamp(0.0, 1.0);
                let mixed = c.lerp(color, s);
                Rgba::new(mixed.r, mixed.g, mixed.b, c.a)
            }
            ColorTransform::Grayscale(strength) => {
                let s = strength.clamp(0.0, 1.0);
                let luma =
                    ((2126 * c.r as u32 + 7152 * c.g as u32 + 722 * c.b as u32) / 10000) as u8;
                let gray = Rgba::new(luma, luma, luma, c.a);
                c.lerp(gray, s)
            }
        }
    }
}

/// Cell shader: pure per-cell post-processing over a layer's contribution
/// — the "post-processing shader over the terminal" hook. `x`/`y` are
/// FRAME (screen) coordinates; `t` is the layer's shader clock in seconds
/// (see [`Layer::set_shader_t`]); `cell` is the layer's cell.
///
/// Determinism contract (REDTEAM golden-tests shaders): `shade` must be a
/// pure function of `(x, y, t, cell)` plus construction-time parameters —
/// `&self`, no interior mutability, no randomness beyond seeded hashes of
/// the arguments. Built-ins avoid libm transcendentals for cross-platform
/// bit-stability (see `anim::shaders::wave`).
///
/// Damage/billing rule (charter: idle burns zero CPU): a shader is only
/// re-evaluated where damage exists. An ANIMATED shader is an ANIMATION —
/// the driver advances [`Layer::set_shader_t`] each frame it should move
/// (which damages the layer bounds) and requests frames like any tween;
/// a static shader (t never advanced) costs nothing after its first paint.
pub trait CellShader {
    /// The layer's cell at frame position `(x, y)`, transformed at shader
    /// clock `t`. Must be pure in `(x, y, t, cell)` + construction params
    /// (see the trait docs for the full determinism contract).
    fn shade(&self, x: i32, y: i32, t: f32, cell: Cell) -> Cell;

    /// Active-region hint: the frame-space rect OUTSIDE which this
    /// shader's output is guaranteed STABLE between clocks `t0` and `t1`
    /// — `shade(x, y, t0, c) == shade(x, y, t1, c)` for every cell `c`
    /// at every `(x, y)` not in the returned rect. `bounds` is the
    /// layer's current frame rect (shaders don't hold their layer, and
    /// band/slab effects need an extent to bound against).
    ///
    /// [`Layer::set_shader_t`] uses this to damage only what a clock
    /// advance can visibly change: a settled reveal, or a
    /// [`Vignette`](crate::anim::shaders::Vignette) that ignores `t`
    /// entirely, returns `Some(Rect::ZERO)` and a tick costs NOTHING; a
    /// sweep band damages its swept slab instead of the whole layer. The
    /// contract is deliberately stability, not identity-with-the-source:
    /// a reveal's not-yet-shown cells are transparent (≠ identity) at
    /// both clocks — stable is what damage needs, and it is what
    /// banded/reveal shaders can honestly promise.
    ///
    /// Default `None` = "anything may have changed" (whole layer damages
    /// — the conservative truth for global effects like
    /// [`Shimmer`](crate::anim::shaders::Shimmer)).
    /// Implementations must be conservative: when unsure (a period wrap,
    /// a non-rectangular change set), return `None`, never a too-small
    /// rect — REDTEAM property-tests stability outside the hint.
    fn changed_region(&self, t0: f32, t1: f32, bounds: Rect) -> Option<Rect> {
        let _ = (t0, t1, bounds);
        None
    }
}

/// `Point`-free alias kept for the compositor's plumbing.
pub(crate) type BoxedShader = Box<dyn CellShader>;

/// One z-ordered compositor input: an owned [`Surface`] plus geometry
/// (origin/z), fade state (opacity/visible), and the effect stages
/// (blend/transform/shader). Every `set_*` mutation records exactly the
/// damage it causes — callers never damage by hand.
pub struct Layer {
    surface: Surface,
    origin: crate::base::Point,
    z: i32,
    opacity: f32,
    visible: bool,
    blend: Blend,
    transform: ColorTransform,
    shader: Option<BoxedShader>,
    shader_t: f32,
    /// Geometry damage in frame coordinates, recorded at mutation time.
    frame_damage: Vec<Rect>,
}

impl Layer {
    /// A fully-visible, identity-effect layer over `surface` at `origin`
    /// with z-order `z` (higher z composes later = on top).
    pub fn new(surface: Surface, origin: crate::base::Point, z: i32) -> Layer {
        Layer {
            surface,
            origin,
            z,
            opacity: 1.0,
            visible: true,
            blend: Blend::Normal,
            transform: ColorTransform::None,
            shader: None,
            shader_t: 0.0,
            frame_damage: Vec::new(),
        }
    }

    /// The layer's content, read-only.
    pub fn surface(&self) -> &Surface {
        &self.surface
    }

    /// Content writes damage through the surface itself.
    pub fn surface_mut(&mut self) -> &mut Surface {
        &mut self.surface
    }

    /// Top-left position in frame coordinates.
    pub fn origin(&self) -> crate::base::Point {
        self.origin
    }

    /// Z-order (higher composes on top; ties keep slice order).
    pub fn z(&self) -> i32 {
        self.z
    }

    /// Whole-layer fade, 0..=1 (scales every contributed alpha).
    pub fn opacity(&self) -> f32 {
        self.opacity
    }

    /// Hidden layers contribute nothing (their damage still drains).
    pub fn visible(&self) -> bool {
        self.visible
    }

    /// How this layer combines with what is below it.
    pub fn blend(&self) -> Blend {
        self.blend
    }

    /// The fixed-function grade applied after the shader.
    pub fn color_transform(&self) -> ColorTransform {
        self.transform
    }

    /// The shader clock, in seconds (see [`Layer::set_shader_t`]).
    pub fn shader_t(&self) -> f32 {
        self.shader_t
    }

    /// True when a cell shader is installed.
    pub fn has_shader(&self) -> bool {
        self.shader.is_some()
    }

    /// The layer's rect in frame coordinates (origin + surface size).
    pub fn bounds(&self) -> Rect {
        Rect::new(
            self.origin.x,
            self.origin.y,
            self.surface.width(),
            self.surface.height(),
        )
    }

    /// Moves the layer. Damages old∪new bounds (reveal + paint).
    pub fn set_origin(&mut self, origin: crate::base::Point) {
        if origin == self.origin {
            return;
        }
        // Old position must repaint (reveals what was underneath) and the
        // new position must paint the moved content.
        self.frame_damage.push(self.bounds());
        self.origin = origin;
        self.frame_damage.push(self.bounds());
    }

    /// Restacks the layer; damages its bounds when z actually changes.
    pub fn set_z(&mut self, z: i32) {
        if z != self.z {
            self.z = z;
            self.frame_damage.push(self.bounds());
        }
    }

    /// Fades the layer (clamped 0..=1); no-op sets are free.
    pub fn set_opacity(&mut self, opacity: f32) {
        let opacity = opacity.clamp(0.0, 1.0);
        if (opacity - self.opacity).abs() > f32::EPSILON {
            self.opacity = opacity;
            self.frame_damage.push(self.bounds());
        }
    }

    /// Shows/hides the layer; the flip damages its bounds.
    pub fn set_visible(&mut self, visible: bool) {
        if visible != self.visible {
            self.visible = visible;
            self.frame_damage.push(self.bounds());
        }
    }

    /// Switches the blend mode; damages on change.
    pub fn set_blend(&mut self, blend: Blend) {
        if blend != self.blend {
            self.blend = blend;
            self.frame_damage.push(self.bounds());
        }
    }

    /// Sets the fixed-function grade; damages on change.
    pub fn set_color_transform(&mut self, transform: ColorTransform) {
        if transform != self.transform {
            self.transform = transform;
            self.frame_damage.push(self.bounds());
        }
    }

    /// Installs (or clears) the cell shader. Damages the whole layer —
    /// the shader changes every contributed cell. See [`CellShader`] for
    /// the determinism and animation contract.
    pub fn set_shader(&mut self, shader: Option<BoxedShader>) {
        self.shader = shader;
        self.frame_damage.push(self.bounds());
    }

    /// Advances (or rewinds) the shader clock. Damages what the advance
    /// can visibly change when a shader is installed — driving `t` IS the
    /// animation tick and is billed as one (the caller requests a frame
    /// per advance). The shader's [`CellShader::changed_region`] hint
    /// bounds the bill: a settled reveal or a `t`-independent shader
    /// ticks for free, a moving band damages its slab, and the default
    /// (`None`) keeps the old whole-bounds behavior.
    pub fn set_shader_t(&mut self, t: f32) {
        if (t - self.shader_t).abs() <= f32::EPSILON {
            return;
        }
        let t0 = self.shader_t;
        self.shader_t = t;
        let Some(shader) = self.shader.as_deref() else {
            return;
        };
        let bounds = self.bounds();
        let region = match shader.changed_region(t0, t, bounds) {
            Some(hint) => hint.intersect(bounds),
            None => bounds,
        };
        if !region.is_empty() {
            self.frame_damage.push(region);
        }
    }

    // -- compositor-side hooks (crate-private) ------------------------------

    /// True when this layer contributes damage this frame.
    pub(crate) fn is_dirty(&self) -> bool {
        !self.frame_damage.is_empty() || (self.visible && self.surface.has_damage())
    }

    /// Drains geometry damage into `out` (frame coordinates).
    pub(crate) fn take_frame_damage(&mut self, out: &mut Vec<Rect>) {
        out.append(&mut self.frame_damage);
    }

    /// The layer's contribution at frame position `(x, y)`: the surface
    /// cell run through shader -> color transform. Returns `None` outside
    /// the layer. Opacity/blend are applied by the compositor (they need
    /// the accumulator).
    pub(crate) fn contribution(&self, x: i32, y: i32) -> Option<Cell> {
        let lx = x - self.origin.x;
        let ly = y - self.origin.y;
        let mut cell = *self.surface.get(lx, ly)?;
        if let Some(shader) = self.shader.as_deref() {
            cell = shader.shade(x, y, self.shader_t, cell);
        }
        if !self.transform.is_identity() {
            cell.fg = self.transform.apply(cell.fg);
            cell.bg = self.transform.apply(cell.bg);
            cell.ul = self.transform.apply(cell.ul);
        }
        Some(cell)
    }
}

/// Saturating premultiplied add: `acc + src*src.a` per channel, alpha
/// saturating too. The Additive blend primitive.
pub(crate) fn add_saturating(acc: Rgba, src: Rgba) -> Rgba {
    if src.is_transparent() {
        return acc;
    }
    let pre = |c: u8| ((c as u16 * src.a as u16 + 127) / 255) as u8;
    Rgba::new(
        acc.r.saturating_add(pre(src.r)),
        acc.g.saturating_add(pre(src.g)),
        acc.b.saturating_add(pre(src.b)),
        acc.a.saturating_add(src.a),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::base::{Point, Size};

    #[test]
    fn transform_setters_record_damage_and_noop_sets_are_free() {
        let mut l = Layer::new(Surface::new(Size::new(4, 2), Cell::EMPTY), Point::ZERO, 0);
        let mut sink = Vec::new();
        l.take_frame_damage(&mut sink);
        sink.clear();

        l.set_color_transform(ColorTransform::Dim(0.5));
        l.set_blend(Blend::Additive);
        l.take_frame_damage(&mut sink);
        assert_eq!(sink.len(), 2, "each grade change damages the bounds");

        sink.clear();
        l.set_color_transform(ColorTransform::Dim(0.5));
        l.set_blend(Blend::Additive);
        l.take_frame_damage(&mut sink);
        assert!(sink.is_empty(), "no-op sets are free");
    }

    #[test]
    fn shader_clock_damages_only_with_a_shader_installed() {
        struct Id;
        impl CellShader for Id {
            fn shade(&self, _x: i32, _y: i32, _t: f32, cell: Cell) -> Cell {
                cell
            }
        }
        let mut l = Layer::new(Surface::new(Size::new(4, 2), Cell::EMPTY), Point::ZERO, 0);
        let mut sink = Vec::new();
        l.take_frame_damage(&mut sink);
        sink.clear();

        l.set_shader_t(0.5);
        l.take_frame_damage(&mut sink);
        assert!(sink.is_empty(), "no shader, no animation, no damage");

        l.set_shader(Some(Box::new(Id)));
        l.set_shader_t(1.0);
        l.take_frame_damage(&mut sink);
        assert_eq!(sink.len(), 2, "install + tick each damage the bounds");
    }

    #[test]
    fn dim_tint_grayscale_math() {
        let c = Rgba::rgb(200, 100, 50);
        assert_eq!(ColorTransform::None.apply(c), c);
        let dim = ColorTransform::Dim(0.5).apply(c);
        assert_eq!((dim.r, dim.g, dim.b), (100, 50, 25));

        let full_tint = ColorTransform::Tint(Rgba::rgb(0, 0, 255), 1.0).apply(c);
        assert_eq!((full_tint.r, full_tint.g, full_tint.b), (0, 0, 255));
        let half_tint = ColorTransform::Tint(Rgba::rgb(0, 0, 255), 0.5).apply(c);
        assert!(half_tint.b > c.b && half_tint.r < c.r);

        let gray = ColorTransform::Grayscale(1.0).apply(c);
        assert_eq!(gray.r, gray.g);
        assert_eq!(gray.g, gray.b);

        // Terminal-default (alpha 0) passes through untouched.
        assert_eq!(
            ColorTransform::Dim(0.1).apply(Rgba::TRANSPARENT),
            Rgba::TRANSPARENT
        );
        // Alpha is preserved by every transform (coverage is opacity's job).
        let translucent = Rgba::new(100, 100, 100, 128);
        assert_eq!(ColorTransform::Grayscale(1.0).apply(translucent).a, 128);
    }

    #[test]
    fn additive_primitive_saturates_and_premultiplies() {
        let acc = Rgba::rgb(200, 200, 200);
        let out = add_saturating(acc, Rgba::rgb(100, 10, 0));
        assert_eq!((out.r, out.g, out.b), (255, 210, 200));
        let dim = add_saturating(Rgba::rgb(0, 0, 0), Rgba::new(100, 100, 100, 128));
        assert!((dim.r as i32 - 50).abs() <= 1, "{dim:?}");
        assert_eq!(add_saturating(acc, Rgba::TRANSPARENT), acc);
    }
}
