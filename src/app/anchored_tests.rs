//! Anchored-panel substrate + completion controller tests (split from
//! anchored.rs for the file budget; `#[path]`-included as its `tests`
//! module). The completion cases drive a REAL mounted composer through
//! real dispatch, with panels living on a real overlay store — events
//! route the way the driver routes them (overlays first, then root).
use std::cell::RefCell;
use std::rc::Rc;

use super::*;
use crate::layout::{Dimension, Style as LayoutStyle};
use crate::reactive::{create_root, flush_effects, RootScope};
use crate::ui::{
    BufferCanvas, Element, Key, KeyEvent, Mods, MouseButton, MouseEvent, MouseKind, UiTree,
};
use crate::widgets::{TextArea, TextAreaState};

// -------------------------------------------------------------- placement

#[test]
fn place_panel_prefers_below_and_flips_when_cramped() {
    let vp = Size::new(80, 24);
    let content = Size::new(20, 4);
    let width = PanelWidth::Content { min: 8, max: 44 };
    // Plenty below: below the anchor, content-sized.
    let anchor = Rect::new(10, 5, 1, 1);
    assert_eq!(
        place_panel(vp, anchor, content, width),
        Rect::new(10, 6, 20, 4)
    );
    // Anchor near the bottom: fewer rows below than needed AND more
    // above -> flip; the panel's bottom edge touches the anchor row.
    let anchor = Rect::new(10, 22, 1, 1);
    assert_eq!(
        place_panel(vp, anchor, content, width),
        Rect::new(10, 18, 20, 4),
        "flipped above"
    );
    // Cramped both sides, below >= above: stays below, height clamped.
    let anchor = Rect::new(0, 1, 1, 1);
    let placed = place_panel(Size::new(80, 5), anchor, content, width);
    assert_eq!(placed, Rect::new(0, 2, 20, 3), "clamped to the space below");
    // Below short but above even shorter: no flip (spec: flip only when
    // above > below), height = what below offers.
    let anchor = Rect::new(0, 1, 1, 1);
    let placed = place_panel(Size::new(80, 4), anchor, content, width);
    assert_eq!(placed, Rect::new(0, 2, 20, 2));
}

#[test]
fn place_panel_clamps_x_and_applies_width_policy() {
    let vp = Size::new(40, 20);
    let content = Size::new(20, 3);
    // Near the right edge: x clamps so the panel stays on screen.
    let anchor = Rect::new(35, 5, 1, 1);
    let placed = place_panel(vp, anchor, content, PanelWidth::Content { min: 8, max: 44 });
    assert_eq!(placed, Rect::new(20, 6, 20, 3));
    // MatchAnchor: the trigger's width wins.
    let anchor = Rect::new(5, 5, 12, 1);
    let placed = place_panel(vp, anchor, content, PanelWidth::MatchAnchor);
    assert_eq!(placed.w, 12);
    assert_eq!(placed.x, 5);
    // Content clamps into min..=max, and never past the viewport.
    let placed = place_panel(
        vp,
        anchor,
        Size::new(2, 3),
        PanelWidth::Content { min: 8, max: 44 },
    );
    assert_eq!(placed.w, 8, "min floor");
    let placed = place_panel(
        Size::new(30, 20),
        anchor,
        Size::new(90, 3),
        PanelWidth::Content { min: 8, max: 44 },
    );
    assert_eq!(placed.w, 30, "viewport caps the max");
}

#[test]
fn place_panel_reports_no_room_honestly() {
    // Anchor fills the only row: nothing above, nothing below.
    let placed = place_panel(
        Size::new(20, 1),
        Rect::new(0, 0, 1, 1),
        Size::new(10, 3),
        PanelWidth::MatchAnchor,
    );
    assert_eq!(placed.h, 0, "no room on either side");
}

// -------------------------------------------------------------- substrate

fn panel_view() -> View {
    Element::new()
        .style(
            LayoutStyle::default()
                .width(Dimension::Percent(1.0))
                .height(Dimension::Percent(1.0)),
        )
        .child(crate::ui::text("panel"))
        .build()
}

