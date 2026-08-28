//! Screen-space anchor pins (the select-inside-modal P1; the
//! gateway-console field report — console-side finding 1050). Overlay
//! trees lay out LAYER-LOCAL and the compositor applies the layer's
//! origin at paint (`overlays::Overlays::create`), but every popup in
//! the anchored family used to capture its anchor from the draw/event
//! rect — layer-local — and hand it to `place_panel`, which places in
//! VIEWPORT space. Inside a centered Modal the popup therefore opened
//! displaced to the top-left by exactly the modal's origin, and the
//! below/above viewport-edge flip judged space against the LOCAL y.
//! Root-layer widgets masked the bug (origin 0,0).
//!
//! These tests reproduce the consumer's recipe through the REAL
//! `Driver`/`CaptureTerm` (a Select inside a centered Modal at 110x34)
//! and pin the symmetric family: Combobox, MultiSelect, Completion,
//! Tooltip, the flip judged at the SCREEN anchor, and the root-layer
//! control that must stay byte-identical. Split file, `#[path]`-included
//! as `anchored::layer_tests`.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use crate::base::{Point, Rect, Size};
use crate::layout::{Dimension, Style as LayoutStyle};
use crate::reactive::{create_root, flush_effects, run_due_timers, Scope};
use crate::term::Capabilities;
use crate::testing::CaptureTerm;
use crate::ui::{text, Element, MouseEvent, MouseKind, UiEvent, View};
use crate::widgets::{TextArea, TextAreaState};

use super::super::driver::{Driver, RunConfig};
use super::super::overlays::{OverlayContent, Overlays};
use super::super::popups::{Modal, MODAL_Z};
use super::super::select::{Combobox, MultiSelect, Select, SelectOption};
use super::super::App;
use super::{Completion, CompletionCandidate, TipContent, Tooltip};

const VP: Size = Size::new(110, 34);

fn options3() -> Vec<SelectOption> {
    vec![
        SelectOption::new("alpha"),
        SelectOption::new("beta"),
        SelectOption::new("gamma"),
    ]
}

fn options5() -> Vec<SelectOption> {
    vec![
        SelectOption::new("alpha"),
        SelectOption::new("beta"),
        SelectOption::new("gamma"),
        SelectOption::new("delta"),
        SelectOption::new("epsilon"),
    ]
}

fn face_layout() -> LayoutStyle {
    LayoutStyle::default().w(24).h(1).shrink(0.0)
}

/// The consumer's rig: a real `App` + `Driver` + `CaptureTerm` at
/// 110x34 with a plain root underneath; tests open a Modal on top.
struct DriverRig {
    app: App,
    term: CaptureTerm,
    driver: Driver,
    overlays: Overlays,
    scope: Scope,
}

fn driver_rig() -> DriverRig {
    let mut term = CaptureTerm::new(VP);
    let mut app = App::new(VP);
    let overlays = app.overlays();
    let holder: Rc<RefCell<Option<Scope>>> = Default::default();
    let h = holder.clone();
    app.mount(move |cx| {
        *h.borrow_mut() = Some(cx);
        Element::new()
            .style(LayoutStyle::column())
            .child(text("root content under the modal"))
            .build()
    })
    .expect("mount");
    let cfg = RunConfig {
        caps: Some(Capabilities::with(|c| {
            c.truecolor = true;
            c.colors_256 = true;
        })),
        enter: None,
        probe: false,
        ..RunConfig::default()
    };
    let driver = Driver::new(&mut app, &mut term, cfg).expect("driver");
    let scope = holder.borrow().expect("mount scope");
    DriverRig {
        app,
        term,
        driver,
        overlays,
        scope,
    }
}

impl DriverRig {
    fn settle(&mut self) {
        for _ in 0..64 {
            if self
                .driver
                .turn(&mut self.app, &mut self.term)
                .expect("turn")
                .idle
            {
                break;
            }
        }
    }

    fn input(&mut self, bytes: &[u8]) {
        self.term.push_input(bytes);
        self.settle();
    }

    fn screen(&self) -> String {
        self.term.screen().to_text()
    }

    fn line(&self, y: i32) -> String {
        self.screen()
            .lines()
            .nth(y as usize)
            .unwrap_or_default()
            .to_string()
    }

    /// The owned popup's layer bounds: the MODAL tree layer stacked
    /// ABOVE the Modal itself (`top_z() + 1` allocation).
    fn popup_bounds(&self) -> Option<Rect> {
        let store = self.overlays.store().borrow();
        store
            .meta
            .iter()
            .zip(&store.layers)
            .filter(|(m, l)| {
                matches!(m.content, OverlayContent::Tree { modal: true, .. }) && l.z() > MODAL_Z
            })
            .map(|(_, l)| l.bounds())
            .next()
    }

    /// The passive panel's layer bounds (non-modal overlay tree).
    fn passive_panel_bounds(&self) -> Option<Rect> {
        let store = self.overlays.store().borrow();
        store
            .meta
            .iter()
            .zip(&store.layers)
            .find_map(|(m, l)| match &m.content {
                OverlayContent::Tree { modal: false, .. } => Some(l.bounds()),
                _ => None,
            })
    }
}

// ------------------------------------------------------------- the P1

