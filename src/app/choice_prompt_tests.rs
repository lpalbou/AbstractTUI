//! ChoicePrompt unit tests (split file, `#[path]`-included as
//! `choice_prompt::tests`). The gate mounts on a real overlay store;
//! events route driver-style (overlays first — the modal owns
//! everything). The 0250 movement-vs-activation split, the
//! exactly-once resolution contract, and the 0297 disposal-safety law
//! are exercised through real dispatch.

use std::cell::RefCell;
use std::rc::Rc;

use super::parts::window_start;
use super::*;
use crate::base::{Point, Size};
use crate::reactive::{create_root, flush_effects, RootScope};
use crate::ui::{BufferCanvas, Key, KeyEvent, MouseButton, MouseEvent, MouseKind, UiEvent};

const VP: Size = Size::new(44, 16);

#[test]
fn window_start_generalizes_the_select_rule_to_variable_heights() {
    // Uniform heights: identical to the select windowing rule.
    let uniform = vec![1; 10];
    assert_eq!(window_start(&uniform, 4, 0), 0);
    assert_eq!(window_start(&uniform, 4, 3), 0);
    assert_eq!(window_start(&uniform, 4, 4), 1, "anchor rides the bottom");
    assert_eq!(window_start(&uniform, 4, 9), 6);
    // Variable heights: a 2-row option counts twice.
    let mixed = vec![2, 1, 2, 1];
    assert_eq!(window_start(&mixed, 3, 0), 0);
    assert_eq!(window_start(&mixed, 3, 1), 0, "2+1 fits a 3-row budget");
    assert_eq!(window_start(&mixed, 3, 2), 1, "1+2 fits; 2+1+2 does not");
    // A taller-than-budget anchor still wins the window.
    assert_eq!(window_start(&mixed, 1, 2), 2);
    // Degenerates.
    assert_eq!(window_start(&[], 4, 0), 0);
    assert_eq!(window_start(&[1], 4, 9), 0, "anchor clamps into range");
}

// ---------------------------------------------------------------- rig

struct Rig {
    root: RootScope,
    overlays: super::super::overlays::Overlays,
}

impl Rig {
    fn send(&mut self, ev: &UiEvent) {
        // Driver-order routing: overlays first (the modal owns
        // everything while open); the rig has no root-tree content.
        let _ = self.overlays.dispatch(ev);
        flush_effects();
    }

    fn key(&mut self, k: Key) {
        self.send(&UiEvent::Key(KeyEvent::plain(k)));
    }

    fn type_str(&mut self, s: &str) {
        for ch in s.chars() {
            self.key(Key::Char(ch));
        }
    }

    fn click(&mut self, x: i32, y: i32) {
        self.send(&UiEvent::Mouse(MouseEvent {
            pos: Point::new(x, y),
            kind: MouseKind::Down(MouseButton::Left),
            mods: crate::ui::Mods::NONE,
        }));
        self.send(&UiEvent::Mouse(MouseEvent {
            pos: Point::new(x, y),
            kind: MouseKind::Up(MouseButton::Left),
            mods: crate::ui::Mods::NONE,
        }));
    }

    fn wheel(&mut self, down: bool) {
        self.send(&UiEvent::Mouse(MouseEvent {
            pos: Point::new(VP.w / 2, VP.h / 2),
            kind: if down {
                MouseKind::ScrollDown
            } else {
                MouseKind::ScrollUp
            },
            mods: crate::ui::Mods::NONE,
        }));
    }

    /// The open modal, drawn into a canvas: (bounds, canvas). None =
    /// closed.
    fn render(&self) -> Option<(crate::base::Rect, BufferCanvas)> {
        let (tree, bounds) = {
            let store = self.overlays.store().borrow();
            store
                .meta
                .iter()
                .zip(&store.layers)
                .find_map(|(m, l)| match &m.content {
                    super::super::overlays::OverlayContent::Tree {
                        tree, modal: true, ..
                    } => Some((tree.handle(), l.bounds())),
                    _ => None,
                })?
        };
        let mut tree = tree.handle();
        tree.layout();
        let mut canvas = BufferCanvas::new(bounds.size());
        tree.draw(&mut canvas);
        Some((bounds, canvas))
    }

