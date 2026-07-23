//! Cycle-3 attack pins on the mermaid crate's own open list (the six
//! items from the 0450 wave report), executed from the CANVAS seat.
//! Dispositions recorded in reviews/wave9/canvas-final-attack.md.

use abstracttui::base::{Point, Size};
use abstracttui::reactive::create_root;
use abstracttui::ui::{BufferCanvas, UiTree};
use abstracttui_mermaid::{parse, Diagram, MermaidView};

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

const HEADS: [char; 4] = ['▲', '▼', '◀', '▶'];

// ---------------------------------------------------------------------------
// (1) `---` open links: the arrowless vocabulary, end to end.
// ---------------------------------------------------------------------------

#[test]
fn open_links_render_arrowless_end_to_end() {
    let mut rig = Rig::mount(Size::new(30, 14), || MermaidView::new("graph TD\nA --- B"));
    let heads: usize = HEADS.iter().map(|&h| rig.count_char(h)).sum();
    assert_eq!(heads, 0, "`---` draws no arrowhead");
    assert_eq!(rig.count_char('╭'), 2, "both cards render");

    // And a mixed diagram keeps exactly the arrows it asked for.
    let mut mixed = Rig::mount(Size::new(30, 20), || {
        MermaidView::new("graph TD\nA --> B\nB --- C")
    });
    let heads: usize = HEADS.iter().map(|&h| mixed.count_char(h)).sum();
    assert_eq!(heads, 1, "one arrowed edge, one open edge");
}

// ---------------------------------------------------------------------------
// (2) Long labels between DISTANT columns: adjacent-pair sizing holds,
// the long-span label truncates LABELED (ellipsis) and centered.
// ---------------------------------------------------------------------------

#[test]
fn distant_pair_labels_truncate_with_a_visible_ellipsis() {
    let src = "sequenceDiagram\n    a->>b: hi\n    b->>c: ok\n    a->>c: a very long label that cannot possibly fit between the columns";
    let mut rig = Rig::mount(Size::new(48, 18), || MermaidView::new(src));
    let rows = rig.rows();
    let label_row = rows
        .iter()
        .find(|r| r.contains("a very long"))
        .unwrap_or_else(|| panic!("long-label prefix visible: {rows:?}"));
    assert!(
        label_row.contains('…'),
        "the truncation is LABELED (ellipsis): {label_row:?}"
    );
    // The gaps were sized by the adjacent pairs (short labels), so the
    // whole picture stays compact: the long span takes what the
    // columns give (documented design) instead of stretching the plan
    // — observed 22 cells of boxes on a 48-cell canvas.
    assert!(
        rows[0].chars().count() <= 24,
        "adjacent-pair sizing holds; the long label did not stretch the plan: {:?}",
        rows[0]
    );
}

// ---------------------------------------------------------------------------
// (3) Lowercase `note` is outside the accepted spelling — and the
// fallback NAMES the line (no silent surprise).
// ---------------------------------------------------------------------------

#[test]
fn lowercase_note_falls_back_naming_the_exact_line() {
    let src = "sequenceDiagram\n    a->>b: hi\n    note over a: docs spell it Note";
    let err = match parse(src) {
        Err(e) => e,
        Ok(_) => panic!("lowercase note must not parse (docs capitalization)"),
    };
    assert_eq!(err.line_no, 3, "the verdict names the offending line");
    assert!(err.line.contains("note over a"), "verbatim line carried");
    assert_eq!(err.reason, "unrecognized statement");

    // Through the view: the notice row spells out line + content.
    let mut rig = Rig::mount(Size::new(72, 10), || MermaidView::new(src));
    let rows = rig.rows();
    assert!(
        rows[0].contains("line 3") && rows[0].contains("note over a"),
        "the on-screen notice names the line: {:?}",
        rows[0]
    );
}

// ---------------------------------------------------------------------------
// (4) Edge chaining stays a table decision (NOT widened) — and the
// fallback reason is the TARGETED one, visible on screen.
// ---------------------------------------------------------------------------

#[test]
fn edge_chaining_falls_back_with_the_targeted_reason() {
    let src = "graph TD\nA --> B --> C";
    let err = parse(src).expect_err("chaining is a v2 row");
    assert!(
        err.reason.contains("edge chaining"),
        "targeted, not generic: {}",
        err.reason
    );
    assert!(err.reason.contains("one edge per statement"));
    let mut rig = Rig::mount(Size::new(72, 8), || MermaidView::new(src));
    let rows = rig.rows();
    assert!(
        rows[0].contains("edge chaining"),
        "the user sees the targeted reason: {:?}",
        rows[0]
    );
    assert_eq!(rig.count_char('╭'), 0, "atomic: no partial diagram");
}