fn open_panel(
    overlays: &Overlays,
    cx: Scope,
    vp: Size,
    anchor: Rect,
    content: Size,
) -> AnchoredPanel {
    AnchoredPanel::open_passive(
        overlays,
        cx,
        vp,
        PanelAnchor { rect: anchor },
        PanelWidth::Content { min: 4, max: 30 },
        content,
        |_| panel_view(),
    )
}

#[test]
fn passive_panel_opens_above_everything_and_never_owns_keys() {
    let overlays = Overlays::new();
    overlays.ensure_root(Size::new(40, 12));
    let modal_like = overlays.layer(1000, Rect::new(0, 0, 10, 4));
    let (root, panel) = create_root(|cx| {
        open_panel(
            &overlays,
            cx,
            Size::new(40, 12),
            Rect::new(5, 2, 1, 1),
            Size::new(10, 3),
        )
    });
    assert!(panel.is_open());
    let rect = panel.rect().expect("placed");
    assert_eq!(rect, Rect::new(5, 3, 10, 3));
    // Above the whole live stack (top_z + 1).
    {
        let store = overlays.store().borrow();
        let max_z = store.layers.iter().map(|l| l.z()).max().unwrap();
        let panel_layer = panel.layer().expect("layer");
        drop(store);
        assert_eq!(panel_layer.bounds(), Some(rect));
        assert_eq!(max_z, 1001, "panel z = top_z() + 1");
    }
    // PASSIVE: the panel tree holds no focus, so keys are None-routed
    // (they fall through to the app's root tree).
    let key_ev = crate::ui::UiEvent::Key(KeyEvent::plain(Key::Char('x')));
    assert_eq!(overlays.dispatch(&key_ev), None, "keys stay with the owner");
    // Presses INSIDE the panel are the panel's own (opacity rule: the
    // event never falls through to covered content, whether or not a
    // handler inside consumed it)…
    let press = crate::ui::UiEvent::Mouse(MouseEvent {
        pos: Point::new(rect.x + 1, rect.y + 1),
        kind: MouseKind::Down(MouseButton::Left),
        mods: Mods::NONE,
    });
    assert!(overlays.dispatch(&press).is_some(), "owned by the panel");
    // …and never focus it (nothing focusable inside — the contract).
    assert_eq!(overlays.dispatch(&key_ev), None);
    modal_like.remove();
    panel.close();
    panel.close(); // idempotent
    assert!(!panel.is_open());
    root.dispose();
}

#[test]
fn update_moves_cheaply_resizes_by_remount_and_hides_without_room() {
    let overlays = Overlays::new();
    overlays.ensure_root(Size::new(40, 12));
    let vp = Size::new(40, 12);
    let (root, panel) =
        create_root(|cx| open_panel(&overlays, cx, vp, Rect::new(5, 2, 1, 1), Size::new(10, 3)));
    let first_layer = panel.layer().expect("open");
    // Pure move: same size, new origin -> the SAME layer slides.
    panel.update(vp, PanelAnchor::cell(Point::new(9, 2)), Size::new(10, 3));
    assert!(first_layer.is_alive(), "moved, not remounted");
    assert_eq!(panel.rect().unwrap().origin(), Point::new(9, 3));
    // Size change: remount (old layer dies, a new one lives).
    panel.update(vp, PanelAnchor::cell(Point::new(9, 2)), Size::new(10, 5));
    assert!(!first_layer.is_alive(), "resize remounts");
    assert_eq!(panel.rect().unwrap().h, 5);
    // No room anywhere: the panel hides…
    panel.update(
        Size::new(40, 1),
        PanelAnchor::cell(Point::new(0, 0)),
        Size::new(10, 5),
    );
    assert!(!panel.is_open(), "no room = no layer");
    // …and returns when room does.
    panel.update(vp, PanelAnchor::cell(Point::new(5, 2)), Size::new(10, 5));
    assert!(panel.is_open());
    root.dispose();
}

