//! Field repro: `cargo run --example images` on a tall-narrow terminal
//! (maintainer report: ~70x60, truecolor, no pixel protocols — mosaic
//! path) rendered the four mosaic panes as skinny 2-column EMPTY
//! bordered strips bunched on the left. This drives the example's exact
//! view composition through the house rig (Driver + CaptureTerm +
//! VtScreen) at the field size plus other geometries and pins the
//! fixed behavior: panes split the full row and every pane interior
//! actually shows image cells.
//!
//! Root cause (the RT8-6 multi-pane collapse class, hidden behind a
//! `dyn_view` boundary): the example's pane ROW had no `grow`, so
//! inside the grow-ing dyn_view HOST it resolved to its intrinsic
//! width — 4 borders x 2 + 3 gaps = 11 cells — and the four grow(1.0)
//! panes split 8 cells: 2 each, all chrome, zero interior. Width-
//! independent (the docs capture showed the same strips at ~100 cols).

use std::sync::Arc;

use abstracttui::app::{App, Driver, RunConfig};
use abstracttui::base::{Point, Rgba, Size};
use abstracttui::gfx::{Bitmap, MosaicMode, MosaicRenderer};
use abstracttui::layout::{Dimension, Edges, Style as LayoutStyle};
use abstracttui::term::Capabilities;
use abstracttui::testing::{CaptureTerm, VtScreen};
use abstracttui::theme::{default_theme, TokenSet};
use abstracttui::ui::{dyn_view, text, Element, View};

const PANELS: [(MosaicMode, &str); 4] = [
    (MosaicMode::HalfBlock, "halfblock 1x2"),
    (MosaicMode::Quadrant, "quadrant 2x2"),
    (MosaicMode::Sextant, "sextant 2x3"),
    (MosaicMode::Braille, "braille 2x4"),
];

/// The example's procedural test card, structurally identical (hue
/// bands over a luminance ramp — colorful everywhere, so any fitted
/// region produces image-colored cells).
fn test_card(w: u32, h: u32) -> Bitmap {
    Bitmap::from_fn(w, h, |x, y| {
        let fx = x as f32 / (w - 1) as f32;
        let fy = y as f32 / (h - 1) as f32;
        let band = (fx * 5.0).floor() / 5.0;
        Rgba::rgb(
            (40.0 + 200.0 * band) as u8,
            (230.0 * (1.0 - fy)) as u8,
            (60.0 + 160.0 * fx) as u8,
        )
    })
}

/// One labeled mosaic panel — byte-for-byte the example's `panel()`
/// composition (Block + grow draw element + the same fit math).
fn panel(t: &TokenSet, img: Arc<Bitmap>, mode: MosaicMode, label: &'static str) -> View {
    let ground = t.bg;
    abstracttui::widgets::Block::new()
        .title(label)
        .fill(t.surface)
        .layout(LayoutStyle::column().grow(1.0).basis(Dimension::Cells(0)))
        .child(
            Element::new()
                .style(LayoutStyle::default().grow(1.0))
                .draw(move |canvas, rect| {
                    if rect.w <= 1 || rect.h <= 1 {
                        return;
                    }
                    let (sx, sy) = mode.cell_pixels();
                    let iw = img.width().max(1) as f32;
                    let ih = img.height().max(1) as f32;
                    let scale =
                        ((rect.w as f32 * sx as f32) / iw).min((rect.h as f32 * sy as f32) / ih);
                    let cw = ((iw * scale / sx as f32).floor() as i32).clamp(1, rect.w);
                    let ch = ((ih * scale / sy as f32).floor() as i32).clamp(1, rect.h);
                    let origin = Point::new(rect.x + (rect.w - cw) / 2, rect.y + (rect.h - ch) / 2);
                    let mut renderer = MosaicRenderer::new();
                    let grid = renderer.render(&img, cw as u32, ch as u32, mode);
                    for (pos, chr, fg, bg) in grid.cell_patches(origin) {
                        let bg = if bg.is_transparent() { ground } else { bg };
                        canvas.put(pos, chr, fg, bg);
                    }
                })
                .build(),
        )
        .element(t)
        .build()
}

