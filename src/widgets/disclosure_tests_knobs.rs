//! Disclosure header-knob tests — child of `disclosure::tests` (split
//! sibling, app-kits/1250): `title_muted` tone precedence and the
//! LIVE `detail_signal` slot (updates without remounting the header —
//! focus survives writes; empty clears; the signal wins over the
//! static slot).

use super::*;

#[test]
fn title_muted_renders_the_title_in_muted_tone_until_hover_or_focus() {
    let t = default_theme().tokens;
    let size = Size::new(28, 4);
    let (root, mut tree) = mount_widget(size, |cx| {
        Element::new()
            .style(LayoutStyle::column())
            .child(
                Disclosure::text("quiet", "body")
                    .title_muted(true)
                    .element(cx, &t)
                    .build(),
            )
            .build()
    });
    let canvas = settle(&mut tree, size);
    // Title starts at x3 ([pad][glyph][gap]); muted base tone.
    assert_eq!(
        canvas.cell(Point::new(3, 0)).expect("title").1,
        t.text_muted,
        "muted base tone"
    );
    // Focus still wins (the state table is unchanged).
    key(&mut tree, Key::Tab);
    flush_effects();
    tree.layout();
    let canvas = render(&mut tree, size);
    assert_eq!(
        canvas.cell(Point::new(3, 0)).expect("title").1,
        t.selection_fg,
        "focus tone beats the muted base"
    );
    root.dispose();
}

#[test]
fn detail_signal_updates_live_without_dropping_header_focus() {
    let t = default_theme().tokens;
    let size = Size::new(30, 4);
    let holder: Rc<RefCell<Option<crate::reactive::Signal<String>>>> = Rc::default();
    let h = holder.clone();
    let (root, mut tree) = mount_widget(size, move |cx| {
        let detail = cx.signal(String::from("0 tk"));
        *h.borrow_mut() = Some(detail);
        Element::new()
            .style(LayoutStyle::column())
            .child(
                Disclosure::text("live", "body")
                    .detail("static loses") // the signal wins
                    .detail_signal(detail)
                    .element(cx, &t)
                    .build(),
            )
            .build()
    });
    let canvas = settle(&mut tree, size);
    let top = canvas.row_text(0);
    assert!(top.contains("0 tk"), "{top:?}");
    assert!(!top.contains("static loses"), "the signal wins: {top:?}");
    key(&mut tree, Key::Tab); // focus the header
    flush_effects();
    tree.layout();
    let detail = holder.borrow().expect("signal");
    detail.set("42 tk".into());
    let canvas = settle(&mut tree, size);
    let top = canvas.row_text(0);
    assert!(top.contains("42 tk"), "live update rendered: {top:?}");
    assert_eq!(
        canvas.cell(Point::new(3, 0)).expect("title").1,
        t.selection_fg,
        "the header element never remounted — focus survives writes"
    );
    // Empty clears the slot.
    detail.set(String::new());
    let canvas = settle(&mut tree, size);
    assert!(!canvas.row_text(0).contains("tk"), "empty = no detail");
    root.dispose();
}
