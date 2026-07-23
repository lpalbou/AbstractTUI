//! Drawer unit tests (split from drawer.rs for the file budget;
//! `#[path]`-included as its `tests` module).

use super::*;
use std::cell::RefCell as StdRefCell;
use std::time::Instant;

use crate::reactive::{create_root, flush_effects, frame_tasks_pending, run_frame_tasks};
use crate::ui::{text, Key, KeyEvent, Mods, MouseButton, MouseEvent, MouseKind, UiEvent};

const MS: fn(u64) -> Duration = Duration::from_millis;

/// Drive frame tasks on synthetic time until every flight lands.
fn settle() {
    flush_effects();
    let mut now = Instant::now();
    for _ in 0..30 {
        if frame_tasks_pending() == 0 {
            break;
        }
        now += MS(50);
        run_frame_tasks(now);
        flush_effects();
    }
    assert_eq!(frame_tasks_pending(), 0, "animations settled");
}

fn key(k: Key) -> UiEvent {
    UiEvent::Key(KeyEvent::new(k, Mods::NONE))
}

fn click(x: i32, y: i32) -> UiEvent {
    UiEvent::Mouse(MouseEvent {
        kind: MouseKind::Down(MouseButton::Left),
        pos: Point::new(x, y),
        mods: Mods::NONE,
    })
}

/// Fresh overlay world + published viewport for a bare-rig test.
fn rig(size: Size) -> Overlays {
    super::super::viewport::publish_viewport(size);
    let overlays = Overlays::new();
    overlays.ensure_root(size);
    overlays
}

type Reasons = Rc<StdRefCell<Vec<DrawerCloseReason>>>;

fn reason_log() -> Reasons {
    Rc::new(StdRefCell::new(Vec::new()))
}

#[test]
fn open_close_toggle_lifecycle_fires_api_reason_once() {
    let size = Size::new(100, 40);
    let overlays = rig(size);
    let reasons = reason_log();
    let r = reasons.clone();
    let (root, handle) = create_root(|cx| {
        Drawer::new(DrawerEdge::Right)
            .size(DrawerSize::Cells(30))
            .motion(MS(20))
            .overlays(&overlays)
            .on_close(move |why| r.borrow_mut().push(why))
            .install(cx, |_| text("inspector"))
    });
    assert!(!handle.is_open(), "installed closed");
    assert!(handle.layer().is_none());

    handle.open();
    assert!(handle.is_open());
    let layer = handle.layer().expect("panel layer while open");
    assert_eq!(
        layer.bounds().unwrap().origin(),
        Point::new(100, 0),
        "starts parked off-screen"
    );
    settle();
    assert_eq!(
        layer.bounds().unwrap(),
        Rect::new(70, 0, 30, 40),
        "slid to rest"
    );

    handle.open(); // no-op while open
    assert_eq!(frame_tasks_pending(), 0, "redundant open schedules nothing");

    handle.toggle(); // close
    assert!(!handle.is_open(), "heading closed immediately");
    settle();
    assert!(
        handle.layer().is_none(),
        "layer removed after the slide out"
    );
    assert_eq!(*reasons.borrow(), vec![DrawerCloseReason::Api]);

    handle.close(); // no-op while closed
    assert_eq!(*reasons.borrow(), vec![DrawerCloseReason::Api]);

    // Reopen works (fresh mount).
    handle.toggle();
    settle();
    assert!(handle.is_open());
    root.dispose();
}

#[test]
fn instant_motion_zero_lands_in_one_frame_pass() {
    let size = Size::new(60, 20);
    let overlays = rig(size);
    let (root, handle) = create_root(|cx| {
        Drawer::new(DrawerEdge::Left)
            .size(DrawerSize::Cells(20))
            .motion(Duration::ZERO)
            .overlays(&overlays)
            .install(cx, |_| text("nav"))
    });
    handle.open();
    flush_effects();
    run_frame_tasks(Instant::now());
    flush_effects();
    assert_eq!(
        frame_tasks_pending(),
        0,
        "zero-duration flight lands at once"
    );
    assert_eq!(
        handle.layer().unwrap().bounds().unwrap().origin(),
        Point::new(0, 0)
    );
    root.dispose();
}

