//! Sequence-diagram statement parser (the `sequenceDiagram` YES row).
//!
//! Accepted spellings: `participant id [as alias]`, messages `->>`
//! `-->>` `->` `-->` with required `: text`, `Note left of/right
//! of/over` (docs capitalization), the control-flow blocks `alt` +
//! `else`, `opt`, `loop` and `par` + `and`, and activations —
//! `activate`/`deactivate` or the `+`/`-` message suffixes, which are
//! sugar for exactly those. Everything else returns the named
//! atomic-fallback verdict.
//!
//! Blocks are assembled into the IR's TREE here, with a stack: this is
//! the ONE place that checks balance, which is what lets every
//! consumer downstream recurse over something that cannot be
//! malformed. Nesting is capped ([`MAX_BLOCK_DEPTH`]) because that
//! recursion is real stack, and a stack overflow aborts the host.

use crate::flowchart::clean_label;
use crate::ir::{
    Block, BlockKind, Branch, Message, MessageKind, Note, NoteAnchor, Participant, SeqItem,
    SequenceIr, Unsupported,
};
use crate::lines::{take_id, Stmt};

/// Message arrows, most specific first (tie on position -> first in
/// this list wins: `-->>` over `-->`, `->>` over `->`).
const ARROWS: [(&str, MessageKind); 4] = [
    ("-->>", MessageKind::DashedArrow),
    ("->>", MessageKind::SolidArrow),
    ("-->", MessageKind::DashedOpen),
    ("->", MessageKind::SolidOpen),
];

/// Nesting cap. Layout and `Drop` both recurse over the block tree,
/// and a stack overflow ABORTS the process — uncatchable, and fatal to
/// a TUI that renders mermaid out of a document it did not write. A
/// named fallback is the contract's own answer for input this crate
/// will not draw, and no human writes a diagram this deep.
const MAX_BLOCK_DEPTH: usize = 64;

/// Keywords still outside the subset: recognized to NAME the
/// fallback rather than reach the generic "unrecognized statement".
const V2_KEYWORDS: [&str; 4] = ["rect", "critical", "break", "box"];

pub(crate) fn parse_sequence(
    stmts: &[Stmt],
    notices: Vec<String>,
) -> Result<SequenceIr, Unsupported> {
    let mut seq = SequenceIr {
        notices,
        ..Default::default()
    };
    // Blocks nest, so items are appended to the INNERMOST open branch.
    // The stack holds the blocks being built; the balance check lives
    // here and only here, which is what lets the IR promise callers a
    // tree that cannot be malformed.
    let mut open: Vec<OpenBlock> = Vec::new();
    for stmt in stmts {
        let word = stmt.text.split_whitespace().next().unwrap_or("");
        let rest = stmt.text[word.len()..].trim();

        if let Some(kind) = BlockKind::from_keyword(word) {
            if open.len() >= MAX_BLOCK_DEPTH {
                return Err(Unsupported::new(
                    stmt.line_no,
                    &stmt.text,
                    format!("blocks nested deeper than {MAX_BLOCK_DEPTH}"),
                ));
            }
            open.push(OpenBlock {
                block: Block {
                    kind,
                    first: Branch {
                        label: clean_label(rest),
                        items: Vec::new(),
                    },
                    rest: Vec::new(),
                },
                line_no: stmt.line_no,
                source: stmt.text.clone(),
            });
            continue;
        }
        if word == "else" || word == "and" {
            let Some(OpenBlock { block, .. }) = open.last_mut() else {
                return Err(Unsupported::new(
                    stmt.line_no,
                    &stmt.text,
                    format!("`{word}` outside any block"),
                ));
            };
            let expected = block.kind.divider();
            if expected != Some(word) {
                return Err(Unsupported::new(
                    stmt.line_no,
                    &stmt.text,
                    match expected {
                        Some(d) => format!(
                            "`{}` divides with `{d}`, not `{word}`",
                            block.kind.keyword()
                        ),
                        None => format!("`{}` takes no `{word}` branch", block.kind.keyword()),
                    },
                ));
            }
            block.rest.push(Branch {
                label: clean_label(rest),
                items: Vec::new(),
            });
            continue;
        }
        if word == "end" {
            if !rest.is_empty() {
                // `end note`, `end foo` — mermaid rejects these, and a
                // silently swallowed word is how a diagram ends up
                // closing a block the author did not mean to close.
                return Err(Unsupported::new(
                    stmt.line_no,
                    &stmt.text,
                    format!("`end` takes no argument (found `{rest}`)"),
                ));
            }
            let Some(OpenBlock { block, .. }) = open.pop() else {
                return Err(Unsupported::new(
                    stmt.line_no,
                    &stmt.text,
                    "`end` without an open block",
                ));
            };
            push_item(&mut seq, &mut open, SeqItem::Block(block));
            continue;
        }
        if word == "activate" || word == "deactivate" {
            // A lookup, NOT a registration: `activate` naming an
            // unknown id used to invent a lifeline, and `activate B`
            // before B's first message put B's column FIRST — the two
            // spellings of one concept drew mirrored diagrams.
            let Some(id) = known_id(&seq, rest) else {
                return Err(Unsupported::new(
                    stmt.line_no,
                    &stmt.text,
                    format!("`{word}` names no declared participant"),
                ));
            };
            let item = if word == "activate" {
                SeqItem::Activate(id)
            } else {
                SeqItem::Deactivate(id)
            };
            push_item(&mut seq, &mut open, item);
            continue;
        }

        // Everything else parses into `seq.items`, then moves into the
        // innermost open branch if there is one.
        let before = seq.items.len();
        parse_statement(&mut seq, stmt)?;
        let produced: Vec<SeqItem> = seq.items.drain(before..).collect();
        for item in produced {
            push_item(&mut seq, &mut open, item);
        }
    }
    if let Some(OpenBlock {
        block,
        line_no,
        source,
    }) = open.pop()
    {
        // The verdict quotes the OPENING line, which is the one the
        // author has to go fix.
        return Err(Unsupported::new(
            line_no,
            &source,
            format!("`{}` block is never closed by `end`", block.kind.keyword()),
        ));
    }
    Ok(seq)
}

