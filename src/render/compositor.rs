//! Layered compositor: z-ordered surfaces flatten into one frame with
//! alpha blending, driven entirely by damage.
//!
//! Two damage sources feed one list (frame coordinates):
//! - content damage: each layer's surface tracks its own writes; flatten
//!   drains it and translates by the layer's *current* origin;
//! - geometry damage: `Layer::set_*` mutations record the layer's old and
//!   new frame bounds at mutation time, so a moved layer repaints where it
//!   was and where it now is without any coordinate skew.
//!
//! Blending model (bottom-up per cell, docs/design/render.md §2.2):
//! - Each layer's contribution is its surface cell run through the layer's
//!   shader -> desaturate -> tint (see `layer.rs`), then opacity-scaled.
//! - `Blend::Normal`: `Glyph::EMPTY` is see-through; a space is opaque
//!   content that erases. An opaque upper background replaces the
//!   accumulator outright; a translucent one composites over it and
//!   *veils* an inherited lower glyph. Fg/ul pre-blend against the final
//!   bg so the presenter only sees concrete colors (alpha 0 = terminal
//!   default, by convention).
//! - `Blend::Additive`: the contribution's premultiplied color ADDS
//!   (saturating). A glyph-less additive cell is a light wash: it
//!   brightens the accumulated bg AND any kept glyph ink. A glyph-bearing
//!   additive cell takes the glyph slot (a cell holds one glyph) and its
//!   ink renders as light over the accumulated background. Adding onto a
//!   "terminal default" (alpha-0) accumulator treats the unknown color as
//!   black — the only honest choice without knowing the theme ground.
//!
//! Wide-glyph safety: every damage rect is expanded ±1 column before
//! compositing so a leader/continuation pair is never split by a rect
//! edge, and a repair pass re-mirrors pairs after blending — a wide glyph
//! renders with its LEADER's final style (a shader cannot split-style a
//! pair; docs/design/render.md §2.2b).

use crate::base::Rect;

use crate::base::Rgba;

use super::cell::Cell;
use super::layer::{add_saturating, Blend, Layer};
use super::surface::Surface;

/// Flattens layers into a frame surface. Owns scratch buffers so
/// steady-state flattening does not allocate.
///
/// ```
/// use abstracttui::base::{Point, Rgba, Size};
/// use abstracttui::render::{Cell, Compositor, Layer, Style, Surface};
///
/// let size = Size::new(40, 10);
/// let mut root = Surface::new(size, Cell::EMPTY);
/// root.draw_text(0, 0, "underneath", Style::new());
/// let mut toast = Surface::new(Size::new(9, 1), Cell::EMPTY);
/// toast.draw_text(0, 0, "saved ✓", Style::new().bg(Rgba::rgb(20, 80, 20)));
///
/// let mut layers = vec![
///     Layer::new(root, Point::ZERO, 0),
///     Layer::new(toast, Point::new(2, 0), 10), // higher z wins
/// ];
/// let mut comp = Compositor::new();
/// let mut frame = Surface::new(size, Cell::EMPTY);
/// let damage = comp.flatten(&mut frame, &mut layers);
/// assert!(!damage.is_empty()); // first frame: everything repaints
/// // Nothing changed since: the next flatten is free (idle costs zero).
/// assert!(comp.flatten(&mut frame, &mut layers).is_empty());
/// ```
pub struct Compositor {
    /// Above this many rects the damage list collapses to their union.
    max_rects: usize,
    /// Union coverage (vs frame area) beyond which damage degrades to
    /// full-frame: past that point per-rect bookkeeping costs more than a
    /// straight full scan.
    full_frame_ratio: f64,
    /// Theme ground: what "terminal default background" (alpha 0) means
    /// when a layer must BLEND against it (additive light, translucent
    /// veils). `None` = legacy behavior (translucent bgs stay translucent,
    /// additive adds onto black) — byte-identical to pre-ground output.
    /// The app wires the active theme's bg here and damages all on theme
    /// switch (contract §5).
    ground: Option<Rgba>,
    /// Damage visualizer (diagnostics): tint the perimeter of every
    /// damage rect after composing, so repaint regions are VISIBLE.
    debug_damage: bool,
    damage: Vec<Rect>,
    gather: Vec<Rect>,
    order: Vec<usize>,
}

impl Default for Compositor {
    fn default() -> Self {
        Compositor::new()
    }
}

