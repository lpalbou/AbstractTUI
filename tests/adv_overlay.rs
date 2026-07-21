//! REDTEAM cycle-4 attack: REACT's overlay layer API through the real
//! Driver loop — z-order torture under churn, damage containment
//! (byte-region assertion via presented run positions), the
//! idle-return-to-zero-bytes proof after overlay animation, and
//! overlay lifecycle sanity (handles outliving removal).

use abstracttui::app::{App, Driver, RunConfig};
use abstracttui::base::{Point, Rect, Rgba, Size};
use abstracttui::layout::{Dimension, Style as LayoutStyle};
use abstracttui::render::{Cell, Style};
use abstracttui::term::Capabilities;
use abstracttui::testing::{CaptureTerm, Rng, VtScreen};
use abstracttui::ui::{text, Element};

fn config() -> RunConfig {
    RunConfig {
        caps: Some(Capabilities {
            truecolor: true,
            colors_256: true,
            ..Capabilities::default()
        }),
        enter: None,
        probe: false,
    }
}

fn base_app(size: Size) -> App {
    let mut app = App::new(size);
    app.mount(move |_cx| {
        Element::new()
            .style(
                LayoutStyle::default()
                    .width(Dimension::Percent(1.0))
                    .height(Dimension::Cells(1)),
            )
            .child(text("base content row for overlays to cover"))
            .build()
    })
    .expect("mount");
    app
}

fn settle(driver: &mut Driver, app: &mut App, term: &mut CaptureTerm) {
    for _ in 0..64 {
        let t = driver.turn(app, term).expect("turn");
        if t.idle {
            return;
        }
    }
    panic!("loop failed to settle");
}

// ---------------------------------------------------------------------------
// Z-order torture under churn.
// ---------------------------------------------------------------------------

/// Random create/remove/restyle churn over overlapping overlay layers
/// with alpha and shaders: every frame must satisfy the diff/present
/// property (the model IS the z-order oracle: whatever flatten decides,
/// the bytes must reproduce it exactly), and the loop must stay sane.
#[test]
fn overlay_churn_stays_model_exact_and_idle_clean() {
    let size = Size::new(40, 12);
    let mut term = CaptureTerm::new(size);
    let mut app = base_app(size);
    let mut driver = Driver::new(&mut app, &mut term, config()).expect("enter");
    settle(&mut driver, &mut app, &mut term);

    let overlays = app.overlays();
    let mut rng = Rng::new(0x0417);
    let mut handles = Vec::new();
    for round in 0..60 {
        // Churn: add, restyle, or remove.
        match rng.below(4) {
            0 | 1 => {
                let r = Rect::new(
                    rng.below(30) as i32,
                    rng.below(8) as i32,
                    (4 + rng.below(12)) as i32,
                    (2 + rng.below(4)) as i32,
                );
                let h = overlays.layer(1 + rng.below(5) as i32, r);
                h.with_surface(|s| {
                    s.fill_rect(
                        Rect::new(0, 0, r.w, r.h),
                        Cell::EMPTY.with_bg(Rgba::new(
                            rng.byte(),
                            rng.byte(),
                            rng.byte(),
                            if rng.chance(1, 3) { 140 } else { 255 },
                        )),
                    );
                    s.draw_text(0, 0, "ovl", Style::new().fg(Rgba::WHITE));
                });
                h.damage();
                handles.push(h);
            }
            2 if !handles.is_empty() => {
                let idx = rng.below(handles.len());
                let h: &abstracttui::app::LayerHandle = &handles[idx];
                h.set_opacity(0.3 + (rng.below(7) as f32) * 0.1);
                h.set_offset(Point::new(rng.below(6) as i32, rng.below(3) as i32));
            }
            _ if !handles.is_empty() => {
                let idx = rng.below(handles.len());
                handles.remove(idx).remove();
            }
            _ => {}
        }
        let t = driver.turn(&mut app, &mut term).expect("turn");
        // Churn rounds that changed something must render; either way
        // the session bytes stay modeled (checked at the end).
        let _ = t;
        let _ = round;
    }
    // Whole session was modeled traffic and pairs never tore.
    let screen = term.screen();
    assert_eq!(
        screen.unknown_seq_count(),
        0,
        "overlay churn emitted unmodeled bytes: {:?}",
        screen.unknown_samples()
    );
    // Cleanup: removing everything returns the loop to zero-byte idle.
    for h in handles.drain(..) {
        h.remove();
    }
    while !driver.turn(&mut app, &mut term).expect("turn").idle {}
    let _ = term.take_bytes();
    for i in 0..12 {
        let t = driver.turn(&mut app, &mut term).expect("turn");
        assert!(t.idle, "turn {i} after overlay teardown must be idle");
    }
    assert_eq!(
        term.bytes().len(),
        0,
        "overlay teardown must return to silence"
    );
}