#[test]
fn opener_scope_death_closes_the_panel() {
    let overlays = Overlays::new();
    overlays.ensure_root(Size::new(40, 12));
    let holder: Rc<RefCell<Option<AnchoredPanel>>> = Default::default();
    let h2 = holder.clone();
    let (root, ()) = create_root(|cx| {
        // The opener dies while the panel lives — the dyn_view
        // regeneration shape (0500's anchor-unmount safety contract).
        let opener = cx.child();
        let panel = open_panel(
            &overlays,
            opener,
            Size::new(40, 12),
            Rect::new(5, 2, 1, 1),
            Size::new(10, 3),
        );
        *h2.borrow_mut() = Some(panel);
        opener.dispose();
    });
    let panel = holder.borrow().clone().expect("panel");
    assert!(!panel.is_open(), "scope death closed the panel");
    assert_eq!(
        overlays.store().borrow().layers.len(),
        1,
        "only the root layer remains — no orphan"
    );
    panel.update(
        Size::new(40, 12),
        PanelAnchor::cell(Point::new(5, 2)),
        Size::new(10, 3),
    );
    assert!(!panel.is_open(), "a dead panel stays dead");
    root.dispose();
}

// ------------------------------------------------------------- completion

struct Rig {
    /// Held for the mount's lifetime (dropping it would dispose scopes).
    _root: RootScope,
    tree: UiTree,
    overlays: Overlays,
    state: TextAreaState,
    queries: Rc<RefCell<Vec<String>>>,
}

/// Mount a bottom-anchored composer with '/' + '@' completion on a real
/// tree + overlay store. Events route driver-style via `Rig::send`.
fn completion_rig(size: Size) -> Rig {
    completion_rig_with(size, false, |c, q| {
        c.trigger('/', move |query| {
            q.borrow_mut().push(query.to_string());
            ["help", "theme", "clear", "quit"]
                .iter()
                .filter(|c| c.starts_with(query))
                .map(|c| CompletionCandidate::new(format!("/{c}"), format!("/{c} ")).detail("cmd"))
                .collect()
        })
        .trigger('@', |query| {
            ["alice", "bob"]
                .iter()
                .filter(|c| c.starts_with(query))
                .map(|c| CompletionCandidate::new(format!("@{c}"), format!("@{c} ")))
                .collect()
        })
        .max_visible(3)
    })
}

/// The parameterized rig behind `completion_rig` (first-app/0292/0294
/// cases): `status_row` mounts a one-row legend UNDER the composer —
/// the filed 0294 shape, where short dropdowns land on chrome — and
/// `wire` registers the triggers/options on the `Completion` (it
/// receives the query-recording cell the rig hands back).
fn completion_rig_with(
    size: Size,
    status_row: bool,
    wire: impl FnOnce(Completion, Rc<RefCell<Vec<String>>>) -> Completion,
) -> Rig {
    super::super::viewport::publish_viewport(size);
    let overlays = Overlays::new();
    overlays.ensure_root(size);
    let queries: Rc<RefCell<Vec<String>>> = Default::default();
    let q2 = queries.clone();
    let holder: Rc<RefCell<Option<TextAreaState>>> = Default::default();
    let h2 = holder.clone();
    let ov = overlays.clone();
    let mut tree = UiTree::new(size);
    let (root, ()) = create_root(|cx| {
        let t = crate::theme::default_theme().tokens;
        let state = TextAreaState::new(cx);
        *h2.borrow_mut() = Some(state.clone());
        let composer = TextArea::new()
            .state(&state)
            .rows(1, 3)
            .element(cx, &t)
            .build();
        let wrapped = wire(Completion::new(), q2).attach(cx, &ov, &state, composer);
        let mut view = Element::new()
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
            .child(wrapped);
        if status_row {
            // The key-legend row the 0294 report shows being clobbered.
            view = view.child(
                Element::new()
                    .style(LayoutStyle::line(1).shrink(0.0))
                    .build(),
            );
        }
        tree.mount(cx, view.build());
    });
    tree.layout();
    let state = holder.borrow().clone().expect("state");
    let mut rig = Rig {
        _root: root,
        tree,
        overlays,
        state,
        queries,
    };
    rig.send(&crate::ui::UiEvent::Key(KeyEvent::plain(Key::Tab))); // focus
    rig
}