impl Compositor {
    /// A compositor with default coalescing thresholds and no ground.
    pub fn new() -> Compositor {
        Compositor {
            max_rects: 16,
            full_frame_ratio: 0.7,
            ground: None,
            debug_damage: false,
            damage: Vec::new(),
            gather: Vec::new(),
            order: Vec::new(),
        }
    }

    /// Damage visualizer toggle (diagnostics; docs/design/render.md
    /// §2.2f). When ON, every composed frame outlines its damage rects in
    /// a magenta bg tint — devs and REDTEAM can SEE what repainted, which
    /// is the minimal-damage claim made visible. The tint is real frame
    /// content (the diff emits it like anything else), so bytes/pixels
    /// change while enabled — a diagnostic mode, never for production
    /// paths or byte-golden tests. Runtime-switchable: toggling damages
    /// nothing by itself; the next damaged region shows/loses outlines.
    pub fn set_debug_damage(&mut self, on: bool) {
        self.debug_damage = on;
    }

    /// Whether the damage visualizer is on.
    pub fn debug_damage(&self) -> bool {
        self.debug_damage
    }

    /// Sets the compositing ground (the theme's background color) used
    /// wherever a layer blends against terminal-default cells. `None`
    /// restores the legacy assume-nothing behavior. The caller owns the
    /// repaint: changing the ground mid-session requires `damage_all` on
    /// the root layer (a theme switch already does — contract §5).
    pub fn set_ground(&mut self, ground: Option<Rgba>) {
        self.ground = ground;
    }

    /// The declared compositing ground, if any.
    pub fn ground(&self) -> Option<Rgba> {
        self.ground
    }