/// The example's mount composition (examples/images.rs), reduced to its
/// layout-load-bearing parts: root column with padding+gap, title line,
/// the grow(1.0) dyn_view hosting the four-pane row, two footer lines.
fn mount_images_view(app: &mut App, bitmap: Arc<Bitmap>) {
    app.mount(move |_cx| {
        let t = default_theme().tokens;
        Element::new()
            .style(LayoutStyle::column().padding(Edges::all(1)).gap(1))
            .child(dyn_view(LayoutStyle::default().h(1), move || {
                text("images — procedural test card (160x96 px)")
            }))
            .child(dyn_view(LayoutStyle::default().grow(1.0), move || {
                // The row itself must grow: the dyn_view HOST is a
                // container and the row is its flex item — without its
                // own grow it sits at intrinsic width (the field bug).
                let mut row = Element::new().style(LayoutStyle::row().gap(1).grow(1.0));
                for (mode, label) in PANELS {
                    row = row.child(panel(&t, bitmap.clone(), mode, label));
                }
                row.build()
            }))
            .child(dyn_view(LayoutStyle::default().h(2), move || {
                Element::new()
                    .style(LayoutStyle::column())
                    .child(text(
                        "dither: off (truecolor)    protocol: off — would use mosaic",
                    ))
                    .child(text("d dither · p protocol · t theme · q quit"))
                    .build()
            }))
            .build()
    })
    .expect("mount");
}

/// Drive the app headlessly at `size` and return the settled screen.
/// Field-faithful caps: truecolor, UTF-8, no pixel protocols (mosaic).
fn render_at(size: Size) -> VtScreen {
    let mut term = CaptureTerm::new(size);
    let mut app = App::new(size);
    mount_images_view(&mut app, Arc::new(test_card(160, 96)));
    let cfg = RunConfig {
        caps: Some(Capabilities::with(|c| {
            c.truecolor = true;
            c.colors_256 = true;
            c.unicode_ok = true;
        })),
        enter: None,
        probe: false,
    };
    let mut driver = Driver::new(&mut app, &mut term, cfg).expect("enter");
    for _ in 0..6 {
        if driver.turn(&mut app, &mut term).expect("turn").idle {
            break;
        }
    }
    let mut vt = VtScreen::new(size);
    vt.feed(&term.take_bytes());
    assert_eq!(
        vt.unknown_seq_count(),
        0,
        "unmodeled sequences: {:?}",
        vt.unknown_samples()
    );
    vt
}

/// Pane rects discovered from the screen itself: every `┌` corner with
/// its matching `┐` on the same row and `└` below (Block plain border).
fn pane_rects(vt: &VtScreen) -> Vec<(i32, i32, i32, i32)> {
    let size = vt.size();
    let mut rects = Vec::new();
    for y in 0..size.h {
        for x in 0..size.w {
            if vt.cell(x, y).map(|c| c.ch()) != Some('┌') {
                continue;
            }
            let mut right = None;
            for xx in (x + 1)..size.w {
                if vt.cell(xx, y).map(|c| c.ch()) == Some('┐') {
                    right = Some(xx);
                    break;
                }
            }
            let mut bottom = None;
            for yy in (y + 1)..size.h {
                if vt.cell(x, yy).map(|c| c.ch()) == Some('└') {
                    bottom = Some(yy);
                    break;
                }
            }
            if let (Some(r), Some(b)) = (right, bottom) {
                rects.push((x, y, r - x + 1, b - y + 1));
            }
        }
    }
    rects
}

