//! Image: bitmap display through the gfx mosaic pipeline.
//!
//! ```no_run
//! use abstracttui::gfx::MosaicMode;
//! use abstracttui::theme::default_theme;
//! use abstracttui::widgets::{Image, ImageFit};
//!
//! let theme = default_theme();
//! let img = Image::from_path("logo.png")
//!     .fit(ImageFit::Contain)
//!     .mode(MosaicMode::HalfBlock)
//!     .element(&theme.tokens);
//! ```
//!
//! (`element` takes only `&TokenSet` — unlike stateful widgets there
//! is no `Scope` parameter, because an image holds no reactive state;
//! see RT8-3.) `Image::from_bitmap` takes `Arc<gfx::Bitmap>` —
//! re-exported here as [`Bitmap`] so the type is one import away.
//!
//! ## The protocol-path seam (honest version)
//!
//! This widget always renders UNICODE MOSAIC cells. Kitty/iTerm2/sixel
//! payloads are byte streams that must reach the terminal through
//! `Presenter::external_write` AFTER the cell diff of a frame (damage
//! contract §6); a draw closure only owns a `StyledCanvas` — cells —
//! and cannot (and must not) smuggle escape bytes into a surface. The
//! pixel-protocol path therefore lives at the APP level:
//! `gfx::pipeline::present_image` renders through the capability
//! ladder and hands protocol bytes to any `ExternalSink` (RENDER's
//! presenter adapts trivially); mosaic falls out as cell patches. Full
//! widget-protocol integration needs a post-present overlay pass owned
//! by the frame loop — filed for cycle 6 in
//! `reviews/cycle3/gfx3d-requests.md` with the exact App support
//! required (placement rect reporting + overlay lifecycle + deletes).
//!
//! Tokens: only `text_faint` (the broken-image label) — the image
//! itself is content, not chrome.
//!
//! OWNER: GFX3D.

use std::sync::Arc;

use crate::base::{Point, Rect, Rgba};
use crate::gfx::decode_image;
use crate::gfx::mosaic::{MosaicMode, MosaicRenderer};
// RT8-4: `Image::from_bitmap` takes `Arc<Bitmap>` — the type is
// re-exported here so image apps find it beside the widget.
pub use crate::gfx::Bitmap;
use crate::layout::Style as LayoutStyle;
use crate::theme::TokenSet;
use crate::ui::{Element, StyledCanvas};

/// How the image maps into the widget rect. Aspect math treats a
/// terminal cell as 1:2 (width:height) — the ubiquitous monospace
/// geometry; when KERNEL's `cell_pixel_size` reaches the widget layer
/// (cycle 6 seam) the real ratio replaces the assumption.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ImageFit {
    /// Largest size that fits entirely, aspect preserved (letterbox).
    Contain,
    /// Fill the rect, aspect preserved, source cropped (center-out).
    Cover,
    /// Fill the rect exactly, aspect ignored (stretch).
    Fill,
    /// One source pixel per mosaic subpixel, clipped to the rect.
    None,
}

/// Per-axis alignment of the fitted image inside the widget rect.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ImageAlign {
    Start,
    Center,
    End,
}

impl ImageAlign {
    fn offset(self, avail: i32, used: i32) -> i32 {
        match self {
            ImageAlign::Start => 0,
            ImageAlign::Center => (avail - used).max(0) / 2,
            ImageAlign::End => (avail - used).max(0),
        }
    }
}

pub struct Image {
    source: Result<Arc<Bitmap>, String>,
    fit: ImageFit,
    mode: MosaicMode,
    align_h: ImageAlign,
    align_v: ImageAlign,
    layout: LayoutStyle,
}

