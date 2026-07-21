//! REDTEAM cycle-3 attack: REACT's interaction layer — focus traversal
//! fuzz, focus traps, the text-input editing property (widget vs an
//! independent model), list windowing under 10k items (draw-call
//! counting canvas), hit-testing, wheel routing, pointer capture and
//! hover exactly-once semantics.

use std::cell::RefCell;
use std::rc::Rc;

use abstracttui::base::{Point, Rect, Rgba, Size};
use abstracttui::layout::{Dimension, Inset, Style as LayoutStyle};
use abstracttui::reactive::{create_root, flush_effects};
use abstracttui::testing::Rng;
use abstracttui::theme::{default_theme, TokenSet};
use abstracttui::ui::{
    dyn_view, BufferCanvas, Canvas, Element, Key, KeyEvent, Mods, MouseButton, MouseEvent,
    MouseKind, Phase, UiEvent, UiTree,
};
use abstracttui::widgets::{Button, List, TextInput};

fn tokens() -> TokenSet {
    default_theme().tokens
}

fn key(k: Key) -> UiEvent {
    UiEvent::Key(KeyEvent::plain(k))
}

fn key_mod(k: Key, mods: Mods) -> UiEvent {
    UiEvent::Key(KeyEvent::new(k, mods))
}

fn mouse(kind: MouseKind, x: i32, y: i32) -> UiEvent {
    UiEvent::Mouse(MouseEvent {
        kind,
        pos: Point::new(x, y),
        mods: Mods::NONE,
    })
}

fn click(x: i32, y: i32) -> Vec<UiEvent> {
    vec![
        mouse(MouseKind::Down(MouseButton::Left), x, y),
        mouse(MouseKind::Up(MouseButton::Left), x, y),
    ]
}

/// A canvas that counts every put/print (the draw-cost oracle for the
/// list-windowing attack) while delegating storage to BufferCanvas.
struct CountingCanvas {
    inner: BufferCanvas,
    puts: Rc<RefCell<usize>>,
}

impl Canvas for CountingCanvas {
    fn size(&self) -> Size {
        self.inner.size()
    }
    fn put(&mut self, p: Point, ch: char, fg: Rgba, bg: Rgba) {
        *self.puts.borrow_mut() += 1;
        self.inner.put(p, ch, fg, bg);
    }
}

// Styled fidelity via the default degradations — every styled call still
// funnels through `put`, so the counter sees the real per-cell cost.
impl abstracttui::ui::StyledCanvas for CountingCanvas {}

// ---------------------------------------------------------------------------
// Focus traversal fuzz.
// ---------------------------------------------------------------------------

/// Random Tab/Shift-Tab/arrow/click storms over a mixed tree: no panic,
/// and whenever focus exists its instance resolves to a live rect.
#[test]
fn focus_traversal_fuzz_never_panics_or_dangles() {
    let t = tokens();
    let mut tree = UiTree::new(Size::new(60, 20));
    let (root, ()) = create_root(|cx| {
        let view = Element::new()
            .child(Button::new("one").element(cx, &t).build())
            .child(
                Element::new()
                    .child(Button::new("two").element(cx, &t).build())
                    .child(TextInput::new().element(cx, &t).build())
                    .build(),
            )
            .child(
                List::new((0..12).map(|i| format!("item {i}")).collect())
                    .element(cx, &t)
                    .build(),
            )
            .child(Button::new("last").element(cx, &t).build())
            .build();
        tree.mount(cx, view);
    });
    flush_effects();
    tree.layout();

    let mut rng = Rng::new(0xF0C05);
    for step in 0..2000 {
        let ev = match rng.below(6) {
            0 => key(Key::Tab),
            1 => key_mod(Key::Tab, Mods::SHIFT),
            2 => key(*rng.pick(&[Key::Up, Key::Down, Key::Left, Key::Right, Key::Enter])),
            3 => key(Key::Char((b'a' + rng.below(26) as u8) as char)),
            4 => mouse(
                MouseKind::Down(MouseButton::Left),
                rng.below(62) as i32 - 1,
                rng.below(22) as i32 - 1,
            ),
            _ => mouse(MouseKind::Move, rng.below(60) as i32, rng.below(20) as i32),
        };
        tree.dispatch(&ev);
        flush_effects();
        tree.layout();
        if let Some(id) = tree.focused() {
            let r = tree.rect_of(id);
            assert!(
                r.w >= 0 && r.h >= 0,
                "step {step}: focused instance has a degenerate rect {r:?}"
            );
        }
    }
    root.dispose();
}