#[test]
fn bound_signal_drives_and_reflects_both_ways() {
    let size = Size::new(80, 24);
    let overlays = rig(size);
    let (root, (open, handle)) = create_root(|cx| {
        let open = cx.signal(false);
        let handle = Drawer::new(DrawerEdge::Right)
            .motion(MS(10))
            .overlays(&overlays)
            .bind(open)
            .install(cx, |_| text("bound"));
        (open, handle)
    });
    // External write opens.
    open.set(true);
    settle();
    assert!(handle.is_open(), "signal write opened the drawer");
    // Handle verb writes back.
    handle.close();
    assert!(!open.get_untracked(), "close reflected into the signal");
    settle();
    assert!(handle.layer().is_none());
    // External write again; internal Esc-class close syncs to false.
    open.set(true);
    settle();
    assert!(handle.is_open());
    handle.close_with(DrawerCloseReason::Escape);
    assert!(!open.get_untracked(), "internal close synced the signal");
    settle();
    root.dispose();
}

#[test]
fn modal_drawer_owns_keys_and_esc_closes_with_reason() {
    let size = Size::new(100, 40);
    let overlays = rig(size);
    let root_keys = Rc::new(StdRefCell::new(0u32));
    let rk = root_keys.clone();
    let reasons = reason_log();
    let r = reasons.clone();
    let (root, handle) = create_root(|cx| {
        Drawer::new(DrawerEdge::Right)
            .size(DrawerSize::Cells(30))
            .motion(MS(10))
            .title("Inspector")
            .overlays(&overlays)
            .on_close(move |why| r.borrow_mut().push(why))
            .install(cx, |_| text("modal page"))
    });
    handle.open();
    settle();
    // Keys are OWNED by the modal drawer (Some = never falls through
    // to the caller's world, consumed or not — the dispatch contract).
    let consumed = overlays.dispatch(&key(Key::Char('x')));
    assert!(consumed.is_some(), "modal drawer owns keys: {consumed:?}");
    let _ = rk; // (root tree not mounted here; ownership is the assert)
    assert_eq!(*root_keys.borrow(), 0);
    // Esc closes with the named reason.
    assert_eq!(overlays.dispatch(&key(Key::Escape)), Some(true));
    assert!(!handle.is_open(), "Esc began the close");
    settle();
    assert_eq!(*reasons.borrow(), vec![DrawerCloseReason::Escape]);
    root.dispose();
}

#[test]
fn passive_drawer_leaves_keys_with_the_app_until_click_in() {
    let size = Size::new(100, 40);
    let overlays = rig(size);
    let (root, handle) = create_root(|cx| {
        Drawer::new(DrawerEdge::Right)
            .size(DrawerSize::Cells(30))
            .focus(DrawerFocus::Passive)
            .motion(MS(10))
            .title("Glance")
            .overlays(&overlays)
            .install(cx, |_| text("passive page"))
    });
    handle.open();
    settle();
    // Unfocused passive overlay: keys fall through to the root.
    assert_eq!(
        overlays.dispatch(&key(Key::Char('x'))),
        None,
        "keys stay with the main surface"
    );
    // Click into the panel (rest rect starts at x=70) focuses it...
    assert!(overlays.dispatch(&click(75, 5)).is_some());
    // ...now the panel owns keys (the cycle-5 focused-overlay rule).
    assert!(overlays.dispatch(&key(Key::Char('x'))).is_some());
    // Esc while focused closes it.
    assert_eq!(overlays.dispatch(&key(Key::Escape)), Some(true));
    assert!(!handle.is_open());
    settle();
    root.dispose();
}

