//! A scrollbar the reader can SEE must own the drag on it, whether or
//! not dragging it would move anything.
//!
//! WHY THIS FILE EXISTS. first-app/1335 gave the strip a `drag_zone` so
//! screen-select mode stands down over the bar. The zone's predicate and
//! the paint's predicate then drifted apart:
//!
//! ```text
//! draw       (scroll.rs)  rect empty            -> paint nothing
//!                         auto_hide && fits     -> blank the strip
//!                         otherwise             -> DRAW THE BAR
//!
//! drag_zone  (scroll.rs)  rect empty            -> None
//!                         auto_hide && fits     -> None
//!                         otherwise             -> metrics(..).overflows()
//!                                                  .then_some(rect)   <-- EXTRA GATE
//! ```
//!
//! `scrollbar_auto_hide` defaults to **false**, so a plainly-built
//! `Scroll::new(body).offset_y(sig)` whose content FITS paints a rail —
//! `scrollbar::draw` consults only `track.w/h`, never `overflows()` —
//! while the zone's `overflows()` gate returns `None`. A visible bar that
//! declares no drag zone, and per 1335's own rule the selection layer
//! then arms an anchor and takes the gesture: dragging on the scrollbar
//! selects text.
//!
//! Reported twice by the operator, most recently as *"when scrolling the
//! cursor on the right, it actually select text"*, and diagnosed with
//! `agora-tui` at `dm:agora-tui--tui#97`. Their hypothesis was that
//! `extent_signal` fed the engine a different `content_h` on plainly-built
//! Scrolls. It does not — `extent_signal` REPLACES the internal signal and
//! receives the same writes (`scroll.rs`, "a caller-bound signal REPLACES
//! the internal one — same writes, app-visible"), so measurement is
//! identical either way. The real variable is `auto_hide`, and the defect
//! is that two predicates describe one strip.
//!
//! THE RULE THIS FILE PINS: the drag zone tracks VISIBILITY, not
//! usefulness. A full-height thumb that cannot move is still chrome, and
//! a no-op drag on it is the correct outcome — text selection starting on
//! the scrollbar is not.
//!
//! THE CONTROL IS THE POINT. An arm asserting `drag_owner` passes just as
//! well when the probe always says true. So the auto-hidden arm — where
//! the bar is genuinely blanked and the zone SHOULD be absent — runs the
//! same probe in the same process and must read false.
//!
//! OWNER: tui.

use abstracttui::app::{App, Driver, RunConfig};
use abstracttui::base::{Point, Size};
use abstracttui::testing::CaptureTerm;
use abstracttui::widgets::Scroll;

const VIEWPORT: Size = Size::new(80, 24);
/// The strip is the rightmost column of the scroll's box.
const STRIP_X: i32 = VIEWPORT.w - 1;
const STRIP_PROBE: Point = Point::new(STRIP_X, 6);

fn config() -> RunConfig {
    RunConfig {
        enter: None,
        probe: false,
        ..RunConfig::default()
    }
}

fn body(lines: usize) -> String {
    (0..lines)
        .map(|i| format!("line {i}"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Mount one `Scroll` and report whether the strip column owns drags.
fn strip_owns_drag(lines: usize, auto_hide: bool) -> bool {
    let mut term = CaptureTerm::new(VIEWPORT);
    let mut app = App::new(VIEWPORT);
    app.mount(move |cx| {
        let mut s = Scroll::new(abstracttui::ui::text(body(lines))).offset_y(cx.signal(0i32));
        if auto_hide {
            s = s.scrollbar_auto_hide(true);
        }
        s.view(cx)
    })
    .expect("mount");
    let mut driver = Driver::new(&mut app, &mut term, config()).expect("driver");
    for _ in 0..64 {
        if driver.turn(&mut app, &mut term).expect("turn").idle {
            break;
        }
    }
    app.tree().press_probe_at(STRIP_PROBE).drag_owner
}

#[test]
fn a_visible_scrollbar_owns_the_drag_even_when_the_content_fits() {
    // OVERFLOWING: the bar is visible and useful. If this reads false the
    // probe or the strip geometry is wrong and every claim below is
    // measuring nothing — this is the arm that proves the instrument.
    let overflowing = strip_owns_drag(200, false);
    assert!(
        overflowing,
        "an OVERFLOWING scrollbar does not own the drag at {STRIP_PROBE:?}. That is 1335 \
         itself regressing, not the fitting-content case this file is about — and it means \
         the arms below prove nothing. Check the strip's rect and `press_probe_at` before \
         reading anything else here."
    );

    // AUTO-HIDDEN AND FITTING: the strip is blanked, so there is nothing
    // to grab and the zone SHOULD be absent. This is the control — it is
    // what stops "always true" from passing this file.
    let hidden = strip_owns_drag(3, true);
    assert!(
        !hidden,
        "an AUTO-HIDDEN scrollbar over fitting content claims the drag at {STRIP_PROBE:?}. \
         The bar is blanked there, so the strip is invisible chrome claiming a gesture the \
         reader cannot see a target for — and it also means the assertion below would pass \
         no matter what the code did."
    );

    // THE DEFECT. Default `scrollbar_auto_hide` is false, so the rail is
    // PAINTED over fitting content — `scrollbar::draw` gates only on
    // `track.w/h`, never on `overflows()`. The zone must agree with the
    // paint.
    let visible_but_fitting = strip_owns_drag(3, false);
    assert!(
        visible_but_fitting,
        "a VISIBLE scrollbar over fitting content declares no drag zone at {STRIP_PROBE:?}. \
         `scrollbar_auto_hide` defaults to false so the rail is painted, but the zone's \
         `overflows()` gate returned None — and per first-app/1335 the selection layer then \
         arms an anchor and a drag on the bar selects text. That is the operator's \
         twice-reported symptom. The zone must track VISIBILITY, not whether the drag would \
         move anything: see `scroll.rs`'s `drag_zone`, whose predicate has to match the \
         `draw` closure's."
    );
}