    /// The open modal as text rows. None = closed.
    fn modal(&self) -> Option<(crate::base::Rect, Vec<String>)> {
        let (bounds, canvas) = self.render()?;
        let rows = (0..bounds.h).map(|y| canvas.row_text(y)).collect();
        Some((bounds, rows))
    }

    /// (fg, bg) of the first cell of `needle` inside the modal.
    fn ink_at(&self, needle: &str) -> Option<(crate::base::Rgba, crate::base::Rgba)> {
        let (bounds, canvas) = self.render()?;
        for y in 0..bounds.h {
            let row = canvas.row_text(y);
            if let Some(ix) = row.find(needle) {
                let x = crate::text::width(&row[..ix]);
                let cell = canvas.cell(Point::new(x, y))?;
                return Some((cell.1, cell.2));
            }
        }
        None
    }

    /// A live handle to the open modal's UiTree (shared core).
    fn modal_tree(&self) -> Option<crate::ui::UiTree> {
        let store = self.overlays.store().borrow();
        store.meta.iter().find_map(|m| match &m.content {
            super::super::overlays::OverlayContent::Tree {
                tree, modal: true, ..
            } => Some(tree.handle()),
            _ => None,
        })
    }

    /// The modal's accessibility snapshot text ("" = closed).
    fn a11y(&self) -> String {
        match self.modal_tree() {
            Some(mut tree) => tree.accessibility_tree_text(),
            None => String::new(),
        }
    }

    fn modal_text(&self) -> String {
        self.modal()
            .map(|(_, rows)| rows.join("\n"))
            .unwrap_or_default()
    }

    fn is_open(&self) -> bool {
        self.modal().is_some()
    }

    /// Screen row (bounds-local) containing `needle`, as absolute
    /// viewport coordinates (x = column of the needle).
    fn find(&self, needle: &str) -> Option<(i32, i32)> {
        let (bounds, rows) = self.modal()?;
        for (y, row) in rows.iter().enumerate() {
            if let Some(ix) = row.find(needle) {
                let x = crate::text::width(&row[..ix]);
                return Some((bounds.x + x, bounds.y + y as i32));
            }
        }
        None
    }
}

fn rig_sized(vp: Size) -> Rig {
    super::super::viewport::publish_viewport(vp);
    let overlays = super::super::overlays::Overlays::new();
    overlays.ensure_root(vp);
    let (root, ()) = create_root(|_cx| {});
    flush_effects();
    Rig { root, overlays }
}

fn rig() -> Rig {
    rig_sized(VP)
}

/// Open a prompt on the rig's root scope, recording outcomes.
fn open_on(
    rig: &Rig,
    build: impl FnOnce(ChoicePrompt) -> ChoicePrompt,
) -> (Rc<RefCell<Vec<ChoiceOutcome>>>, ChoicePromptHandle) {
    let outcomes: Rc<RefCell<Vec<ChoiceOutcome>>> = Default::default();
    let sink = outcomes.clone();
    let prompt = build(
        ChoicePrompt::new("Proceed how?")
            .overlays(&rig.overlays)
            .on_resolve(move |o| sink.borrow_mut().push(o)),
    );
    let handle = prompt.open(rig.root.scope());
    flush_effects();
    (outcomes, handle)
}

fn basic(p: ChoicePrompt) -> ChoicePrompt {
    p.option("a", "Alpha")
        .option("b", "Beta")
        .option("c", "Gamma")
}

// ------------------------------------------------------------- single

