//! SelectHandle (backlog 0296) unit tests — child of `select::tests`
//! (split sibling, the select_tests_faces.rs pattern; the shared Rig
//! lives in the parent module). The programmatic-open contract:
//! anchor = the trigger's last-painted rect, one-frame-after-mount
//! refusal, disposal safety through dyn_view regeneration.
use super::*;
use crate::ui::dyn_view_scoped;

/// Paint the root tree once: the faces record their trigger rect at
/// draw time — the anchor programmatic opens use.
fn draw_root(rig: &mut Rig) {
    rig.tree.layout();
    let mut canvas = BufferCanvas::new(VP);
    rig.tree.draw(&mut canvas);
}

#[test]
fn handle_refuses_before_first_paint_then_opens_at_the_trigger() {
    let h = SelectHandle::new();
    let hh = h.clone();
    let mut rig = rig(move |cx, ov| {
        Select::new(fruit_options())
            .handle(&hh)
            .layout(face_layout())
            .overlays(ov)
            .element(cx, &default_theme().tokens)
            .build()
    });
    // The rig laid out but never DREW: no painted rect, no honest
    // anchor — the documented one-frame-after-mount caveat.
    assert!(!h.open(), "unpainted face must refuse a programmatic open");
    assert!(rig.popup().is_none());

    draw_root(&mut rig);
    assert!(h.open(), "painted face opens");
    let (bounds, rows) = rig.popup().expect("popup open");
    assert_eq!(bounds.y, 1, "anchored below the trigger row");
    assert_eq!(bounds.w, 24, "MatchAnchor width from the painted rect");
    assert!(rows[0].contains("alpha"), "{rows:?}");
    // Already open: open() reports the achieved state, opens nothing new.
    assert!(h.open(), "already-open counts as open");

    // The popup behaves exactly like a gesture-opened one.
    rig.key(Key::Down);
    rig.key(Key::Escape);
    assert!(rig.popup().is_none(), "Escape dismisses");
    assert!(h.open(), "reopens after dismissal");
    assert!(rig.popup().is_some());
}

#[test]
fn handle_open_commit_flows_like_a_gesture_open() {
    let value_holder: Rc<RefCell<Option<Signal<usize>>>> = Default::default();
    let vh = value_holder.clone();
    let h = SelectHandle::new();
    let hh = h.clone();
    let mut rig = rig(move |cx, ov| {
        let value = cx.signal(0usize);
        *vh.borrow_mut() = Some(value);
        Select::new(fruit_options())
            .value(value)
            .handle(&hh)
            .layout(face_layout())
            .overlays(ov)
            .element(cx, &default_theme().tokens)
            .build()
    });
    let value = value_holder.borrow().unwrap();
    draw_root(&mut rig);
    assert!(h.open());
    rig.key(Key::Down);
    rig.key(Key::Enter);
    assert!(rig.popup().is_none(), "commit closed");
    assert_eq!(value.get_untracked(), 1, "beta committed");
}

#[test]
fn handle_refuses_disabled_and_empty_faces() {
    let h = SelectHandle::new();
    let hh = h.clone();
    let mut rig = rig(move |cx, ov| {
        Select::new(fruit_options())
            .disabled(true)
            .handle(&hh)
            .layout(face_layout())
            .overlays(ov)
            .element(cx, &default_theme().tokens)
            .build()
    });
    draw_root(&mut rig);
    assert!(!h.open(), "disabled face refuses programmatic opens too");
    assert!(rig.popup().is_none());

    let h2 = SelectHandle::new();
    let hh2 = h2.clone();
    let mut empty_rig = self::rig(move |cx, ov| {
        Select::new(Vec::new())
            .handle(&hh2)
            .layout(face_layout())
            .overlays(ov)
            .element(cx, &default_theme().tokens)
            .build()
    });
    draw_root(&mut empty_rig);
    assert!(!h2.open(), "no options = nothing to open");
    assert!(empty_rig.popup().is_none());
}

#[test]
fn handle_dies_with_the_face_and_rewires_on_regeneration() {
    let h = SelectHandle::new();
    let hh = h.clone();
    let show: Rc<RefCell<Option<Signal<bool>>>> = Default::default();
    let sh = show.clone();
    let mut rig = rig(move |cx, ov| {
        let flag = cx.signal(true);
        *sh.borrow_mut() = Some(flag);
        let ov = ov.clone();
        let hh = hh.clone();
        // The regeneration scope is the wire's owner: each rebuild
        // wires the handle afresh under the NEW scope; the old scope's
        // cleanup runs on regeneration/unmount.
        dyn_view_scoped(face_layout(), move |dcx| {
            if flag.get() {
                Select::new(fruit_options())
                    .handle(&hh)
                    .layout(face_layout())
                    .overlays(&ov)
                    .element(dcx, &default_theme().tokens)
                    .build()
            } else {
                text("gone")
            }
        })
    });
    let flag = show.borrow().unwrap();
    draw_root(&mut rig);
    assert!(h.open(), "wired face opens");
    rig.key(Key::Escape);

    // Unmount the face: the wire dies with its scope (disposal safety).
    flag.set(false);
    crate::reactive::flush_effects();
    draw_root(&mut rig);
    assert!(!h.open(), "unmounted face refuses, never panics");
    assert!(rig.popup().is_none());

    // Regeneration wires the NEW face; the stale cleanup must not sever
    // it (the generation guard).
    flag.set(true);
    crate::reactive::flush_effects();
    draw_root(&mut rig);
    assert!(h.open(), "regenerated face rewired the handle");
    assert!(rig.popup().is_some());
    rig.key(Key::Escape);
}

#[test]
fn handle_works_on_combobox_and_multiselect() {
    // Combobox: the popup INCLUDES the anchor row (editor mounts over
    // the trigger) — programmatic opens keep that geometry.
    let h = SelectHandle::new();
    let hh = h.clone();
    let mut rig = rig(move |cx, ov| {
        Combobox::new(fruit_options())
            .handle(&hh)
            .layout(face_layout())
            .overlays(ov)
            .element(cx, &default_theme().tokens)
            .build()
    });
    draw_root(&mut rig);
    assert!(h.open());
    let (bounds, _) = rig.popup().expect("combobox popup");
    assert_eq!(bounds.y, 0, "anchor row included: starts AT the trigger");
    rig.key(Key::Escape);

    let h2 = SelectHandle::new();
    let hh2 = h2.clone();
    let mut rig2 = self::rig(move |cx, ov| {
        MultiSelect::new(fruit_options())
            .handle(&hh2)
            .layout(face_layout())
            .overlays(ov)
            .element(cx, &default_theme().tokens)
            .build()
    });
    draw_root(&mut rig2);
    assert!(h2.open());
    assert!(rig2.popup().is_some());
    rig2.key(Key::Escape);
}