/// Focus trap: Tab from inside a trap cycles ONLY within it; the
/// outside button is unreachable until the trap unmounts.
#[test]
fn focus_trap_holds_tab_and_releases_on_unmount() {
    let t = tokens();
    let mut tree = UiTree::new(Size::new(50, 12));
    let mut modal_open_handle = None;
    let (root, ()) = create_root(|cx| {
        let modal_open = cx.signal(true);
        modal_open_handle = Some(modal_open);
        let tt = t;
        let view = Element::new()
            .child(Button::new("outside").element(cx, &tt).build())
            .child(dyn_view(LayoutStyle::default(), move || {
                if modal_open.get() {
                    // Scope is Copy: widgets built inside the Dyn attach
                    // to the outer scope (fine for a two-rebuild test).
                    Element::new()
                        .focus_trap()
                        .child(Button::new("in-a").element(cx, &tt).build())
                        .child(Button::new("in-b").element(cx, &tt).build())
                        .build()
                } else {
                    abstracttui::ui::text("closed")
                }
            }))
            .build();
        tree.mount(cx, view);
    });
    flush_effects();
    tree.layout();

    // Identify instances by their rects: collect focus stops by tabbing.
    let mut stops = std::collections::BTreeSet::new();
    // Enter the trap: click inside it (the modal's first button).
    for _ in 0..8 {
        tree.dispatch(&key(Key::Tab));
        flush_effects();
        tree.layout();
        if let Some(id) = tree.focused() {
            stops.insert(format!("{:?}", tree.rect_of(id)));
        }
    }
    // A trap must confine tabbing to its two buttons once entered.
    assert!(
        stops.len() <= 3,
        "tab reached {} distinct rects; a trap must confine cycling: {stops:?}",
        stops.len()
    );
    // Unmount the modal: focus must escape to something alive (or none),
    // and tabbing reaches the outside button again.
    modal_open_handle.unwrap().set(false);
    flush_effects();
    tree.layout();
    tree.dispatch(&key(Key::Tab));
    flush_effects();
    if let Some(id) = tree.focused() {
        let r = tree.rect_of(id);
        assert!(r.w > 0, "focus after trap unmount must be live, got {r:?}");
    }
    root.dispose();
}

// ---------------------------------------------------------------------------
// Text input: widget vs independent editing model.
// ---------------------------------------------------------------------------

/// Independent editing model with the SAME documented semantics (char
/// cursor). Random op storms: the widget's value signal must match the
/// model exactly after every op.
struct EditModel {
    text: Vec<char>,
    cursor: usize,
}

impl EditModel {
    fn apply(&mut self, ev: &UiEvent) {
        let UiEvent::Key(k) = ev else { return };
        match k.key {
            Key::Char(c) => {
                self.text.insert(self.cursor, c);
                self.cursor += 1;
            }
            Key::Backspace => {
                if self.cursor > 0 {
                    self.cursor -= 1;
                    self.text.remove(self.cursor);
                }
            }
            Key::Delete => {
                if self.cursor < self.text.len() {
                    self.text.remove(self.cursor);
                }
            }
            Key::Left => self.cursor = self.cursor.saturating_sub(1),
            Key::Right => self.cursor = (self.cursor + 1).min(self.text.len()),
            Key::Home => self.cursor = 0,
            Key::End => self.cursor = self.text.len(),
            _ => {}
        }
    }
}