impl Image {
    /// Display an already-decoded bitmap (shared handle: cheap clones,
    /// no copies per rebuild). `Bitmap` is re-exported from `widgets`
    /// and the prelude (RT8-4):
    ///
    /// ```no_run
    /// use std::sync::Arc;
    /// use abstracttui::prelude::*;
    ///
    /// // Decoded, generated, or captured — any RGBA bitmap works.
    /// let pixels = Arc::new(Bitmap::new(64, 48, Rgba::TRANSPARENT));
    /// let image = Image::from_bitmap(pixels);
    /// ```
    pub fn from_bitmap(bitmap: Arc<Bitmap>) -> Image {
        Image::new(if bitmap.is_empty() {
            Err("empty bitmap".to_string())
        } else {
            Ok(bitmap)
        })
    }

    /// Decode an image file at view-build time. PNG + baseline JPEG
    /// (the engine's decoders, magic-routed — widened from PNG-only in
    /// the 0144 wave); other formats produce the labeled broken-image
    /// state, never a panic and never a silent blank.
    pub fn from_path(path: impl AsRef<std::path::Path>) -> Image {
        let path = path.as_ref();
        let source = match std::fs::read(path) {
            Err(e) => Err(format!("unreadable: {e}")),
            Ok(bytes) => match decode_image(&bytes) {
                Ok(bmp) => Ok(Arc::new(bmp)),
                Err(e) => Err(format!("undecodable: {e}")),
            },
        };
        Image::new(source)
    }

    fn new(source: Result<Arc<Bitmap>, String>) -> Image {
        Image {
            source,
            fit: ImageFit::Contain,
            mode: MosaicMode::HalfBlock,
            align_h: ImageAlign::Center,
            align_v: ImageAlign::Center,
            layout: LayoutStyle::default(),
        }
    }

    pub fn fit(mut self, fit: ImageFit) -> Image {
        self.fit = fit;
        self
    }

    /// Mosaic glyph family override (default half blocks — exact colors,
    /// universal fonts).
    pub fn mode(mut self, mode: MosaicMode) -> Image {
        self.mode = mode;
        self
    }

    pub fn align(mut self, horizontal: ImageAlign, vertical: ImageAlign) -> Image {
        self.align_h = horizontal;
        self.align_v = vertical;
        self
    }

    pub fn layout(mut self, style: LayoutStyle) -> Image {
        self.layout = style;
        self
    }

    /// The decode error, if the source is broken (test/diagnostic
    /// surface; the draw closure shows the labeled state).
    pub fn error(&self) -> Option<&str> {
        self.source.as_ref().err().map(String::as_str)
    }

    /// Canonical one-call build (RT8-3 uniformity): same shape as the
    /// interactive widgets — tokens resolve from the app's theme
    /// context, the finished `View` comes back. `element(&tokens)`
    /// remains the explicit-theming door.
    pub fn view(self, cx: crate::reactive::Scope) -> crate::ui::View {
        let t = crate::widgets::theme_tokens(cx);
        self.element(&t).build()
    }

    /// Native cell footprint of `source` through `mode`'s subpixel
    /// density — the CSS "natural size" analog and the widget's
    /// intrinsic answer (one source pixel per mosaic subpixel, so the
    /// aspect stays correct in cell space). Broken sources answer the
    /// labeled state's footprint ("⌧ image" + one message row) so the
    /// label survives `Auto` contexts instead of collapsing away.
    fn natural_cells(source: &Result<Arc<Bitmap>, String>, mode: MosaicMode) -> crate::base::Size {
        match source {
            Ok(bitmap) => {
                let (subw, subh) = mode.cell_pixels();
                crate::base::Size::new(
                    (bitmap.width().div_ceil(subw) as i32).max(1),
                    (bitmap.height().div_ceil(subh) as i32).max(1),
                )
            }
            Err(_) => crate::base::Size::new(7, 2), // "⌧ image" + message
        }
    }

