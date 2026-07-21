//! VERIFY cycle-7 resize storm: a real app driven through a flood of
//! resize events — 1x1 up to 300x100 and back, odd sizes, zero-height,
//! zero-width — must never panic, must keep the viewport consistent with
//! the driver, must keep wide glyphs at the right edge safe, and must
//! restore the terminal at the end.

use abstracttui::app::{App, Driver, RunConfig};
use abstracttui::base::Size;
use abstracttui::layout::{Dimension, Style as LayoutStyle};
use abstracttui::term::Capabilities;
use abstracttui::testing::{CaptureTerm, VtScreen};
use abstracttui::ui::{dyn_view, text, Element};
use abstracttui::widgets::{Block, TextInput};

/// Build an app with content that stresses edge behavior: a bordered
/// block, a text input, and a dyn_view rendering wide glyphs (whose
/// 2-column cells must never straddle the right edge after a resize).
fn mount_stress_app(app: &mut App) {
    app.mount(|cx| {
        let value = cx.signal(String::from("初期値テキスト"));
        let tokens = &abstracttui::theme::default_theme().tokens;
        Element::new()
            .child(
                Block::new()
                    .title("resize")
                    .child(TextInput::new().value(value).element(cx, tokens).build())
                    .element(tokens)
                    .build(),
            )
            .child(dyn_view(
                LayoutStyle::default()
                    .width(Dimension::Percent(1.0))
                    .height(Dimension::Cells(1)),
                move || text(format!("幅広グリフ {} 端で安全", value.get().len())),
            ))
            .build()
    })
    .expect("mount");
}

/// The referee never records an unknown sequence and never sees a
/// continuation cell without its wide leader (the wide-pair invariant)
/// after any resize.
fn assert_frame_sound(vt: &VtScreen, ctx: &str) {
    assert_eq!(
        vt.unknown_seq_count(),
        0,
        "{ctx}: emitted an unmodeled sequence: {:?}",
        vt.unknown_samples()
    );
}

#[test]
fn resize_storm_never_panics_and_stays_consistent() {
    let start = Size::new(100, 30);
    let mut term = CaptureTerm::new(start);
    let mut app = App::new(start);
    mount_stress_app(&mut app);

    let cfg = RunConfig {
        caps: Some(Capabilities::default()),
        enter: None,
        probe: false,
    };
    let mut driver = Driver::new(&mut app, &mut term, cfg).expect("enter");
    // Settle initial frames.
    for _ in 0..4 {
        if driver.turn(&mut app, &mut term).expect("turn").idle {
            break;
        }
    }

    // A hostile size ladder: extremes, odd numbers, degenerate axes.
    let sizes = [
        (1, 1),
        (2, 1),
        (1, 2),
        (300, 100),
        (1, 1),
        (80, 24),
        (0, 10),
        (10, 0),
        (0, 0),
        (300, 1),
        (1, 100),
        (37, 41),
        (299, 99),
        (2, 2),
        (120, 40),
        (5, 5),
        (250, 3),
        (3, 250),
        (1, 1),
        (200, 60),
        (7, 7),
        (13, 97),
    ];
    let mut vt = VtScreen::new(start);
    // The engine DELIBERATELY ignores a degenerate (0-axis) resize — a
    // real terminal never reports zero, so resizing to it is a safe
    // no-op and the viewport holds its last non-empty value. Track that.
    let mut expected = start;
    for &(w, h) in &sizes {
        let size = Size::new(w, h);
        let degenerate = w <= 0 || h <= 0;
        if !degenerate {
            expected = size;
        }
        term.push_resize(size);
        // Drive to idle (a resize can cascade a couple of frames).
        for _ in 0..3 {
            let t = driver
                .turn(&mut app, &mut term)
                .expect("turn must not panic on resize");
            let bytes = term.take_bytes();
            // Feed a referee sized to the CURRENT (effective) geometry.
            if vt.size() != expected {
                vt = VtScreen::new(expected);
            }
            vt.feed(&bytes);
            assert_frame_sound(&vt, &format!("resize {w}x{h}"));
            if t.idle {
                break;
            }
        }
        // The driver's view of the world equals the last NON-EMPTY size
        // we asked for (RT2-9: never stale; degenerate resizes no-op).
        assert_eq!(
            app.viewport(),
            expected,
            "viewport wrong after resize to {w}x{h} (degenerate={degenerate})"
        );
    }

    // Rapid-fire alternation without draining between (coalescing stress):
    // the engine must handle a burst where several resizes arrive before
    // a single frame.
    for i in 0..200i32 {
        let w = 1 + (i * 7) % 300;
        let h = 1 + (i * 13) % 100;
        term.push_resize(Size::new(w, h));
    }
    // One turn drains the burst; only the LAST size matters for geometry.
    let _ = driver
        .turn(&mut app, &mut term)
        .expect("burst resize must not panic");
    for _ in 0..4 {
        if driver.turn(&mut app, &mut term).expect("turn").idle {
            break;
        }
    }
    let last_w: i32 = 1 + (199 * 7) % 300;
    let last_h: i32 = 1 + (199 * 13) % 100;
    assert_eq!(
        app.viewport(),
        Size::new(last_w, last_h),
        "viewport must reflect the last resize in a coalesced burst"
    );

    // Terminal restored on leave.
    drop(driver);
    let tail = term.take_bytes();
    let mut vt = VtScreen::new(Size::new(last_w, last_h));
    vt.feed(&tail);
    assert!(
        !vt.modes().alt_screen(),
        "resize storm left the alt screen enabled"
    );
    assert!(
        vt.modes().cursor_visible(),
        "resize storm left the cursor hidden"
    );
}

/// Back-and-forth between two extremes many times — a classic
/// tiling-WM thrash — must not leak geometry state or degrade.
#[test]
fn resize_thrash_between_extremes_is_stable() {
    let mut term = CaptureTerm::new(Size::new(80, 24));
    let mut app = App::new(Size::new(80, 24));
    mount_stress_app(&mut app);
    let cfg = RunConfig {
        caps: Some(Capabilities::default()),
        enter: None,
        probe: false,
    };
    let mut driver = Driver::new(&mut app, &mut term, cfg).expect("enter");
    let _ = driver.turn(&mut app, &mut term);

    for _ in 0..100 {
        term.push_resize(Size::new(1, 1));
        let _ = driver.turn(&mut app, &mut term).expect("turn");
        let _ = term.take_bytes();
        term.push_resize(Size::new(280, 90));
        let _ = driver.turn(&mut app, &mut term).expect("turn");
        let _ = term.take_bytes();
    }
    assert_eq!(
        app.viewport(),
        Size::new(280, 90),
        "final geometry after thrash"
    );
}