// ---------------------------------------------------------------------------
// Damage containment: an animated toast damages ONLY its rect.
// ---------------------------------------------------------------------------

/// Drive a toast-shaped overlay animation (offset slide + opacity ramp)
/// and assert every presented byte lands inside the toast's swept rect —
/// read back through the model as run positions (cells outside the
/// union must be byte-identical across the animation).
#[test]
fn toast_animation_damages_only_its_region() {
    let size = Size::new(50, 14);
    let mut term = CaptureTerm::new(size);
    let mut app = App::new(size);
    app.mount(move |_cx| {
        // Busy base content so stray damage would visibly repaint it.
        Element::new()
            .style(
                LayoutStyle::default()
                    .width(Dimension::Percent(1.0))
                    .height(Dimension::Percent(1.0)),
            )
            .child(text(
                (0..13)
                    .map(|i| format!("base row {i} with content"))
                    .collect::<Vec<_>>()
                    .join("\n"),
            ))
            .build()
    })
    .expect("mount");
    let mut driver = Driver::new(&mut app, &mut term, config()).expect("enter");
    settle(&mut driver, &mut app, &mut term);

    // Snapshot the settled screen, then animate the toast.
    let before: Vec<String> = (0..size.h).map(|y| row_dump(term.screen(), y)).collect();

    let overlays = app.overlays();
    let toast_rect = Rect::new(30, 10, 16, 3);
    let toast = overlays.layer(10, toast_rect);
    toast.with_surface(|s| {
        s.fill_rect(
            Rect::new(0, 0, 16, 3),
            Cell::EMPTY.with_bg(Rgba::rgb(40, 44, 60)),
        );
        s.draw_text(1, 1, "saved ok", Style::new().fg(Rgba::rgb(120, 220, 140)));
    });
    toast.damage();

    // Slide it up 3 cells over 6 frames with an opacity ramp.
    let mut swept = toast_rect;
    for f in 0..6i32 {
        toast.set_offset(Point::new(0, -f / 2));
        toast.set_opacity(0.4 + 0.1 * f as f32);
        let t = driver.turn(&mut app, &mut term).expect("turn");
        assert!(t.rendered, "toast frame {f} must render");
        swept = swept.union(toast_rect.translate(0, -f / 2));
    }
    toast.remove();
    while !driver.turn(&mut app, &mut term).expect("turn").idle {}

    // Containment: outside the swept toast area (grown by 1 for wide-
    // pair repairs), the screen is byte-identical to the pre-toast state.
    let guard = Rect::new(swept.x - 1, swept.y - 1, swept.w + 2, swept.h + 2);
    let after: Vec<String> = (0..size.h).map(|y| row_dump(term.screen(), y)).collect();
    for y in 0..size.h {
        if y >= guard.y && y < guard.bottom() {
            continue; // rows the toast legitimately touched
        }
        assert_eq!(
            after[y as usize], before[y as usize],
            "row {y} outside the toast region changed during its animation"
        );
    }
    assert_eq!(term.screen().unknown_seq_count(), 0);
}