/// The operator's screenshot, reproduced: a Select inside a centered
/// Modal at 110x34. The modal centers at (35, 13); panel padding puts
/// the trigger row at SCREEN (36, 15). The popup must open directly
/// under the trigger — before the fix it opened at the LAYER-LOCAL
/// anchor (1, 3), i.e. displaced to the top-left by the modal origin.
#[test]
fn select_popup_inside_centered_modal_opens_adjacent_to_trigger() {
    let mut rig = driver_rig();
    rig.settle();
    let _modal = Modal::open(&rig.overlays, rig.scope, VP, Size::new(40, 7), |mcx| {
        Element::new()
            .style(
                LayoutStyle::column()
                    .width(Dimension::Percent(1.0))
                    .height(Dimension::Percent(1.0)),
            )
            .child(text("pick a channel"))
            .child(Select::new(options3()).layout(face_layout()).view(mcx))
            .build()
    });
    rig.settle();
    let trigger = Rect::new(36, 15, 24, 1);
    assert!(
        rig.line(trigger.y).contains("choose"),
        "trigger renders on its screen row:\n{}",
        rig.screen()
    );
    rig.input(b"\r"); // Enter on the modal-focused trigger opens
    let popup = rig.popup_bounds().expect("popup layer open");
    assert_eq!(
        popup,
        Rect::new(trigger.x, trigger.bottom(), trigger.w, 3),
        "popup adjacent to the trigger in SCREEN space (the P1: it \
         opened displaced to the top-left by the modal's origin)"
    );
    assert!(
        rig.line(trigger.bottom()).contains("alpha"),
        "first option row painted directly under the trigger:\n{}",
        rig.screen()
    );
}

/// Symmetric pin: the Combobox popup INCLUDES the anchor row (its
/// editor mounts over the trigger), so its layer must START at the
/// trigger's SCREEN row.
#[test]
fn combobox_popup_inside_modal_mounts_editor_over_screen_trigger_row() {
    let mut rig = driver_rig();
    rig.settle();
    let _modal = Modal::open(&rig.overlays, rig.scope, VP, Size::new(40, 7), |mcx| {
        Element::new()
            .style(
                LayoutStyle::column()
                    .width(Dimension::Percent(1.0))
                    .height(Dimension::Percent(1.0)),
            )
            .child(text("pick a model"))
            .child(Combobox::new(options3()).layout(face_layout()).view(mcx))
            .build()
    });
    rig.settle();
    let trigger = Rect::new(36, 15, 24, 1);
    rig.input(b"\r");
    let popup = rig.popup_bounds().expect("popup layer open");
    // Editor row (over the trigger) + 3 option rows + 1 status row.
    assert_eq!(
        popup,
        Rect::new(trigger.x, trigger.y, trigger.w, 5),
        "anchor-row-inclusive popup starts AT the trigger's screen row"
    );
    assert!(
        rig.line(trigger.y).contains("search"),
        "editor mounted over the trigger row:\n{}",
        rig.screen()
    );
    assert!(
        rig.line(trigger.y + 4).contains("3 of 3"),
        "status row at the popup's screen bottom:\n{}",
        rig.screen()
    );
}

/// Symmetric pin: MultiSelect rides the same anchor capture.
#[test]
fn multiselect_popup_inside_modal_opens_adjacent_to_screen_trigger() {
    let mut rig = driver_rig();
    rig.settle();
    let _modal = Modal::open(&rig.overlays, rig.scope, VP, Size::new(40, 7), |mcx| {
        Element::new()
            .style(
                LayoutStyle::column()
                    .width(Dimension::Percent(1.0))
                    .height(Dimension::Percent(1.0)),
            )
            .child(text("pick tags"))
            .child(MultiSelect::new(options3()).layout(face_layout()).view(mcx))
            .build()
    });
    rig.settle();
    let trigger = Rect::new(36, 15, 24, 1);
    rig.input(b"\r");
    let popup = rig.popup_bounds().expect("popup layer open");
    assert_eq!(popup, Rect::new(trigger.x, trigger.bottom(), trigger.w, 3));
}

/// The flip-math half of the P1: a Select parked near the BOTTOM of a
/// tall modal sits at screen y=30 of 34 — 3 rows below, 30 above — so
/// the popup must flip ABOVE the trigger. Judged at the LOCAL y (28 in
/// a 34-row space) the old math kept it below.
#[test]
fn select_popup_flips_above_when_modal_trigger_sits_near_screen_bottom() {
    let mut rig = driver_rig();
    rig.settle();
    let _modal = Modal::open(&rig.overlays, rig.scope, VP, Size::new(40, 30), |mcx| {
        Element::new()
            .style(
                LayoutStyle::column()
                    .width(Dimension::Percent(1.0))
                    .height(Dimension::Percent(1.0)),
            )
            .child(
                Element::new()
                    .style(LayoutStyle::default().grow(1.0))
                    .build(),
            )
            .child(Select::new(options5()).layout(face_layout()).view(mcx))
            .build()
    });
    rig.settle();
    // Modal (40, 30) centers at (35, 2); the spacer parks the trigger
    // on the content box's LAST row: local (1, 28) -> screen (36, 30).
    let trigger = Rect::new(36, 30, 24, 1);
    assert!(
        rig.line(trigger.y).contains("choose"),
        "trigger on its screen row:\n{}",
        rig.screen()
    );
    rig.input(b"\r");
    let popup = rig.popup_bounds().expect("popup layer open");
    assert_eq!(
        popup,
        Rect::new(trigger.x, trigger.y - 5, trigger.w, 5),
        "3 rows below vs 30 above at the SCREEN anchor: the popup \
         flips ABOVE and ends adjacent to the trigger row"
    );
    assert_eq!(
        popup.bottom(),
        trigger.y,
        "flipped popup's bottom edge touches the trigger"
    );
}

