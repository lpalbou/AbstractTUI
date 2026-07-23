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
fn first_explicit_declaration_wins() {
    let fc = flowchart("graph TD\nA --> B\nA[First] --> C\nA[Second] --> D");
    let a = fc.nodes.iter().find(|n| n.id == "A").unwrap();
    assert_eq!(
        a.text.as_deref(),
        Some("First"),
        "bare mention never resets"
    );
    assert_eq!(a.shape, NodeShape::Rect);
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
    let sub = parse("graph TD\nA-->B\nsubgraph one\nB-->C\nend").unwrap_err();
    assert_eq!(sub.line_no, 3);
    assert!(sub.reason.contains("subgraph"), "{}", sub.reason);

    let infix = parse("graph LR\nA-- text -->B").unwrap_err();
    assert!(infix.reason.contains("infix"), "{}", infix.reason);

    let amp = parse("graph TD\nA & B --> C").unwrap_err();
    assert!(amp.reason.contains("&"), "{}", amp.reason);

    let chain = parse("graph LR\nA --> B --> C").unwrap_err();
    assert!(chain.reason.contains("chaining"), "{}", chain.reason);

    let unknown = parse("graph LR\nA --o B").unwrap_err();
    assert!(
        unknown.reason.contains("unrecognized"),
        "{}",
        unknown.reason
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
