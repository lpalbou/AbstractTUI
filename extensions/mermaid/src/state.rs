//! `stateDiagram-v2` FLAT parser (the stretch row) — a third front
//! end to the flowchart IR, which is exactly why it ships: transitions
//! become edges, `[*]` becomes synthetic start/end nodes, and layout +
//! rendering reuse the flowchart engine byte-for-byte.
//!
//! Accepted spellings only: `[*]`, `id`, `id : display`, `A --> B`
//! with optional `: label`. Composite states, notes and directions
//! fall back atomically with named reasons.

use crate::ir::{FlowEdge, FlowNode, FlowchartIr, NodeShape, Unsupported};
use crate::lines::{take_id, Stmt};

/// Synthetic ids for `[*]`: brackets cannot appear in user ids, so
/// these can never collide.
pub(crate) const START_ID: &str = "[*]start";
pub(crate) const END_ID: &str = "[*]end";

/// Keywords that name the v2 fallback precisely.
const V2_KEYWORDS: [&str; 5] = ["state", "note", "direction", "{", "}"];

pub(crate) fn parse_state(
    stmts: &[Stmt],
    notices: Vec<String>,
) -> Result<FlowchartIr, Unsupported> {
    let mut fc = FlowchartIr {
        notices,
        ..Default::default()
    };
    for stmt in stmts {
        parse_statement(&mut fc, stmt)?;
    }
    Ok(fc)
}

fn parse_statement(fc: &mut FlowchartIr, stmt: &Stmt) -> Result<(), Unsupported> {
    let s = stmt.text.as_str();
    let first_token = s.split_whitespace().next().unwrap_or("");
    if V2_KEYWORDS.contains(&first_token) || s.contains('{') || s.contains('}') {
        return Err(Unsupported::new(
            stmt.line_no,
            s,
            "composite states / notes / directions are not supported (v2 — flat states only)",
        ));
    }

    if let Some((lhs, rhs)) = s.split_once("-->") {
        // Transition: `A --> B` with optional `: label`.
        let from = endpoint(fc, lhs, true)
            .ok_or_else(|| Unsupported::new(stmt.line_no, s, "unrecognized transition source"))?;
        let (target, label) = match rhs.split_once(':') {
            Some((t, l)) => {
                let l = l.trim();
                if l.is_empty() {
                    return Err(Unsupported::new(
                        stmt.line_no,
                        s,
                        "transition label after `:` is empty",
                    ));
                }
                (t, Some(l.to_string()))
            }
            None => (rhs, None),
        };
        let to = endpoint(fc, target, false)
            .ok_or_else(|| Unsupported::new(stmt.line_no, s, "unrecognized transition target"))?;
        fc.edges.push(FlowEdge {
            from,
            to,
            label,
            kind: Default::default(),
        });
        return Ok(());
    }

    if let Some((id, text)) = s.split_once(':') {
        // `id : display text` (first declaration wins).
        let id = id.trim();
        let text = text.trim();
        let whole = take_id(id).is_some_and(|(_, tail)| tail.is_empty());
        if !whole || text.is_empty() {
            return Err(Unsupported::new(stmt.line_no, s, "unrecognized statement"));
        }
        let node = ensure(fc, id);
        if node.text.is_none() {
            node.text = Some(text.to_string());
        }
        return Ok(());
    }

    // Bare state mention.
    if take_id(s).is_some_and(|(_, tail)| tail.is_empty()) {
        ensure(fc, s);
        return Ok(());
    }
    Err(Unsupported::new(stmt.line_no, s, "unrecognized statement"))
}

/// `[*]` (start on the left, end on the right) or a plain state id.
fn endpoint(fc: &mut FlowchartIr, s: &str, is_source: bool) -> Option<String> {
    let s = s.trim();
    if s == "[*]" {
        let (id, glyph) = if is_source {
            (START_ID, "●")
        } else {
            (END_ID, "◉")
        };
        if !fc.nodes.iter().any(|n| n.id == id) {
            fc.nodes.push(FlowNode {
                id: id.to_string(),
                text: Some(glyph.to_string()),
                shape: NodeShape::Rounded,
            });
        }
        return Some(id.to_string());
    }
    let (id, tail) = take_id(s)?;
    if !tail.is_empty() {
        return None;
    }
    ensure(fc, id);
    Some(id.to_string())
}

/// States render as rounded cards (mermaid's state look).
fn ensure<'a>(fc: &'a mut FlowchartIr, id: &str) -> &'a mut FlowNode {
    if let Some(pos) = fc.nodes.iter().position(|n| n.id == id) {
        return &mut fc.nodes[pos];
    }
    fc.nodes.push(FlowNode {
        id: id.to_string(),
        text: None,
        shape: NodeShape::Rounded,
    });
    fc.nodes.last_mut().expect("just pushed")
}
