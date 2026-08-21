//! Flowchart parser conformance: every YES-row spelling accepts,
//! every named v2 spelling falls back with its reason, and the
//! verdict names the FIRST offending line.

use abstracttui_mermaid::{parse, Diagram, Direction, EdgeKind, FlowchartIr, NodeShape};

fn flowchart(src: &str) -> FlowchartIr {
    match parse(src) {
        Ok(Diagram::Flowchart(fc)) => fc,
        other => panic!("expected flowchart, got {other:?}"),
    }
}

#[test]
fn header_spellings_and_directions() {
    assert_eq!(flowchart("graph TD\nA-->B").direction, Direction::TopDown);
    assert_eq!(flowchart("graph TB\nA-->B").direction, Direction::TopDown);
    assert_eq!(
        flowchart("flowchart LR\nA-->B").direction,
        Direction::LeftRight
    );
    assert_eq!(
        flowchart("flowchart BT\nA-->B").direction,
        Direction::BottomTop
    );
    assert_eq!(flowchart("graph RL\nA-->B").direction, Direction::RightLeft);
    // Unknown direction: named fallback on line 1.
    let err = parse("graph XX\nA-->B").unwrap_err();
    assert_eq!(err.line_no, 1);
    assert!(err.reason.contains("direction"), "{}", err.reason);
    // Extra header tokens are not an accepted spelling.
    assert!(parse("graph TD extra\nA-->B").is_err());
}

#[test]
fn node_shape_spellings() {
    let fc = flowchart(
        "graph TD\nP[Process]\nR(Round)\nD{Choice}\nS([Stadium])\nX\nQ[\"has [inner] brackets\"]",
    );
    let shape = |id: &str| fc.nodes.iter().find(|n| n.id == id).unwrap();
    assert_eq!(shape("P").shape, NodeShape::Rect);
    assert_eq!(shape("P").text.as_deref(), Some("Process"));
    assert_eq!(shape("R").shape, NodeShape::Rounded);
    assert_eq!(shape("D").shape, NodeShape::Diamond);
    assert_eq!(shape("S").shape, NodeShape::Stadium);
    assert_eq!(shape("X").shape, NodeShape::Plain);
    assert_eq!(shape("X").text, None);
    assert_eq!(
        shape("Q").text.as_deref(),
        Some("has [inner] brackets"),
        "quoted text accepts brackets"
    );
}

#[test]
fn edge_spellings_and_labels() {
    let fc = flowchart("graph TD\na --> b\nb --- c\nc -.-> d\nd ==> e\ne -->|go| f\nf-->|tight|g");
    let kinds: Vec<EdgeKind> = fc.edges.iter().map(|e| e.kind).collect();
    assert_eq!(
        kinds,
        vec![
            EdgeKind::Arrow,
            EdgeKind::Open,
            EdgeKind::Dotted,
            EdgeKind::Thick,
            EdgeKind::Arrow,
            EdgeKind::Arrow,
        ]
    );
    assert_eq!(fc.edges[4].label.as_deref(), Some("go"));
    assert_eq!(fc.edges[5].label.as_deref(), Some("tight"), "no-space form");
    assert_eq!(fc.edges[0].label, None);
}

#[test]
fn shaped_nodes_inline_in_edges() {
    let fc = flowchart("graph LR\nA[Start] -->|go| B{Choice}\nB --> C(End)");
    assert_eq!(fc.nodes.len(), 3);
    assert_eq!(fc.edges.len(), 2);
    assert_eq!(
        fc.nodes.iter().find(|n| n.id == "B").unwrap().shape,
        NodeShape::Diamond
    );
}

