//! Rendered-cell tests through the real UiTree: flowchart cards +
//! badges + notices, the sequence golden, and the ATOMIC fallback
//! (code fence + named notice + live link; no diagram chrome).

use abstracttui::base::{Point, Size};
use abstracttui::reactive::create_root;
use abstracttui::ui::{BufferCanvas, UiTree};
use abstracttui_mermaid::MermaidView;

struct Rig {
    _root: abstracttui::reactive::RootScope,
    tree: UiTree,
    size: Size,
}

impl Rig {
    fn mount(size: Size, build: impl FnOnce() -> MermaidView) -> Rig {
        let mut tree = UiTree::new(size);
        let (_root, ()) = create_root(|cx| {
            let view = build().view(cx);
            tree.mount(cx, view);
        });
        Rig { _root, tree, size }
    }

    fn rows(&mut self) -> Vec<String> {
        let mut canvas = BufferCanvas::new(self.size);
        self.tree.draw(&mut canvas);
        (0..self.size.h)
            .map(|y| canvas.row_text(y).trim_end().to_string())
            .collect()
    }

    fn count_char(&mut self, ch: char) -> usize {
        let mut canvas = BufferCanvas::new(self.size);
        self.tree.draw(&mut canvas);
        let mut n = 0;
        for y in 0..self.size.h {
            for x in 0..self.size.w {
                if canvas.cell(Point::new(x, y)).unwrap().0 == ch {
                    n += 1;
                }
            }
        }
        n
    }
}

#[test]
fn flowchart_renders_cards_badges_and_arrowheads() {
    let src = "graph TD\nA[Go] --> B{Ok?}";
    let mut rig = Rig::mount(Size::new(40, 16), || MermaidView::new(src));
    assert_eq!(rig.count_char('╭'), 2, "two cards");
    assert_eq!(rig.count_char('◆'), 1, "decision badge sigil");
    assert_eq!(rig.count_char('▼'), 1, "TD arrowhead");
    let rows = rig.rows();
    assert!(rows.iter().any(|r| r.contains("Go")), "{rows:?}");
    assert!(rows.iter().any(|r| r.contains("Ok?")));

    // Determinism: a second mount renders identical cells.
    let mut rig2 = Rig::mount(Size::new(40, 16), || MermaidView::new(src));
    assert_eq!(rig.rows(), rig2.rows());
}

#[test]
fn dropped_directives_render_a_notice_line() {
    let src = "%%{init: {\"theme\":\"dark\"}}%%\ngraph TD\nA --> B";
    let mut rig = Rig::mount(Size::new(48, 14), || MermaidView::new(src));
    let rows = rig.rows();
    assert!(rows[0].contains("init/theme directive ignored"), "{rows:?}");
    assert_eq!(rig.count_char('╭'), 2, "render proceeds under the notice");
}

/// Golden: the docs' Alice/John greeting, pinned rows (glyphs only —
/// inks are theme business).
#[test]
fn sequence_golden_alice_john() {
    let src = "sequenceDiagram\n    participant Alice\n    participant John\n    Alice->>John: Hello John, how are you?\n    John-->>Alice: Great!";
    let mut rig = Rig::mount(Size::new(46, 14), || MermaidView::new(src));
    let rows = rig.rows();
    let expected = sequence_golden_rows();
    assert!(!expected.is_empty(), "golden must be pinned, not vacuous");
    for (i, want) in expected.iter().enumerate() {
        assert_eq!(rows[i], *want, "row {i}");
    }
}

fn sequence_golden_rows() -> Vec<&'static str> {
    vec![
        "╭───────╮                   ╭──────╮",
        "│ Alice │                   │ John │",
        "╰───────╯                   ╰──────╯",
        "    │                           │",
        "    │ Hello John, how are you?  │",
        "    │──────────────────────────▶│",
        "    │                           │",
        "    │          Great!           │",
        "    │◀╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌│",
        "    │                           │",
        "    │                           │",
        "",
    ]
}

#[test]
fn fallback_is_atomic_fence_notice_and_link() {
    // Valid until line 4 — the WHOLE diagram must fence.
    let src = "graph TD\nA --> B\nB --> C\nsubgraph nope\nC --> D\nend";
    let mut rig = Rig::mount(Size::new(72, 14), || MermaidView::new(src));
    let rows = rig.rows();
    assert!(
        rows[0].contains("unsupported mermaid at line 4"),
        "{}",
        rows[0]
    );
    assert!(rows[0].contains("subgraph"));
    assert!(
        rows[1].contains("view online: https://mermaid.live/edit#base64:"),
        "{}",
        rows[1]
    );
    // The fence: every source line verbatim, in order, under the
    // notice rows.
    for (i, line) in src.lines().enumerate() {
        assert!(
            rows[2 + i].starts_with(line),
            "fence row {i}: {:?} vs {line:?}",
            rows[2 + i]
        );
    }
    // No diagram chrome leaked: no cards, no arrowheads.
    assert_eq!(rig.count_char('╭'), 0);
    assert_eq!(rig.count_char('▼'), 0);
    assert_eq!(rig.count_char('▶'), 0);
}

#[test]
fn live_link_opt_out_removes_the_link_row() {
    let src = "gantt\ntitle X";
    let mut rig = Rig::mount(Size::new(60, 8), || MermaidView::new(src).live_link(false));
    let rows = rig.rows();
    assert!(rows[0].contains("gantt"));
    assert!(!rows.iter().any(|r| r.contains("mermaid.live")));
    assert!(rows[1].starts_with("gantt"), "fence directly under notice");
}