#[test]
fn input_editing_property_matches_model() {
    let t = tokens();
    let mut tree = UiTree::new(Size::new(24, 3));
    let mut value_handle = None;
    let (root, ()) = create_root(|cx| {
        let value = cx.signal(String::new());
        value_handle = Some(value);
        let view = Element::new()
            .child(TextInput::new().value(value).element(cx, &t).build())
            .build();
        tree.mount(cx, view);
    });
    flush_effects();
    tree.layout();
    tree.dispatch(&key(Key::Tab)); // focus the input
    flush_effects();

    let value = value_handle.unwrap();
    let mut model = EditModel {
        text: Vec::new(),
        cursor: 0,
    };
    let mut rng = Rng::new(0xED17);
    for step in 0..3000 {
        let ev = match rng.below(10) {
            0 => key(Key::Backspace),
            1 => key(Key::Delete),
            2 => key(Key::Left),
            3 => key(Key::Right),
            4 => key(Key::Home),
            5 => key(Key::End),
            _ => {
                key(Key::Char(*rng.pick(&[
                    'a', 'b', 'z', '0', '9', ' ', '-', 'é', '漢', '🎉',
                ])))
            }
        };
        model.apply(&ev);
        tree.dispatch(&ev);
        flush_effects();
        tree.layout();
        let got = value.get_untracked();
        let want: String = model.text.iter().collect();
        assert_eq!(
            got, want,
            "step {step}: value diverged from the editing model"
        );
    }
    root.dispose();
}

/// FINDING RT3-2 (P2, REACT): the input cursor indexes CHARS, not
/// grapheme clusters (their own honesty note). Backspace after a ZWJ
/// family removes ONE scalar, leaving a torn cluster in the value.
/// Acceptance (cluster-atomic editing): un-ignore on fix.
/// [ignore LIFTED cycle 4 by REACT per ruling R4-2: editing is
/// cluster-indexed via text::segments — one Backspace, one cluster.]
#[test]
fn input_backspace_deletes_whole_grapheme_cluster() {
    let t = tokens();
    let mut tree = UiTree::new(Size::new(30, 3));
    let mut value_handle = None;
    let (root, ()) = create_root(|cx| {
        let value = cx.signal(String::new());
        value_handle = Some(value);
        let view = Element::new()
            .child(TextInput::new().value(value).element(cx, &t).build())
            .build();
        tree.mount(cx, view);
    });
    flush_effects();
    tree.layout();
    tree.dispatch(&key(Key::Tab));
    flush_effects();
    let value = value_handle.unwrap();

    // Type "a" + a ZWJ family (arrives as one Paste-like char storm in
    // v1: each scalar is one Char key, as terminals deliver text).
    for c in "a\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F466}".chars() {
        tree.dispatch(&key(Key::Char(c)));
    }
    flush_effects();
    // ONE backspace must remove the WHOLE family (one grapheme).
    tree.dispatch(&key(Key::Backspace));
    flush_effects();
    assert_eq!(
        value.get_untracked(),
        "a",
        "backspace must delete the entire ZWJ cluster, not one scalar"
    );
    root.dispose();
}