/// Completion inside a modal: the dropdown anchors at the caret's
/// SCREEN cell. The caret cell signal publishes LAYER-LOCAL (the
/// textarea's own `current_rect`), so the controller owns the
/// translation.
#[test]
fn completion_dropdown_inside_modal_anchors_at_screen_caret() {
    let mut rig = driver_rig();
    rig.settle();
    let state_holder: Rc<RefCell<Option<TextAreaState>>> = Default::default();
    let sh = state_holder.clone();
    let ov = rig.overlays.clone();
    let _modal = Modal::open(&rig.overlays, rig.scope, VP, Size::new(40, 7), move |mcx| {
        let t = crate::widgets::theme_tokens(mcx);
        let state = TextAreaState::new(mcx);
        *sh.borrow_mut() = Some(state.clone());
        let composer = TextArea::new()
            .state(&state)
            .rows(1, 3)
            .element(mcx, &t)
            .build();
        Completion::new()
            .trigger('/', |query| {
                ["help", "theme", "clear"]
                    .iter()
                    .filter(|c| c.starts_with(query))
                    .map(|c| CompletionCandidate::new(format!("/{c}"), format!("/{c} ")))
                    .collect()
            })
            .max_visible(3)
            .attach(mcx, &ov, &state, composer)
    });
    rig.settle();
    rig.input(b"/");
    let state = state_holder.borrow().clone().expect("state");
    let cell = state
        .caret_cell()
        .get_untracked()
        .expect("focused composer published its caret");
    // The caret cell is LAYER-LOCAL; the modal sits at (35, 13).
    let screen_cell = Point::new(cell.x + 35, cell.y + 13);
    let panel = rig.passive_panel_bounds().expect("dropdown open");
    assert_eq!(
        (panel.x, panel.y),
        (screen_cell.x, screen_cell.y + 1),
        "dropdown directly under the caret's SCREEN cell (caret local \
         {cell:?}, modal at (35, 13)); got {panel:?}"
    );
    assert!(
        rig.line(panel.y).contains("/help"),
        "candidates painted at the screen position:\n{}",
        rig.screen()
    );
}

/// Root-layer control (regression pin): at origin (0,0) the behavior
/// must stay exactly what 0.2.19 shipped — popup right under the
/// trigger at the same cells, same rows on screen.
#[test]
fn root_layer_select_popup_geometry_unchanged() {
    let mut term = CaptureTerm::new(VP);
    let mut app = App::new(VP);
    let overlays = app.overlays();
    app.mount(move |cx| {
        Element::new()
            .style(LayoutStyle::column())
            .child(text("header row"))
            .child(Select::new(options3()).layout(face_layout()).view(cx))
            .build()
    })
    .expect("mount");
    let cfg = RunConfig {
        caps: Some(Capabilities::with(|c| {
            c.truecolor = true;
            c.colors_256 = true;
        })),
        enter: None,
        probe: false,
        ..RunConfig::default()
    };
    let mut driver = Driver::new(&mut app, &mut term, cfg).expect("driver");
    let mut settle = |app: &mut App, term: &mut CaptureTerm| {
        for _ in 0..64 {
            if driver.turn(app, term).expect("turn").idle {
                break;
            }
        }
    };
    settle(&mut app, &mut term);
    term.push_input(b"\t"); // focus the trigger
    settle(&mut app, &mut term);
    term.push_input(b"\r");
    settle(&mut app, &mut term);
    let popup = {
        let store = overlays.store().borrow();
        store
            .meta
            .iter()
            .zip(&store.layers)
            .find_map(|(m, l)| match &m.content {
                OverlayContent::Tree { modal: true, .. } => Some(l.bounds()),
                _ => None,
            })
            .expect("popup open")
    };
    assert_eq!(
        popup,
        Rect::new(0, 2, 24, 3),
        "origin (0,0): local == screen, placement byte-identical"
    );
    let screen = term.screen().to_text();
    let line2 = screen.lines().nth(2).unwrap_or_default();
    assert!(line2.contains("alpha"), "first option on row 2:\n{screen}");
}

// -------------------------------------------------- tooltip (bare store)

/// Tooltip anchored inside a POSITIONED non-modal overlay (the drawer
/// shape, unit-level): the tip must appear under the hovered element's
/// SCREEN row, not under its layer-local row.
#[test]
fn tooltip_inside_positioned_overlay_places_tip_at_screen_position() {
    let vp = Size::new(60, 20);
    super::super::viewport::publish_viewport(vp);
    let overlays = Overlays::new();
    overlays.ensure_root(vp);
    let panel_bounds = Rect::new(10, 4, 20, 6);
    let ov = overlays.clone();
    let (root, _layer) = create_root(|cx| {
        let target = Element::new()
            .style(LayoutStyle::line(1).w(10))
            .child(text("hover me"))
            .build();
        let wrapped = Tooltip::attach(cx, &ov, "the tip", Duration::ZERO, target);
        let view = Element::new()
            .style(
                LayoutStyle::column()
                    .width(Dimension::Percent(1.0))
                    .height(Dimension::Percent(1.0)),
            )
            .child(wrapped)
            .child(text("below"))
            .build();
        ov.layer_tree(5, panel_bounds, false, cx, view)
    });
    // Hover the wrapped element at SCREEN (12, 4) = panel-local (2, 0).
    overlays.dispatch(&UiEvent::Mouse(MouseEvent {
        pos: Point::new(12, 4),
        kind: MouseKind::Move,
        mods: crate::ui::Mods::NONE,
    }));
    flush_effects();
    run_due_timers(Instant::now());
    let tip = {
        let store = overlays.store().borrow();
        store
            .meta
            .iter()
            .zip(&store.layers)
            .find_map(|(m, l)| match &m.content {
                OverlayContent::Draw { .. } => Some(l.bounds()),
                _ => None,
            })
            .expect("tip layer open")
    };
    assert_eq!(
        (tip.x, tip.y),
        (10, 5),
        "tip under the hovered element's SCREEN row (panel at (10, 4)), \
         not its layer-local row; got {tip:?}"
    );
    root.dispose();
}

