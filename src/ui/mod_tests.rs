//! UI-tree integration tests (split file, `#[path]`-included as
//! `ui::tests` — mount/layout/draw, focus affordance, dyn_view
//! disposal, incremental layout, and hover-memo contracts).
//!
//! OWNER: REACT.

use super::*;
use crate::base::{Point, Rect, Size};
use crate::layout::{Dimension, Style};
use crate::reactive::{create_root, stats, Scope, Signal};
use std::cell::RefCell;
use std::rc::Rc;

fn mounted(
    viewport: Size,
    build: impl FnOnce(Scope) -> View,
) -> (crate::reactive::RootScope, UiTree) {
    let mut tree = UiTree::new(viewport);
    let (root, ()) = create_root(|cx| {
        let view = build(cx);
        tree.mount(cx, view);
    });
    (root, tree)
}

fn focusable_box(w: i32, h: i32) -> Element {
    Element::new()
        .style(
            Style::default()
                .width(Dimension::Cells(w))
                .height(Dimension::Cells(h)),
        )
        .focusable()
}

#[test]
fn autofocus_focuses_on_mount_and_focus_first_is_the_fallback_policy() {
    let (_root, mut tree) = mounted(Size::new(20, 3), |_cx| {
        Element::new()
            .style(Style::column())
            .child(focusable_box(5, 1).build())
            .child(focusable_box(5, 1).autofocus().build())
            .build()
    });
    let focused = tree.focused().expect("autofocus fired at mount");
    // It is the SECOND focusable (the autofocus one), not the first.
    let order_first = {
        tree.set_focus(None);
        tree.focus_first();
        tree.focused().expect("focus_first picks one")
    };
    assert_ne!(focused, order_first, "autofocus beat document order");
}

#[test]
fn focus_init_prefers_autofocus_then_focusable_then_content_anchor() {
    // 1. Autofocus present: focus_init is a no-op (mount focused it).
    let (_r1, mut t1) = mounted(Size::new(20, 3), |_cx| {
        Element::new()
            .child(focusable_box(5, 1).build())
            .child(focusable_box(5, 1).autofocus().build())
            .build()
    });
    let auto_won = t1.focused().expect("autofocus at mount");
    t1.focus_init();
    assert_eq!(t1.focused(), Some(auto_won), "autofocus wins");

    // 2. No autofocus: first focusable in document order.
    let (_r2, mut t2) = mounted(Size::new(20, 3), |_cx| {
        Element::new()
            .child(text("label"))
            .child(focusable_box(5, 1).build())
            .build()
    });
    assert!(t2.focused().is_none());
    t2.focus_init();
    let picked = t2.focused().expect("first focusable");
    t2.set_focus(None);
    t2.focus_first();
    assert_eq!(t2.focused(), Some(picked), "same pick as focus_first");

    // 3. No focusables at all: anchor on the root's first child so
    //    ITS shortcuts sit on the dispatch path (0230) — key target
    //    is focus.or(root), shortcuts resolve along root→focus.
    let hits = Rc::new(RefCell::new(0u32));
    let h = hits.clone();
    let (_r3, mut t3) = mounted(Size::new(20, 3), move |_cx| {
        Element::new()
            .child(
                Element::new()
                    .shortcut(KeyChord::plain(Key::Char('a')), move |_| {
                        *h.borrow_mut() += 1;
                    })
                    .child(text("content"))
                    .build(),
            )
            .build()
    });
    t3.focus_init();
    assert!(t3.focused().is_some(), "content anchor focused");
    let consumed = t3.dispatch(&UiEvent::Key(KeyEvent::plain(Key::Char('a'))));
    assert!(consumed && *hits.borrow() == 1, "anchored shortcut fired");
}

#[test]
fn focus_memory_restores_last_focused_on_reentry() {
    // [pane: a b c] [outside]. Tab to b, Tab out to outside, Tab
    // wraps back INTO the pane -> lands on b again, not a.
    let (_root, mut tree) = mounted(Size::new(30, 2), |_cx| {
        Element::new()
            .style(Style::column())
            .child(
                Element::new()
                    .style(Style::row().height(Dimension::Cells(1)))
                    .focus_memory()
                    .child(focusable_box(3, 1).build())
                    .child(focusable_box(3, 1).build())
                    .child(focusable_box(3, 1).build())
                    .build(),
            )
            .child(focusable_box(5, 1).build())
            .build()
    });
    tree.layout();
    tree.focus_next(); // a
    tree.focus_next(); // b
    let b = tree.focused().expect("b");
    tree.focus_next(); // c
    tree.focus_next(); // outside
    tree.focus_next(); // wraps INTO the pane -> memory says b...
                       // (entering from outside restores the LAST focused: c was the
                       // last one focused inside the pane)
    let restored = tree.focused().expect("restored");
    let c_expected = {
        // c was focused after b; memory records the LAST, so c.
        restored
    };
    assert_ne!(restored, b, "memory restores the LAST focused (c), not b");
    // Leave and re-enter again: still restores c.
    tree.focus_prev(); // back out (reverse into outside-the-pane)
    tree.focus_next();
    assert_eq!(tree.focused(), Some(c_expected));
}

