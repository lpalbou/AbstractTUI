//! The one image pipeline: capability ladder + a facade that turns a
//! Bitmap + target cell rect into either mosaic cell patches or a
//! protocol byte payload for `Presenter::external_write(bytes, at)`
//! (damage contract §6 — gfx never touches the terminal).
//!
//! Ladder: kitty > iterm2 > sixel > mosaic (docs/design/gfx-three.md
//! §1). Every degradation is labeled with a `#FALLBACK` warning string
//! on the result — never silent.

use crate::base::{Point, Rect, Rgba};
use crate::gfx::bitmap::Bitmap;
use crate::gfx::mosaic::{blit_into, CellPatch, MosaicMode, MosaicRenderer};
use crate::gfx::proto::{iterm2, kitty, sixel};
use crate::term::caps::{GraphicsCaps, WrapKind};

/// Which output channel a render used (or would use).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Channel {
    Kitty,
    Iterm2,
    Sixel,
    Mosaic,
}

/// Capability ladder, best channel first. Pure over capability bits:
/// kitty and iTerm2 scale server-side via cell-fit keys, so they only
/// need their protocol bit; sixel additionally needs the cell pixel
/// geometry to rasterize at the right size — without it the ladder
/// falls through to mosaic (the caller gets a labeled warning from
/// [`ImageRenderer::render`], which knows a rung was skipped).
pub fn choose_channel(caps: &GraphicsCaps) -> Channel {
    if caps.kitty_graphics {
        Channel::Kitty
    } else if caps.iterm2_images {
        Channel::Iterm2
    } else if caps.sixel && caps.cell_pixel_size.is_some_and(|p| !p.is_empty()) {
        Channel::Sixel
    } else {
        Channel::Mosaic
    }
}

/// What to do with a rendered image.
#[derive(Clone, Debug)]
pub enum ImageOutput {
    /// Write these cells into a surface (RENDER's `blit_mosaic` or the
    /// `CellPatch` bridge).
    Cells(Vec<CellPatch>),
    /// Hand these bytes to `Presenter::external_write(bytes, at)`.
    Bytes { bytes: Vec<u8>, at: Point },
}

/// A render result: channel used, output, labeled degradations.
#[derive(Clone, Debug)]
pub struct RenderedImage {
    pub channel: Channel,
    pub output: ImageOutput,
    /// `#FALLBACK`-prefixed, human-readable degradation labels.
    pub warnings: Vec<String>,
    /// The kitty image id when `channel == Kitty` (callers need it to
    /// delete/re-place the image later).
    pub kitty_id: Option<u32>,
}

/// Mosaic rung options (cycle-4 quality pass).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct MosaicOpts {
    /// Glyph family.
    pub mode: MosaicMode,
    /// Pre-quantize + Floyd–Steinberg the source to this many colors
    /// before cell fitting. `None` = truecolor (the default — mosaic
    /// itself is truecolor; dithering earns its keep when the OUTPUT
    /// terminal is 256/16-color, where the presenter's quantization
    /// would otherwise band gradients).
    pub dither: Option<u16>,
}

impl Default for MosaicOpts {
    fn default() -> Self {
        MosaicOpts {
            mode: MosaicMode::HalfBlock,
            dither: None,
        }
    }
}

/// Pipeline configuration (all fields have working defaults).
#[derive(Clone, Debug)]
pub struct RenderConfig {
    /// Mosaic rung options (glyph family + optional dither).
    pub mosaic: MosaicOpts,
    /// Kitty wire format. RGBA+zlib is the deterministic default;
    /// `Png` is smaller for flat-color content.
    pub kitty_format: kitty::Format,
    /// z-index for pixel placements (negative = under text).
    pub z: i32,
    /// Sixel register budget before caps clamping.
    pub sixel_registers: u16,
    /// Sixel dithering.
    pub sixel_dither: bool,
}

impl Default for RenderConfig {
    fn default() -> Self {
        RenderConfig {
            mosaic: MosaicOpts::default(),
            kitty_format: kitty::Format::Rgba32,
            z: 0,
            sixel_registers: 64,
            sixel_dither: true,
        }
    }
}