// ---------------------------------------------------------------------
// TipContent::Card — the rich hover preview (laurent, dm:laurent--tui#19)
// ---------------------------------------------------------------------

/// A real App + Driver + CaptureTerm whose ROOT is a tip-wrapped
/// target, so the assertions can read composed cells. Overlay
/// composition lives in the driver, so a hand-rolled `Overlays` cannot
/// answer "what does the user see" — and that is the only question
/// worth asking of a hover card.
struct TipRig {
    app: App,
    term: CaptureTerm,
    driver: Driver,
    overlays: Overlays,
}

fn tip_rig(make: impl Fn(Scope, &Overlays, View) -> View + 'static) -> TipRig {
    let mut term = CaptureTerm::new(VP);
    let mut app = App::new(VP);
    let overlays = app.overlays();
    let ov = overlays.clone();
    app.mount(move |cx| {
        let target = Element::new()
            .style(LayoutStyle::line(1).w(8))
            .child(text("cite"))
            .build();
        Element::new()
            .style(
                LayoutStyle::column()
                    .width(Dimension::Percent(1.0))
                    .height(Dimension::Percent(1.0)),
            )
            .child(make(cx, &ov, target))
            .build()
    })
    .expect("mount");
    let cfg = RunConfig {
        caps: Some(Capabilities::with(|c| {
            c.truecolor = true;
            c.colors_256 = true;
        })),
        enter: None,
        probe: false,
        // DELIBERATELY false. Mode 1003 — motion with no button held —
        // is what a hover tip runs on, and every rig here used to arm
        // it by hand. That is why no test could see the real hole: an
        // app that mounts a tooltip and calls `App::run()` got
        // `ButtonDrag`, so the tip opened on CLICK and never on hover.
        // The tip declares its own need now
        // (`Overlays::require_pointer_motion`), so leaving this off is
        // what keeps that declaration load-bearing.
        hover_ink: false,
        ..RunConfig::default()
    };
    let driver = Driver::new(&mut app, &mut term, cfg).expect("driver");
    TipRig {
        app,
        term,
        driver,
        overlays,
    }
}

impl TipRig {
    fn settle(&mut self) {
        for _ in 0..64 {
            if self
                .driver
                .turn(&mut self.app, &mut self.term)
                .expect("turn")
                .idle
            {
                break;
            }
        }
    }

    /// Move the pointer through the REAL pipeline: SGR 1006 motion
    /// bytes into the terminal, not a hand-built UiEvent. A tip that
    /// only opens for a synthesised event is a tip that never opens.
    fn mouse_to(&mut self, x: i32) {
        self.term
            .push_input(format!("\x1b[<35;{};1M", x + 1).as_bytes());
        self.settle();
        flush_effects();
    }

    /// Keys through the REAL pipeline too — wire bytes into the
    /// terminal, decoded by the same reader an app gets. The mouse
    /// half of this rig learned the lesson already: a trigger that
    /// only fires for a synthesised event is a trigger that never
    /// fires.
    fn keys(&mut self, bytes: &[u8]) {
        self.term.push_input(bytes);
        self.settle();
        flush_effects();
    }

    fn fire_timers(&mut self, after: Duration) {
        run_due_timers(Instant::now() + after);
        flush_effects();
        self.settle();
    }

    fn screen(&self) -> String {
        self.term.screen().to_text()
    }

    fn layer_kinds(&self) -> Vec<&'static str> {
        let store = self.overlays.store().borrow();
        store
            .meta
            .iter()
            .map(|m| match &m.content {
                OverlayContent::Tree { .. } => "tree",
                OverlayContent::Draw { .. } => "draw",
                _ => "other",
            })
            .collect()
    }
}

/// The point of the whole feature: a card tip mounts a TREE and its
/// widget content actually reaches the screen. Asserted on composed
/// cells rather than on the overlay store, because a layer that opened
/// and painted nothing satisfies every store-shaped check.
#[test]
fn a_card_tip_mounts_a_tree_and_its_content_reaches_the_screen() {
    let mut rig = tip_rig(|cx, ov, target| {
        Tooltip::attach_content(
            cx,
            ov,
            TipContent::card(Size::new(24, 3), |_cx| {
                Element::new()
                    .style(LayoutStyle::column())
                    .child(text("cited body"))
                    .build()
            }),
            Duration::ZERO,
            target,
        )
    });
    rig.settle();
    assert!(
        !rig.screen().contains("cited body"),
        "a dormant tip must paint nothing"
    );

    rig.mouse_to(0);
    rig.fire_timers(Duration::ZERO);

    let kinds = rig.layer_kinds();
    assert!(
        kinds.contains(&"tree"),
        "a card must open a TREE layer, not a draw layer; got {kinds:?}"
    );
    assert!(
        !kinds.contains(&"draw"),
        "a card must not fall back to the label's draw path: {kinds:?}"
    );
    assert!(
        rig.screen().contains("cited body"),
        "the card's widget content never reached the screen:\n{}",
        rig.screen()
    );
}

/// The published API is unchanged by the widening: a label tip still
/// takes the zero-tree draw path. `attach` now routes through
/// `attach_content`, so this is the regression guard on that hop.
#[test]
fn a_label_tip_still_takes_the_zero_tree_draw_path() {
    let mut rig =
        tip_rig(|cx, ov, target| Tooltip::attach(cx, ov, "plain tip", Duration::ZERO, target));
    rig.settle();
    rig.mouse_to(0);
    rig.fire_timers(Duration::ZERO);

    let kinds = rig.layer_kinds();
    assert_eq!(
        kinds.iter().filter(|k| **k == "draw").count(),
        1,
        "the label path must stay exactly one draw layer; got {kinds:?}"
    );
    assert!(
        rig.screen().contains("plain tip"),
        "the label never painted:\n{}",
        rig.screen()
    );
}