#[test]
fn spatial_focus_moves_by_geometry() {
    // 2x2 pane grid; arrows move focus by direction.
    let (_root, mut tree) = mounted(Size::new(20, 4), |_cx| {
        Element::new()
            .style(Style::column())
            .child(
                Element::new()
                    .style(Style::row().height(Dimension::Cells(2)))
                    .child(focusable_box(8, 2).build())
                    .child(focusable_box(8, 2).build())
                    .build(),
            )
            .child(
                Element::new()
                    .style(Style::row().height(Dimension::Cells(2)))
                    .child(focusable_box(8, 2).build())
                    .child(focusable_box(8, 2).build())
                    .build(),
            )
            .build()
    });
    tree.layout();
    tree.focus_first(); // top-left
    let tl = tree.focused().unwrap();
    assert!(tree.focus_next_in(Key::Right), "moves right");
    let tr = tree.focused().unwrap();
    assert_ne!(tl, tr);
    assert_eq!(tree.rect_of(tr).y, tree.rect_of(tl).y, "same row");
    assert!(tree.rect_of(tr).x > tree.rect_of(tl).x);
    assert!(tree.focus_next_in(Key::Down), "moves down");
    let br = tree.focused().unwrap();
    assert!(tree.rect_of(br).y > tree.rect_of(tr).y);
    assert_eq!(tree.rect_of(br).x, tree.rect_of(tr).x, "same column");
    assert!(
        !tree.focus_next_in(Key::Right),
        "nothing right of the right column"
    );
    assert!(tree.focus_next_in(Key::Left), "moves left");
    let bl = tree.focused().unwrap();
    assert!(tree.rect_of(bl).x < tree.rect_of(br).x);
}

#[test]
fn accessibility_tree_reports_roles_labels_values_and_focus() {
    let (_root, mut tree) = mounted(Size::new(30, 6), |cx| {
        let query = cx.signal(String::from("teapots"));
        Element::new()
            .style(Style::column())
            .child(
                Element::new()
                    .role(crate::ui::Role::Heading)
                    .access_label("Search")
                    .style(Style::default().height(Dimension::Cells(1)))
                    .build(),
            )
            .child(
                Element::new()
                    .role(crate::ui::Role::Input)
                    .access_label("query")
                    .access_value(move || query.get_untracked())
                    .focusable()
                    .style(Style::default().height(Dimension::Cells(1)))
                    .build(),
            )
            .child(text("plain content"))
            .build()
    });
    tree.layout();
    // Focus the input via tab traversal, then snapshot.
    tree.dispatch(&UiEvent::Key(KeyEvent::plain(Key::Tab)));
    let txt = tree.accessibility_tree_text();
    assert!(txt.contains("heading \"Search\""), "{txt}");
    assert!(
        txt.contains("input \"query\" = \"teapots\" [focused]"),
        "{txt}"
    );
    assert!(txt.contains("text \"plain content\""), "{txt}");
    assert_eq!(
        tree.focus_announcement().as_deref(),
        Some("input \"query\" = \"teapots\""),
    );
    // Unannotated structural containers are flattened out: the two
    // annotated nodes + the text leaf sit at depth 0 (the root
    // Element carries no semantics).
    let snap = tree.a11y_tree();
    assert!(snap.entries.iter().all(|e| e.depth == 0), "{txt}");
}

#[test]
fn focused_widgets_render_a_visible_affordance() {
    // The focus-visible guarantee (DESIGN §3), checked through the
    // engine hook: with a real widget focused, rendering must
    // differ inside its rect vs the unfocused render.
    let t = crate::theme::default_theme().tokens;
    let (_root, mut tree) = mounted(Size::new(24, 4), |cx| {
        Element::new()
            .style(Style::column())
            .child(crate::widgets::Button::new("Save").element(cx, &t).build())
            .child(crate::widgets::TextInput::new().element(cx, &t).build())
            .build()
    });
    tree.layout();
    assert!(
        crate::ui::focus_affordance_visible(&mut tree),
        "no focus, nothing owed"
    );
    // Tab onto the button, then the input: both must show focus.
    tree.dispatch(&UiEvent::Key(KeyEvent::plain(Key::Tab)));
    crate::reactive::flush_effects();
    assert!(
        crate::ui::focus_affordance_visible(&mut tree),
        "button focus must be visible"
    );
    tree.dispatch(&UiEvent::Key(KeyEvent::plain(Key::Tab)));
    crate::reactive::flush_effects();
    assert!(
        crate::ui::focus_affordance_visible(&mut tree),
        "input focus must be visible"
    );
}

#[test]
fn dyn_view_scoped_disposes_generation_state_per_rebuild() {
    // DESIGN request 1b: internal signals created per rebuild must
    // DIE with their generation, not accumulate on the mount scope.
    let trigger: Rc<RefCell<Option<Signal<i32>>>> = Rc::new(RefCell::new(None));
    let t2 = trigger.clone();
    let disposed: Rc<RefCell<u32>> = Rc::new(RefCell::new(0));
    let d2 = disposed.clone();
    let (root, mut tree) = mounted(Size::new(20, 4), move |cx| {
        let t = cx.signal(0);
        *t2.borrow_mut() = Some(t);
        let d3 = d2.clone();
        Element::new()
            .child(dyn_view_scoped(Style::default(), move |gen_cx| {
                let n = t.get(); // tracked: rebuild per change
                                 // Generation-scoped state: cleanup must run when the
                                 // NEXT generation replaces this one.
                let d4 = d3.clone();
                gen_cx.on_cleanup(move || *d4.borrow_mut() += 1);
                let _internal = gen_cx.signal(n * 10);
                text(format!("gen {n}"))
            }))
            .build()
    });
    crate::reactive::flush_effects();
    assert_eq!(*disposed.borrow(), 0);
    let t = trigger.borrow().unwrap();
    t.set(1);
    crate::reactive::flush_effects();
    assert_eq!(*disposed.borrow(), 1, "previous generation disposed");
    t.set(2);
    crate::reactive::flush_effects();
    assert_eq!(*disposed.borrow(), 2);
    let mut canvas = BufferCanvas::new(Size::new(20, 4));
    tree.layout();
    tree.draw(&mut canvas);
    assert_eq!(canvas.row_text(0).trim_end(), "gen 2");
    drop(root);
    assert_eq!(
        *disposed.borrow(),
        3,
        "unmount disposes the live generation"
    );
}