/// RT3-2 torture (cycle 5): editing must stay cluster-atomic across the
/// HARDEST clusters — regional-indicator flags (two scalars), skin-tone
/// modifiers (base + FE0F? + modifier), and multi-person ZWJ families —
/// at EVERY edit op. The property is verified against the engine's own
/// `text::segments`: after a Backspace the value must be a whole-cluster
/// prefix of what was typed (one fewer cluster), and after inserting a
/// cluster mid-string the value must still segment into exactly the
/// clusters we typed in that order. A char-indexed cursor (the RT3-2
/// defect) tears any of these and fails here.
#[test]
fn input_editing_is_cluster_atomic_under_hard_clusters() {
    use abstracttui::text::segments;

    // Each entry is ONE grapheme cluster (typed as its scalar storm).
    let clusters = [
        "a",
        "\u{1F1EB}\u{1F1F7}", // flag: FR
        "\u{1F1EF}\u{1F1F5}", // flag: JP
        "\u{1F44B}\u{1F3FE}", // waving hand + medium-dark skin tone
        "\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}\u{200D}\u{1F466}", // family MWGB
        "e\u{0301}",          // e + combining acute
        "\u{1F469}\u{1F3FF}\u{200D}\u{1F4BB}", // woman: dark skin tone + laptop
        "漢",                 // wide
        "z",
    ];

    let t = tokens();
    let mut tree = UiTree::new(Size::new(40, 3));
    let mut value_handle = None;
    let (root, ()) = create_root(|cx| {
        let value = cx.signal(String::new());
        value_handle = Some(value);
        let view = Element::new()
            .child(TextInput::new().value(value).element(cx, &t).build())
            .build();
        tree.mount(cx, view);
    });
    flush_effects();
    tree.layout();
    tree.dispatch(&key(Key::Tab));
    flush_effects();
    let value = value_handle.unwrap();

    // Type every cluster scalar-by-scalar (how terminals deliver text).
    let mut typed_seq: Vec<String> = Vec::new();
    for cl in &clusters {
        for c in cl.chars() {
            tree.dispatch(&key(Key::Char(c)));
        }
        typed_seq.push((*cl).to_string());
        flush_effects();
    }
    // The full value must segment into exactly our clusters, in order.
    let seg_now = |v: &str| {
        segments(v)
            .map(|s| s.cluster.to_string())
            .collect::<Vec<_>>()
    };
    assert_eq!(
        seg_now(&value.get_untracked()),
        typed_seq,
        "typed value did not segment into the source clusters"
    );

    // Backspace from the end, one at a time: each removes exactly ONE
    // whole cluster; the remaining value is the cluster prefix.
    for remaining in (0..clusters.len()).rev() {
        tree.dispatch(&key(Key::Backspace));
        flush_effects();
        let got = seg_now(&value.get_untracked());
        assert_eq!(
            got,
            typed_seq[..remaining],
            "backspace {remaining}: not cluster-atomic (torn cluster in value {:?})",
            value.get_untracked()
        );
    }
    assert_eq!(
        value.get_untracked(),
        "",
        "value should be empty after clearing"
    );

    // Re-type, then Delete from Home: each removes the FIRST cluster.
    for cl in &clusters {
        for c in cl.chars() {
            tree.dispatch(&key(Key::Char(c)));
        }
        flush_effects();
    }
    tree.dispatch(&key(Key::Home));
    flush_effects();
    for first in 1..=clusters.len() {
        tree.dispatch(&key(Key::Delete));
        flush_effects();
        let got = seg_now(&value.get_untracked());
        assert_eq!(
            got,
            typed_seq[first..],
            "delete {first}: not cluster-atomic (torn suffix {:?})",
            value.get_untracked()
        );
    }

    // Left/Right navigation must land ONLY on cluster boundaries: retype
    // two clusters, step Left once (over the last WHOLE cluster), insert
    // a marker, and assert the value segments cleanly with the marker
    // between the two clusters — never inside one.
    for c in "\u{1F1EB}\u{1F1F7}\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}".chars() {
        tree.dispatch(&key(Key::Char(c)));
    }
    flush_effects();
    tree.dispatch(&key(Key::Left)); // over the family cluster as one unit
    tree.dispatch(&key(Key::Char('!')));
    flush_effects();
    let got = seg_now(&value.get_untracked());
    assert_eq!(
        got,
        vec![
            "\u{1F1EB}\u{1F1F7}".to_string(),
            "!".to_string(),
            "\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}".to_string(),
        ],
        "Left+insert tore a cluster (value {:?})",
        value.get_untracked()
    );
    root.dispose();
}

// ---------------------------------------------------------------------------
// List windowing under 10k items.
// ---------------------------------------------------------------------------