#[test]
fn scrim_rules_modal_only_and_configurable() {
    let size = Size::new(80, 24);
    let overlays = rig(size);
    let layer_count = || overlays.store().borrow().layers.len();
    assert_eq!(layer_count(), 1, "root only");
    let (root, (modal, passive, plain)) = create_root(|cx| {
        let modal = Drawer::new(DrawerEdge::Right)
            .motion(Duration::ZERO)
            .overlays(&overlays)
            .install(cx, |_| text("m"));
        let passive = Drawer::new(DrawerEdge::Left)
            .focus(DrawerFocus::Passive)
            .motion(Duration::ZERO)
            .overlays(&overlays)
            .install(cx, |_| text("p"));
        let plain = Drawer::new(DrawerEdge::Top)
            .scrim(false)
            .motion(Duration::ZERO)
            .overlays(&overlays)
            .install(cx, |_| text("t"));
        (modal, passive, plain)
    });
    modal.open();
    settle();
    assert_eq!(layer_count(), 3, "modal: scrim + panel");
    modal.close();
    settle();
    assert_eq!(layer_count(), 1, "closed: both layers gone");

    passive.open();
    settle();
    assert_eq!(layer_count(), 2, "passive: never a scrim");
    passive.close();
    settle();

    plain.open();
    settle();
    assert_eq!(layer_count(), 2, "modal with scrim(false): panel only");
    plain.close();
    settle();
    assert_eq!(layer_count(), 1);
    root.dispose();
}

#[test]
fn outside_press_close_is_configurable() {
    let size = Size::new(100, 40);
    let overlays = rig(size);
    let reasons = reason_log();
    let r = reasons.clone();
    let (root, (closes, stays)) = create_root(|cx| {
        let closes = Drawer::new(DrawerEdge::Right)
            .size(DrawerSize::Cells(30))
            .motion(MS(10))
            .overlays(&overlays)
            .on_close(move |why| r.borrow_mut().push(why))
            .install(cx, |_| text("dismissable"));
        let stays = Drawer::new(DrawerEdge::Left)
            .size(DrawerSize::Cells(20))
            .close_on_outside(false)
            .motion(MS(10))
            .overlays(&overlays)
            .install(cx, |_| text("sticky"));
        (closes, stays)
    });
    closes.open();
    settle();
    // Press outside the panel (the scrim region): swallowed AND closes.
    assert_eq!(overlays.dispatch(&click(5, 5)), Some(true));
    assert!(!closes.is_open());
    settle();
    assert_eq!(*reasons.borrow(), vec![DrawerCloseReason::OutsidePress]);

    stays.open();
    settle();
    assert_eq!(
        overlays.dispatch(&click(90, 5)),
        Some(true),
        "modal still swallows the outside press"
    );
    assert!(stays.is_open(), "close_on_outside(false) keeps it open");
    root.dispose();
}

#[test]
fn one_drawer_per_edge_replaces_the_incumbent_instantly() {
    let size = Size::new(100, 40);
    let overlays = rig(size);
    let reasons = reason_log();
    let r = reasons.clone();
    let (root, (a, b, other_edge)) = create_root(|cx| {
        let a = Drawer::new(DrawerEdge::Right)
            .size(DrawerSize::Cells(30))
            .motion(MS(10))
            .overlays(&overlays)
            .on_close(move |why| r.borrow_mut().push(why))
            .install(cx, |_| text("first"));
        let b = Drawer::new(DrawerEdge::Right)
            .size(DrawerSize::Cells(20))
            .motion(MS(10))
            .overlays(&overlays)
            .install(cx, |_| text("second"));
        let other_edge = Drawer::new(DrawerEdge::Left)
            .size(DrawerSize::Cells(10))
            .motion(MS(10))
            .overlays(&overlays)
            .install(cx, |_| text("left"));
        (a, b, other_edge)
    });
    a.open();
    settle();
    assert!(a.is_open());
    b.open(); // same edge: a finishes NOW, no slide-out
    assert!(!a.is_open(), "incumbent replaced");
    assert!(a.layer().is_none(), "incumbent's layer removed instantly");
    assert_eq!(*reasons.borrow(), vec![DrawerCloseReason::Replaced]);
    settle();
    assert!(b.is_open());

    other_edge.open(); // different edge: coexists
    settle();
    assert!(b.is_open() && other_edge.is_open(), "edges are independent");
    root.dispose();
}