#[test]
fn single_arrows_move_candidate_and_enter_commits_once() {
    let mut r = rig();
    let (outcomes, handle) = open_on(&r, basic);
    assert!(r.is_open(), "gate opened");
    assert!(handle.is_open());
    let text = r.modal_text();
    assert!(text.contains("Proceed how?"), "prompt renders: {text}");
    assert!(text.contains("● Alpha"), "candidate glyph on row 0: {text}");

    // Movement is NOT activation (0250): arrows resolve nothing.
    r.key(Key::Down);
    r.key(Key::Down);
    assert!(outcomes.borrow().is_empty(), "moves never resolve");
    assert!(r.modal_text().contains("● Gamma"), "candidate followed");

    r.key(Key::Up); // back to Beta
    r.key(Key::Enter);
    assert_eq!(
        outcomes.borrow().as_slice(),
        [ChoiceOutcome::Answered(ChoiceAnswer {
            selected: vec!["b".into()],
            other: None,
        })],
        "Enter commits the candidate"
    );
    assert!(!r.is_open(), "modal closed on resolve");
    assert!(!handle.is_open());

    // Exactly-once: further input reaches a closed gate.
    r.key(Key::Enter);
    handle.cancel();
    assert_eq!(outcomes.borrow().len(), 1, "resolution is exactly-once");
}

#[test]
fn single_click_selects_then_click_on_selected_commits() {
    let mut r = rig();
    let (outcomes, _h) = open_on(&r, basic);
    let (bx, by) = r.find("Beta").expect("Beta row");
    r.click(bx, by);
    assert!(outcomes.borrow().is_empty(), "first click only selects");
    assert!(r.modal_text().contains("● Beta"), "candidate moved");
    r.click(bx, by);
    assert_eq!(
        outcomes.borrow().as_slice(),
        [ChoiceOutcome::Answered(ChoiceAnswer {
            selected: vec!["b".into()],
            other: None,
        })],
        "click-on-selected commits"
    );
    assert!(!r.is_open());
}

#[test]
fn number_keys_jump_single_and_jump_toggle_multiple() {
    let mut r = rig();
    let (outcomes, _h) = open_on(&r, basic);
    r.key(Key::Char('3'));
    assert!(outcomes.borrow().is_empty(), "jump never commits");
    assert!(r.modal_text().contains("● Gamma"), "3 jumped the candidate");
    r.key(Key::Char('9')); // out of range: ignored
    assert!(r.modal_text().contains("● Gamma"));
    r.key(Key::Enter);
    assert_eq!(outcomes.borrow().len(), 1);

    // Multiple: jump + toggle.
    let mut r = rig();
    let (outcomes, _h) = open_on(&r, |p| basic(p).allow_multiple(true));
    r.key(Key::Char('2'));
    let text = r.modal_text();
    assert!(text.contains("☑ Beta"), "2 jump-toggled Beta: {text}");
    r.key(Key::Enter);
    assert_eq!(
        outcomes.borrow().as_slice(),
        [ChoiceOutcome::Answered(ChoiceAnswer {
            selected: vec!["b".into()],
            other: None,
        })]
    );
}

// ------------------------------------------------------------ endings

#[test]
fn escape_resolves_cancelled_never_silent() {
    let mut r = rig();
    let (outcomes, handle) = open_on(&r, basic);
    r.key(Key::Escape);
    assert_eq!(
        outcomes.borrow().as_slice(),
        [ChoiceOutcome::Cancelled],
        "Esc is an explicit outcome"
    );
    assert!(!r.is_open(), "modal closed");
    assert!(!handle.is_open());
}

#[test]
fn outside_press_does_not_dismiss_the_gate() {
    let mut r = rig();
    let (outcomes, _h) = open_on(&r, basic);
    // Press far outside the panel: swallowed by the modal, gate stays.
    r.click(0, 0);
    r.click(VP.w - 1, VP.h - 1);
    assert!(outcomes.borrow().is_empty(), "no resolution");
    assert!(r.is_open(), "a decision gate has explicit endings only");
}

#[test]
fn cancel_button_resolves_and_handle_cancel_is_idempotent() {
    let mut r = rig();
    let (outcomes, handle) = open_on(&r, basic);
    let (cx, cy) = r.find("Cancel").expect("Cancel button");
    r.click(cx, cy);
    assert_eq!(outcomes.borrow().as_slice(), [ChoiceOutcome::Cancelled]);
    assert!(!r.is_open());
    handle.cancel();
    handle.cancel();
    assert_eq!(outcomes.borrow().len(), 1, "cancel after resolve no-ops");
}