    pub fn element(self, t: &TokenSet) -> Element {
        let faint = t.text_faint;
        let fit = self.fit;
        let mode = self.mode;
        let (ah, av) = (self.align_h, self.align_v);
        // Intrinsic size (RT8-6 collapse fix): a draw-only element
        // answers ZERO to `Auto` sizing, so images in unsized rows or
        // content-sized panels used to vanish entirely. The widget now
        // measures as its natural cell footprint — flex grow/shrink and
        // the fit modes reconcile it with the rect actually granted,
        // exactly as they would for an overlong text leaf.
        let natural = Self::natural_cells(&self.source, mode);
        let source = self.source;
        // FnMut state: the mosaic renderer's buffers persist across
        // draws (steady-state repaints allocate nothing new).
        let mut renderer = MosaicRenderer::new();
        Element::new()
            .style(self.layout)
            .measure(move |_avail| natural)
            .draw(move |canvas, rect| {
                if rect.w <= 0 || rect.h <= 0 {
                    return;
                }
                let bitmap = match &source {
                    Ok(b) => b,
                    Err(label) => {
                        // Labeled broken state: never blank, never fake.
                        canvas.print(rect.origin(), "⌧ image", faint, Rgba::TRANSPARENT);
                        let msg: String = label.chars().take(rect.w.max(0) as usize).collect();
                        if rect.h > 1 {
                            canvas.print(
                                Point::new(rect.x, rect.y + 1),
                                &msg,
                                faint,
                                Rgba::TRANSPARENT,
                            );
                        }
                        return;
                    }
                };
                draw_fitted(canvas, rect, bitmap, fit, mode, ah, av, &mut renderer);
            })
    }
}