#[test]
fn resize_reclamps_geometry_instead_of_dismissing() {
    let size = Size::new(100, 40);
    let overlays = rig(size);
    let reasons = reason_log();
    let r = reasons.clone();
    let (root, handle) = create_root(|cx| {
        Drawer::new(DrawerEdge::Right)
            .size(DrawerSize::Percent(0.5))
            .motion(MS(10))
            .overlays(&overlays)
            .on_close(move |why| r.borrow_mut().push(why))
            .install(cx, |_| text("resizes"))
    });
    handle.open();
    settle();
    assert_eq!(
        handle.layer().unwrap().bounds().unwrap(),
        Rect::new(50, 0, 50, 40)
    );

    super::super::viewport::publish_viewport(Size::new(80, 30));
    flush_effects(); // the re-clamp effect + the slide re-place
    assert!(handle.is_open(), "resize never dismisses a drawer");
    assert!(reasons.borrow().is_empty());
    assert_eq!(
        handle.layer().unwrap().bounds().unwrap(),
        Rect::new(40, 0, 40, 30),
        "geometry re-solved against the fresh viewport"
    );
    root.dispose();
}

#[test]
fn host_scope_death_closes_with_host_gone() {
    let size = Size::new(80, 24);
    let overlays = rig(size);
    let reasons = reason_log();
    let r = reasons.clone();
    let (root, (handle, host)) = create_root(|cx| {
        let host = cx.child();
        let handle = Drawer::new(DrawerEdge::Right)
            .motion(MS(10))
            .overlays(&overlays)
            .on_close(move |why| r.borrow_mut().push(why))
            .install(host, |_| text("doomed"));
        (handle, host)
    });
    handle.open();
    settle();
    assert!(handle.is_open());
    host.dispose(); // the opener's world unmounts mid-life
    assert!(!handle.is_open());
    assert!(handle.layer().is_none(), "layers torn down with the host");
    assert_eq!(*reasons.borrow(), vec![DrawerCloseReason::HostGone]);
    assert_eq!(
        overlays.store().borrow().layers.len(),
        1,
        "only the root layer remains"
    );
    root.dispose();
}

#[test]
fn host_death_mid_slide_is_quiet_and_complete() {
    // The animate disposal guard's integration face: kill the host
    // while the open flight is LIVE — no panic, layers gone, the next
    // frame pass drains to zero.
    let size = Size::new(80, 24);
    let overlays = rig(size);
    let (root, (handle, host)) = create_root(|cx| {
        let host = cx.child();
        let handle = Drawer::new(DrawerEdge::Bottom)
            .motion(MS(500))
            .overlays(&overlays)
            .install(host, |_| text("mid-flight"));
        (handle, host)
    });
    handle.open();
    flush_effects();
    run_frame_tasks(Instant::now()); // stamp the flight, still flying
    assert_eq!(frame_tasks_pending(), 1, "flight is live");
    host.dispose();
    assert!(handle.layer().is_none());
    let left = run_frame_tasks(Instant::now() + MS(50));
    assert_eq!(left, 0, "orphaned flight cancelled quietly");
    root.dispose();
}

#[test]
fn reopen_during_close_reverses_without_a_close_firing() {
    let size = Size::new(80, 24);
    let overlays = rig(size);
    let reasons = reason_log();
    let r = reasons.clone();
    let (root, handle) = create_root(|cx| {
        Drawer::new(DrawerEdge::Right)
            .motion(MS(200))
            .overlays(&overlays)
            .on_close(move |why| r.borrow_mut().push(why))
            .install(cx, |_| text("wobbles"))
    });
    handle.open();
    settle();
    handle.close();
    assert!(!handle.is_open());
    // Reverse before the slide lands: same mount continues.
    handle.open();
    assert!(handle.is_open());
    settle();
    assert!(handle.layer().is_some(), "still mounted");
    assert!(reasons.borrow().is_empty(), "the close never landed");
    root.dispose();
}

#[test]
fn state_outside_the_builder_survives_reopen() {
    // The Tabs rule made concrete: signals created in the installing
    // scope and captured by `build` carry across close/reopen; the
    // mount scope's own state dies with each close.
    let size = Size::new(80, 24);
    let overlays = rig(size);
    let seen = Rc::new(StdRefCell::new(Vec::<i32>::new()));
    let (root, (handle, counter)) = create_root(|cx| {
        let counter = cx.signal(0i32);
        let log = seen.clone();
        let handle = Drawer::new(DrawerEdge::Right)
            .motion(Duration::ZERO)
            .overlays(&overlays)
            .install(cx, move |_| {
                log.borrow_mut().push(counter.get_untracked());
                text("stateful")
            });
        (handle, counter)
    });
    handle.open();
    settle();
    counter.set(42); // written while open, lives in the INSTALL scope
    handle.close();
    settle();
    handle.open();
    settle();
    assert_eq!(*seen.borrow(), vec![0, 42], "state survived the close");
    root.dispose();
}

