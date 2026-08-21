//! Sequence + state parser conformance.

use abstracttui_mermaid::{
    parse, Block, BlockKind, Diagram, FlowchartIr, MessageKind, NoteAnchor, SeqItem, SequenceIr,
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

    // `alt`/`opt`/`loop`/`par` and activations render now; what is
    // still outside the subset must still fall back BY NAME.
    for kw in [
        "rect rgb(0,0,0)",
        "critical fetch",
        "break oops",
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

/// Quotes are mermaid's escape for a comma, not part of the text —
/// in aliases and in message bodies alike.
#[test]
fn quoted_aliases_and_message_text_lose_their_quotes() {
    let src = "sequenceDiagram\n  participant a as \"Alice B.\"\n  a->>b: \"hello, world\"";
    let Ok(Diagram::Sequence(seq)) = parse(src) else {
        panic!("supported sequence");
    };
    assert_eq!(seq.participants[0].label(), "Alice B.");
    let SeqItem::Message(m) = &seq.items[0] else {
        panic!("a message");
    };
    assert_eq!(m.text, "hello, world");
}

// ---------------------------------------------------------------------------
// Control-flow blocks: alt/else, opt, loop, par/and — and activations.
// The IR is a TREE, so these tests are about STRUCTURE, not tokens.
// ---------------------------------------------------------------------------

fn block_of(item: &SeqItem) -> &Block {
    match item {
        SeqItem::Block(b) => b,
        other => panic!("expected a block, got {other:?}"),
    }
}

#[test]
fn alt_else_becomes_one_block_with_two_branches() {
    let seq = sequence(
        "sequenceDiagram\n  A->>B: Hungry?\n  alt is lunchtime\n    B-->>A: Yes\n  else not yet\n    B-->>A: Later\n  end",
    );
    // The message before the block stays outside it.
    assert!(matches!(seq.items[0], SeqItem::Message(_)));
    let block = block_of(&seq.items[1]);
    assert_eq!(block.kind, BlockKind::Alt);
    assert_eq!(block.branches().count(), 2);
    assert_eq!(block.first.label, "is lunchtime");
    assert_eq!(block.rest[0].label, "not yet");
    assert_eq!(block.first.items.len(), 1);
    assert_eq!(block.rest[0].items.len(), 1);
    assert_eq!(block.label(), "is lunchtime", "the tab shows the opener");
}

#[test]
fn opt_and_loop_carry_one_branch_and_par_carries_its_ands() {
    let opt = sequence("sequenceDiagram\n  opt if slow\n    A->>B: warn\n  end");
    assert_eq!(block_of(&opt.items[0]).kind, BlockKind::Opt);
    assert_eq!(block_of(&opt.items[0]).branches().count(), 1);

    let lp = sequence("sequenceDiagram\n  loop every minute\n    A->>B: poll\n  end");
    assert_eq!(block_of(&lp.items[0]).kind, BlockKind::Loop);
    assert_eq!(block_of(&lp.items[0]).first.label, "every minute");

    let par = sequence(
        "sequenceDiagram\n  par to Bob\n    A->>B: hi\n  and to Carl\n    A->>C: hi\n  and to Dan\n    A->>D: hi\n  end",
    );
    let block = block_of(&par.items[0]);
    assert_eq!(block.kind, BlockKind::Par);
    assert_eq!(block.branches().count(), 3, "one branch per `and`");
}

#[test]
fn blocks_nest() {
    let seq =
        sequence("sequenceDiagram\n  alt outer\n    loop inner\n      A->>B: tick\n    end\n  end");
    let outer = block_of(&seq.items[0]);
    assert_eq!(outer.kind, BlockKind::Alt);
    let inner = block_of(&outer.first.items[0]);
    assert_eq!(inner.kind, BlockKind::Loop);
    assert!(matches!(inner.first.items[0], SeqItem::Message(_)));
}

/// The tree's whole promise: a malformed nesting cannot reach a
/// consumer. Each of these is caught ONCE, in the parser, by name.
#[test]
fn unbalanced_blocks_are_named_not_half_built() {
    let never_closed = parse("sequenceDiagram\n  alt one\n    A->>B: hi").unwrap_err();
    assert!(
        never_closed.reason.contains("never closed"),
        "{}",
        never_closed.reason
    );

    let stray_end = parse("sequenceDiagram\n  A->>B: hi\n  end").unwrap_err();
    assert!(
        stray_end.reason.contains("without an open block"),
        "{}",
        stray_end.reason
    );

    let stray_else = parse("sequenceDiagram\n  else nope").unwrap_err();
    assert!(
        stray_else.reason.contains("outside any block"),
        "{}",
        stray_else.reason
    );

    // A divider that does not belong to its block names the one that
    // does — `alt` divides with `else`, `par` with `and`.
    let wrong = parse("sequenceDiagram\n  par x\n    A->>B: hi\n  else y\n  end").unwrap_err();
    assert!(wrong.reason.contains("`and`"), "{}", wrong.reason);
    let none = parse("sequenceDiagram\n  loop x\n    A->>B: hi\n  else y\n  end").unwrap_err();
    assert!(none.reason.contains("no `else`"), "{}", none.reason);
}

/// `+`/`-` are sugar for activate/deactivate, so ONE representation
/// reaches layout: a message, then the activation event it implies.
#[test]
fn activation_shorthand_expands_to_the_same_items_as_the_keywords() {
    let sugar = sequence("sequenceDiagram\n  A->>+B: work\n  B-->>-A: done");
    let keywords =
        sequence("sequenceDiagram\n  A->>B: work\n  activate B\n  B-->>A: done\n  deactivate B");
    assert_eq!(
        sugar.items, keywords.items,
        "the shorthand IS the keywords: {:?}",
        sugar.items
    );
    // `+` activates the TARGET; `-` deactivates the SENDER.
    assert_eq!(sugar.items[1], SeqItem::Activate("B".into()));
    assert_eq!(sugar.items[3], SeqItem::Deactivate("B".into()));
}

/// Items inside a block belong to the block, and participants
/// mentioned only inside one are still registered in encounter order.
#[test]
fn participants_inside_blocks_are_registered_in_encounter_order() {
    let seq =
        sequence("sequenceDiagram\n  A->>B: one\n  alt x\n    C->>D: two\n  end\n  E->>A: three");
    let ids: Vec<&str> = seq.participants.iter().map(|p| p.id.as_str()).collect();
    assert_eq!(ids, ["A", "B", "C", "D", "E"]);
    assert_eq!(seq.items.len(), 3, "block is ONE item: {:?}", seq.items);
}

/// `activate` is a LOOKUP, never a registration. Registering from it
/// invented lifelines for typos, and `activate B` before B's first
/// message put B's column first — so the two spellings of one concept
/// drew mirrored diagrams.
#[test]
fn activate_never_invents_or_reorders_participants() {
    let seq = sequence("sequenceDiagram\n  A->>B: hi\n  activate B\n  deactivate B");
    let ids: Vec<&str> = seq.participants.iter().map(|p| p.id.as_str()).collect();
    assert_eq!(ids, ["A", "B"], "message order decides the columns");

    // The shorthand agrees, participants included — not just items.
    let sugar = sequence("sequenceDiagram\n  A->>+B: hi\n  deactivate B");
    let sugar_ids: Vec<&str> = sugar.participants.iter().map(|p| p.id.as_str()).collect();
    assert_eq!(sugar_ids, ids, "one concept, one diagram");

    // An id nobody declared is a typo; saying so beats drawing a
    // lifeline for a participant who never speaks.
    let ghost = parse("sequenceDiagram\n  A->>B: hi\n  activate Ghost").unwrap_err();
    assert!(
        ghost.reason.contains("no declared participant"),
        "{}",
        ghost.reason
    );
    let bad = parse("sequenceDiagram\n  A->>B: x\n  activate").unwrap_err();
    assert!(bad.reason.contains("participant"), "{}", bad.reason);
}

/// Nesting recurses in layout, in rendering, and in `Drop` — and a
/// stack overflow ABORTS the process, which a crate that renders
/// mermaid out of other people's markdown cannot do. The cap is a
/// named fallback, the contract's own answer.
#[test]
fn absurd_nesting_is_refused_not_fatal() {
    let deep = format!(
        "sequenceDiagram\n{}\n  A->>B: x\n{}",
        "  opt x\n".repeat(5_000),
        "  end\n".repeat(5_000)
    );
    let err = parse(&deep).expect_err("5000-deep nesting must not be accepted");
    assert!(err.reason.contains("nested deeper"), "{}", err.reason);

    // The cap is generous enough that no human diagram meets it.
    let ok = format!(
        "sequenceDiagram\n{}\n  A->>B: x\n{}",
        "  opt x\n".repeat(20),
        "  end\n".repeat(20)
    );
    assert!(parse(&ok).is_ok(), "20 deep is a legal diagram");
}

/// `end` closes a block and takes nothing else: a swallowed word is
/// how a diagram closes a block its author did not mean to close.
#[test]
fn end_takes_no_argument() {
    let err = parse("sequenceDiagram\n  opt x\n    A->>B: y\n  end note").unwrap_err();
    assert!(err.reason.contains("no argument"), "{}", err.reason);
}

/// The verdict quotes the line the author has to go fix — the OPENING
/// line of the block, not the keyword alone.
#[test]
fn an_unclosed_block_quotes_its_opening_line() {
    let err = parse("sequenceDiagram\n  alt lunchtime is here\n    A->>B: x").unwrap_err();
    assert_eq!(err.line, "alt lunchtime is here");
    assert_eq!(err.line_no, 2);
}