/// Fit resolution: (target cell rect, source crop). Pure — unit-tested
/// directly, the draw closure is a thin shell around it.
fn resolve_fit(
    rect: Rect,
    iw: u32,
    ih: u32,
    fit: ImageFit,
    mode: MosaicMode,
    ah: ImageAlign,
    av: ImageAlign,
) -> (Rect, Option<(u32, u32, u32, u32)>) {
    // Total on hostile inputs: a degenerate rect or an empty source
    // resolves to an empty target (the draw shell skips it) instead of
    // panicking inside clamp/division. Any rect >= 1x1 with a real
    // source yields at least a 1x1 target — the fit never rounds a
    // visible pane down to nothing.
    if rect.w <= 0 || rect.h <= 0 || iw == 0 || ih == 0 {
        return (Rect::new(rect.x, rect.y, 0, 0), None);
    }
    // Physical aspect: a cell is 1 unit wide, 2 tall.
    let img_aspect = iw as f32 / ih as f32; // square-pixel image
    let (subw, subh) = mode.cell_pixels();
    match fit {
        ImageFit::Fill => (rect, None),
        ImageFit::Contain => {
            let box_w = rect.w as f32; // physical units
            let box_h = rect.h as f32 * 2.0;
            let (used_w, used_h) = if box_w / box_h > img_aspect {
                (box_h * img_aspect, box_h)
            } else {
                (box_w, box_w / img_aspect)
            };
            let cw = (used_w.round() as i32).clamp(1, rect.w);
            let ch = ((used_h / 2.0).round() as i32).clamp(1, rect.h);
            let target = Rect::new(
                rect.x + ah.offset(rect.w, cw),
                rect.y + av.offset(rect.h, ch),
                cw,
                ch,
            );
            (target, None)
        }
        ImageFit::Cover => {
            let box_aspect = rect.w as f32 / (rect.h as f32 * 2.0);
            let (cw, chh) = if img_aspect > box_aspect {
                // Wider than the box: crop width.
                (((ih as f32) * box_aspect).round().max(1.0) as u32, ih)
            } else {
                (iw, ((iw as f32) / box_aspect).round().max(1.0) as u32)
            };
            let cx = ah.offset(iw as i32, cw as i32) as u32;
            let cy = av.offset(ih as i32, chh as i32) as u32;
            (rect, Some((cx, cy, cw, chh)))
        }
        ImageFit::None => {
            // Native: one source px per subpixel; clip to the rect.
            let cw = (iw.div_ceil(subw) as i32).min(rect.w).max(1);
            let ch = (ih.div_ceil(subh) as i32).min(rect.h).max(1);
            let target = Rect::new(
                rect.x + ah.offset(rect.w, cw),
                rect.y + av.offset(rect.h, ch),
                cw,
                ch,
            );
            // Crop the source to what the clipped target can show.
            let crop = (0, 0, (cw as u32) * subw, (ch as u32) * subh);
            (target, Some(crop))
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_fitted(
    canvas: &mut dyn StyledCanvas,
    rect: Rect,
    bitmap: &Bitmap,
    fit: ImageFit,
    mode: MosaicMode,
    ah: ImageAlign,
    av: ImageAlign,
    renderer: &mut MosaicRenderer,
) {
    let (target, crop) = resolve_fit(rect, bitmap.width(), bitmap.height(), fit, mode, ah, av);
    if target.is_empty() {
        return;
    }
    // Cropping allocates a sub-bitmap; only Cover/None pay it, and only
    // on damage repaints (not per frame — draw runs on damage).
    let grid = match crop {
        None => renderer.render(bitmap, target.w as u32, target.h as u32, mode),
        Some((x, y, w, h)) => {
            let sub = bitmap.crop(x, y, w, h);
            renderer.render(&sub, target.w as u32, target.h as u32, mode)
        }
    };
    for (pos, ch, fg, bg) in grid.cell_patches(target.origin()) {
        canvas.put(pos, ch, fg, bg);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::base::Size;
    use crate::theme::default_theme;
    use crate::widgets::test_util::draw_into;

    fn stripes() -> Arc<Bitmap> {
        // 8x8: left half white, right half black (token-rule-safe
        // constants; the lint below this directory forbids color
        // construction even in tests).
        Arc::new(Bitmap::from_fn(8, 8, |x, _| {
            if x < 4 {
                Rgba::WHITE
            } else {
                Rgba::BLACK
            }
        }))
    }

    #[test]
    fn contain_letterboxes_square_image_in_wide_rect() {
        // 8x8 image (physical aspect 1.0) in a 20x4 rect (physical
        // 20x8 = 2.5): height-bound -> 8 physical wide = 8 cells, 4
        // rows; centered horizontally at x = (20-8)/2 = 6.
        let (target, crop) = resolve_fit(
            Rect::new(0, 0, 20, 4),
            8,
            8,
            ImageFit::Contain,
            MosaicMode::HalfBlock,
            ImageAlign::Center,
            ImageAlign::Center,
        );
        assert_eq!(target, Rect::new(6, 0, 8, 4));
        assert!(crop.is_none());
    }

    #[test]
    fn cover_crops_the_long_axis() {
        // Square image into a wide box: cover must crop VERTICALLY
        // (keep width, cut height to box aspect 20/(4*2)=2.5 -> crop
        // height = 8/2.5 rounded).
        let (target, crop) = resolve_fit(
            Rect::new(0, 0, 20, 4),
            8,
            8,
            ImageFit::Cover,
            MosaicMode::HalfBlock,
            ImageAlign::Center,
            ImageAlign::Center,
        );
        assert_eq!(target, Rect::new(0, 0, 20, 4));
        let (_, cy, cw, ch) = crop.unwrap();
        assert_eq!(cw, 8, "full width kept");
        assert!(ch < 8, "height cropped: {ch}");
        assert!(cy > 0, "center crop offset");
    }

    #[test]
    fn none_is_native_size_clipped() {
        let (target, crop) = resolve_fit(
            Rect::new(2, 1, 3, 2),
            8,
            8,
            ImageFit::None,
            MosaicMode::HalfBlock, // 1x2 px per cell -> native 8x4 cells
            ImageAlign::Start,
            ImageAlign::Start,
        );
        assert_eq!(target, Rect::new(2, 1, 3, 2), "clipped to rect");
        assert_eq!(crop.unwrap(), (0, 0, 3, 4), "source crop matches clip");
    }

    #[test]
    fn draws_mosaic_cells_into_canvas() {
        let t = default_theme().tokens;
        let el = Image::from_bitmap(stripes())
            .fit(ImageFit::Fill)
            .element(&t);
        let c = draw_into(el, Size::new(8, 4));
        // Left cells white-ish, right cells black-ish (half-block bg/fg
        // both carry the stripe color; uniform cells emit space + bg).
        let left = c.cell(Point::new(1, 1)).unwrap();
        let right = c.cell(Point::new(6, 1)).unwrap();
        assert!(left.2.r > 200, "{left:?}");
        assert!(right.2.r < 60, "{right:?}");
    }

    #[test]
    fn broken_source_shows_labeled_state() {
        let t = default_theme().tokens;
        let img = Image::from_path("/nonexistent/definitely/missing.png");
        assert!(img.error().unwrap().contains("unreadable"));
        let c = draw_into(img.element(&t), Size::new(20, 3));
        assert!(c.row_text(0).contains("image"), "{:?}", c.row_text(0));
    }

    #[test]
    fn non_png_is_labeled_undecodable() {
        // A real file that is not an image: this crate's manifest.
        let img = Image::from_path(concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml"));
        assert!(
            img.error().unwrap().contains("undecodable"),
            "{:?}",
            img.error()
        );
    }

    #[test]
    fn from_path_decodes_a_real_file_through_the_unified_decoder() {
        // 0144 widening: from_path routes by magic (PNG + JPEG), not a
        // PNG-only call. Round-trip a generated PNG through a temp file.
        // Token-rule-safe constants only (the directory lint forbids
        // color construction even in tests — see `stripes` above);
        // pixel variety is irrelevant to the routing under test.
        let bmp = Bitmap::from_fn(
            5,
            3,
            |x, _| if x % 2 == 0 { Rgba::WHITE } else { Rgba::BLACK },
        );
        let path = std::env::temp_dir().join(format!(
            "abstracttui_image_probe_{}.png",
            std::process::id()
        ));
        std::fs::write(&path, crate::gfx::png_encode::encode(&bmp)).unwrap();
        let img = Image::from_path(&path);
        assert!(img.error().is_none(), "{:?}", img.error());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn degenerate_rects_never_panic() {
        let t = default_theme().tokens;
        for size in [Size::new(0, 0), Size::new(1, 1), Size::new(2, 1)] {
            let el = Image::from_bitmap(stripes()).element(&t);
            let _ = draw_into(el, size);
        }
    }

    // -- fit math at hostile aspect ratios (field-bug wave) ---------------

    /// The fit must be total and never produce a zero-size target for
    /// any rect >= 1x1 — tall-narrow panes (the 70x60 field class),
    /// short-wide strips, single columns/rows, and extreme-aspect
    /// sources all included. Aspect distortion at the 1-cell floor is
    /// accepted; vanishing is not.
    #[test]
    fn fit_never_zero_for_visible_rects_at_hostile_aspects() {
        let rects = [
            Rect::new(0, 0, 3, 50),  // tall-narrow pane
            Rect::new(0, 0, 200, 1), // one-row strip
            Rect::new(0, 0, 1, 40),  // one-column strip
            Rect::new(0, 0, 40, 1),  // short-wide sliver
            Rect::new(0, 0, 1, 1),   // single cell
            Rect::new(0, 0, 15, 51), // the field pane (70x60 quarter)
        ];
        let sources = [(160, 96), (8, 8), (1000, 2), (2, 1000), (1, 1)];
        let modes = [
            MosaicMode::HalfBlock,
            MosaicMode::Quadrant,
            MosaicMode::Sextant,
            MosaicMode::Braille,
        ];
        let fits = [
            ImageFit::Contain,
            ImageFit::Cover,
            ImageFit::Fill,
            ImageFit::None,
        ];
        for rect in rects {
            for (iw, ih) in sources {
                for mode in modes {
                    for fit in fits {
                        let (target, crop) = resolve_fit(
                            rect,
                            iw,
                            ih,
                            fit,
                            mode,
                            ImageAlign::Center,
                            ImageAlign::Center,
                        );
                        assert!(
                            target.w >= 1 && target.h >= 1,
                            "zero target {target:?} for rect {rect:?} img {iw}x{ih} {mode:?} {fit:?}"
                        );
                        assert!(
                            target.w <= rect.w && target.h <= rect.h,
                            "target {target:?} escapes rect {rect:?} ({mode:?} {fit:?})"
                        );
                        if let Some((_, _, cw, ch)) = crop {
                            assert!(
                                cw >= 1 && ch >= 1,
                                "zero crop for rect {rect:?} img {iw}x{ih} {mode:?} {fit:?}"
                            );
                        }
                    }
                }
            }
        }
    }

    /// Degenerate rects and empty sources resolve to an EMPTY target
    /// (the draw shell skips it) — total, no clamp panic, no division
    /// blowup. The widget's draw guard makes these unreachable live;
    /// the pure fn must still be safe to call directly.
    #[test]
    fn fit_is_total_on_degenerate_inputs() {
        for rect in [
            Rect::new(2, 3, 0, 10),
            Rect::new(2, 3, 10, 0),
            Rect::new(2, 3, 0, 0),
        ] {
            for fit in [
                ImageFit::Contain,
                ImageFit::Cover,
                ImageFit::Fill,
                ImageFit::None,
            ] {
                let (target, crop) = resolve_fit(
                    rect,
                    160,
                    96,
                    fit,
                    MosaicMode::HalfBlock,
                    ImageAlign::Start,
                    ImageAlign::Start,
                );
                assert!(target.is_empty(), "{fit:?}: {target:?}");
                assert!(crop.is_none());
            }
        }
        // Empty source, healthy rect: same empty-target answer.
        let (target, _) = resolve_fit(
            Rect::new(0, 0, 10, 5),
            0,
            0,
            ImageFit::Contain,
            MosaicMode::HalfBlock,
            ImageAlign::Start,
            ImageAlign::Start,
        );
        assert!(target.is_empty());
    }

    /// The widget's intrinsic answer is its native cell footprint
    /// through the mode's subpixel density (aspect-correct in cell
    /// space), never zero; broken sources answer the labeled state's
    /// footprint so the label survives `Auto` contexts.
    #[test]
    fn natural_cells_answers_mode_density() {
        let src: Result<Arc<Bitmap>, String> = Ok(Arc::new(Bitmap::new(160, 96, Rgba::WHITE)));
        assert_eq!(
            Image::natural_cells(&src, MosaicMode::HalfBlock),
            Size::new(160, 48)
        );
        assert_eq!(
            Image::natural_cells(&src, MosaicMode::Quadrant),
            Size::new(80, 48)
        );
        assert_eq!(
            Image::natural_cells(&src, MosaicMode::Sextant),
            Size::new(80, 32)
        );
        assert_eq!(
            Image::natural_cells(&src, MosaicMode::Braille),
            Size::new(80, 24)
        );
        // Odd sizes round UP (a 3x3 quadrant image still needs 2x2 cells).
        let odd: Result<Arc<Bitmap>, String> = Ok(Arc::new(Bitmap::new(3, 3, Rgba::WHITE)));
        assert_eq!(
            Image::natural_cells(&odd, MosaicMode::Quadrant),
            Size::new(2, 2)
        );
        let broken: Result<Arc<Bitmap>, String> = Err("unreadable".into());
        assert_eq!(
            Image::natural_cells(&broken, MosaicMode::HalfBlock),
            Size::new(7, 2)
        );
    }
}
