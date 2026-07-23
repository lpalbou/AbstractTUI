//! Scroll/panel COMPOSITION tests for the content views (wave 13, the
//! operator's "it doesn't even scroll" complaint): `MarkdownView` and
//! `CodeView` must compose with `Scroll` out of the box (intrinsic
//! measure — the wave-12 "Scroll over measureless content collapses"
//! class, fixed in the ENGINE this time), must size content-height in
//! Auto parents (a table in a content-sized panel must never vanish),
//! and must keep the leftover-not-content default in definite flex
//! parents (the 0240 modal-overflow class must not return).

use crate::base::Size;
use crate::layout::{Dimension, Style as LayoutStyle};
use crate::reactive::{flush_effects, run_due_timers};
use crate::theme::default_theme;
use crate::ui::{BufferCanvas, Element, Key, MouseKind, UiTree};
use crate::widgets::itest_util::{key, mount_widget, mouse, render};
use crate::widgets::{CodeView, MarkdownView, Scroll};

/// Settle the deferred geometry loop (draw -> probes -> timers ->
/// effects -> re-layout until quiet). Local copy of the scroll-test
/// helper: that one is `pub(super)` to `widgets::scroll`, and test
/// scaffolding stays lane-local by the ownership rules.
fn settle(tree: &mut UiTree, size: Size) -> BufferCanvas {
    flush_effects();
    tree.layout();
    let mut canvas = render(tree, size);
    for _ in 0..4 {
        let fired = run_due_timers(std::time::Instant::now());
        flush_effects();
        tree.layout();
        canvas = render(tree, size);
        if fired == 0 && !tree.has_pending_work() {
            break;
        }
    }
    canvas
}

/// A markdown doc long enough to overflow a 6-row viewport at width 24:
/// numbered paragraphs so any visible row names its position.
fn long_doc() -> String {
    let mut doc = String::from("# Title\n");
    for i in 0..20 {
        doc.push_str(&format!("\npara {i}\n"));
    }
    doc
}

#[test]
fn scroll_of_markdown_view_scrolls_out_of_the_box() {
    // THE complaint repro: no content_size hint, no app-managed
    // scroll_offset — just Scroll::new(MarkdownView…). The intrinsic
    // measure must feed the scroll extent so End reaches the bottom.
    let t = &default_theme().tokens;
    let size = Size::new(24, 6);
    let doc = long_doc();
    let (_root, mut tree) = mount_widget(size, |cx| {
        Scroll::new(
            MarkdownView::new(doc)
                .element(&default_theme().tokens)
                .build(),
        )
        .element(cx, t)
        .build()
    });
    let canvas = settle(&mut tree, size);
    assert!(
        canvas.row_text(0).starts_with("Title"),
        "top of the doc first: {:?}",
        canvas.row_text(0)
    );
    key(&mut tree, Key::Tab);
    key(&mut tree, Key::End);
    let canvas = settle(&mut tree, size);
    let bottom: Vec<String> = (0..6).map(|y| canvas.row_text(y)).collect();
    assert!(
        bottom.iter().any(|r| r.starts_with("para 19")),
        "End must reach the measured bottom: {bottom:?}"
    );
    // Wheel back up moves the pane (the offset really is live).
    mouse(&mut tree, MouseKind::ScrollUp, 2, 2);
    let canvas = settle(&mut tree, size);
    assert!(
        !canvas.row_text(5).starts_with("para 19") || canvas.row_text(0).contains("para"),
        "wheel moves the viewport"
    );
}

#[test]
fn scroll_of_code_view_scrolls_out_of_the_box() {
    let t = &default_theme().tokens;
    let size = Size::new(24, 4);
    let src: String = (0..30)
        .map(|i| format!("line_{i}();\n"))
        .collect::<String>();
    let (_root, mut tree) = mount_widget(size, |cx| {
        Scroll::new(
            CodeView::new(src)
                .line_numbers(false)
                .element(&default_theme().tokens)
                .build(),
        )
        .element(cx, t)
        .build()
    });
    let canvas = settle(&mut tree, size);
    assert!(canvas.row_text(0).starts_with("line_0"));
    key(&mut tree, Key::Tab);
    key(&mut tree, Key::End);
    let canvas = settle(&mut tree, size);
    let rows: Vec<String> = (0..4).map(|y| canvas.row_text(y)).collect();
    assert!(
        rows.iter().any(|r| r.starts_with("line_29")),
        "End reaches the last source line: {rows:?}"
    );
}

#[test]
fn markdown_in_an_auto_height_panel_sizes_to_content_not_zero() {
    // The "tables vanish in panels" repro: a content-sized column (the
    // Modal/popup shape) around a MarkdownView with a table. Before the
    // measure seam the view contributed ZERO height and rendered
    // nothing at all.
    let size = Size::new(30, 10);
    let doc = "| Name | N |\n|:-----|--:|\n| alpha | 1 |\n| beta | 22 |";
    let (_root, mut tree) = mount_widget(size, |cx| {
        let _ = cx;
        Element::new()
            .style(LayoutStyle::column().width(Dimension::Cells(30)))
            .child(
                MarkdownView::new(doc)
                    .layout(LayoutStyle::default())
                    .element(&default_theme().tokens)
                    .build(),
            )
            .build()
    });
    let canvas = settle(&mut tree, size);
    let rows: Vec<String> = (0..10).map(|y| canvas.row_text(y)).collect();
    assert!(
        rows.iter().any(|r| r.starts_with("Name")),
        "table header renders in a content-sized panel: {rows:?}"
    );
    assert!(
        rows.iter().any(|r| r.starts_with("beta")),
        "table body renders in a content-sized panel: {rows:?}"
    );
}

#[test]
fn default_flex_geometry_keeps_fixed_siblings_alive() {
    // Guard for the 0240 modal-overflow class: the DEFAULT MarkdownView
    // layout in a definite column must take LEFTOVER space (basis 0),
    // never push a fixed sibling row out with its content height.
    let t = &default_theme().tokens;
    let size = Size::new(24, 6);
    let doc = long_doc();
    let (_root, mut tree) = mount_widget(size, |cx| {
        let _ = cx;
        Element::new()
            .style(
                LayoutStyle::column()
                    .width(Dimension::Percent(1.0))
                    .height(Dimension::Cells(6)),
            )
            .child(
                MarkdownView::new(doc)
                    .element(&default_theme().tokens)
                    .build(),
            )
            .child(
                Element::new()
                    .style(LayoutStyle::default().height(Dimension::Cells(1)))
                    .child(crate::ui::text("FOOTER"))
                    .build(),
            )
            .build()
    });
    let canvas = settle(&mut tree, size);
    let rows: Vec<String> = (0..6).map(|y| canvas.row_text(y)).collect();
    assert!(
        rows.iter().any(|r| r.starts_with("FOOTER")),
        "fixed sibling must survive beside a long markdown view: {rows:?}"
    );
    let _ = t;
}