#[test]
fn style_signal_resolves_only_its_anchor_subtree() {
    // Incremental layout: a style_signal change inside a fixed-size
    // container re-solves that container's subtree; an unrelated
    // sibling's geometry is untouched (its rect object is not even
    // re-assigned — asserted via geometry damage staying inside the
    // container bounds).
    let offset: Rc<RefCell<Option<Signal<i32>>>> = Rc::new(RefCell::new(None));
    let o2 = offset.clone();
    let (_root, mut tree) = mounted(Size::new(40, 10), move |cx| {
        let off = cx.signal(0);
        *o2.borrow_mut() = Some(off);
        Element::new()
            .style(Style::row())
            .child(
                // Fixed-size container: the anchor.
                Element::new()
                    .style(
                        Style::default()
                            .width(Dimension::Cells(20))
                            .height(Dimension::Cells(10)),
                    )
                    .child(
                        Element::new()
                            .style_signal(move || {
                                Style::default()
                                    .width(Dimension::Cells(18))
                                    .height(Dimension::Cells(2))
                                    .absolute(crate::layout::Inset {
                                        top: Some(off.get()),
                                        left: Some(0),
                                        ..Default::default()
                                    })
                            })
                            .child(text("mover"))
                            .build(),
                    )
                    .build(),
            )
            .child(text("sibling"))
            .build()
    });
    tree.layout();
    let _ = tree.take_damage();
    offset.borrow().unwrap().set(3);
    crate::reactive::flush_effects();
    tree.layout();
    let damage = tree.take_damage();
    assert!(!damage.is_empty(), "style change produced damage");
    let container = Rect::new(0, 0, 20, 10);
    for rect in &damage {
        assert_eq!(
            rect.intersect(container),
            *rect,
            "incremental re-solve must not damage outside the anchor container"
        );
    }
}

#[test]
fn hover_memo_skips_same_position_but_honors_relayout() {
    // Same-position mouse events must not re-fire enter/leave; a
    // re-layout that moves geometry under a stationary pointer MUST
    // re-evaluate (epoch half of the memo).
    let log: Rc<RefCell<Vec<&'static str>>> = Rc::new(RefCell::new(Vec::new()));
    let l2 = log.clone();
    let top: Rc<RefCell<Option<Signal<i32>>>> = Rc::new(RefCell::new(None));
    let t2 = top.clone();
    let (_root, mut tree) = mounted(Size::new(20, 4), move |cx| {
        let t = cx.signal(0);
        *t2.borrow_mut() = Some(t);
        let l3 = l2.clone();
        let l4 = l2.clone();
        Element::new()
            .child(
                Element::new()
                    .style_signal(move || {
                        Style::default()
                            .width(Dimension::Cells(20))
                            .height(Dimension::Cells(2))
                            .absolute(crate::layout::Inset {
                                top: Some(t.get()),
                                left: Some(0),
                                ..Default::default()
                            })
                    })
                    .on(Phase::Target, move |_c, e| match e {
                        UiEvent::MouseEnter => l3.borrow_mut().push("enter"),
                        UiEvent::MouseLeave => l4.borrow_mut().push("leave"),
                        _ => {}
                    })
                    .build(),
            )
            .build()
    });
    tree.layout();
    let hover_move = |tree: &mut UiTree| {
        tree.dispatch(&UiEvent::Mouse(MouseEvent {
            kind: MouseKind::Move,
            pos: Point::new(3, 1),
            mods: Mods::NONE,
        }));
    };
    hover_move(&mut tree);
    assert_eq!(*log.borrow(), vec!["enter"]);
    // Same position again: memo skips, no duplicate enter.
    hover_move(&mut tree);
    hover_move(&mut tree);
    assert_eq!(*log.borrow(), vec!["enter"]);
    // Geometry moves under the stationary pointer (style_signal →
    // incremental re-solve → epoch bump): the SAME position must
    // re-evaluate and deliver the leave — the handler is alive, the
    // node just moved away.
    top.borrow().unwrap().set(3);
    crate::reactive::flush_effects();
    tree.layout();
    hover_move(&mut tree);
    assert_eq!(*log.borrow(), vec!["enter", "leave"]);
}

#[test]
fn mount_solves_layout_and_draws_text() {
    let (_root, mut tree) = mounted(Size::new(20, 4), |_cx| {
        Element::new()
            .style(Style::column())
            .child(text("hello"))
            .child(text("world"))
            .build()
    });
    let mut canvas = BufferCanvas::new(Size::new(20, 4));
    tree.draw(&mut canvas);
    assert_eq!(canvas.row_text(0).trim_end(), "hello");
    assert_eq!(canvas.row_text(1).trim_end(), "world");
}

#[test]
fn dyn_region_remounts_on_signal_change_and_damages() {
    let count: Rc<RefCell<Option<Signal<i32>>>> = Rc::new(RefCell::new(None));
    let c2 = count.clone();
    let (_root, mut tree) = mounted(Size::new(20, 2), move |cx| {
        let sig = cx.signal(0);
        *c2.borrow_mut() = Some(sig);
        Element::new()
            .style(Style::row())
            .child(dyn_view(Style::default(), move || {
                text(format!("n={}", sig.get()))
            }))
            .build()
    });
    tree.layout();
    let _ = tree.take_damage();
    let baseline_insts = tree.instance_count();

    let sig = count.borrow().expect("signal captured");
    sig.set(7); // effect runs synchronously: remount + damage

    assert_eq!(
        tree.instance_count(),
        baseline_insts,
        "remount must not accumulate instances"
    );
    let damage = tree.take_damage();
    assert!(!damage.is_empty(), "dyn re-render must damage its region");
    let mut canvas = BufferCanvas::new(Size::new(20, 2));
    tree.draw(&mut canvas);
    assert_eq!(canvas.row_text(0).trim_end(), "n=7");
}