#[test]
fn last_explicit_declaration_wins_like_mermaid() {
    let fc = flowchart("graph TD\nA --> B\nA[First] --> C\nA(Second) --> D");
    let a = fc.nodes.iter().find(|n| n.id == "A").unwrap();
    // mermaid renders the last declaration; matching it is the whole
    // point of a mermaid renderer.
    assert_eq!(a.text.as_deref(), Some("Second"), "last declaration wins");
    assert_eq!(a.shape, NodeShape::Rounded);
    // A BARE mention is not a declaration and never resets one.
    let fc = flowchart("graph TD\nA[Named] --> B\nA --> C");
    let a = fc.nodes.iter().find(|n| n.id == "A").unwrap();
    assert_eq!(
        a.text.as_deref(),
        Some("Named"),
        "bare mention never resets"
    );
}

#[test]
fn ignored_directives_notice_and_proceed() {
    let fc = flowchart(
        "%%{init: {\"theme\":\"dark\"}}%%\ngraph TD\n%% plain comment\nA-->B\nclassDef x fill:#fff\nstyle A fill:#000",
    );
    assert_eq!(fc.edges.len(), 1);
    assert_eq!(fc.notices.len(), 3, "{:?}", fc.notices);
    assert!(fc.notices[0].contains("init/theme"));
    assert!(fc.notices[1].contains("classDef"));
    assert!(fc.notices[2].contains("style"));
}

#[test]
fn named_v2_fallbacks() {
    // Infix labels, `&` groups, chaining and the `x`/`o` heads are no
    // longer v2 rows — they parse. Their spellings are pinned in
    // `supported_spellings` below; what stays here is what still falls
    // back, and it must still fall back BY NAME.
    let composite = parse("stateDiagram-v2\n  state A {\n    B --> C\n  }").unwrap_err();
    assert!(
        !composite.reason.is_empty(),
        "composite state names a reason"
    );
}

#[test]
fn verdict_names_the_first_bad_line() {
    let err = parse("graph TD\nA-->B\nB-->C\nweird !! stuff\nC-->D\nsubgraph later").unwrap_err();
    assert_eq!(err.line_no, 4, "first offense wins, not the subgraph below");
    assert_eq!(err.line, "weird !! stuff");
}

#[test]
fn unknown_diagram_kinds_are_named() {
    for (src, kind) in [
        ("classDiagram\nA <|-- B", "classDiagram"),
        ("erDiagram\nA ||--o{ B : has", "erDiagram"),
        ("gantt\ntitle X", "gantt"),
        ("pie title Pets\n\"a\" : 1", "pie"),
        ("mindmap\nroot", "mindmap"),
        ("gitGraph\ncommit", "gitGraph"),
    ] {
        let err = parse(src).unwrap_err();
        assert_eq!(err.line_no, 1);
        assert!(err.reason.contains(kind), "{}: {}", kind, err.reason);
    }
    assert!(parse("").is_err(), "empty source falls back");
    assert!(parse("   \n \n").is_err());
}

// ---------------------------------------------------------------------------
// The widened statement scanner: chaining, infix labels, `&` groups,
// the arrow vocabulary, and the shapes — all one pass.
// ---------------------------------------------------------------------------

/// THE ambiguity in mermaid's link grammar, and the rule that settles
/// it: a body of exactly two dashes OPENS a labelled link, three or
/// more IS a complete open link. Without this rule `A --- B --> C`
/// (a chain) and `A -- B --> C` (a label) are indistinguishable.
#[test]
fn two_dashes_label_three_dashes_chain() {
    // Exactly two: "B" is the LABEL of one A->C edge.
    let fc = flowchart("graph LR\nA -- B --> C");
    assert_eq!(fc.edges.len(), 1, "one labelled edge: {:?}", fc.edges);
    assert_eq!(fc.edges[0].label.as_deref(), Some("B"));
    assert_eq!(
        (fc.edges[0].from.as_str(), fc.edges[0].to.as_str()),
        ("A", "C")
    );
    assert_eq!(fc.nodes.len(), 2, "B is a label, not a node");

    // Three or more: a chain through B.
    let fc = flowchart("graph LR\nA --- B --> C");
    assert_eq!(fc.edges.len(), 2, "chain: {:?}", fc.edges);
    assert_eq!(fc.nodes.len(), 3, "B is a node here");
    assert_eq!(fc.edges[0].kind, EdgeKind::Open);
    assert_eq!(fc.edges[1].kind, EdgeKind::Arrow);
}