impl Rig {
    /// Driver-order routing: overlays first, root tree on fall-through,
    /// then the effect flush a frame turn would run.
    fn send(&mut self, ev: &crate::ui::UiEvent) {
        if self.overlays.dispatch(ev).is_none() {
            self.tree.dispatch(ev);
        }
        flush_effects();
    }

    fn type_str(&mut self, s: &str) {
        for ch in s.chars() {
            self.send(&crate::ui::UiEvent::Key(KeyEvent::plain(Key::Char(ch))));
        }
    }

    fn key(&mut self, k: Key) {
        self.send(&crate::ui::UiEvent::Key(KeyEvent::plain(k)));
    }

    /// The one passive panel, drawn: (bounds, rendered rows).
    fn panel(&self) -> Option<(Rect, Vec<String>)> {
        let (tree, bounds) = {
            let store = self.overlays.store().borrow();
            let found = store
                .meta
                .iter()
                .zip(&store.layers)
                .find_map(|(m, l)| match &m.content {
                    super::super::overlays::OverlayContent::Tree {
                        tree, modal: false, ..
                    } => Some((tree.handle(), l.bounds())),
                    _ => None,
                });
            found?
        };
        let mut tree = tree.handle();
        tree.layout();
        let mut canvas = BufferCanvas::new(bounds.size());
        tree.draw(&mut canvas);
        let rows = (0..bounds.h).map(|y| canvas.row_text(y)).collect();
        Some((bounds, rows))
    }
}

#[test]
fn trigger_opens_typing_refilters_enter_accepts() {
    let mut rig = completion_rig(Size::new(40, 10));
    assert!(rig.panel().is_none(), "closed before any trigger");
    rig.type_str("/");
    let (rect, rows) = rig.panel().expect("trigger opened the dropdown");
    assert!(rows.iter().any(|r| r.contains("/help")), "{rows:?}");
    assert!(rows.iter().any(|r| r.contains("cmd")), "detail renders");
    // Bottom composer: the panel flips ABOVE the caret row.
    let caret = rig.state.caret_cell().get_untracked().expect("anchor");
    assert!(
        rect.bottom() <= caret.y,
        "flipped above: {rect:?} vs {caret:?}"
    );
    assert_eq!(rect.h, 3, "4 candidates windowed to max_visible 3");
    // Refilter: provider sees the growing query; the list narrows.
    rig.type_str("th");
    assert_eq!(
        rig.queries.borrow().as_slice(),
        ["", "t", "th"],
        "one provider call per edit"
    );
    let (rect2, rows2) = rig.panel().expect("still open");
    assert!(rows2.iter().any(|r| r.contains("/theme")));
    assert!(!rows2.iter().any(|r| r.contains("/help")), "filtered out");
    assert_eq!(rect2.h, 1, "panel shrank to the match count");
    // Enter accepts: token replaced (trigger included), panel closed.
    rig.key(Key::Enter);
    assert_eq!(rig.state.text(), "/theme ");
    assert!(rig.panel().is_none(), "accept closes");
    // The accept edit itself must not reopen (one-shot skip).
    assert_eq!(
        rig.queries.borrow().len(),
        3,
        "no post-accept provider call"
    );
}