#[test]
fn handle_cancel_resolves_cancelled_exactly_once() {
    let r = rig();
    let (outcomes, handle) = open_on(&r, basic);
    assert!(handle.is_open());
    handle.cancel();
    flush_effects();
    assert_eq!(outcomes.borrow().as_slice(), [ChoiceOutcome::Cancelled]);
    assert!(!r.is_open(), "programmatic cancel closes the modal");
    handle.cancel();
    assert_eq!(outcomes.borrow().len(), 1);
}

// ----------------------------------------------------------- multiple

#[test]
fn multiple_space_toggles_enter_commits_canonical_order() {
    let mut r = rig();
    let (outcomes, _h) = open_on(&r, |p| basic(p).allow_multiple(true));
    let text = r.modal_text();
    assert!(text.contains("☐ Alpha"), "checkbox glyphs: {text}");
    assert!(text.contains("Confirm"), "Confirm button in multiple mode");

    // Toggle Gamma first, then Alpha — the ANSWER canonicalizes to
    // option order regardless of toggle order.
    r.key(Key::Down);
    r.key(Key::Down);
    r.key(Key::Char(' '));
    r.key(Key::Home);
    r.key(Key::Char(' '));
    let text = r.modal_text();
    assert!(
        text.contains("☑ Alpha") && text.contains("☑ Gamma"),
        "{text}"
    );
    assert!(outcomes.borrow().is_empty(), "toggles never resolve (0250)");

    r.key(Key::Enter);
    assert_eq!(
        outcomes.borrow().as_slice(),
        [ChoiceOutcome::Answered(ChoiceAnswer {
            selected: vec!["a".into(), "c".into()],
            other: None,
        })],
        "canonical option order"
    );
}

#[test]
fn multiple_confirm_button_commits_and_empty_set_is_legal() {
    let mut r = rig();
    let (outcomes, _h) = open_on(&r, |p| basic(p).allow_multiple(true));
    let (cx, cy) = r.find("Confirm").expect("Confirm button");
    r.click(cx, cy);
    assert_eq!(
        outcomes.borrow().as_slice(),
        [ChoiceOutcome::Answered(ChoiceAnswer {
            selected: vec![],
            other: None,
        })],
        "an empty set is the caller's to judge — the gate reports it"
    );
}

#[test]
fn multiple_click_toggles_and_checked_seeds_apply() {
    let mut r = rig();
    let (outcomes, _h) = open_on(&r, |p| {
        basic(p).allow_multiple(true).checked(["b"]).initial("b")
    });
    assert!(r.modal_text().contains("☑ Beta"), "seeded check renders");
    let (x, y) = r.find("Alpha").expect("Alpha row");
    r.click(x, y);
    assert!(r.modal_text().contains("☑ Alpha"), "click toggles on");
    r.click(x, y);
    assert!(r.modal_text().contains("☐ Alpha"), "click toggles off");
    assert!(outcomes.borrow().is_empty());
    r.key(Key::Enter);
    assert_eq!(
        outcomes.borrow().as_slice(),
        [ChoiceOutcome::Answered(ChoiceAnswer {
            selected: vec!["b".into()],
            other: None,
        })]
    );
}

// Other lane, disposal/reopen, windowing, render honesty, sequences:
// the flows sibling (same rig — file budget split).
#[path = "choice_prompt_tests_flows.rs"]
mod flows;

// Wave-5 cycle-2 folds (shortcut letters, must-choose mode, layered
// Esc, danger tint, a11y tree, focus affordance): the c2 sibling.
#[path = "choice_prompt_tests_c2.rs"]
mod c2;

// First-app 0271 folds (body_width, dismiss_label, handle.retire):
// the c3 sibling.
#[path = "choice_prompt_tests_c3.rs"]
mod c3;