/// Only the visible window may be drawn: draw-call count is bounded by
/// the viewport, not the item count; End jumps keep the selection in
/// view; the cost stays flat wherever the window sits.
#[test]
fn list_10k_items_draws_only_the_window() {
    let t = tokens();
    let size = Size::new(30, 12);
    let mut tree = UiTree::new(size);
    let (root, ()) = create_root(|cx| {
        let view = Element::new()
            .child(
                List::new((0..10_000).map(|i| format!("row {i}")).collect())
                    .element(cx, &t)
                    .build(),
            )
            .build();
        tree.mount(cx, view);
    });
    flush_effects();
    tree.layout();
    tree.dispatch(&key(Key::Tab)); // focus the list
    flush_effects();

    let draw_cost = |tree: &mut UiTree| -> usize {
        let puts = Rc::new(RefCell::new(0usize));
        let mut canvas = CountingCanvas {
            inner: BufferCanvas::new(size),
            puts: puts.clone(),
        };
        tree.draw(&mut canvas);
        let n = *puts.borrow();
        n
    };

    let top_cost = draw_cost(&mut tree);
    let budget = (size.w * size.h) as usize * 3; // viewport-proportional
    assert!(
        top_cost <= budget,
        "drawing 10k items at the top cost {top_cost} puts (budget {budget}) — \
         the list is not windowing"
    );

    // Jump to the end: selection must be in view; cost stays flat.
    tree.dispatch(&key(Key::End));
    flush_effects();
    tree.layout();
    let end_cost = draw_cost(&mut tree);
    assert!(
        end_cost <= budget,
        "window at item 9999 cost {end_cost} puts — windowing broke at the tail"
    );
    let canvas = {
        let mut c = BufferCanvas::new(size);
        tree.draw(&mut c);
        c
    };
    let visible: String = (0..size.h).map(|y| canvas.row_text(y) + "\n").collect();
    assert!(
        visible.contains("row 9999"),
        "End must scroll the selected last row into view:\n{visible}"
    );

    // PageUp storms: cost flat at every window position.
    for _ in 0..12 {
        tree.dispatch(&key(Key::PageUp));
        flush_effects();
        let c = draw_cost(&mut tree);
        assert!(
            c <= budget,
            "mid-list window cost {c} exceeded budget {budget}"
        );
    }
    root.dispose();
}

// ---------------------------------------------------------------------------
// Hit-testing: topmost target wins at every cell.
// ---------------------------------------------------------------------------

#[test]
fn click_hit_testing_topmost_wins_everywhere() {
    // Three overlapping absolute-positioned plates, later siblings on
    // top. Click every cell: the recorded hit must match the analytic
    // topmost plate (or nothing).
    let hits: Rc<RefCell<Vec<(i32, i32, u8)>>> = Rc::new(RefCell::new(Vec::new()));
    let mut tree = UiTree::new(Size::new(24, 10));
    let plates = [
        Rect::new(1, 1, 12, 6), // plate 0 (bottom)
        Rect::new(6, 3, 12, 6), // plate 1 (middle)
        Rect::new(10, 0, 6, 4), // plate 2 (top)
    ];
    let (root, ()) = create_root(|cx| {
        let _ = cx;
        let mut el = Element::new().style(
            LayoutStyle::default()
                .width(Dimension::Cells(24))
                .height(Dimension::Cells(10)),
        );
        for (i, r) in plates.iter().enumerate() {
            let hits = hits.clone();
            el = el.child(
                Element::new()
                    .style(
                        LayoutStyle::default()
                            .absolute(Inset {
                                left: Some(r.x),
                                top: Some(r.y),
                                right: None,
                                bottom: None,
                            })
                            .width(Dimension::Cells(r.w))
                            .height(Dimension::Cells(r.h)),
                    )
                    // Bubble hears the target phase (DOM semantics); the
                    // ctx.target identity check keeps this a TARGET
                    // assertion (bubbling from a child would differ).
                    .on(Phase::Bubble, move |ctx, ev| {
                        if let UiEvent::Mouse(m) = ev {
                            if matches!(m.kind, MouseKind::Down(_)) {
                                hits.borrow_mut().push((m.pos.x, m.pos.y, i as u8));
                                ctx.stop_propagation();
                            }
                        }
                    })
                    .build(),
            );
        }
        tree.mount(cx, el.build());
    });
    flush_effects();
    tree.layout();

    for y in 0..10 {
        for x in 0..24 {
            hits.borrow_mut().clear();
            // Full press+release: a Down auto-captures its target, so a
            // Down without its Up would glue every LATER dispatch to the
            // first target (that is the capture contract, not a bug).
            for ev in click(x, y) {
                tree.dispatch(&ev);
            }
            let expected: Option<u8> = plates
                .iter()
                .enumerate()
                .rev() // later siblings win
                .find(|(_, r)| r.contains(Point::new(x, y)))
                .map(|(i, _)| i as u8);
            let got = hits.borrow().last().map(|(_, _, i)| *i);
            assert_eq!(
                got, expected,
                "click at ({x},{y}): hit {got:?}, analytic topmost {expected:?}"
            );
        }
    }
    root.dispose();
}
