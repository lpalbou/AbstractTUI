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
use crate::ui::{text, Element, MouseEvent, MouseKind, UiEvent};
use crate::widgets::{TextArea, TextAreaState};

use super::super::driver::{Driver, RunConfig};
use super::super::overlays::{OverlayContent, Overlays};
use super::super::popups::{Modal, MODAL_Z};
use super::super::select::{Combobox, MultiSelect, Select, SelectOption};
use super::super::App;
use super::{Completion, CompletionCandidate, Tooltip};

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