/// Count interior cells that carry IMAGE content: a mosaic glyph, or a
/// background repainted away from the panel chrome (surface fill /
/// theme ground). Uniform mosaic regions legitimately emit ' ' + bg, so
/// "non-space" alone would miss half-block flats — color is the truth.
fn image_cells_inside(vt: &VtScreen, rect: (i32, i32, i32, i32)) -> usize {
    let t = default_theme().tokens;
    let chrome = [t.surface, t.bg];
    let (x, y, w, h) = rect;
    let mut n = 0;
    for yy in (y + 1)..(y + h - 1) {
        for xx in (x + 1)..(x + w - 1) {
            let Some(cell) = vt.cell(xx, yy) else {
                continue;
            };
            let glyph = cell.ch() != ' ' && cell.ch() != '\0';
            let colored = cell
                .paint
                .bg
                .map(|bg| chrome.iter().all(|c| *c != bg))
                .unwrap_or(false);
            if glyph || colored {
                n += 1;
            }
        }
    }
    n
}

fn dump(vt: &VtScreen, label: &str) {
    let size = vt.size();
    eprintln!("--- {label} ({}x{}) ---", size.w, size.h);
    for y in 0..size.h {
        let mut line = String::new();
        for x in 0..size.w {
            match vt.cell(x, y) {
                Some(c) if c.is_continuation() => {}
                Some(c) => line.push_str(c.display()),
                None => line.push(' '),
            }
        }
        eprintln!("{:>3}|{}", y, line.trim_end());
    }
}

/// The pinned scenario matrix: field size (70x60), a shorter narrow
/// (70x45), a wide-short (100x30), and the example's default (110x30).
/// Every geometry must produce four panes that tile the row and carry
/// image cells — the field defect produced 2-col empty strips.
fn assert_panes_sound(size: Size) {
    let vt = render_at(size);
    dump(&vt, "images example");
    let rects = pane_rects(&vt);
    assert_eq!(
        rects.len(),
        4,
        "expected 4 mosaic panes at {}x{}, found rects {rects:?}",
        size.w,
        size.h
    );
    // Content box: viewport minus root padding (1 each side). Four
    // panes + 3 gaps must TILE it — each pane gets its quarter, and
    // the rightmost pane must reach the right edge of the content box.
    let content_w = size.w - 2;
    let fair = (content_w - 3) / 4;
    let min_w = fair - 1; // integer split slack
    for (i, r) in rects.iter().enumerate() {
        assert!(
            r.2 >= min_w,
            "pane {i} at {}x{}: width {} < fair share {min_w} (rects {rects:?})",
            size.w,
            size.h,
            r.2
        );
    }
    let rightmost = rects.iter().map(|r| r.0 + r.2).max().unwrap();
    assert!(
        rightmost >= size.w - 1 - 1,
        "panes stop at column {rightmost}, leaving the right of the {}-col content box empty",
        size.w
    );
    // Every pane interior must actually show the image: the field bug
    // rendered EMPTY strips (interior width 0 -> the draw guard bailed).
    for (i, r) in rects.iter().enumerate() {
        let cells = image_cells_inside(&vt, *r);
        let interior = ((r.2 - 2).max(0) * (r.3 - 2).max(0)) as usize;
        assert!(
            cells >= 8.min(interior),
            "pane {i} at {}x{} shows no image content ({cells} image cells in {interior} interior cells; rect {r:?})",
            size.w,
            size.h
        );
    }
}

#[test]
fn images_panes_carry_content_at_field_size_70x60() {
    assert_panes_sound(Size::new(70, 60));
}

#[test]
fn images_panes_carry_content_at_70x45() {
    assert_panes_sound(Size::new(70, 45));
}

#[test]
fn images_panes_carry_content_at_100x30() {
    assert_panes_sound(Size::new(100, 30));
}

#[test]
fn images_panes_carry_content_at_example_default_110x30() {
    assert_panes_sound(Size::new(110, 30));
}

// ---------------------------------------------------------------------------
// The widget-level half of the fix: four `Image` widgets (one per
// mosaic family) in bordered Blocks in a row with NO grow anywhere.
// Before the Image intrinsic-measure fix, every pane collapsed to its
// border (an Image answered 0x0 to `Auto` sizing — the same RT8-6
// class at the widget level). Now the images' natural cell footprints
// feed the flex basis and shrink shares the row: every pane keeps a
// usable width and shows image cells.
// ---------------------------------------------------------------------------

