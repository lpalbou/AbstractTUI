//! Sequence-diagram statement parser (the `sequenceDiagram` YES row).
//!
//! Accepted spellings: `participant id [as alias]`, messages `->>`
//! `-->>` `->` `-->` with required `: text`, and `Note left of/right
//! of/over` (docs capitalization). Blocks, activations and everything
//! else return the named atomic-fallback verdict.

use crate::ir::{
    Message, MessageKind, Note, NoteAnchor, Participant, SeqItem, SequenceIr, Unsupported,
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

/// v2 block/keyword vocabulary: recognized to NAME the fallback.
const V2_KEYWORDS: [&str; 14] = [
    "loop",
    "alt",
    "opt",
    "par",
    "else",
    "end",
    "rect",
    "critical",
    "break",
    "activate",
    "deactivate",
    "autonumber",
    "box",
    "actor",
];

pub(crate) fn parse_sequence(
    stmts: &[Stmt],
    notices: Vec<String>,
) -> Result<SequenceIr, Unsupported> {
    let mut seq = SequenceIr {
        notices,
        ..Default::default()
    };
    for stmt in stmts {
        parse_statement(&mut seq, stmt)?;
    }
    Ok(seq)
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
    let after = rest.trim_start();
    if after.starts_with('+') || after.starts_with('-') {
        return Err(Unsupported::new(
            stmt.line_no,
            s,
            "activations (`+`/`-`) are not supported (v2)",
        ));
    }
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
    seq.items.push(SeqItem::Message(Message {
        from,
        to,
        kind,
        text: text.to_string(),
    }));
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
