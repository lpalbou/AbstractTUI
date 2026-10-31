use crate::widgets::itest_util::{mouse, settle};
use crate::widgets::{ContentKind, ContentSelection, Feed, FeedItem, FeedState};
use crate::{
    base::{Point, Size},
    layout::{Dimension, Style as LayoutStyle},
    reactive::{create_root, flush_effects},
    ui::{Element, Mods, MouseButton, MouseEvent, MouseKind, UiEvent, UiTree},
};

fn fixture(
    width: i32,
    source: &str,
) -> (
    crate::reactive::RootScope,
    UiTree,
    FeedState,
    ContentSelection,
) {
    let mut tree = UiTree::new(Size::new(width, 25));
    let (root, (feed, selected)) = create_root(|cx| {
        let feed = FeedState::new(cx);
        let selection = ContentSelection::new(cx);
        feed.push("message", FeedItem::markdown(source));
        tree.mount(
            cx,
            Element::new()
                .style(
                    LayoutStyle::column()
                        .width(Dimension::Percent(1.0))
                        .height(Dimension::Percent(1.0)),
                )
                .child(
                    Feed::new(&feed)
                        .gap(0)
                        .content_selection(selection.clone())
                        .view(cx),
                )
                .build(),
        );
        (feed, selection)
    });
    settle(&mut tree, Size::new(width, 25));
    (root, tree, feed, selected)
}
#[test]
fn provenance_survives_emphasis_unicode_soft_wrap_and_sentence_expansion() {
    let source = "**bold** café 👩‍💻 世界. Another sentence wraps across several rows.";
    let (root, mut tree, feed, selected) = fixture(18, source);
    let hit = feed.content_at(1, 0).unwrap();
    assert_eq!(&source[hit.bytes.clone()], "o");
    assert_eq!(hit.select(ContentKind::Word).text, "bold");
    selected.choose(&hit, ContentKind::Sentence);
    assert_eq!(
        selected.value().get_untracked().unwrap().text,
        "bold café 👩‍💻 世界."
    );
    selected.expand(ContentKind::Paragraph);
    assert_eq!(
        selected.value().get_untracked().unwrap().text,
        "bold café 👩‍💻 世界. Another sentence wraps across several rows."
    );
    settle(&mut tree, Size::new(18, 25));
    root.dispose();
}
#[test]
fn table_cells_use_source_boundaries_in_grid_and_record_layout() {
    let source = "| Name | Value |\n|---|---|\n| **café** | a\\|b |\n| 世界 | z |";
    for width in [30, 5] {
        let (root, _tree, feed, _selected) = fixture(width, source);
        let mut values = Vec::new();
        for y in 0..feed.total_rows().get_untracked() {
            for x in 0..width {
                if let Some(hit) = feed.content_at(x, y) {
                    if hit.kind == ContentKind::TableCell {
                        let selected = hit.select(ContentKind::TableCell);
                        if selected.text == "a|b" {
                            assert!(
                                matches!(selected.context.as_deref(), Some(crate::widgets::ContentContext::TableCell {row: Some(0), column: 1, header}) if header.as_ref() == "Value")
                            );
                        }
                        values.push(selected.text);
                    }
                }
            }
        }
        for value in ["café", "a|b", "世界", "z"] {
            assert!(
                values.iter().any(|v| v == value),
                "{width}: {value} missing from {values:?}"
            );
        }
        root.dispose();
    }
}
#[test]
fn code_block_hit_preserves_exact_code_without_fence_or_display_padding() {
    let source = "```rust\nlet café = 1;\nprintln!(\"世界\");\n```";
    let (root, _tree, feed, _) = fixture(30, source);
    assert!(feed.content_at(0, 0).is_none(), "code chrome is not source");
    let hit = feed.content_at(3, 0).unwrap();
    assert_eq!(
        hit.select(ContentKind::CodeBlock).text,
        "let café = 1;\nprintln!(\"世界\");"
    );
    assert!(
        matches!(hit.context.as_deref(), Some(crate::widgets::ContentContext::Code {language, lines}) if language.as_ref() == "rust" && lines == &(2..4))
    );
    assert_eq!(&source[hit.scope], "let café = 1;\nprintln!(\"世界\");");
    root.dispose();
}
#[test]
fn drag_retains_selection_and_source_update_clears_it() {
    let (root, mut tree, feed, selection) = fixture(12, "alpha beta gamma delta");
    mouse(&mut tree, MouseKind::Down(MouseButton::Left), 1, 0);
    mouse(&mut tree, MouseKind::Drag(MouseButton::Left), 4, 1);
    mouse(&mut tree, MouseKind::Up(MouseButton::Left), 4, 1);
    let selected = selection
        .value()
        .get_untracked()
        .expect("release retains logical selection");
    assert_eq!(selected.text, "lpha beta gamma");
    assert_eq!(selected.kind, ContentKind::Span);
    feed.update("message", FeedItem::markdown("replacement"));
    flush_effects();
    assert!(selection.value().get_untracked().is_none());
    root.dispose();
}
#[test]
fn hyperlink_and_void_clicks_do_not_claim_source_selection() {
    let (root, mut tree, feed, selection) = fixture(20, "[link](https://example.com) body");
    assert!(feed.content_at(1, 0).is_none());
    tree.dispatch(&UiEvent::Mouse(MouseEvent {
        pos: Point::new(19, 9),
        kind: MouseKind::Down(MouseButton::Left),
        mods: Mods::NONE,
    }));
    assert!(selection.value().get_untracked().is_none());
    root.dispose();
}