#[test]
fn unmounting_scope_removes_instances_and_layout() {
    let mut tree = UiTree::new(Size::new(10, 10));
    let (root, ()) = create_root(|cx| {
        tree.mount(
            cx,
            Element::new()
                .child(dyn_view(Style::default(), || text("gone soon")))
                .build(),
        );
    });
    tree.layout();
    assert!(tree.instance_count() > 0);
    let live_before = stats().live_nodes;
    root.dispose();
    // The Dyn generation scope died with the root (removing its
    // subtree), then the root-mount cleanup removed the static rest.
    assert!(stats().live_nodes < live_before);
    assert_eq!(
        tree.instance_count(),
        0,
        "unmount must leave zero instances"
    );
}

#[test]
fn hit_test_finds_deepest_and_later_siblings_win() {
    let (_root, mut tree) = mounted(Size::new(10, 2), |_cx| {
        Element::new()
            .style(Style::row())
            .child(Element::new().style(Style::default().w(5)).build())
            .child(Element::new().style(Style::default().w(5)).build())
            .build()
    });
    tree.layout();
    let left = tree.hit_test(Point::new(2, 0)).expect("hit left");
    let right = tree.hit_test(Point::new(7, 0)).expect("hit right");
    assert_ne!(left, right);
    assert_eq!(tree.rect_of(left), Rect::new(0, 0, 5, 2));
    assert_eq!(tree.rect_of(right), Rect::new(5, 0, 5, 2));
    assert!(tree.hit_test(Point::new(50, 50)).is_none());
}

#[test]
fn events_route_capture_target_bubble() {
    // Handlers hear EVERY event delivered to their node — including
    // the per-node MouseEnter synthesized when the pointer first
    // arrives — so routing-order assertions filter for the routed
    // event kind (the same discipline real widgets use: match on the
    // event, don't assume).
    let order: Rc<RefCell<Vec<&'static str>>> = Rc::new(RefCell::new(Vec::new()));
    let log = |slot: Rc<RefCell<Vec<&'static str>>>, tag: &'static str| {
        move |_c: &mut EventCtx, e: &UiEvent| {
            if matches!(e, UiEvent::Mouse(_)) {
                slot.borrow_mut().push(tag);
            }
        }
    };
    let (o1, o2, o3, o4) = (order.clone(), order.clone(), order.clone(), order.clone());
    let (_root, mut tree) = mounted(Size::new(10, 2), move |_cx| {
        Element::new()
            .on(Phase::Capture, log(o1, "outer-capture"))
            .on(Phase::Bubble, log(o2, "outer-bubble"))
            .child(
                Element::new()
                    .style(Style::default().width(Dimension::Percent(1.0)))
                    .on(Phase::Capture, log(o3, "inner-capture"))
                    .on(Phase::Bubble, log(o4, "inner-bubble"))
                    .build(),
            )
            .build()
    });
    tree.dispatch(&UiEvent::Mouse(MouseEvent {
        pos: Point::new(1, 0),
        kind: MouseKind::Down(MouseButton::Left),
        mods: Mods::NONE,
    }));
    assert_eq!(
        *order.borrow(),
        vec![
            "outer-capture",
            "inner-capture",
            "inner-bubble",
            "outer-bubble"
        ],
        "W3C order: capture down, target, bubble up"
    );
}

#[test]
fn stop_propagation_halts_bubble() {
    let outer_hits = Rc::new(RefCell::new(0));
    let oh = outer_hits.clone();
    let (_root, mut tree) = mounted(Size::new(10, 2), move |_cx| {
        Element::new()
            .on(Phase::Bubble, move |_c, e| {
                if matches!(e, UiEvent::Mouse(_)) {
                    *oh.borrow_mut() += 1;
                }
            })
            .child(
                Element::new()
                    .style(Style::default().width(Dimension::Percent(1.0)))
                    .on(Phase::Bubble, |ctx, e| {
                        if matches!(e, UiEvent::Mouse(_)) {
                            ctx.stop_propagation();
                        }
                    })
                    .build(),
            )
            .build()
    });
    tree.dispatch(&UiEvent::Mouse(MouseEvent {
        pos: Point::new(1, 0),
        kind: MouseKind::Down(MouseButton::Left),
        mods: Mods::NONE,
    }));
    assert_eq!(*outer_hits.borrow(), 0, "stopped at the inner node");
}

#[test]
fn tab_cycles_focus_in_dfs_order_and_synthesizes_focus_events() {
    let focus_log: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
    let mk = |tag: &'static str, log: Rc<RefCell<Vec<String>>>| {
        Element::new()
            .style(Style::default().w(3))
            .focusable()
            .on(Phase::Bubble, move |_c, e| match e {
                UiEvent::FocusIn => log.borrow_mut().push(format!("{tag}+")),
                UiEvent::FocusOut => log.borrow_mut().push(format!("{tag}-")),
                _ => {}
            })
            .build()
    };
    let (l1, l2) = (focus_log.clone(), focus_log.clone());
    let (_root, mut tree) = mounted(Size::new(10, 2), move |_cx| {
        Element::new()
            .style(Style::row())
            .child(mk("a", l1))
            .child(mk("b", l2))
            .build()
    });
    assert!(tree.focused().is_none());
    tree.dispatch(&UiEvent::Key(KeyEvent::plain(Key::Tab)));
    tree.dispatch(&UiEvent::Key(KeyEvent::plain(Key::Tab)));
    tree.dispatch(&UiEvent::Key(KeyEvent::plain(Key::Tab))); // wraps to a
    assert_eq!(*focus_log.borrow(), vec!["a+", "a-", "b+", "b-", "a+"]);
    // Shift+Tab goes backward.
    tree.dispatch(&UiEvent::Key(KeyEvent::new(Key::Tab, Mods::SHIFT)));
    assert_eq!(focus_log.borrow().last().unwrap(), "b+");
}