/// Leave-before-due must defeat a card exactly as it defeats a label.
/// The generation counter is SHARED between the two paths, and this
/// says so by measurement rather than trusting that sharing the code
/// shared the behaviour.
#[test]
fn a_card_tip_does_not_open_when_the_pointer_left_before_the_delay() {
    let mut rig = tip_rig(|cx, ov, target| {
        Tooltip::attach_content(
            cx,
            ov,
            TipContent::card(Size::new(20, 3), |_cx| text("must not appear")),
            Duration::from_millis(50),
            target,
        )
    });
    rig.settle();
    rig.mouse_to(0); // enter: arms the one-shot
    rig.mouse_to(60); // leave before it is due
    rig.fire_timers(Duration::from_millis(100));

    assert!(
        !rig.screen().contains("must not appear"),
        "a stale timer opened a card after the pointer left:\n{}",
        rig.screen()
    );
}

/// Build a card whose content genuinely runs `rows` deep, so a card
/// reported as truncated is one the viewport really could not fit.
fn tall_card(rows: i32) -> TipContent {
    TipContent::card(Size::new(24, rows), move |_cx| {
        let mut col = Element::new().style(LayoutStyle::column());
        for i in 0..rows {
            col = col.child(text(format!("row {i}")));
        }
        col.build()
    })
}

/// A card the viewport cannot fit must SAY it was cut.
///
/// This is the terminal's version of a scrollbar. A browser can clip a
/// preview honestly because the scrollbar is visible evidence there is
/// more; here a 40-row card cut to 33 looks exactly like a 33-row card,
/// so the reader is shown less than there is and told nothing.
/// Asserted on composed cells: a marker layer that opened and painted
/// nothing would satisfy any store-shaped check.
#[test]
fn a_card_too_tall_for_the_viewport_says_how_much_it_hid() {
    let mut rig = tip_rig(|cx, ov, target| {
        Tooltip::attach_content(cx, ov, tall_card(40), Duration::ZERO, target)
    });
    rig.settle();
    rig.mouse_to(0);
    rig.fire_timers(Duration::ZERO);

    let screen = rig.screen();
    // VP is 34 rows and the anchor occupies the first, so the card is
    // lent 33 and the marker displaces one of them: 8 rows unreachable.
    assert!(
        screen.contains("… 8 more"),
        "a card cut from 40 rows to 33 reported nothing:\n{screen}"
    );
    assert!(
        !screen.contains("row 39"),
        "the last row cannot be on screen if the card was truncated:\n{screen}"
    );
}

/// The other half, and the one that makes the guard mean something: a
/// card that FITS must not wear the marker. Without this, a marker
/// hardcoded to always paint passes the test above.
#[test]
fn a_card_that_fits_carries_no_truncation_marker() {
    let mut rig = tip_rig(|cx, ov, target| {
        Tooltip::attach_content(cx, ov, tall_card(3), Duration::ZERO, target)
    });
    rig.settle();
    rig.mouse_to(0);
    rig.fire_timers(Duration::ZERO);

    let screen = rig.screen();
    assert!(
        screen.contains("row 2"),
        "the card did not open at all, so the assertion below proves nothing:\n{screen}"
    );
    assert!(
        !screen.contains("more"),
        "a card that fits whole must not claim it hid rows:\n{screen}"
    );
}

/// A rig whose anchor can MOVE without any mouse input: a leading
/// spacer whose height is a signal, so setting it shifts the anchor row
/// down exactly the way a list scrolling by a keystroke does.
///
/// Returns the rig and the shift signal. The distinction from
/// [`tip_rig`] is the whole point — there the anchor is nailed to row 0,
/// which is precisely the case where this class of bug cannot appear.
/// The two handles [`scrolling_tip_rig`] publishes out of the mount
/// scope: how far the anchor is pushed down, and the anchor's own ink.
type TipHandles = (
    crate::reactive::Signal<i32>,
    crate::reactive::Signal<&'static str>,
);

fn scrolling_tip_rig(
    make: impl Fn(Scope, &Overlays, View) -> View + 'static,
) -> (TipRig, TipHandles) {
    let mut term = CaptureTerm::new(VP);
    let mut app = App::new(VP);
    let overlays = app.overlays();
    let ov = overlays.clone();
    let out: Rc<RefCell<Option<TipHandles>>> = Rc::new(RefCell::new(None));
    let sink = out.clone();
    app.mount(move |cx| {
        let shift = cx.signal(0i32);
        // The anchor's own content, so a test can damage the anchor
        // WITHOUT moving it — a reply-count chip ticking up.
        let label = cx.signal("cite");
        *sink.borrow_mut() = Some((shift, label));
        let target = Element::new()
            .style(LayoutStyle::line(1).w(8))
            .child(crate::ui::dyn_view(LayoutStyle::default(), move || {
                text(label.get())
            }))
            .build();
        Element::new()
            .style(
                LayoutStyle::column()
                    .width(Dimension::Percent(1.0))
                    .height(Dimension::Percent(1.0)),
            )
            .child(
                Element::new()
                    .style_signal(move || LayoutStyle::default().w(1).h(shift.get()))
                    .build(),
            )
            .child(make(cx, &ov, target))
            .build()
    })
    .expect("mount");
    let cfg = RunConfig {
        caps: Some(Capabilities::with(|c| {
            c.truecolor = true;
            c.colors_256 = true;
        })),
        enter: None,
        probe: false,
        // Off on purpose — see `tip_rig`: the tooltip arms mode 1003
        // itself, and an app flag here would hide it if that stopped.
        hover_ink: false,
        ..RunConfig::default()
    };
    let driver = Driver::new(&mut app, &mut term, cfg).expect("driver");
    let handles = out.borrow().expect("mount ran and published its signals");
    (
        TipRig {
            app,
            term,
            driver,
            overlays,
        },
        handles,
    )
}