/// An id that already names a participant. Unknown ids are refused
/// rather than registered: an `activate` for someone who never speaks
/// is a typo, and drawing them a lifeline hides it.
fn known_id(seq: &SequenceIr, s: &str) -> Option<String> {
    let s = s.trim();
    let (id, tail) = take_id(s)?;
    if !tail.is_empty() {
        return None;
    }
    seq.participants
        .iter()
        .any(|p| p.id == id)
        .then(|| id.to_string())
}

/// A block being built, and where it opened (for the verdict).
struct OpenBlock {
    block: Block,
    line_no: usize,
    source: String,
}

/// Append to the innermost open branch, or to the diagram itself.
fn push_item(seq: &mut SequenceIr, open: &mut [OpenBlock], item: SeqItem) {
    match open.last_mut() {
        Some(OpenBlock { block, .. }) => block
            .rest
            .last_mut()
            .unwrap_or(&mut block.first)
            .items
            .push(item),
        None => seq.items.push(item),
    }
}

fn parse_statement(seq: &mut SequenceIr, stmt: &Stmt) -> Result<(), Unsupported> {
    let s = stmt.text.as_str();
    let first_token = s.split_whitespace().next().unwrap_or("");
    if V2_KEYWORDS.contains(&first_token) {
        return Err(Unsupported::new(
            stmt.line_no,
            s,
            format!("sequence `{first_token}` is not supported (v2)"),
        ));
    }

    if let Some(rest) = s.strip_prefix("participant ") {
        return parse_participant(seq, stmt, rest);
    }
    if let Some(rest) = s.strip_prefix("Note ") {
        return parse_note(seq, stmt, rest);
    }
    if let Some((pos, token, kind)) = find_arrow(s) {
        return parse_message(seq, stmt, pos, token, kind);
    }
    Err(Unsupported::new(stmt.line_no, s, "unrecognized statement"))
}

fn parse_participant(seq: &mut SequenceIr, stmt: &Stmt, rest: &str) -> Result<(), Unsupported> {
    let rest = rest.trim();
    let (id, alias) = match rest.split_once(" as ") {
        Some((id, alias)) => (id.trim(), Some(alias.trim())),
        None => (rest, None),
    };
    // The alias is DISPLAY text: quotes are mermaid's escape for a
    // comma, not part of the name.
    let cleaned = alias.map(crate::flowchart::clean_label);
    let alias = cleaned.as_deref();
    let full_id = take_id(id).is_some_and(|(_, tail)| tail.is_empty());
    if !full_id || alias.is_some_and(str::is_empty) {
        return Err(Unsupported::new(
            stmt.line_no,
            &stmt.text,
            "unrecognized participant declaration",
        ));
    }
    match seq.participants.iter_mut().find(|p| p.id == id) {
        // First-explicit-wins — the crate rule flowchart `register()`
        // documents ("a bare mention never resets a declared node"),
        // applied to participants (cycle-3 fix): a message/note that
        // auto-registered the id is ENRICHED by the first explicit
        // alias (column order stays first-encounter); later aliases
        // never re-label. Before this, `a->>b: hi` followed by
        // `participant a as Alice` silently dropped the alias.
        Some(p) => {
            if p.alias.is_none() {
                p.alias = alias.map(str::to_string);
            }
        }
        None => seq.participants.push(Participant {
            id: id.to_string(),
            alias: alias.map(str::to_string),
        }),
    }
    Ok(())
}