/// Reusable facade: owns the scratch buffers (mosaic renderer, sixel
/// scaling bitmap, patch vec) and the kitty image-id counter so frame
/// loops allocate nothing in steady state.
pub struct ImageRenderer {
    pub config: RenderConfig,
    mosaic: MosaicRenderer,
    patches: Vec<CellPatch>,
    sixel_scratch: Bitmap,
    mosaic_scratch: Bitmap,
    next_kitty_id: u32,
}

/// Where pixel-protocol bytes go: RENDER's `Presenter::external_write`
/// bracket, behind a one-method seam so app code and tests do not
/// depend on the concrete presenter type.
pub trait ExternalSink {
    fn external_write(&mut self, bytes: &[u8], at: Point);
}

/// App-level image presentation through the full capability ladder —
/// the PROTOCOL path the `widgets::Image` draw closure deliberately
/// cannot take (draw closures own cells, not the wire; damage contract
/// §6 puts escape bytes under presenter custody AFTER the cell diff).
///
/// Byte channels (kitty/iTerm2/sixel) are written to `sink`
/// immediately; the mosaic fallback comes back as cell patches for the
/// caller to blit into a surface. Returns the full `RenderedImage`
/// (channel, warnings, kitty id for later deletes).
pub fn present_image(
    renderer: &mut ImageRenderer,
    sink: &mut dyn ExternalSink,
    img: &Bitmap,
    rect: Rect,
    caps: &GraphicsCaps,
) -> RenderedImage {
    let rendered = renderer.render(img, rect, caps);
    if let ImageOutput::Bytes { bytes, at } = &rendered.output {
        sink.external_write(bytes, *at);
    }
    rendered
}

impl Default for ImageRenderer {
    fn default() -> Self {
        ImageRenderer {
            config: RenderConfig::default(),
            mosaic: MosaicRenderer::new(),
            patches: Vec::new(),
            sixel_scratch: Bitmap::default(),
            mosaic_scratch: Bitmap::default(),
            next_kitty_id: 1,
        }
    }
}

impl ImageRenderer {
    pub fn new() -> ImageRenderer {
        ImageRenderer::default()
    }