/// The anchor moves out from under an OPEN card with no mouse input at
/// all — a feed scrolling by one keystroke — and the card must not be
/// left pointing at a row it never described.
///
/// This is the terminal form of the browser bug agora-wui reported at
/// `type-scale-and-hovercards#8`, and it is sharper here: their card
/// sits 8px clear of its chip and dies to a *sampled* pointer crossing,
/// whereas a TUI moves a whole row of content at once and never samples
/// the pointer at all. Hover is recomputed from mouse REPORTS, so with
/// no motion byte arriving there is nothing to synthesise a
/// `MouseLeave` from: the tip's captured anchor silently stops being
/// where the anchor is.
///
/// Falsified against the unguarded implementation, where the card stays
/// open at its open-time coordinates.
#[test]
fn a_card_tip_closes_when_its_anchor_scrolls_out_from_under_it() {
    let (mut rig, (shift, _label)) = scrolling_tip_rig(|cx, ov, target| {
        Tooltip::attach_content(cx, ov, tall_card(3), Duration::ZERO, target)
    });
    rig.settle();
    rig.mouse_to(0);
    rig.fire_timers(Duration::ZERO);
    assert!(
        rig.screen().contains("row 2"),
        "the card never opened, so the assertion below would pass vacuously:\n{}",
        rig.screen()
    );

    // No mouse input: content moves under a stationary pointer.
    shift.set(6);
    flush_effects();
    rig.settle();
    rig.fire_timers(Duration::ZERO);

    assert!(
        !rig.screen().contains("row 2"),
        "the anchor moved 6 rows and the card stayed at its open-time \
         coordinates, describing a line that is no longer there:\n{}",
        rig.screen()
    );
}

/// The other half, and the one that makes the guard above mean
/// something: an anchor that REPAINTS WITHOUT MOVING keeps its card.
///
/// The guard compares a draw-time rect against an event-time capture.
/// Those are two different code paths into the same geometry, and if
/// they ever disagree — a layer origin applied twice, a content box
/// against a border box — every tip would die on its anchor's first
/// repaint while the test above still passed, because it would be
/// closing for the wrong reason.
///
/// So the anchor is damaged without being moved: a chip whose label
/// ticks over at the same width, which is what a live reply count is.
/// The card must survive it. The first assertion is what stops this
/// being decoration — an earlier version of this test drove a no-op
/// layout write instead, the anchor was never damaged, its draw closure
/// never ran, and the test passed against a guard hacked to close
/// unconditionally.
#[test]
fn a_card_tip_survives_its_anchor_repainting_in_place() {
    let (mut rig, (_shift, label)) = scrolling_tip_rig(|cx, ov, target| {
        Tooltip::attach_content(cx, ov, tall_card(3), Duration::ZERO, target)
    });
    rig.settle();
    rig.mouse_to(0);
    rig.fire_timers(Duration::ZERO);
    assert!(rig.screen().contains("row 2"), "the card never opened");

    // Same width, different ink: the anchor is damaged and repaints at
    // exactly the rect it already had.
    label.set("cit3");
    flush_effects();
    rig.settle();
    rig.fire_timers(Duration::ZERO);

    assert!(
        rig.screen().contains("cit3"),
        "the anchor never actually repainted, so the assertion below \
         would prove nothing:\n{}",
        rig.screen()
    );
    assert!(
        rig.screen().contains("row 2"),
        "the anchor repainted where it already was and the card closed \
         anyway — the draw-time rect disagrees with the event-time \
         capture:\n{}",
        rig.screen()
    );
}

/// A label wider than the viewport ellipsises rather than letting the
/// canvas clip it. A clipped label ends on a real word and reads as the
/// whole message — the same silent-truncation class as the card, on the
/// other axis.
#[test]
fn a_label_too_wide_for_the_viewport_ends_in_an_ellipsis() {
    let long = format!("{} END", "wide ".repeat(40));
    let mut rig = tip_rig(move |cx, ov, target| {
        Tooltip::attach(cx, ov, long.clone(), Duration::ZERO, target)
    });
    rig.settle();
    rig.mouse_to(0);
    rig.fire_timers(Duration::ZERO);

    let screen = rig.screen();
    assert!(
        screen.contains('…'),
        "a label 205 cells wide in a 110-cell viewport was cut with no marker:\n{screen}"
    );
    assert!(
        !screen.contains("END"),
        "the tail cannot be on screen if the label was truncated:\n{screen}"
    );
}

// --- keyboard reach -------------------------------------------------
//
// A hover-only affordance is invisible to a keyboard user and to a
// terminal with no mouse reporting at all, and it was invisible here:
// the tip's trigger lived on a WRAPPER element, and focus transitions
// are delivered target-only (`ui::focus`) while hover is delivered
// per-node along the hovered path. So the wrapper could hear the mouse
// and could never hear focus, whatever handler it carried.
//
// These drive Tab and Escape as WIRE BYTES with the pointer never
// moved, so nothing here can pass on a hover that happens to be live.