fn mount_image_widget_row(app: &mut App, bitmap: Arc<Bitmap>) {
    app.mount(move |_cx| {
        let t = default_theme().tokens;
        let mut row = Element::new().style(LayoutStyle::row().gap(1));
        for (mode, label) in PANELS {
            row = row.child(
                abstracttui::widgets::Block::new()
                    .title(label)
                    .child(
                        abstracttui::widgets::Image::from_bitmap(bitmap.clone())
                            .mode(mode)
                            .element(&t)
                            .build(),
                    )
                    .element(&t)
                    .build(),
            );
        }
        Element::new()
            .style(LayoutStyle::column().padding(Edges::all(1)))
            .child(row.build())
            .build()
    })
    .expect("mount");
}

fn assert_image_widget_row_sound(size: Size) {
    let mut term = CaptureTerm::new(size);
    let mut app = App::new(size);
    mount_image_widget_row(&mut app, Arc::new(test_card(160, 96)));
    let cfg = RunConfig {
        caps: Some(Capabilities::with(|c| {
            c.truecolor = true;
            c.colors_256 = true;
            c.unicode_ok = true;
        })),
        enter: None,
        probe: false,
    };
    let mut driver = Driver::new(&mut app, &mut term, cfg).expect("enter");
    for _ in 0..6 {
        if driver.turn(&mut app, &mut term).expect("turn").idle {
            break;
        }
    }
    let mut vt = VtScreen::new(size);
    vt.feed(&term.take_bytes());
    dump(&vt, "image widget row (no grow)");
    let rects = pane_rects(&vt);
    assert_eq!(
        rects.len(),
        4,
        "expected 4 Image panes at {}x{}, found {rects:?}",
        size.w,
        size.h
    );
    for (i, r) in rects.iter().enumerate() {
        assert!(
            r.2 >= 6,
            "pane {i} at {}x{}: width {} unusable (rects {rects:?})",
            size.w,
            size.h,
            r.2
        );
        let cells = image_cells_inside(&vt, *r);
        assert!(
            cells >= 4,
            "pane {i} at {}x{} shows no image content ({cells} image cells; rect {r:?})",
            size.w,
            size.h
        );
    }
}

#[test]
fn image_widgets_in_unsized_row_survive_field_size_70x60() {
    assert_image_widget_row_sound(Size::new(70, 60));
}

#[test]
fn image_widgets_in_unsized_row_survive_100x30() {
    assert_image_widget_row_sound(Size::new(100, 30));
}

// ---------------------------------------------------------------------------
// The engine seam under both fixes: `Element::measure` feeds Auto
// sizing exactly like a text leaf's measurement.
// ---------------------------------------------------------------------------

#[test]
fn element_measure_feeds_auto_sizing() {
    use abstracttui::reactive::create_root;
    use abstracttui::ui::{BufferCanvas, UiTree};
    let mut tree = UiTree::new(Size::new(20, 4));
    let (root, ()) = create_root(|cx| {
        let view = Element::new()
            .style(LayoutStyle::row())
            .child(
                // A draw widget that DECLARES its content size: it must
                // occupy exactly 7 columns in the unsized row, where a
                // measureless draw element would have collapsed to 0.
                Element::new()
                    .measure(|_avail| Size::new(7, 2))
                    .draw(|canvas, rect| {
                        for y in rect.y..rect.bottom() {
                            for x in rect.x..rect.right() {
                                canvas.put(Point::new(x, y), '#', Rgba::WHITE, Rgba::TRANSPARENT);
                            }
                        }
                    })
                    .build(),
            )
            .child(text("after"))
            .build();
        tree.mount(cx, view);
    });
    let mut canvas = BufferCanvas::new(Size::new(20, 4));
    tree.draw(&mut canvas);
    assert_eq!(
        canvas.row_text(0).trim_end(),
        "#######after",
        "the measured leaf takes its declared 7 columns, the text follows"
    );
    root.dispose();
}