    /// Render `img` into the cell rect `rect` using the best channel
    /// `caps` offers. `rect` is in screen cells; `rect.origin()` is
    /// where the presenter must park the cursor for byte payloads.
    pub fn render(&mut self, img: &Bitmap, rect: Rect, caps: &GraphicsCaps) -> RenderedImage {
        let mut warnings = Vec::new();
        if rect.is_empty() || img.is_empty() {
            return RenderedImage {
                channel: Channel::Mosaic,
                output: ImageOutput::Cells(Vec::new()),
                warnings,
                kitty_id: None,
            };
        }
        let (cols, rows) = (rect.w as u32, rect.h as u32);
        let channel = choose_channel(caps);
        // A skipped sixel rung is a degradation the user should see.
        if caps.sixel && channel == Channel::Mosaic {
            warnings.push(
                "#FALLBACK sixel advertised but cell pixel size unknown; using unicode mosaic"
                    .to_string(),
            );
        }

        let mut rendered = match channel {
            Channel::Kitty => {
                let id = self.next_kitty_id;
                // Wrapping add skipping 0: id 0 means "terminal picks"
                // and would make deletes impossible.
                self.next_kitty_id = self.next_kitty_id.checked_add(1).unwrap_or(1);
                let opts = kitty::Options {
                    id,
                    format: self.config.kitty_format,
                    fit_cols: Some(cols),
                    fit_rows: Some(rows),
                    z: self.config.z,
                    compress: true,
                };
                RenderedImage {
                    channel,
                    output: ImageOutput::Bytes {
                        bytes: kitty::transmit_display(img, &opts),
                        at: rect.origin(),
                    },
                    warnings,
                    kitty_id: Some(id),
                }
            }
            Channel::Iterm2 => {
                let opts = iterm2::Options {
                    fit_cols: Some(cols),
                    fit_rows: Some(rows),
                    preserve_aspect: true,
                };
                RenderedImage {
                    channel,
                    output: ImageOutput::Bytes {
                        bytes: iterm2::inline_png(img, &opts),
                        at: rect.origin(),
                    },
                    warnings,
                    kitty_id: None,
                }
            }
            Channel::Sixel => {
                let cell = caps.cell_pixel_size.expect("choose_channel checked");
                let (pw, ph) = (cols * cell.w as u32, rows * cell.h as u32);
                // Honor the terminal's register report; keep our
                // default budget when it is silent or generous.
                let mut registers = self.config.sixel_registers;
                if let Some(max) = caps.sixel_max_registers {
                    if max < registers {
                        registers = max;
                        warnings.push(format!(
                            "#FALLBACK terminal caps sixel palette at {max} registers"
                        ));
                    }
                }
                let source: &Bitmap = if img.width() == pw && img.height() == ph {
                    img
                } else {
                    if self.sixel_scratch.width() != pw || self.sixel_scratch.height() != ph {
                        self.sixel_scratch = Bitmap::new(pw, ph, Rgba::TRANSPARENT);
                    }
                    img.resize_bilinear_into(&mut self.sixel_scratch);
                    &self.sixel_scratch
                };
                let opts = sixel::Options {
                    max_registers: registers,
                    dither: self.config.sixel_dither,
                    register_base: 0,
                };
                RenderedImage {
                    channel,
                    output: ImageOutput::Bytes {
                        bytes: sixel::encode(source, &opts),
                        at: rect.origin(),
                    },
                    warnings,
                    kitty_id: None,
                }
            }
            Channel::Mosaic => {
                let opts = self.config.mosaic;
                let source: &Bitmap = match opts.dither {
                    None => img,
                    Some(colors) => {
                        // Low-color pre-pass: quantize + error-diffuse
                        // into the reusable scratch, then cell-fit.
                        if self.mosaic_scratch.width() != img.width()
                            || self.mosaic_scratch.height() != img.height()
                        {
                            self.mosaic_scratch =
                                Bitmap::new(img.width(), img.height(), Rgba::TRANSPARENT);
                        }
                        self.mosaic_scratch
                            .pixels_mut()
                            .copy_from_slice(img.pixels());
                        let palette =
                            crate::gfx::quantize::median_cut(img.pixels(), colors.max(2) as usize);
                        crate::gfx::dither::floyd_steinberg(&mut self.mosaic_scratch, &palette);
                        &self.mosaic_scratch
                    }
                };
                self.mosaic.render(source, cols, rows, opts.mode);
                blit_into(self.mosaic.grid(), rect.origin(), &mut self.patches);
                RenderedImage {
                    channel,
                    output: ImageOutput::Cells(std::mem::take(&mut self.patches)),
                    warnings,
                    kitty_id: None,
                }
            }
        };

        // Multiplexer passthrough (KERNEL's cycle-4 detection): when the
        // outer terminal must receive the payload THROUGH tmux, every
        // escape payload is wrapped — verified passthrough only, so this
        // is routing, not degradation (no warning label).
        if let (Some(WrapKind::Tmux), ImageOutput::Bytes { bytes, .. }) =
            (caps.wrap, &mut rendered.output)
        {
            *bytes = crate::term::tmux_wrap(bytes);
        }
        rendered
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::base::PixelSize;

    fn caps(kitty: bool, iterm2: bool, sixel: bool, px: Option<PixelSize>) -> GraphicsCaps {
        GraphicsCaps {
            wrap: None,
            kitty_graphics: kitty,
            iterm2_images: iterm2,
            sixel,
            sixel_max_registers: None,
            cell_pixel_size: px,
        }
    }

    #[test]
    fn ladder_order() {
        let px = Some(PixelSize::new(8, 16));
        assert_eq!(choose_channel(&caps(true, true, true, px)), Channel::Kitty);
        assert_eq!(
            choose_channel(&caps(false, true, true, px)),
            Channel::Iterm2
        );
        assert_eq!(
            choose_channel(&caps(false, false, true, px)),
            Channel::Sixel
        );
        assert_eq!(
            choose_channel(&caps(false, false, false, px)),
            Channel::Mosaic
        );
        // Sixel without pixel geometry cannot rasterize honestly.
        assert_eq!(
            choose_channel(&caps(false, false, true, None)),
            Channel::Mosaic
        );
        assert_eq!(
            choose_channel(&caps(false, false, true, Some(PixelSize::new(0, 0)))),
            Channel::Mosaic
        );
    }

    #[test]
    fn kitty_render_bytes_and_ids() {
        let mut r = ImageRenderer::new();
        let img = Bitmap::new(4, 4, Rgba::rgb(9, 9, 9));
        let rect = Rect::new(3, 2, 10, 5);
        let out = r.render(&img, rect, &caps(true, false, false, None));
        assert_eq!(out.channel, Channel::Kitty);
        assert_eq!(out.kitty_id, Some(1));
        let ImageOutput::Bytes { bytes, at } = &out.output else {
            panic!("expected bytes")
        };
        assert_eq!(*at, Point::new(3, 2));
        let s = String::from_utf8_lossy(bytes);
        assert!(s.contains("c=10") && s.contains("r=5"));
        // Ids advance per image.
        let out2 = r.render(&img, rect, &caps(true, false, false, None));
        assert_eq!(out2.kitty_id, Some(2));
        assert!(out.warnings.is_empty());
    }

    #[test]
    fn iterm2_render_shape() {
        let mut r = ImageRenderer::new();
        let img = Bitmap::new(4, 4, Rgba::rgb(1, 2, 3));
        let out = r.render(&img, Rect::new(0, 0, 8, 4), &caps(false, true, false, None));
        assert_eq!(out.channel, Channel::Iterm2);
        let ImageOutput::Bytes { bytes, .. } = &out.output else {
            panic!()
        };
        assert!(bytes.starts_with(b"\x1b]1337;File="));
    }

    #[test]
    fn sixel_render_scales_to_cell_pixels() {
        let mut r = ImageRenderer::new();
        let img = Bitmap::new(4, 4, Rgba::rgb(255, 0, 0));
        let px = Some(PixelSize::new(10, 20));
        let out = r.render(&img, Rect::new(0, 0, 3, 2), &caps(false, false, true, px));
        assert_eq!(out.channel, Channel::Sixel);
        let ImageOutput::Bytes { bytes, .. } = &out.output else {
            panic!()
        };
        let s = String::from_utf8_lossy(bytes);
        // Raster attributes carry the pixel geometry 30x40.
        assert!(s.contains("\"1;1;30;40"), "{s}");
    }

    #[test]
    fn sixel_register_cap_warns_labeled() {
        let mut r = ImageRenderer::new();
        let img = Bitmap::from_fn(8, 8, |x, y| Rgba::rgb((x * 30) as u8, (y * 30) as u8, 0));
        let mut c = caps(false, false, true, Some(PixelSize::new(4, 8)));
        c.sixel_max_registers = Some(16);
        let out = r.render(&img, Rect::new(0, 0, 4, 2), &c);
        assert!(out
            .warnings
            .iter()
            .any(|w| w.starts_with("#FALLBACK") && w.contains("16")));
    }

    #[test]
    fn mosaic_fallback_with_label_when_sixel_unusable() {
        let mut r = ImageRenderer::new();
        let img = Bitmap::new(4, 4, Rgba::rgb(0, 200, 0));
        let out = r.render(&img, Rect::new(1, 1, 2, 2), &caps(false, false, true, None));
        assert_eq!(out.channel, Channel::Mosaic);
        let ImageOutput::Cells(cells) = &out.output else {
            panic!()
        };
        assert_eq!(cells.len(), 4);
        assert_eq!(cells[0].pos, Point::new(1, 1));
        assert!(out.warnings.iter().any(|w| w.starts_with("#FALLBACK")));
    }

    #[test]
    fn plain_mosaic_has_no_warning() {
        let mut r = ImageRenderer::new();
        let img = Bitmap::new(2, 2, Rgba::WHITE);
        let out = r.render(
            &img,
            Rect::new(0, 0, 1, 1),
            &caps(false, false, false, None),
        );
        assert_eq!(out.channel, Channel::Mosaic);
        assert!(
            out.warnings.is_empty(),
            "no degradation happened: {:?}",
            out.warnings
        );
    }

    #[test]
    fn empty_inputs_are_noops() {
        let mut r = ImageRenderer::new();
        let img = Bitmap::new(2, 2, Rgba::WHITE);
        let out = r.render(&img, Rect::ZERO, &caps(true, false, false, None));
        assert!(matches!(&out.output, ImageOutput::Cells(c) if c.is_empty()));
        let out = r.render(
            &Bitmap::default(),
            Rect::new(0, 0, 2, 2),
            &caps(true, false, false, None),
        );
        assert!(matches!(&out.output, ImageOutput::Cells(c) if c.is_empty()));
    }

    #[test]
    fn tmux_wrap_applies_to_protocol_bytes_only() {
        let mut r = ImageRenderer::new();
        let img = Bitmap::new(2, 2, Rgba::WHITE);
        let mut c = caps(true, false, false, None);
        c.wrap = Some(WrapKind::Tmux);
        let out = r.render(&img, Rect::new(0, 0, 2, 2), &c);
        let ImageOutput::Bytes { bytes, .. } = &out.output else {
            panic!("kitty channel expected")
        };
        assert!(bytes.starts_with(b"\x1bPtmux;"), "passthrough header");
        assert!(bytes.ends_with(b"\x1b\\"));
        // Inner ESCs doubled: the kitty APC intro appears as ESC ESC _G.
        assert!(
            bytes.windows(4).any(|w| w == b"\x1b\x1b_G"),
            "inner escapes must be doubled"
        );
        // Mosaic output is cells — never wrapped.
        let mut c2 = caps(false, false, false, None);
        c2.wrap = Some(WrapKind::Tmux);
        let out = r.render(&img, Rect::new(0, 0, 1, 1), &c2);
        assert!(matches!(out.output, ImageOutput::Cells(_)));
    }

    #[test]
    fn present_image_routes_bytes_to_the_sink() {
        struct Sink(Vec<(Vec<u8>, Point)>);
        impl ExternalSink for Sink {
            fn external_write(&mut self, bytes: &[u8], at: Point) {
                self.0.push((bytes.to_vec(), at));
            }
        }
        let mut r = ImageRenderer::new();
        let mut sink = Sink(Vec::new());
        let img = Bitmap::new(2, 2, Rgba::WHITE);

        // Kitty channel: bytes reach the sink with the rect origin.
        let out = present_image(
            &mut r,
            &mut sink,
            &img,
            Rect::new(4, 2, 6, 3),
            &caps(true, false, false, None),
        );
        assert_eq!(out.channel, Channel::Kitty);
        assert_eq!(sink.0.len(), 1);
        assert_eq!(sink.0[0].1, Point::new(4, 2));

        // Mosaic channel: nothing written, cells returned.
        let out = present_image(
            &mut r,
            &mut sink,
            &img,
            Rect::new(0, 0, 2, 1),
            &caps(false, false, false, None),
        );
        assert_eq!(sink.0.len(), 1, "mosaic writes no bytes");
        assert!(matches!(&out.output, ImageOutput::Cells(c) if !c.is_empty()));
    }

    #[test]
    fn mosaic_grid_cell_patches_iterator_shape() {
        // The exact tuple shape RENDER's blit_mosaic consumes.
        let img = Bitmap::from_fn(2, 2, |x, _| {
            if x == 0 {
                Rgba::rgb(255, 0, 0)
            } else {
                Rgba::rgb(0, 0, 255)
            }
        });
        let grid = crate::gfx::mosaic::render(&img, 2, 1, MosaicMode::HalfBlock);
        let v: Vec<(Point, char, Rgba, Rgba)> = grid.cell_patches(Point::new(5, 6)).collect();
        assert_eq!(v.len(), 2);
        assert_eq!(v[0].0, Point::new(5, 6));
        assert_eq!(v[1].0, Point::new(6, 6));
        assert_eq!(v[0].2, Rgba::rgb(255, 0, 0)); // uniform column -> bg==fg
    }
}