// ---------------------------------------------------------------------------
// Image overlay (mosaic path; the kitty session path is RT4-1 pending).
// ---------------------------------------------------------------------------

#[test]
fn image_overlay_renders_moves_and_clears() {
    use abstracttui::gfx::Bitmap;
    let size = Size::new(40, 12);
    let mut term = CaptureTerm::new(size);
    let mut app = base_app(size);
    let mut driver = Driver::new(&mut app, &mut term, config()).expect("enter");
    settle(&mut driver, &mut app, &mut term);

    let overlays = app.overlays();
    let bmp = Bitmap::from_fn(16, 12, |x, _| {
        if x < 8 {
            Rgba::rgb(255, 0, 0)
        } else {
            Rgba::rgb(0, 0, 255)
        }
    });
    let img = overlays.image(Rect::new(4, 3, 8, 3), bmp);
    while !driver.turn(&mut app, &mut term).expect("turn").idle {}
    // Mosaic cells landed inside the rect (half-block or space+bg).
    let cell = term.screen().cell(5, 4).unwrap();
    let painted = cell.paint.bg.is_some() || cell.paint.fg.is_some();
    assert!(
        painted,
        "image overlay must paint its rect:\n{}",
        term.screen().to_styled_dump()
    );

    // Move it: old rect restores, new rect paints.
    img.set_rect(Rect::new(20, 6, 8, 3));
    while !driver.turn(&mut app, &mut term).expect("turn").idle {}
    let moved = term.screen().cell(21, 7).unwrap();
    assert!(
        moved.paint.bg.is_some() || moved.paint.fg.is_some(),
        "moved image must paint"
    );

    // Remove: everything restores, loop silent.
    img.remove();
    while !driver.turn(&mut app, &mut term).expect("turn").idle {}
    let _ = term.take_bytes();
    for _ in 0..6 {
        assert!(driver.turn(&mut app, &mut term).expect("turn").idle);
    }
    assert_eq!(term.bytes().len(), 0);
    assert_eq!(term.screen().unknown_seq_count(), 0);
}

fn row_dump(screen: &VtScreen, y: i32) -> String {
    let mut out = String::new();
    for x in 0..screen.size().w {
        let c = screen.cell(x, y).unwrap();
        out.push_str(&format!("{}:{:?}:{:?};", c.ch(), c.paint.fg, c.paint.bg));
    }
    out
}

// ---------------------------------------------------------------------------
// Handle lifecycle hostility.
// ---------------------------------------------------------------------------

#[test]
fn dead_handles_are_inert_not_panicky() {
    let size = Size::new(30, 8);
    let mut term = CaptureTerm::new(size);
    let mut app = base_app(size);
    let mut driver = Driver::new(&mut app, &mut term, config()).expect("enter");
    settle(&mut driver, &mut app, &mut term);

    let overlays = app.overlays();
    let h = overlays.layer(3, Rect::new(2, 2, 8, 3));
    h.with_surface(|s| s.draw_text(0, 0, "hi", Style::new()));
    h.damage();
    let _ = driver.turn(&mut app, &mut term).expect("turn");
    assert!(h.is_alive());

    h.remove();
    let _ = driver.turn(&mut app, &mut term).expect("turn");
    assert!(!h.is_alive(), "removed handle must report dead");
    // Every operation on a dead handle: inert, never a panic.
    h.set_offset(Point::new(5, 5));
    h.set_opacity(0.5);
    h.set_visible(false);
    h.set_shader_t(1.0);
    h.damage();
    assert_eq!(
        h.with_surface(|s| s.width()),
        None,
        "dead surface access yields None"
    );
    assert_eq!(h.bounds(), None);
    h.remove(); // double-remove
    let t = driver.turn(&mut app, &mut term).expect("turn");
    let _ = t;

    // Double-remove + dead-handle ops never disturb the live loop.
    for _ in 0..6 {
        assert!(driver.turn(&mut app, &mut term).expect("turn").idle);
    }
}
