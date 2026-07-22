//! Cycle-3 close (reviews/study/r2-cross-review.md, finding F9): an
//! OWNED popup open across a terminal resize used to keep the stale
//! placement solved at open — after a shrink it could sit partly or
//! fully off-viewport while still modal-owning every key (an invisible
//! modal). The fix dismisses the popup on viewport change with the new
//! `DismissReason::Resize`, wired through the reactive viewport signal
//! the driver publishes from `apply_resize`. This test drives the REAL
//! path: wire resize event in via `CaptureTerm`, `Driver::turn`
//! applies it, the popup ends with `Resize` exactly once, and the
//! vacated cells repaint from below.
//!
//! Same harness posture as wave_r2_review.rs (helper duplication
//! across integration files is the house style — each is its own
//! crate). The unit twin (reason + exactly-once + self-scope-disposal
//! tolerance, no driver) lives in `app::anchored::owned::tests`.

use std::cell::RefCell;
use std::rc::Rc;

use abstracttui::app::anchored::{PanelAnchor, PanelWidth};
use abstracttui::app::{App, Driver, RunConfig};
use abstracttui::base::{Rect, Size};
use abstracttui::prelude::*;
use abstracttui::reactive::Scope;
use abstracttui::term::Capabilities;
use abstracttui::testing::{CaptureTerm, VtScreen};
use abstracttui::ui::text;

const W: i32 = 44;
const H: i32 = 14;

fn config() -> RunConfig {
    RunConfig {
        caps: Some(Capabilities::with(|c| {
            c.truecolor = true;
            c.colors_256 = true;
        })),
        enter: None,
        probe: false,
    }
}

fn settle(driver: &mut Driver, app: &mut App, term: &mut CaptureTerm) {
    for _ in 0..64 {
        let turn = driver.turn(app, term).expect("turn");
        if turn.idle {
            return;
        }
    }
    panic!("loop failed to settle within 64 turns");
}

/// Open popup + shrink the terminal through the driver: the popup must
/// end with `Resize` (exactly once), and the resized frame must not
/// carry the popup's content anywhere.
#[test]
fn terminal_resize_dismisses_open_owned_popup_with_resize_reason() {
    let mut app = App::new(Size::new(W, H));
    let holder: Rc<RefCell<Option<Scope>>> = Default::default();
    let h2 = holder.clone();
    app.mount(move |cx| {
        *h2.borrow_mut() = Some(cx);
        Element::new()
            .style(LayoutStyle::column())
            .child(text("== app chrome =="))
            .child(text("trigger row"))
            .build()
    })
    .expect("mount");
    let cx = holder.borrow().expect("scope stashed at mount");
    let overlays = app.overlays();

    let mut term = CaptureTerm::new(Size::new(W, H));
    let mut driver = Driver::new(&mut app, &mut term, config()).expect("driver");
    settle(&mut driver, &mut app, &mut term);

    // Open an owned popup against the trigger row (the imperative shape
    // a component's activation handler uses).
    let reasons: Rc<RefCell<Vec<DismissReason>>> = Default::default();
    let r2 = reasons.clone();
    let popup = Popup::open(
        &overlays,
        cx,
        app.viewport(),
        PanelAnchor {
            rect: Rect::new(2, 1, 12, 1),
        },
        PanelWidth::MatchAnchor,
        Size::new(12, 3),
        |_pcx, _flipped| {
            Element::new()
                .style(
                    LayoutStyle::default()
                        .width(Dimension::Percent(1.0))
                        .height(Dimension::Percent(1.0)),
                )
                .child(text("POPUP-BODY"))
                .build()
        },
    )
    .expect("room below the anchor");
    popup.on_dismiss(move |r| r2.borrow_mut().push(r));
    settle(&mut driver, &mut app, &mut term);
    assert!(popup.is_open());
    assert!(
        term.screen().to_text().contains("POPUP-BODY"),
        "popup painted before the resize:\n{}",
        term.screen().to_text()
    );
    let _ = term.take_bytes(); // drain: the referee below sees only post-resize bytes

    // The terminal SHRINKS while the popup is open (the direction that
    // used to leave a stale rect off-viewport).
    let shrunk = Size::new(W - 10, H - 4);
    term.push_resize(shrunk);
    settle(&mut driver, &mut app, &mut term);

    assert!(!popup.is_open(), "resize dismissed the popup");
    assert_eq!(
        reasons.borrow().as_slice(),
        [DismissReason::Resize],
        "on_dismiss fired exactly once, with Resize"
    );

    // The resize poisons the previous frame, so the next present
    // rewrites every cell — a referee sized to the NEW geometry sees
    // the full frame, and the popup's content must be gone from it.
    // (CaptureTerm's own screen keeps the old grid on scripted resize;
    // byte assertions after a resize go through a fresh VtScreen.)
    let mut vt = VtScreen::new(shrunk);
    vt.feed(&term.take_bytes());
    let after = vt.to_text();
    assert!(
        !after.contains("POPUP-BODY"),
        "vacated popup content repainted from below:\n{after}"
    );
    assert!(
        after.contains("app chrome"),
        "root content survived the resize:\n{after}"
    );
    assert_eq!(vt.unknown_seq_count(), 0, "all resize bytes modeled");

    driver.finish(&mut term).expect("leave");
}