#[test]
fn arrow_vocabulary_and_lengths() {
    let cases: [(&str, EdgeKind); 8] = [
        ("A --> B", EdgeKind::Arrow),
        ("A ---> B", EdgeKind::Arrow),
        ("A -----> B", EdgeKind::Arrow),
        ("A --- B", EdgeKind::Open),
        ("A ----- B", EdgeKind::Open),
        ("A -.-> B", EdgeKind::Dotted),
        ("A -..-> B", EdgeKind::Dotted),
        ("A ==> B", EdgeKind::Thick),
    ];
    for (stmt, want) in cases {
        let fc = flowchart(&format!("graph LR\n{stmt}"));
        assert_eq!(fc.edges.len(), 1, "{stmt}");
        assert_eq!(fc.edges[0].kind, want, "{stmt}");
    }
    // `===` is an open thick link, and `-.-` an open dotted one.
    assert_eq!(flowchart("graph LR\nA === B").edges[0].kind, EdgeKind::Open);
    assert_eq!(
        flowchart("graph LR\nA -.- B").edges[0].kind,
        EdgeKind::Dotted
    );
}

/// `--x` and `--o` parse; the terminal has no glyph for those heads,
/// so the downgrade is LABELED rather than silent.
#[test]
fn cross_and_circle_heads_parse_with_a_labeled_downgrade() {
    let fc = flowchart("graph LR\nA --x B\nB --o C");
    assert_eq!(fc.edges.len(), 2);
    assert!(
        fc.notices.iter().any(|n| n.contains("arrowheads")),
        "the downgrade is on the record: {:?}",
        fc.notices
    );
    // `x`/`o` only count as heads when they do not start an id.
    let fc = flowchart("graph LR\nA --- xyz");
    assert_eq!(fc.edges[0].to, "xyz", "`xyz` is a node, not an `x` head");
}

#[test]
fn amp_groups_are_a_cross_product() {
    let fc = flowchart("graph TD\nA & B --> C & D");
    let mut pairs: Vec<String> = fc
        .edges
        .iter()
        .map(|e| format!("{}->{}", e.from, e.to))
        .collect();
    pairs.sort();
    assert_eq!(pairs, ["A->C", "A->D", "B->C", "B->D"]);
}

#[test]
fn chains_carry_their_own_labels() {
    let fc = flowchart("graph LR\nA[Start] -->|first| B{Check} -- second --> C(Done)");
    assert_eq!(fc.edges.len(), 2);
    assert_eq!(fc.edges[0].label.as_deref(), Some("first"));
    assert_eq!(fc.edges[1].label.as_deref(), Some("second"));
    assert_eq!(fc.nodes.len(), 3);
}

/// Labels are TEXT: the quotes mermaid uses to escape commas are not
/// part of the label, in every position that takes one.
#[test]
fn quoted_labels_lose_their_quotes() {
    let fc = flowchart("graph LR\nA[\"a, b\"] -->|\"yes, always\"| B");
    assert_eq!(fc.nodes[0].text.as_deref(), Some("a, b"));
    assert_eq!(fc.edges[0].label.as_deref(), Some("yes, always"));
}

/// An arrow INSIDE a quoted label is text. This used to work on the
/// right of a link and fail on the left — the scanner is quote-aware
/// on both sides now.
#[test]
fn arrows_inside_labels_are_text_on_both_sides() {
    let fc = flowchart("graph LR\nA[\"a --> b\"] --> B[\"c --> d\"]");
    assert_eq!(fc.edges.len(), 1, "one real edge: {:?}", fc.edges);
    assert_eq!(fc.nodes[0].text.as_deref(), Some("a --> b"));
    assert_eq!(fc.nodes[1].text.as_deref(), Some("c --> d"));
}