#[test]
fn malformed_public_hit_mutation_is_safe_and_split_grapheme_columns_share_source() {
    let (root, _tree, feed, _) = fixture(30, "世界 **bo**ld");
    let a = feed.content_at(0, 0).unwrap();
    let b = feed.content_at(1, 0).unwrap();
    assert_eq!(
        a.bytes, b.bytes,
        "both cells of a wide grapheme map to one source cluster"
    );
    let hit = feed.content_at(6, 0).unwrap();
    assert_eq!(hit.select(ContentKind::Word).text, "bold");
    let mut invalid = hit;
    invalid.scope = 0..usize::MAX;
    assert!(invalid.select(ContentKind::Word).text.is_empty());
    invalid.scope = 0..1;
    invalid.bytes = 0..1;
    assert!(
        invalid.select(ContentKind::Word).text.is_empty(),
        "UTF8-split range refused again after mutation"
    );
    root.dispose();
}
#[test]
fn centered_surrounding_context_keeps_a_late_selection_and_labels_both_omissions() {
    let source = format!(
        "{} selectedword {}",
        "before ".repeat(800),
        "after ".repeat(800)
    );
    let start = source.find("selectedword").unwrap();
    let hit = crate::widgets::ContentHit::from_source(
        "item",
        0,
        source.as_str(),
        start..start + 12,
        0..source.len(),
        ContentKind::Span,
    )
    .unwrap();
    let selected = hit.select(ContentKind::Word);
    let (nearby, before, after) = selected.surrounding(120);
    assert!(nearby.contains("selectedword"));
    assert!(before && after);
    assert!(nearby.chars().count() <= 120);
}

#[test]
fn changing_units_roundtrips_to_the_original_click_and_drag_retains_its_unit() {
    let source = "first second third. Next sentence.";
    let (root, mut tree, feed, selected) = fixture(30, source);
    let hit = feed.content_at(8, 0).unwrap();
    selected.choose(&hit, ContentKind::Word);
    assert_eq!(selected.value().get_untracked().unwrap().text, "second");
    selected.expand(ContentKind::Sentence);
    selected.expand(ContentKind::Word);
    assert_eq!(selected.value().get_untracked().unwrap().text, "second");
    mouse(&mut tree, MouseKind::Down(MouseButton::Left), 7, 0);
    mouse(&mut tree, MouseKind::Drag(MouseButton::Left), 9, 0);
    mouse(&mut tree, MouseKind::Up(MouseButton::Left), 9, 0);
    selected.expand(ContentKind::Paragraph);
    assert_eq!(selected.value().get_untracked().unwrap().text, source);
    root.dispose();
}

#[test]
fn malformed_custom_resolver_drag_refuses_ranges_and_releases_capture() {
    for bad in [1..2, 0..usize::MAX] {
        let mut tree = UiTree::new(Size::new(12, 2));
        let (root, selected) = create_root(|cx| {
            let selected = ContentSelection::new(cx);
            let resolver_bad = bad.clone();
            let element = Element::new()
                .style(
                    LayoutStyle::default()
                        .width(Dimension::Cells(12))
                        .height(Dimension::Cells(2)),
                )
                .draw(|canvas, rect| {
                    canvas.print_styled(rect.origin(), "é rest", &crate::render::Style::EMPTY);
                });
            let view = selected.bind(element, move |point| {
                let mut hit = crate::widgets::ContentHit::from_source(
                    "custom",
                    0,
                    "é rest",
                    0..2,
                    0..7,
                    ContentKind::Span,
                )
                .unwrap();
                if point.x > 0 {
                    hit.bytes = resolver_bad.clone();
                }
                Some(hit)
            });
            tree.mount(cx, view.build());
            selected
        });
        settle(&mut tree, Size::new(12, 2));
        mouse(&mut tree, MouseKind::Down(MouseButton::Left), 0, 0);
        assert!(tree.pointer_capture().is_some());
        mouse(&mut tree, MouseKind::Drag(MouseButton::Left), 3, 0);
        assert!(selected.value().get_untracked().is_none());
        assert!(tree.pointer_capture().is_none());
        root.dispose();
    }
}