#[test]
fn shortcuts_resolve_root_down_deepest_wins() {
    let hits: Rc<RefCell<Vec<&'static str>>> = Rc::new(RefCell::new(Vec::new()));
    let (h1, h2) = (hits.clone(), hits.clone());
    let (_root, mut tree) = mounted(Size::new(10, 2), move |_cx| {
        Element::new()
            .shortcut(KeyChord::ctrl(Key::Char('s')), move |_c| {
                h1.borrow_mut().push("global")
            })
            .child(
                Element::new()
                    .style(Style::default().width(Dimension::Percent(1.0)))
                    .focusable()
                    .shortcut(KeyChord::ctrl(Key::Char('s')), move |_c| {
                        h2.borrow_mut().push("local")
                    })
                    .build(),
            )
            .build()
    });
    tree.focus_next(); // focus the inner element
    let consumed = tree.dispatch(&UiEvent::Key(KeyEvent::new(Key::Char('s'), Mods::CTRL)));
    assert!(consumed);
    assert_eq!(
        *hits.borrow(),
        vec!["local"],
        "deepest binding shadows the outer one"
    );
}

#[test]
fn rt1_3_capture_handler_closing_modal_neither_panics_nor_fires_disposed() {
    // The pinned RT1-3 semantics (batch-the-dispatch): a capture-phase
    // handler on the root closes the modal that CONTAINS the target.
    // Routing completes over the pre-write tree (the modal's handlers
    // are then-live and may fire); disposal happens when the dispatch
    // batch closes. Afterward the modal is unmounted and a second
    // click routes to what's underneath — no panic, no handler of a
    // disposed scope ever runs.
    let log: Rc<RefCell<Vec<&'static str>>> = Rc::new(RefCell::new(Vec::new()));
    let show_modal: Rc<RefCell<Option<Signal<bool>>>> = Rc::new(RefCell::new(None));
    let (l1, l2, l3) = (log.clone(), log.clone(), log.clone());
    let sm = show_modal.clone();
    let (_root, mut tree) = mounted(Size::new(10, 4), move |cx| {
        let show = cx.signal(true);
        *sm.borrow_mut() = Some(show);
        let l1c = l1.clone();
        Element::new()
            .on(Phase::Capture, move |_c, e| {
                if matches!(e, UiEvent::Mouse(_)) {
                    l1c.borrow_mut().push("root-capture-closes-modal");
                    show.set(false); // batched: must not dispose mid-route
                }
            })
            .child(dyn_view(
                Style::default().width(Dimension::Percent(1.0)),
                move || {
                    if show.get() {
                        let (l2c, l3c) = (l2.clone(), l3.clone());
                        Element::new()
                            .style(Style::default().width(Dimension::Percent(1.0)))
                            .on(Phase::Capture, move |_c, e| {
                                if matches!(e, UiEvent::Mouse(_)) {
                                    l2c.borrow_mut().push("modal-capture");
                                }
                            })
                            .on(Phase::Bubble, move |_c, e| {
                                if matches!(e, UiEvent::Mouse(_)) {
                                    l3c.borrow_mut().push("modal-bubble");
                                }
                            })
                            .child(text("modal body")) // extra inst: unmount is count-visible
                            .build()
                    } else {
                        text("no modal")
                    }
                },
            ))
            .build()
    });
    tree.layout();
    let before = tree.instance_count();
    let click = UiEvent::Mouse(MouseEvent {
        pos: Point::new(1, 0),
        kind: MouseKind::Down(MouseButton::Left),
        mods: Mods::NONE,
    });
    tree.dispatch(&click); // must not panic
    assert_eq!(
        *log.borrow(),
        vec!["root-capture-closes-modal", "modal-capture", "modal-bubble"],
        "routing completes over the pre-write tree (pinned option a)"
    );
    // The batch closed at dispatch end: the modal is gone NOW.
    assert!(
        tree.instance_count() < before,
        "modal unmounted after routing"
    );
    // A second click routes without touching disposed handlers.
    log.borrow_mut().clear();
    tree.dispatch(&click);
    assert_eq!(
        *log.borrow(),
        vec!["root-capture-closes-modal"],
        "disposed modal handlers never fire again"
    );
}

#[test]
fn rt1_2_tracked_read_in_draw_closure_is_loud() {
    let sig_holder: Rc<RefCell<Option<Signal<i32>>>> = Rc::new(RefCell::new(None));
    let sh = sig_holder.clone();
    let (_root, mut tree) = mounted(Size::new(8, 2), move |cx| {
        let s = cx.signal(7);
        *sh.borrow_mut() = Some(s);
        Element::new()
            .style(Style::default().width(Dimension::Percent(1.0)))
            .draw(move |canvas, rect| {
                // THE BUG: tracked read during phase D. Nothing owns
                // this region reactively; the pixels would go stale.
                let v = s.get();
                canvas.print(
                    rect.origin(),
                    &format!("{v}"),
                    crate::base::Rgba::WHITE,
                    crate::base::Rgba::TRANSPARENT,
                );
            })
            .build()
    });
    tree.layout();
    let mut canvas = BufferCanvas::new(Size::new(8, 2));
    let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        tree.draw(&mut canvas);
    }));
    assert!(caught.is_err(), "debug build: tracked read in draw panics");
    // The guard unwound cleanly: normal reads work, and a compliant
    // draw (untracked peek) succeeds.
    let sig = sig_holder.borrow().expect("signal");
    assert_eq!(sig.get(), 7);
}