    /// Composes all visible layers into `frame` within the union of all
    /// pending damage. Returns the coalesced damage list (frame
    /// coordinates) — exactly what `FrameDiff::compute` expects.
    ///
    /// `frame` is the compositor's own back buffer; callers keep it
    /// between frames (its size defines the viewport) and never draw into
    /// it directly. Shader time is per-layer state (`Layer::set_shader_t`),
    /// driven by the app clock, not by flatten — replaying the same layer
    /// state always composes the same frame.
    pub fn flatten<'a>(&'a mut self, frame: &mut Surface, layers: &mut [Layer]) -> &'a [Rect] {
        let bounds = Rect::from_size(frame.size());
        self.gather.clear();
        for layer in layers.iter_mut() {
            Self::gather_layer_damage(layer, &mut self.gather);
        }
        self.finish_damage(bounds);
        if self.damage.is_empty() {
            return &self.damage;
        }
        self.sort_layers(layers);
        // Compose each damage rect. `order`/`damage` are only mutated in
        // collect/sort above, so index-walking here keeps borrows simple.
        for di in 0..self.damage.len() {
            let rect = self.damage[di];
            for y in rect.y..rect.bottom() {
                for x in rect.x..rect.right() {
                    let (composed, owner) = self.compose_cell(layers, x, y);
                    // Pooled glyph / link ids are owner-local; re-intern
                    // into the frame's pool and URI table.
                    let composed = match owner {
                        Some(li) if composed.glyph.is_pooled() || composed.link != 0 => {
                            frame.adopt_from(composed, layers[li].surface())
                        }
                        _ => composed,
                    };
                    frame.put_composed(x, y, composed);
                }
                frame.repair_wide_pairs(y, rect.x, rect.right());
            }
        }
        if self.debug_damage {
            for di in 0..self.damage.len() {
                outline_damage(frame, self.damage[di]);
            }
        }
        &self.damage
    }

    /// Bottom-up blend of every visible layer covering `(x, y)`. Returns
    /// the composed cell plus the index of the layer owning its glyph/link
    /// (ids are surface-local and need adoption into the frame).
    fn compose_cell(&self, layers: &[Layer], x: i32, y: i32) -> (Cell, Option<usize>) {
        let mut acc = Cell::EMPTY;
        let mut owner: Option<usize> = None;
        for &li in &self.order {
            let layer = &layers[li];
            let blend = layer.blend();
            let op = layer.opacity();
            let Some(src) = layer.contribution(x, y) else {
                continue;
            };
            let bg = scale_alpha(src.bg, op);
            // Continuations count as glyph content: they are the trailing
            // half of the layer's wide glyph and must shadow lower content
            // exactly like their leader does.
            let has_glyph = !src.glyph.is_empty();

            if !has_glyph && bg.is_transparent() {
                continue; // fully see-through cell
            }

            match blend {
                Blend::Normal => {
                    if bg.is_opaque() {
                        acc.bg = bg;
                        if has_glyph {
                            acc.glyph = src.glyph;
                            acc.fg = blend_fg(scale_alpha(src.fg, op), bg);
                            acc.ul = blend_fg(scale_alpha(src.ul, op), bg);
                            acc.attrs = src.attrs;
                            acc.link = src.link;
                            owner = Some(li);
                        } else {
                            // Opaque empty cell: erases everything below.
                            acc.glyph = super::cell::Glyph::EMPTY;
                            acc.fg = Cell::EMPTY.fg;
                            acc.ul = Cell::EMPTY.ul;
                            acc.attrs = Cell::EMPTY.attrs;
                            acc.link = 0;
                            owner = None;
                        }
                        continue;
                    }
                    // Translucent background: composite over what is
                    // below — with the theme ground standing in for
                    // "terminal default" when the app declared one — and
                    // veil any glyph the accumulator keeps (underline ink
                    // veils like fg). A fully transparent bg blends
                    // NOTHING: it must not materialize the ground (glyph
                    // cells with default bg stay terminal-default).
                    if !bg.is_transparent() {
                        acc.bg = bg.over(self.grounded(acc.bg));
                    }
                    if has_glyph {
                        acc.glyph = src.glyph;
                        acc.fg = blend_fg(scale_alpha(src.fg, op), acc.bg);
                        acc.ul = blend_fg(scale_alpha(src.ul, op), acc.bg);
                        acc.attrs = src.attrs;
                        acc.link = src.link;
                        owner = Some(li);
                    } else if !acc.glyph.is_empty() && !bg.is_transparent() {
                        acc.fg = bg.over(acc.fg);
                        if !acc.ul.is_transparent() {
                            acc.ul = bg.over(acc.ul);
                        }
                    }
                }
                Blend::Additive => {
                    // Same no-materialization rule as the Normal branch:
                    // zero light leaves a terminal-default ground alone.
                    if !bg.is_transparent() {
                        acc.bg = add_saturating(self.grounded(acc.bg), bg);
                    }
                    if has_glyph {
                        // The glyph slot is exclusive; the additive glyph's
                        // ink renders as light on the lit ground.
                        acc.glyph = src.glyph;
                        acc.fg = add_saturating(acc.bg, scale_alpha(src.fg, op));
                        acc.ul = add_saturating(acc.bg, scale_alpha(src.ul, op));
                        acc.attrs = src.attrs;
                        acc.link = src.link;
                        owner = Some(li);
                    } else {
                        // Pure light wash: brightens kept glyph ink too.
                        if !acc.glyph.is_empty() {
                            acc.fg = add_saturating(acc.fg, bg);
                            if !acc.ul.is_transparent() {
                                acc.ul = add_saturating(acc.ul, bg);
                            }
                        }
                    }
                }
            }
        }
        (acc, owner)
    }

    /// Substitutes the declared theme ground for a terminal-default
    /// accumulator background AT BLEND TIME only. Cells nothing blends
    /// against keep alpha 0 and still present as SGR 49 — the ground
    /// never leaks into untouched content.
    fn grounded(&self, acc_bg: Rgba) -> Rgba {
        match self.ground {
            Some(g) if acc_bg.is_transparent() => g,
            _ => acc_bg,
        }
    }

    /// Drains one layer's damage into `gather`: geometry damage is
    /// already frame-space; content damage translates by the current
    /// origin. Hidden layers still drain (cheap) but contribute nothing —
    /// their reveal was damaged by `set_visible`.
    fn gather_layer_damage(layer: &mut Layer, gather: &mut Vec<Rect>) {
        layer.take_frame_damage(gather);
        let origin = layer.origin();
        let visible = layer.visible();
        let start = gather.len();
        layer.surface_mut().take_damage(gather);
        if !visible {
            gather.truncate(start);
        } else {
            for r in &mut gather[start..] {
                *r = r.translate(origin.x, origin.y);
            }
        }
    }

    /// Clips gathered damage, expands ±1 column (wide pairs), coalesces
    /// and caps into `self.damage`.
    fn finish_damage(&mut self, bounds: Rect) {
        self.damage.clear();
        for i in 0..self.gather.len() {
            let r = self.gather[i];
            let r = Rect::new(r.x - 1, r.y, r.w + 2, r.h).intersect(bounds);
            if r.is_empty() {
                continue;
            }
            Self::push_coalesced(&mut self.damage, r);
        }

        if self.damage.len() > self.max_rects {
            let union = self.damage.drain(..).fold(Rect::ZERO, Rect::union);
            self.damage.push(union);
        }
        if let [only] = self.damage[..] {
            let frame_area = bounds.area() as f64;
            if frame_area > 0.0 && only.area() as f64 / frame_area >= self.full_frame_ratio {
                self.damage[0] = bounds;
            }
        }
    }

    /// Inserts `rect`, merging with any existing rect it intersects (or
    /// duplicates). Quadratic in the worst case but the list is capped and
    /// short; precision matters more than asymptotics at n ≤ 16.
    fn push_coalesced(list: &mut Vec<Rect>, rect: Rect) {
        let mut rect = rect;
        let mut i = 0;
        while i < list.len() {
            let other = list[i];
            if rect.intersects(other) || contains_rect(other, rect) {
                rect = rect.union(other);
                list.swap_remove(i);
                // Restart: the grown rect may now touch earlier entries.
                i = 0;
            } else {
                i += 1;
            }
        }
        list.push(rect);
    }

    /// Stable insertion sort of layer indices by z (ties keep slice order,
    /// bottom first). Insertion sort: allocation-free, stable, and layer
    /// counts are small.
    fn sort_layers(&mut self, layers: &[Layer]) {
        self.order.clear();
        for (i, layer) in layers.iter().enumerate() {
            if !layer.visible() {
                continue;
            }
            let mut pos = self.order.len();
            while pos > 0 && layers[self.order[pos - 1]].z() > layer.z() {
                pos -= 1;
            }
            self.order.insert(pos, i);
        }
    }

    /// True when any layer or surface has pending damage — lets the app
    /// skip flatten/diff/present entirely on idle frames.
    pub fn any_dirty(layers: &[Layer]) -> bool {
        layers.iter().any(Layer::is_dirty)
    }
}