#[test]
fn navigation_moves_highlight_and_tab_accepts() {
    let mut rig = completion_rig(Size::new(40, 10));
    rig.type_str("/");
    let sel_bg = crate::theme::default_theme().tokens.selection_bg;
    let highlight_row = |rig: &Rig| -> Option<usize> {
        let (rect, _) = rig.panel()?;
        let store = rig.overlays.store().borrow();
        let (tree, _) =
            store
                .meta
                .iter()
                .zip(&store.layers)
                .find_map(|(m, l)| match &m.content {
                    super::super::overlays::OverlayContent::Tree {
                        tree, modal: false, ..
                    } => Some((tree.handle(), l.bounds())),
                    _ => None,
                })?;
        drop(store);
        let mut tree = tree.handle();
        tree.layout();
        let mut canvas = BufferCanvas::new(rect.size());
        tree.draw(&mut canvas);
        (0..rect.h as usize).find(|y| {
            canvas
                .cell(Point::new(1, *y as i32))
                .is_some_and(|c| c.2 == sel_bg)
        })
    };
    assert_eq!(highlight_row(&rig), Some(0), "opens on the first row");
    rig.key(Key::Down);
    assert_eq!(highlight_row(&rig), Some(1), "Down moves the highlight");
    rig.key(Key::Up);
    rig.key(Key::Up); // clamps at the top, no wrap
    assert_eq!(highlight_row(&rig), Some(0));
    rig.key(Key::Down);
    rig.key(Key::Tab); // Tab accepts the highlighted candidate
    assert_eq!(rig.state.text(), "/theme ");
    assert!(rig.panel().is_none());
    // While closed, Down/Up fall through to the composer (history edge
    // arrows), and Tab returns to focus traversal.
    rig.key(Key::Down);
    assert_eq!(rig.state.text(), "/theme ", "no dropdown, no highlight");
}

#[test]
fn escape_dismisses_and_the_same_token_stays_muted() {
    let mut rig = completion_rig(Size::new(40, 10));
    rig.type_str("/t");
    assert!(rig.panel().is_some());
    rig.key(Key::Escape);
    assert!(rig.panel().is_none(), "Escape closes");
    rig.type_str("h");
    assert!(
        rig.panel().is_none(),
        "typing inside the dismissed token stays calm"
    );
    // Leaving the token re-arms: whitespace, then a fresh trigger.
    rig.type_str(" @");
    let (_, rows) = rig.panel().expect("fresh trigger reopens");
    assert!(rows.iter().any(|r| r.contains("@alice")), "{rows:?}");
}

#[test]
fn focus_loss_empty_results_and_mid_word_triggers_close_or_never_open() {
    let mut rig = completion_rig(Size::new(40, 10));
    // A trigger char mid-word is not a token start.
    rig.type_str("a/b");
    assert!(rig.panel().is_none(), "mid-word '/' never opens");
    rig.type_str(" /zz");
    assert!(rig.panel().is_none(), "no matches = closed");
    rig.type_str(" /h");
    assert!(rig.panel().is_some());
    // Focus leaving the composer closes the dropdown (owner-driven).
    rig.tree.set_focus(None);
    flush_effects();
    assert!(rig.panel().is_none(), "blur closes");
}

#[test]
fn mouse_click_on_a_row_accepts_it() {
    let mut rig = completion_rig(Size::new(40, 10));
    rig.type_str("/");
    let (rect, rows) = rig.panel().expect("open");
    let target_row = rows
        .iter()
        .position(|r| r.contains("/theme"))
        .expect("theme row visible") as i32;
    rig.send(&crate::ui::UiEvent::Mouse(MouseEvent {
        pos: Point::new(rect.x + 2, rect.y + target_row),
        kind: MouseKind::Down(MouseButton::Left),
        mods: Mods::NONE,
    }));
    assert_eq!(rig.state.text(), "/theme ", "click-to-accept");
    assert!(rig.panel().is_none());
}

#[test]
fn panel_follows_the_caret_anchor() {
    let mut rig = completion_rig(Size::new(40, 10));
    rig.type_str("some words first ");
    rig.type_str("/");
    let (r1, _) = rig.panel().expect("open");
    rig.key(Key::Escape);
    // New token further right: the panel opens further right too.
    rig.type_str("qq /");
    let (r2, _) = rig.panel().expect("reopened");
    assert!(r2.x > r1.x, "anchor moved right: {r1:?} -> {r2:?}");
}

// Trigger position policies (first-app/0292) + placement bias
// (first-app/0294) — a child module in a sibling file (<600-line
// budget) sharing this rig through `super::*`.
#[path = "anchored_policy_tests.rs"]
mod policy;