fn parse_note(seq: &mut SequenceIr, stmt: &Stmt, rest: &str) -> Result<(), Unsupported> {
    let bad = || {
        Unsupported::new(
            stmt.line_no,
            &stmt.text,
            "unrecognized note (accepted: `Note left of|right of|over id[,id]: text`)",
        )
    };
    let (anchor_src, text) = rest.split_once(':').ok_or_else(bad)?;
    let text = text.trim();
    if text.is_empty() {
        return Err(Unsupported::new(
            stmt.line_no,
            &stmt.text,
            "note text after `:` is required",
        ));
    }
    let anchor_src = anchor_src.trim();
    let anchor = if let Some(id) = anchor_src.strip_prefix("left of ") {
        NoteAnchor::LeftOf(full_id(seq, id).ok_or_else(bad)?)
    } else if let Some(id) = anchor_src.strip_prefix("right of ") {
        NoteAnchor::RightOf(full_id(seq, id).ok_or_else(bad)?)
    } else if let Some(ids) = anchor_src.strip_prefix("over ") {
        match ids.split_once(',') {
            Some((a, b)) => NoteAnchor::Over(
                full_id(seq, a).ok_or_else(bad)?,
                Some(full_id(seq, b).ok_or_else(bad)?),
            ),
            None => NoteAnchor::Over(full_id(seq, ids).ok_or_else(bad)?, None),
        }
    } else {
        return Err(bad());
    };
    seq.items.push(SeqItem::Note(Note {
        anchor,
        text: text.to_string(),
    }));
    Ok(())
}

fn parse_message(
    seq: &mut SequenceIr,
    stmt: &Stmt,
    pos: usize,
    token: &str,
    kind: MessageKind,
) -> Result<(), Unsupported> {
    let s = stmt.text.as_str();
    let from = s[..pos].trim();
    let rest = &s[pos + token.len()..];
    // `A->>+B` activates the TARGET when the message lands;
    // `B-->>-A` deactivates the SENDER when it replies. They are pure
    // sugar for `activate`/`deactivate`, so the parser expands them —
    // one concept reaches layout, not two spellings of it.
    let after = rest.trim_start();
    let (after, activation) = match after.as_bytes().first() {
        Some(b'+') => (after[1..].trim_start(), Some(true)),
        Some(b'-') => (after[1..].trim_start(), Some(false)),
        _ => (after, None),
    };
    let Some((to, text)) = after.split_once(':') else {
        return Err(Unsupported::new(
            stmt.line_no,
            s,
            "message text (`: text`) is required by the v1 spelling",
        ));
    };
    let text = text.trim();
    if text.is_empty() {
        return Err(Unsupported::new(
            stmt.line_no,
            s,
            "message text after `:` is required",
        ));
    }
    let (Some(from), Some(to)) = (full_id(seq, from), full_id(seq, to)) else {
        return Err(Unsupported::new(stmt.line_no, s, "unrecognized statement"));
    };
    let (activate_target, deactivate_source) = (to.clone(), from.clone());
    seq.items.push(SeqItem::Message(Message {
        from,
        to,
        kind,
        text: crate::flowchart::clean_label(text),
    }));
    match activation {
        Some(true) => seq.items.push(SeqItem::Activate(activate_target)),
        Some(false) => seq.items.push(SeqItem::Deactivate(deactivate_source)),
        None => {}
    }
    Ok(())
}

/// Earliest arrow occurrence (ties: most specific first).
fn find_arrow(s: &str) -> Option<(usize, &'static str, MessageKind)> {
    let mut best: Option<(usize, &'static str, MessageKind)> = None;
    for (token, kind) in ARROWS {
        if let Some(pos) = s.find(token) {
            if best.is_none_or(|(bp, _, _)| pos < bp) {
                best = Some((pos, token, kind));
            }
        }
    }
    best
}

/// A whole-token id; registers an implicit participant on first
/// encounter (mermaid semantics: messages/notes create participants).
fn full_id(seq: &mut SequenceIr, s: &str) -> Option<String> {
    let s = s.trim();
    let (id, tail) = take_id(s)?;
    if !tail.is_empty() {
        return None;
    }
    if !seq.participants.iter().any(|p| p.id == id) {
        seq.participants.push(Participant {
            id: id.to_string(),
            alias: None,
        });
    }
    Some(id.to_string())
}
