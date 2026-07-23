//! Sequence + state parser conformance.

use abstracttui_mermaid::{
    parse, Diagram, FlowchartIr, MessageKind, NoteAnchor, SeqItem, SequenceIr,
};

fn sequence(src: &str) -> SequenceIr {
    match parse(src) {
        Ok(Diagram::Sequence(s)) => s,
        other => panic!("expected sequence, got {other:?}"),
    }
}

#[test]
fn participants_aliases_and_implicit_order() {
    let seq = sequence(
        "sequenceDiagram\nparticipant a as Alice\nparticipant b\na->>b: hi\nb-->>c: implicit c",
    );
    let ids: Vec<&str> = seq.participants.iter().map(|p| p.id.as_str()).collect();
    assert_eq!(ids, vec!["a", "b", "c"], "declared first, implicit after");
    assert_eq!(seq.participants[0].label(), "Alice");
    assert_eq!(seq.participants[1].label(), "b");
}

#[test]
fn message_arrow_spellings() {
    let seq = sequence("sequenceDiagram\na->>b: one\na-->>b: two\na->b: three\na-->b: four");
    let kinds: Vec<MessageKind> = seq
        .items
        .iter()
        .map(|i| match i {
            SeqItem::Message(m) => m.kind,
            other => panic!("{other:?}"),
        })
        .collect();
    assert_eq!(
        kinds,
        vec![
            MessageKind::SolidArrow,
            MessageKind::DashedArrow,
            MessageKind::SolidOpen,
            MessageKind::DashedOpen,
        ]
    );
}

#[test]
fn note_spellings() {
    let seq = sequence(
        "sequenceDiagram\na->>b: hi\nNote left of a: L\nNote right of b: R\nNote over a: O\nNote over a,b: OB",
    );
    let anchors: Vec<&NoteAnchor> = seq
        .items
        .iter()
        .filter_map(|i| match i {
            SeqItem::Note(n) => Some(&n.anchor),
            _ => None,
        })
        .collect();
    assert_eq!(anchors.len(), 4);
    assert_eq!(*anchors[0], NoteAnchor::LeftOf("a".into()));
    assert_eq!(*anchors[1], NoteAnchor::RightOf("b".into()));
    assert_eq!(*anchors[2], NoteAnchor::Over("a".into(), None));
    assert_eq!(*anchors[3], NoteAnchor::Over("a".into(), Some("b".into())));
}

#[test]
fn self_messages_are_accepted() {
    let seq = sequence("sequenceDiagram\na->>a: think");
    assert_eq!(seq.items.len(), 1);
    assert_eq!(seq.participants.len(), 1);
}

#[test]
fn required_text_and_named_v2_fallbacks() {
    let no_colon = parse("sequenceDiagram\na->>b hi").unwrap_err();
    assert!(no_colon.reason.contains("`: text`"), "{}", no_colon.reason);

    let empty = parse("sequenceDiagram\na->>b:   ").unwrap_err();
    assert!(empty.reason.contains("required"), "{}", empty.reason);

    let act = parse("sequenceDiagram\na->>+b: hi").unwrap_err();
    assert!(act.reason.contains("activation"), "{}", act.reason);

    for kw in [
        "loop x",
        "alt y",
        "par z",
        "activate b",
        "autonumber",
        "box Purple",
    ] {
        let err = parse(&format!("sequenceDiagram\na->>b: hi\n{kw}")).unwrap_err();
        assert_eq!(err.line_no, 3, "{kw}");
        assert!(err.reason.contains("v2"), "{kw}: {}", err.reason);
    }

    // Lowercase `note` is outside the accepted spelling (docs
    // capitalization) — honest fallback, not silent acceptance.
    assert!(parse("sequenceDiagram\nnote over a: x").is_err());
}

fn state(src: &str) -> FlowchartIr {
    match parse(src) {
        Ok(Diagram::Flowchart(fc)) => fc,
        other => panic!("expected state-as-flowchart, got {other:?}"),
    }
}

#[test]
fn state_flat_compiles_to_the_flowchart_engine() {
    let fc = state(
        "stateDiagram-v2\n[*] --> Still\nStill --> [*]\nStill --> Moving\nMoving --> Crash : boom\nStill : At rest",
    );
    // Synthetic [*] ids can never collide with user ids (brackets are
    // not in the id charset).
    assert!(fc.nodes.iter().any(|n| n.id == "[*]start"));
    assert!(fc.nodes.iter().any(|n| n.id == "[*]end"));
    let still = fc.nodes.iter().find(|n| n.id == "Still").unwrap();
    assert_eq!(still.text.as_deref(), Some("At rest"));
    let boom = fc.edges.iter().find(|e| e.to == "Crash").unwrap();
    assert_eq!(boom.label.as_deref(), Some("boom"));
    assert_eq!(fc.edges.len(), 4);
}

#[test]
fn state_composite_falls_back_named() {
    let err = parse("stateDiagram-v2\n[*] --> A\nstate A {\n[*] --> b\n}").unwrap_err();
    assert_eq!(err.line_no, 3);
    assert!(err.reason.contains("flat"), "{}", err.reason);
    // v1 stateDiagram (not -v2) is a NO-row kind.
    assert!(parse("stateDiagram\n[*] --> A").is_err());
}