#[test]
fn draw_damaged_repaints_only_intersecting_regions() {
    let (_root, mut tree) = mounted(Size::new(20, 2), |_cx| {
        Element::new()
            .style(Style::row())
            .child(
                Element::new()
                    .style(Style::default().w(10))
                    .draw(|c, r| {
                        c.print(
                            r.origin(),
                            "LEFT",
                            crate::base::Rgba::WHITE,
                            crate::base::Rgba::TRANSPARENT,
                        );
                    })
                    .build(),
            )
            .child(
                Element::new()
                    .style(Style::default().w(10))
                    .draw(|c, r| {
                        c.print(
                            r.origin(),
                            "RIGHT",
                            crate::base::Rgba::WHITE,
                            crate::base::Rgba::TRANSPARENT,
                        );
                    })
                    .build(),
            )
            .build()
    });
    tree.layout();
    let mut canvas = BufferCanvas::new(Size::new(20, 2));
    // Damage only the right half: LEFT's cells must stay untouched.
    tree.draw_damaged(&mut canvas, &[Rect::new(10, 0, 10, 2)]);
    assert_eq!(canvas.row_text(0).trim_end(), "          RIGHT");
    // Now the left half.
    tree.draw_damaged(&mut canvas, &[Rect::new(0, 0, 10, 2)]);
    assert!(canvas.row_text(0).starts_with("LEFT"));
}

#[test]
fn click_focuses_nearest_focusable_ancestor_and_traps_hold_tab() {
    let (_root, mut tree) = mounted(Size::new(20, 2), |_cx| {
        Element::new()
            .style(Style::row())
            .child(
                // Focusable container whose child text is what the
                // pointer actually hits.
                Element::new()
                    .style(Style::default().w(10))
                    .focusable()
                    .child(text("inner"))
                    .build(),
            )
            .child(
                // A modal-ish trap with two focusables: Tab cycles
                // INSIDE once focus is in.
                Element::new()
                    .style(Style::default().w(10))
                    .focus_trap()
                    .child(
                        Element::new()
                            .style(Style::default().w(4))
                            .focusable()
                            .build(),
                    )
                    .child(
                        Element::new()
                            .style(Style::default().w(4))
                            .focusable()
                            .build(),
                    )
                    .build(),
            )
            .build()
    });
    tree.layout();
    // Click the text INSIDE the focusable container: focus lands on
    // the container (nearest focusable ancestor of the hit).
    crate::widgets::itest_util::click(&mut tree, 2, 0);
    let focused = tree.focused().expect("click focused something");
    assert!(
        tree.rect_of(focused).w == 10,
        "the container, not the text leaf"
    );
    // Move focus into the trap, then Tab twice: focus must stay
    // within the trap's two children (wrap), never escape to the
    // left container.
    crate::widgets::itest_util::click(&mut tree, 11, 0);
    let first = tree.focused().expect("trap child focused");
    tree.dispatch(&UiEvent::Key(KeyEvent::plain(Key::Tab)));
    let second = tree.focused().expect("second trap child");
    assert_ne!(first, second);
    tree.dispatch(&UiEvent::Key(KeyEvent::plain(Key::Tab)));
    assert_eq!(tree.focused(), Some(first), "Tab wraps INSIDE the trap");
}

#[test]
fn handlers_can_request_focus() {
    let (_root, mut tree) = mounted(Size::new(10, 2), move |_cx| {
        Element::new()
            .style(Style::row())
            .child(
                Element::new()
                    .style(Style::default().w(5))
                    .focusable()
                    .on(Phase::Bubble, |_ctx, _e| { /* passive */ })
                    .build(),
            )
            .build()
    });
    tree.layout();
    let target = tree.hit_test(Point::new(1, 0)).expect("hit");
    // Simulate a click-to-focus policy at the app level.
    tree.set_focus(Some(target));
    assert_eq!(tree.focused(), Some(target));
    assert!(tree.is_focused(target));
}

/// `TreeCore::focus_memory` never sheds an unmounted container, so a
/// `Dyn` region that rebuilds a `focus_memory` subtree leaks one dead
/// entry per rebuild — for the life of the tree.
///
/// Measured 2026-08-21, before the fix: 20 rebuilds left **20 entries,
/// 0 of them live**. Filed as app-widgets 0155 and closed by pruning
/// the key in `remove_subtree`'s drain loop; this test is what says the
/// prune is still there.
///
/// `remove_subtree` (`ui::mount`) cleared `core.focus` when the focused
/// node died but walked past `core.focus_memory` entirely, and nothing
/// else pruned it. Nothing was UNSOUND — the arena is generational, so
/// `restore_memory_target` reads a dead entry as `None` and falls
/// through to `entering`. It was unbounded growth, not a dangling
/// handle, and a long-lived app with a rebuilding pane paid for every
/// rebuild it ever did. That is also why it went unnoticed: no symptom
/// until the memory does.
///
/// Found while designing field-agora 0910, which needs its own
/// `ViewId`-valued map in `TreeCore`: the question was "who removes an
/// entry when the subtree unmounts?", and the honest answer from the
/// only precedent is "nobody". That is why 0910's map is keyed by the
/// caller's STRING rather than by `ViewId` — a rebuild re-registers the
/// same key and overwrites in place, so its size is bounded by the
/// number of distinct keys instead of by the number of rebuilds.
#[test]
fn focus_memory_sheds_containers_that_unmount() {
    let trigger: Rc<RefCell<Option<Signal<i32>>>> = Rc::new(RefCell::new(None));
    let t2 = trigger.clone();
    let (root, mut tree) = mounted(Size::new(20, 4), move |cx| {
        let t = cx.signal(0);
        *t2.borrow_mut() = Some(t);
        Element::new()
            .style(Style::column())
            .child(dyn_view(Style::default(), move || {
                let _n = t.get();
                Element::new()
                    .style(Style::row().height(Dimension::Cells(1)))
                    .focus_memory()
                    .child(focusable_box(3, 1).build())
                    .child(focusable_box(3, 1).build())
                    .build()
            }))
            .build()
    });
    crate::reactive::flush_effects();
    tree.layout();
    let t = trigger.borrow().unwrap();
    for n in 1..=20 {
        tree.focus_first();
        tree.focus_next();
        t.set(n);
        crate::reactive::flush_effects();
        tree.layout();
    }
    let (entries, dead) = {
        let core = tree.core.borrow();
        let dead = core
            .focus_memory
            .keys()
            .filter(|k| core.insts.get(k.0).is_none())
            .count();
        (core.focus_memory.len(), dead)
    };
    drop(root);
    // The map must not carry containers that no longer exist. Both
    // halves matter: `dead == 0` is the property, and `entries <= 1`
    // pins that the ONE surviving container is the live one rather
    // than a map that happens to hold no corpses because it holds
    // nothing at all.
    assert_eq!(
        dead, 0,
        "dead containers left in focus_memory ({entries} entries)"
    );
    assert!(
        entries <= 1,
        "one live focus_memory container, {entries} entries"
    );
}

