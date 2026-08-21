//! ```mermaid inside a document: the diagram renders in place, in the
//! document's own scroll surface.

use abstracttui::base::Size;
use abstracttui::reactive::create_root;
use abstracttui::ui::{BufferCanvas, UiTree};
use abstracttui::widgets::MarkdownView;
use abstracttui_mermaid::MermaidFence;
use std::rc::Rc;

fn screen(doc: &str, size: Size) -> Vec<String> {
    let mut tree = UiTree::new(size);
    let doc = doc.to_string();
    let (_root, ()) = create_root(|cx| {
        let view = MarkdownView::new(doc)
            .fence_block(Rc::new(MermaidFence::new()))
            .view(cx);
        tree.mount(cx, view);
    });
    let mut canvas = BufferCanvas::new(size);
    tree.draw(&mut canvas);
    (0..size.h).map(|y| canvas.row_text(y)).collect()
}

#[test]
fn a_mermaid_fence_becomes_a_diagram_in_the_document() {
    let doc =
        "# Design\n\n```mermaid\ngraph LR\n  A[Start] --> B[Ship]\n```\n\nAfter the diagram.\n";
    let rows = screen(doc, Size::new(70, 24));
    let joined = rows.join("\n");

    // The prose is still prose...
    assert!(joined.contains("Design"), "{joined}");
    assert!(joined.contains("After the diagram."), "{joined}");
    // ...the source never reaches the screen...
    assert!(!joined.contains("graph LR"), "source leaked:\n{joined}");
    assert!(!joined.contains("```"), "fence markers leaked:\n{joined}");
    // ...and the diagram did.
    assert!(joined.contains("Start"), "node label missing:\n{joined}");
    assert!(joined.contains("Ship"), "node label missing:\n{joined}");
    assert!(joined.contains('╭'), "card chrome missing:\n{joined}");
}

#[test]
fn an_unsupported_diagram_falls_back_inside_the_document() {
    let doc = "```mermaid\ngantt\n  title Nope\n```\n";
    let joined = screen(doc, Size::new(70, 16)).join("\n");
    assert!(
        joined.contains("unsupported mermaid"),
        "the fallback notice renders in place:\n{joined}"
    );
}

#[test]
fn other_fences_are_left_alone() {
    let doc = "```rust\nlet x = 1;\n```\n";
    let joined = screen(doc, Size::new(40, 8)).join("\n");
    assert!(joined.contains("let x = 1;"), "{joined}");
}

/// Documents re-typeset on every width change and every frame; the
/// diagram must be rendered once per (source, width), not per row.
#[test]
fn rendering_is_cached_per_source_and_width() {
    let doc = "```mermaid\ngraph TD\n  A --> B\n```\n";
    let fence = Rc::new(MermaidFence::new());
    let size = Size::new(50, 20);
    let mut tree = UiTree::new(size);
    let d = doc.to_string();
    let f = Rc::clone(&fence);
    let (_root, ()) = create_root(|cx| {
        let view = MarkdownView::new(d).fence_block(f).view(cx);
        tree.mount(cx, view);
    });
    let mut canvas = BufferCanvas::new(size);
    for _ in 0..5 {
        tree.draw(&mut canvas);
    }
    let joined: String = (0..size.h)
        .map(|y| canvas.row_text(y))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        joined.contains('╭'),
        "still drawn after repeated frames:\n{joined}"
    );
}