fn contains_rect(a: Rect, b: Rect) -> bool {
    a.intersect(b) == b
}

/// Tints the perimeter cells of `rect` toward magenta (bg 50% blend) —
/// the damage visualizer's outline. Wide pairs stay consistent: tinting
/// writes to leaders/narrow cells and mirrors onto continuations via the
/// pair repair, exactly like any composed write.
fn outline_damage(frame: &mut Surface, rect: Rect) {
    const DEBUG_PINK: Rgba = Rgba::new(255, 0, 200, 255);
    let r = rect.intersect(frame.bounds());
    if r.is_empty() {
        return;
    }
    let tint = |x: i32, y: i32, frame: &mut Surface| {
        let Some(&cell) = frame.get(x, y) else { return };
        if cell.is_continuation() {
            return; // the leader's tint mirrors over
        }
        let bg = cell.bg.lerp(DEBUG_PINK, 0.5);
        frame.put_composed(x, y, Cell { bg, ..cell });
    };
    for x in r.x..r.right() {
        tint(x, r.y, frame);
        tint(x, r.bottom() - 1, frame);
    }
    for y in r.y..r.bottom() {
        tint(r.x, y, frame);
        tint(r.right() - 1, y, frame);
    }
    for y in r.y..r.bottom() {
        frame.repair_wide_pairs(y, r.x, r.right());
    }
}

fn scale_alpha(c: crate::base::Rgba, opacity: f32) -> crate::base::Rgba {
    if opacity >= 1.0 {
        return c;
    }
    let a = (c.a as f32 * opacity.clamp(0.0, 1.0)).round() as u8;
    c.with_alpha(a)
}

/// Pre-blends a foreground against its final background so downstream
/// stages only handle concrete colors. A fully transparent fg stays
/// transparent ("terminal default foreground").
fn blend_fg(fg: crate::base::Rgba, bg: crate::base::Rgba) -> crate::base::Rgba {
    if fg.is_opaque() || fg.is_transparent() {
        fg
    } else {
        fg.over(bg)
    }
}

#[cfg(test)]
#[path = "compositor_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "profile_tests.rs"]
mod profile;