// ---------------------------------------------------------------------------
// (5) Sequence arrows THROUGH intermediate lifelines at minimum gaps:
// legibility golden — the crossing replaces the lifeline cell with the
// message line; the lifeline resumes above and below.
// ---------------------------------------------------------------------------

#[test]
fn arrows_cross_intermediate_lifelines_legibly_golden() {
    // One-letter participants = minimum boxes (6 wide) at the minimum
    // 2-cell gap; a->>c crosses b's lifeline.
    let src =
        "sequenceDiagram\n    participant a\n    participant b\n    participant c\n    a->>c: go";
    let mut rig = Rig::mount(Size::new(30, 12), || MermaidView::new(src));
    let rows = rig.rows();
    // The golden, minted from the shipped painter and reviewed: the
    // arrow row REPLACES b's lifeline cell with the message line (the
    // documented z-order — messages over lifelines), and the lifeline
    // resumes on the rows above and below. VERDICT: legible at the
    // minimum 2-cell gap.
    assert_eq!(rows[0], "╭────╮  ╭────╮  ╭────╮");
    assert_eq!(rows[1], "│ a  │  │ b  │  │ c  │");
    assert_eq!(rows[3], "   │       │       │");
    assert_eq!(rows[4], "   │      go       │");
    assert_eq!(
        rows[5], "   │──────────────▶│",
        "the crossing shows the message line, not b's lifeline"
    );
    assert_eq!(rows[6], "   │       │       │");
    // Explicit cell pins at b's lifeline column (11).
    let cell = |r: &str, x: usize| r.chars().nth(x);
    assert_eq!(cell(&rows[3], 11), Some('│'));
    assert_eq!(
        cell(&rows[5], 11),
        Some('─'),
        "crossed cell carries the line"
    );
    assert_eq!(cell(&rows[6], 11), Some('│'));
}

// ---------------------------------------------------------------------------
// (6) First-explicit-wins, pinned for BOTH diagram kinds.
// ---------------------------------------------------------------------------

#[test]
fn flowchart_first_explicit_declaration_wins_and_bare_mentions_never_reset() {
    let src = "graph TD\nA --> B\nB[Named]\nB(Other)\nB --> C";
    let Ok(Diagram::Flowchart(fc)) = parse(src) else {
        panic!("supported flowchart");
    };
    let b = fc.nodes.iter().find(|n| n.id == "B").expect("B registered");
    assert_eq!(
        b.text.as_deref(),
        Some("Named"),
        "the FIRST explicit declaration wins"
    );
    // The bare mention on line 2 reserved the slot (input order), and
    // the later `B(Other)` did not re-shape it.
    let ids: Vec<&str> = fc.nodes.iter().map(|n| n.id.as_str()).collect();
    assert_eq!(ids, ["A", "B", "C"], "first-mention order");
}

/// FAILING-FIRST for the sequence half (citation: flowchart.rs
/// `register()` documents the crate rule — "the first EXPLICIT
/// shape/text declaration wins (a bare mention never resets a
/// declared node)" — but `parse_participant` dropped a later explicit
/// alias when a message had already auto-registered the id, silently
/// losing the label with no notice; the one rule now covers both
/// diagram kinds).
#[test]
fn sequence_first_explicit_alias_wins_even_after_implicit_registration() {
    let src =
        "sequenceDiagram\n    a->>b: hi\n    participant a as Alice\n    participant a as Again";
    let Ok(Diagram::Sequence(seq)) = parse(src) else {
        panic!("supported sequence");
    };
    let a = seq
        .participants
        .iter()
        .find(|p| p.id == "a")
        .expect("a registered");
    assert_eq!(
        a.alias.as_deref(),
        Some("Alice"),
        "the first EXPLICIT alias enriches the implicit registration"
    );
    // Position stays first-encounter: `a` was met before `b`.
    let ids: Vec<&str> = seq.participants.iter().map(|p| p.id.as_str()).collect();
    assert_eq!(ids, ["a", "b"], "column order is first encounter");

    // And the label reaches the rendered box.
    let mut rig = Rig::mount(Size::new(40, 12), || MermaidView::new(src));
    let rows = rig.rows();
    assert!(
        rows[1].contains("Alice"),
        "the alias renders on the participant box: {:?}",
        rows[1]
    );
}