#[test]
fn geometry_solves_and_clamps_every_edge() {
    let vp = Size::new(100, 40);
    // Cells, each edge: cross axis fills, panel hugs its edge.
    assert_eq!(
        solve_rect(vp, DrawerEdge::Left, DrawerSize::Cells(30)),
        Rect::new(0, 0, 30, 40)
    );
    assert_eq!(
        solve_rect(vp, DrawerEdge::Right, DrawerSize::Cells(30)),
        Rect::new(70, 0, 30, 40)
    );
    assert_eq!(
        solve_rect(vp, DrawerEdge::Top, DrawerSize::Cells(10)),
        Rect::new(0, 0, 100, 10)
    );
    assert_eq!(
        solve_rect(vp, DrawerEdge::Bottom, DrawerSize::Cells(10)),
        Rect::new(0, 30, 100, 10)
    );
    // Percent rounds against the slide axis.
    assert_eq!(
        solve_rect(vp, DrawerEdge::Right, DrawerSize::Percent(0.25)),
        Rect::new(75, 0, 25, 40)
    );
    assert_eq!(
        solve_rect(vp, DrawerEdge::Bottom, DrawerSize::Percent(0.5)),
        Rect::new(0, 20, 100, 20)
    );
    // Oversize clamps to the viewport; undersize floors at one cell.
    assert_eq!(
        solve_rect(vp, DrawerEdge::Left, DrawerSize::Cells(500)),
        Rect::new(0, 0, 100, 40)
    );
    assert_eq!(
        solve_rect(vp, DrawerEdge::Left, DrawerSize::Cells(0)),
        Rect::new(0, 0, 1, 40)
    );
    assert_eq!(
        solve_rect(vp, DrawerEdge::Top, DrawerSize::Percent(9.0)),
        Rect::new(0, 0, 100, 40),
        "percent clamps to 1.0"
    );
}

// Wave-8 cross-review pins (TABS) — sibling file for the size budget.
#[path = "drawer_review_tests.rs"]
mod review;

#[test]
fn closed_origin_is_fully_off_screen_and_slide_interpolates() {
    let vp = Size::new(100, 40);
    let geo = solve_geometry(vp, DrawerEdge::Right, DrawerSize::Cells(30));
    assert_eq!(
        geo.closed,
        Point::new(100, 0),
        "right: parked past the edge"
    );
    assert_eq!(origin_at(geo, 0.0), geo.closed);
    assert_eq!(origin_at(geo, 1.0), geo.rect.origin());
    let mid = origin_at(geo, 0.5);
    assert_eq!(mid.y, 0);
    assert!(mid.x > geo.rect.x && mid.x < geo.closed.x, "{mid:?}");

    let left = solve_geometry(vp, DrawerEdge::Left, DrawerSize::Cells(20));
    assert_eq!(left.closed, Point::new(-20, 0));
    let top = solve_geometry(vp, DrawerEdge::Top, DrawerSize::Cells(8));
    assert_eq!(top.closed, Point::new(0, -8));
    let bottom = solve_geometry(vp, DrawerEdge::Bottom, DrawerSize::Cells(8));
    assert_eq!(bottom.closed, Point::new(0, 40));
}

#[test]
fn per_edge_z_slots_are_distinct_and_below_the_modal_band() {
    let edges = [
        DrawerEdge::Left,
        DrawerEdge::Right,
        DrawerEdge::Top,
        DrawerEdge::Bottom,
    ];
    let mut zs = Vec::new();
    for e in edges {
        assert_eq!(e.scrim_z(), e.panel_z() - 1, "scrim directly under panel");
        assert!(e.panel_z() >= DRAWER_Z && e.panel_z() < super::super::popups::MODAL_Z);
        zs.push(e.scrim_z());
        zs.push(e.panel_z());
    }
    let n = zs.len();
    zs.sort_unstable();
    zs.dedup();
    assert_eq!(zs.len(), n, "no two drawer layers share a z (equal-z trap)");
}