/// The tip's anchor is FOCUSABLE, and a second focusable follows it so
/// Tab has somewhere to go.
fn focusable_tip_rig(make: impl Fn(Scope, &Overlays, View) -> View + 'static) -> TipRig {
    let mut term = CaptureTerm::new(VP);
    let mut app = App::new(VP);
    let overlays = app.overlays();
    let ov = overlays.clone();
    app.mount(move |cx| {
        let target = Element::new()
            .style(LayoutStyle::line(1).w(8))
            .focusable()
            .child(text("cite"))
            .build();
        Element::new()
            .style(
                LayoutStyle::column()
                    .width(Dimension::Percent(1.0))
                    .height(Dimension::Percent(1.0)),
            )
            .child(make(cx, &ov, target))
            .child(
                Element::new()
                    .style(LayoutStyle::line(1).w(8))
                    .focusable()
                    .child(text("elsewhere"))
                    .build(),
            )
            .build()
    })
    .expect("mount");
    let cfg = RunConfig {
        caps: Some(Capabilities::with(|c| {
            c.truecolor = true;
            c.colors_256 = true;
        })),
        enter: None,
        probe: false,
        // Off on purpose — see `tip_rig`: the tooltip arms mode 1003
        // itself, and an app flag here would hide it if that stopped.
        hover_ink: false,
        ..RunConfig::default()
    };
    let driver = Driver::new(&mut app, &mut term, cfg).expect("driver");
    TipRig {
        app,
        term,
        driver,
        overlays,
    }
}

/// THE guard for the field defect: **a tooltip opened on click and not
/// on hover.**
///
/// `MouseEnter` is recomputed only from mouse REPORTS, and the default
/// session posture (`MouseMode::ButtonDrag`) makes a terminal report
/// motion only while a button is held. So an app that mounted a tooltip
/// and called `App::run()` — doing nothing wrong — got a tip that
/// appeared when you pressed the mouse down and never when you moved
/// over the anchor. Every test rig here armed `hover_ink` by hand, so
/// nothing on this side could see it; laurent found it by using the
/// example.
///
/// Mounting a `Tooltip` now declares the need itself
/// (`Overlays::require_pointer_motion`), and this asserts on the bytes
/// the driver actually wrote to the terminal with `hover_ink` left
/// FALSE. Delete the `require_pointer_motion` call in
/// `Tooltip::attach_content` and this goes red on both halves.
#[test]
fn mounting_a_tooltip_arms_motion_reporting_without_the_app_asking() {
    let mut term = CaptureTerm::new(VP);
    let mut app = App::new(VP);
    let overlays = app.overlays();
    let ov = overlays.clone();
    app.mount(move |cx| {
        Element::new()
            .style(LayoutStyle::column())
            .child(Tooltip::attach(
                cx,
                &ov,
                "a tip",
                Duration::from_millis(1),
                Element::new()
                    .style(LayoutStyle::line(1).w(8))
                    .child(text("cite"))
                    .build(),
            ))
            .build()
    })
    .expect("mount");
    assert!(
        overlays.pointer_motion_required(),
        "mounting a Tooltip must declare that this app needs motion \
         reports — nothing else will ask for them"
    );
    let _driver = Driver::new(
        &mut app,
        &mut term,
        RunConfig {
            probe: false,
            // The app asked for NOTHING. That is the case that broke.
            hover_ink: false,
            ..RunConfig::default()
        },
    )
    .expect("driver");
    assert_eq!(
        term.enter_options().expect("entered").mouse,
        crate::term::MouseMode::AnyMotion,
        "a tooltip app entered in ButtonDrag: the terminal only reports \
         motion while a button is down, so the tip opens on CLICK"
    );
    let wrote = String::from_utf8_lossy(term.bytes()).into_owned();
    assert!(
        wrote.contains("[?1003h"),
        "mode 1003 never reached the terminal, so no amount of hovering \
         will produce a MouseEnter"
    );
}

/// The other half of the same rule, so the fix above cannot become
/// "everyone pays for 1003": an app with no motion-dependent widget
/// must still enter in `ButtonDrag`.
#[test]
fn an_app_without_a_tooltip_still_pays_nothing_for_motion() {
    let mut term = CaptureTerm::new(VP);
    let mut app = App::new(VP);
    let overlays = app.overlays();
    app.mount(|_cx| {
        Element::new()
            .style(LayoutStyle::column())
            .child(text("no tips here"))
            .build()
    })
    .expect("mount");
    assert!(!overlays.pointer_motion_required());
    let _driver = Driver::new(
        &mut app,
        &mut term,
        RunConfig {
            probe: false,
            ..RunConfig::default()
        },
    )
    .expect("driver");
    assert_eq!(
        term.enter_options().expect("entered").mouse,
        crate::term::MouseMode::ButtonDrag,
        "an app with no motion-dependent widget must not wake its event \
         loop on every pointer cell"
    );
}

/// The whole point: Tab reaches the anchor and the card opens, with the
/// pointer never having moved.
#[test]
fn a_card_tip_opens_when_its_anchor_takes_focus() {
    let mut rig = focusable_tip_rig(|cx, ov, target| {
        Tooltip::attach_content(cx, ov, tall_card(3), Duration::ZERO, target)
    });
    rig.settle();
    assert!(
        !rig.screen().contains("row 2"),
        "a dormant tip must paint nothing"
    );

    rig.keys(b"\t");
    rig.fire_timers(Duration::ZERO);

    assert!(
        rig.screen().contains("row 2"),
        "Tab focused the anchor and no tip appeared — the card is \
         reachable by mouse only:\n{}",
        rig.screen()
    );
}