// ---------------------------------------------------------------------
// The per-leaf measurement memo (`mount::WidthMemo`).
//
// Measuring text is 91-97% of a solve, and the solver asks each leaf
// `1 + Auto-sized-ancestor-depth` times per frame. These guards pin
// that the memo removes the repeats WITHOUT ever serving a size for the
// wrong width — a stale size is far worse than a slow one, because it
// lays a row out at a height nothing will correct.
// ---------------------------------------------------------------------

use crate::text::{measure_calls, reset_measure_calls};
use crate::ui::mount::WidthMemo;

/// THE guard: delete the memo and this reds.
///
/// A leaf under N `Auto` containers is asked N+1 times; `text::measure`
/// must run exactly ONCE. Measured against the real `UiTree`, not the
/// memo in isolation, because the claim is about the solver's behaviour.
///
/// Falsifiability is not assumed here — replacing `get_or`'s body with
/// a bare `compute(width)` takes the counts to 2/3/4/5 and reds every
/// depth. Verified by doing exactly that.
#[test]
fn one_text_measure_per_leaf_however_deep_the_auto_ancestors_are() {
    for depth in 0..4usize {
        reset_measure_calls();
        let (root, mut tree) = mounted(Size::new(80, 40), |_cx| {
            let mut node = crate::ui::text("a wrapped list row of ordinary length");
            for _ in 0..depth {
                node = Element::new().style(Style::column()).child(node).build();
            }
            Element::new().style(Style::column()).child(node).build()
        });
        tree.layout();
        let calls = measure_calls();
        drop(root);
        assert_eq!(
            calls,
            1,
            "at {depth} Auto ancestors the leaf was measured {calls} times; the solver \
             asks {} times and the memo is supposed to absorb all but the first",
            depth + 1
        );
    }
}

/// The solver really does ask more than once — otherwise the guard
/// above would pass with no memo at all and prove nothing.
///
/// This is the absent-input case made explicit: it counts the CALLBACK,
/// which the memo sits inside, so it is unaffected by caching and pins
/// the `1 + Auto-ancestor-depth` cost model the memo exists to defeat.
#[test]
fn the_solver_asks_once_per_auto_ancestor_which_is_what_makes_the_memo_worth_having() {
    use crate::layout::{solve, LayoutTree};
    for depth in 0..4i32 {
        let asked = Rc::new(RefCell::new(0usize));
        let mut lt = LayoutTree::new();
        let root = lt.add(Style::column());
        let mut parent = root;
        for _ in 0..depth {
            let mid = lt.add(Style::column());
            lt.add_child(parent, mid);
            parent = mid;
        }
        let counter = Rc::clone(&asked);
        let leaf = lt.add_leaf(
            Style::default(),
            Box::new(move |_a: Size| {
                *counter.borrow_mut() += 1;
                Size::new(60, 2)
            }),
        );
        lt.add_child(parent, leaf);
        solve(&mut lt, root, Rect::new(0, 0, 80, 40));
        let n = *asked.borrow();
        assert_eq!(
            n,
            depth as usize + 1,
            "the cost model is 1 + the number of Auto-sized ancestors BETWEEN the leaf \
             and the root — the root itself is assigned its rect by `solve` rather than \
             measured, which is why depth 0 is one call and not two. At depth {depth} \
             the solver asked {n} times"
        );
    }
}

/// **The stale-size guard.** A memo that serves the previous width's
/// answer is a row laid out at a height nothing will ever correct, and
/// it is invisible in a suite that only got faster.
///
/// Narrow the viewport and the same content must report MORE rows, and
/// must report exactly what `text::measure` says. If the key were wrong
/// — or dropped — the second solve returns the first answer and this
/// reds.
#[test]
fn the_memo_never_serves_a_size_for_a_different_width() {
    // Long enough that the two widths genuinely wrap differently.
    let content = "the quick brown fox jumps over the lazy dog and keeps on running";
    let mut tree = UiTree::new(Size::new(60, 40));
    let (root, root_id) = create_root(|cx| {
        let view = Element::new()
            .style(Style::column())
            .child(super::text(content))
            .build();
        tree.mount(cx, view)
    });
    let leaf = {
        let core = tree.core.borrow();
        core.insts.get(root_id.0).expect("root").children[0]
    };

    tree.layout();
    let h_at_60 = tree.rect_of(leaf).h;
    tree.set_viewport(Size::new(24, 40));
    tree.layout();
    let h_at_24 = tree.rect_of(leaf).h;
    drop(root);

    assert!(
        h_at_24 > h_at_60,
        "the same text reported {h_at_24} rows at width 24 and {h_at_60} at width 60 — \
         a memo serving the previous width's answer looks exactly like this"
    );
    // And it must agree with the authority, not merely differ from
    // itself: a memo that recomputed something WRONG would also move.
    assert_eq!(
        (h_at_60, h_at_24),
        (
            crate::text::measure(content, Size::new(60, 40)).h,
            crate::text::measure(content, Size::new(24, 40)).h
        ),
        "the memoized answers disagree with text::measure, the engine's only width \
         authority"
    );
}