#[test]
fn the_shape_table_covers_the_common_spellings() {
    let fc = flowchart(
        "graph TD\nA((c)) --> B[[s]]\nB --> C[(db)]\nC --> D{{h}}\nD --> E>flag]\nE --> F([stad])",
    );
    let shape = |id: &str| fc.nodes.iter().find(|n| n.id == id).unwrap().shape;
    assert_eq!(shape("A"), NodeShape::Circle);
    assert_eq!(shape("B"), NodeShape::Subroutine);
    assert_eq!(shape("C"), NodeShape::Cylinder);
    assert_eq!(shape("D"), NodeShape::Hexagon);
    assert_eq!(shape("E"), NodeShape::Asymmetric);
    assert_eq!(shape("F"), NodeShape::Stadium);
}

/// A `<br/>` used to reach the screen as literal text inside a
/// one-line card. It is flattened to a word break, and the flattening
/// is on the record.
#[test]
fn line_breaks_flatten_with_a_notice() {
    let fc = flowchart("graph TD\nA[\"first<br/>second\"] --> B");
    assert_eq!(fc.nodes[0].text.as_deref(), Some("first second"));
    assert!(
        fc.notices.iter().any(|n| n.contains("line breaks")),
        "{:?}",
        fc.notices
    );
}

/// One `click` line must not abort an otherwise-supported diagram —
/// the same deal `classDef`/`style` already had.
#[test]
fn interaction_directives_are_dropped_not_fatal() {
    let fc = flowchart(
        "graph TD\nA --> B\nclick A \"https://example.com\"\nlinkStyle 0 stroke:#f00\nclass A big",
    );
    assert_eq!(fc.edges.len(), 1);
    for directive in ["click", "linkStyle", "class"] {
        assert!(
            fc.notices.iter().any(|n| n.contains(directive)),
            "{directive} noticed: {:?}",
            fc.notices
        );
    }
}

/// `subgraph` is the most common reason a real diagram used to fall
/// back. Groups are FLATTENED — the layout engine draws no clusters —
/// and the loss is a notice, not a silent difference.
#[test]
fn subgraphs_flatten_with_a_notice() {
    let fc = flowchart(
        "flowchart TB\n  A --> B\n  subgraph core [Core engine]\n    direction LR\n    B --> C\n    C --> D\n  end\n  D --> E",
    );
    assert_eq!(fc.edges.len(), 4, "every edge survives: {:?}", fc.edges);
    assert_eq!(fc.nodes.len(), 5);
    assert!(
        fc.notices
            .iter()
            .any(|n| n.contains("Core engine") && n.contains("flattened")),
        "the grouping loss is on the record: {:?}",
        fc.notices
    );
    // ONE line per diagram, not one per group, and no second line for
    // a group-local `direction` — a reader cannot act on that, and the
    // flattening notice already says the box is gone.
    assert_eq!(fc.notices.len(), 1, "one notice: {:?}", fc.notices);

    let many = flowchart(
        "flowchart TB\n subgraph a [One]\n  X --> Y\n end\n subgraph b [Two]\n  Y --> Z\n end",
    );
    assert_eq!(many.notices.len(), 1, "still one line: {:?}", many.notices);
    assert!(
        many.notices[0].contains("2 subgraphs"),
        "{:?}",
        many.notices
    );
    assert!(many.notices[0].contains("One, Two"), "{:?}", many.notices);

    // Nesting is depth-counted, not pattern-matched.
    let fc = flowchart("flowchart TD\n subgraph a\n  subgraph b\n   X --> Y\n  end\n end");
    assert_eq!(fc.edges.len(), 1);

    // An edge naming a GROUP still finds a node, carrying its title.
    let fc = flowchart("flowchart LR\n subgraph core [Core engine]\n  B --> C\n end\n A --> core");
    let core = fc
        .nodes
        .iter()
        .find(|n| n.id == "core")
        .expect("group is a node");
    assert_eq!(core.text.as_deref(), Some("Core engine"));

    // An unclosed block is still an error, named.
    let err = parse("flowchart TD\n subgraph one\n A --> B").unwrap_err();
    assert!(err.reason.contains("never closed"), "{}", err.reason);
}