/// Tabbing on must take the tip with it, or the card outlives the
/// reason it was shown and sits over the row the reader moved to.
#[test]
fn a_focus_opened_tip_closes_when_focus_moves_on() {
    let mut rig = focusable_tip_rig(|cx, ov, target| {
        Tooltip::attach_content(cx, ov, tall_card(3), Duration::ZERO, target)
    });
    rig.settle();
    rig.keys(b"\t");
    rig.fire_timers(Duration::ZERO);
    assert!(
        rig.screen().contains("row 2"),
        "the tip never opened, so the assertion below would pass \
         vacuously:\n{}",
        rig.screen()
    );

    rig.keys(b"\t");
    rig.fire_timers(Duration::ZERO);

    assert!(
        !rig.screen().contains("row 2"),
        "focus moved to the next widget and the card stayed up:\n{}",
        rig.screen()
    );
}

/// Escape dismisses a tip the reader can see. The second half is the
/// half that matters: an Escape arriving with NO tip open must pass
/// through, or every anchor in the app becomes a place where Escape
/// silently stops working for the dialog behind it.
#[test]
fn escape_dismisses_an_open_tip_and_passes_through_when_there_is_none() {
    let seen: Rc<RefCell<u32>> = Rc::new(RefCell::new(0));
    let counter = seen.clone();
    let mut rig = focusable_tip_rig(move |cx, ov, target| {
        let counter = counter.clone();
        // Stands in for the app behind the anchor: anything that also
        // wants Escape, on the path Escape bubbles up.
        Element::new()
            .on(crate::ui::Phase::Bubble, move |_ctx, ev| {
                if matches!(ev, UiEvent::Key(k) if k.key == crate::ui::Key::Escape) {
                    *counter.borrow_mut() += 1;
                }
            })
            .child(Tooltip::attach_content(
                cx,
                ov,
                tall_card(3),
                Duration::ZERO,
                target,
            ))
            .build()
    });
    rig.settle();
    rig.keys(b"\t");
    rig.fire_timers(Duration::ZERO);
    assert!(
        rig.screen().contains("row 2"),
        "the tip never opened:\n{}",
        rig.screen()
    );

    // CSI 27u, not a bare `\x1b`: a lone ESC byte is held for the
    // reader's disambiguation window and only becomes the Esc KEY when
    // that deadline passes, so a bare one either resolves late or gets
    // eaten by the next test step. This spelling is unambiguous on the
    // wire, which is what the assertion needs to be about the tip
    // rather than about input timing.
    rig.keys(b"\x1b[27u");
    rig.fire_timers(Duration::ZERO);
    assert!(
        !rig.screen().contains("row 2"),
        "Escape left the card up:\n{}",
        rig.screen()
    );
    assert_eq!(
        *seen.borrow(),
        0,
        "the Escape that dismissed the tip must be consumed, not also \
         acted on by the app behind it"
    );

    // Second Escape, nothing open: it belongs to whatever is behind.
    rig.keys(b"\x1b[27u");
    rig.fire_timers(Duration::ZERO);
    assert_eq!(
        *seen.borrow(),
        1,
        "an Escape with no tip open was swallowed — the anchor has \
         become a hole where Escape stops working"
    );
}

/// The documented LIMIT, pinned so it stays a decision instead of
/// becoming folklore: the trigger sits on the root of the view handed
/// to `attach_content`, and focus is delivered target-only. An anchor
/// that merely CONTAINS the focusable thing therefore gets no keyboard
/// tip — attach to the focusable node itself.
///
/// The first assertion is what stops this being decoration: it fails if
/// the descendant never actually took focus, in which case the absence
/// below would prove nothing.
#[test]
fn a_focusable_descendant_of_the_anchor_does_not_trigger_the_tip() {
    let mut term = CaptureTerm::new(VP);
    let mut app = App::new(VP);
    let overlays = app.overlays();
    let ov = overlays.clone();
    app.mount(move |cx| {
        let focused = cx.signal(false);
        // The anchor WRAPS the focusable rather than being it.
        let target = Element::new()
            .style(LayoutStyle::line(1).w(20))
            .child(
                Element::new()
                    .style(LayoutStyle::line(1).w(20))
                    .focusable()
                    .focus_signal(focused)
                    .child(crate::ui::dyn_view(LayoutStyle::default(), move || {
                        text(if focused.get() {
                            "INNER-FOCUSED"
                        } else {
                            "cite"
                        })
                    }))
                    .build(),
            )
            .build();
        Element::new()
            .style(
                LayoutStyle::column()
                    .width(Dimension::Percent(1.0))
                    .height(Dimension::Percent(1.0)),
            )
            .child(Tooltip::attach_content(
                cx,
                &ov,
                tall_card(3),
                Duration::ZERO,
                target,
            ))
            .build()
    })
    .expect("mount");
    let cfg = RunConfig {
        caps: Some(Capabilities::with(|c| {
            c.truecolor = true;
            c.colors_256 = true;
        })),
        enter: None,
        probe: false,
        // Off on purpose — see `tip_rig`: the tooltip arms mode 1003
        // itself, and an app flag here would hide it if that stopped.
        hover_ink: false,
        ..RunConfig::default()
    };
    let driver = Driver::new(&mut app, &mut term, cfg).expect("driver");
    let mut rig = TipRig {
        app,
        term,
        driver,
        overlays,
    };
    rig.settle();
    rig.keys(b"\t");
    rig.fire_timers(Duration::ZERO);

    assert!(
        rig.screen().contains("INNER-FOCUSED"),
        "the descendant never took focus, so the assertion below would \
         pass for the wrong reason:\n{}",
        rig.screen()
    );
    assert!(
        !rig.screen().contains("row 2"),
        "a tip opened for a focus its listener cannot hear — either \
         focus started bubbling or the trigger moved; whichever it is, \
         the documented limit on `attach_content` is now wrong:\n{}",
        rig.screen()
    );
}