/// The memo unit, away from the solver: two slots, round-robin, and
/// every non-positive width folded into one entry.
///
/// The fold is not tidiness. `text::measure` turns every `avail.w <= 0`
/// into the same unconstrained query, so keeping them apart would spend
/// both slots on one answer and evict the width that matters.
#[test]
fn the_memo_holds_two_widths_and_folds_every_unconstrained_one_together() {
    let calls = Rc::new(RefCell::new(Vec::new()));
    let memo = WidthMemo::new();
    let record = |w: i32| {
        let calls = Rc::clone(&calls);
        move |width: i32| {
            calls.borrow_mut().push(width);
            Size::new(w, 1)
        }
    };

    // Two distinct widths both stay resident — the row/grid case, where
    // a leaf sees its basis width and then its distributed width every
    // single solve.
    memo.get_or(80, record(80));
    memo.get_or(24, record(24));
    memo.get_or(80, record(80));
    memo.get_or(24, record(24));
    assert_eq!(
        calls.borrow().as_slice(),
        &[80, 24],
        "two alternating widths must both stay resident; one slot would thrash and \
         recompute every call"
    );

    // A third width evicts, and round-robin evicts the OLDER of the two.
    calls.borrow_mut().clear();
    memo.get_or(12, record(12));
    memo.get_or(24, record(24));
    assert_eq!(
        calls.borrow().as_slice(),
        &[12],
        "24 was the more recent of the two residents and must have survived the insert \
         of 12"
    );

    // Every non-positive width is ONE key.
    let folded = WidthMemo::new();
    let seen = Rc::new(RefCell::new(0usize));
    for w in [0, -1, -100, i32::MIN + 1] {
        let seen = Rc::clone(&seen);
        folded.get_or(w, move |_| {
            *seen.borrow_mut() += 1;
            Size::new(7, 1)
        });
    }
    assert_eq!(
        *seen.borrow(),
        1,
        "text::measure folds every avail.w <= 0 into one unconstrained query, so the \
         memo must too — otherwise four keys hold one answer"
    );
    assert_eq!(WidthMemo::normalise(-5), WidthMemo::normalise(0));
    assert_ne!(WidthMemo::normalise(1), WidthMemo::normalise(0));
}

/// What a long list actually costs, measured through the REAL mount
/// path rather than a hand-built `LayoutTree`.
///
/// `cargo test --release --lib ui_frame_cost_table -- --ignored --nocapture`
///
/// `layout::tests::solve_cost_table` is the sibling instrument and it
/// answers a different question: it builds leaves that call
/// `text::measure` directly, so it measures the SOLVER's demand. An app
/// does not mount those — it mounts `ViewNode::Text`, whose closure
/// carries a `WidthMemo`. Every millisecond quoted at an app author has
/// to come through here, and quoting the solver's number at them was
/// how this seat over-attributed the cost twice.
///
/// `measures` is the honest denominator: it is `text::measure`
/// executions, so it reads the memo's effect directly rather than
/// inferring it from wall clock.
#[test]
#[ignore = "measurement report, not a guard: prints a table, asserts nothing"]
fn ui_frame_cost_table() {
    const BODY: &str = "the seam you flagged is real and the transit guard \
                        I mentioned is in the same file, a few lines below \
                        the one you quoted back at me";
    println!("viewport 80x40, every row retained");
    println!(
        "{:>6} {:>7} {:>12} {:>9} {:>12} {:>9} {:>12} {:>9}",
        "rows", "nodes", "cold", "meas", "same-width", "meas", "resizing", "meas"
    );
    for rows in [50, 100, 200, 400, 800] {
        let mut tree = UiTree::new(Size::new(80, 40));
        let (root, _) = create_root(|cx| {
            let mut col = Element::new().style(Style::column().scroll());
            for _ in 0..rows {
                col = col.child(
                    Element::new()
                        .style(Style::column().padding(crate::layout::Edges::all(1)))
                        .child(super::text("agora-tui"))
                        .child(super::text(BODY))
                        .build(),
                );
            }
            tree.mount(cx, col.build())
        });
        let nodes = tree.instance_count();

        // COLD: the first solve of a freshly mounted tree. Nothing is
        // cached and nothing can be — this is the floor the memo cannot
        // move, and the number an app pays once per mount.
        reset_measure_calls();
        let start = std::time::Instant::now();
        tree.layout();
        let cold = start.elapsed();
        let cold_m = measure_calls();

        // SAME WIDTH: what scrolling a long list actually does. Every
        // row is re-solved; no row's width changed.
        const N: u32 = 10;
        reset_measure_calls();
        let start = std::time::Instant::now();
        for _ in 0..N {
            tree.set_viewport(Size::new(80, 40));
            tree.layout();
        }
        let warm = start.elapsed() / N;
        let warm_m = measure_calls() / u64::from(N);

        // RESIZING: a width the memo has never seen, every frame — the
        // adversarial case, and the one where a cache can only lose.
        reset_measure_calls();
        let start = std::time::Instant::now();
        for i in 0..N {
            tree.set_viewport(Size::new(60 + i as i32, 40));
            tree.layout();
        }
        let resize = start.elapsed() / N;
        let resize_m = measure_calls() / u64::from(N);

        drop(root);
        println!(
            "{rows:>6} {nodes:>7} {cold:>12?} {cold_m:>9} {warm:>12?} {warm_m:>9} \
{resize:>12?} {resize_m:>9}"
        );
    }
}
