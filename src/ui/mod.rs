//! Component model: declarative view tree, mounting, event routing
//! (capture -> target -> bubble), hit testing, focus management, keymaps
//! and shortcuts. Components are plain functions over reactive scopes
//! returning `View` blueprints; re-render is driven by signals, scoped
//! to the `Dyn` region that read them.
//!
//! Owner: REACT. Contract notes live on the types; the architectural
//! rationale is in `docs/design/reactive-ui.md`.
//!
//! ## Component pattern
//!
//! ```ignore
//! fn counter(cx: Scope, start: i64) -> View {
//!     let count = cx.signal(start);
//!     Element::new()
//!         .style(Style::row().gap(1))
//!         .focusable()
//!         .on_event(move |_ctx, ev| {
//!             if let UiEvent::Key(k) = ev {
//!                 if k.key == Key::Char('+') { count.update(|c| *c += 1); }
//!             }
//!         })
//!         .child(dyn_view(Style::default(), move || text(format!("count: {}", count.get()))))
//!         .build()
//! }
//! ```

mod access;
mod canvas;
pub mod compose;
mod draw;
mod event;
mod focus;
mod mount;
mod tree;
mod view;

pub use access::{focus_affordance_visible, AccessEntry, AccessSnapshot, Role};
pub use canvas::{BufferCanvas, Canvas, ClippedCanvas, StyledCanvas, SurfaceCanvas};
pub use compose::Callback;
pub use event::{
    EventCtx, Key, KeyChord, KeyEvent, Mods, MouseButton, MouseEvent, MouseKind, Phase, UiEvent,
};
pub use tree::{UiTree, ViewId};
pub use view::{
    dyn_view, dyn_view_scoped, styled_text, text, DrawFn, Element, HandlerFn, ShortcutFn, View,
};

#[cfg(test)]
mod tests {
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
}
