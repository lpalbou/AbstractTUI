//! Flowchart statement parser (the `flowchart`/`graph` YES rows).
//!
//! One classifier arm per accepted spelling; the FIRST statement that
//! matches nothing returns the named [`Unsupported`] verdict (atomic
//! fallback — no partial IR ever escapes). Known v2 constructs get
//! targeted reasons (`subgraph`, infix labels, `&`-chaining, edge
//! chaining); everything else is honestly "unrecognized statement".

use abstracttui_graph::Direction;

use crate::ir::{EdgeKind, FlowEdge, FlowNode, FlowchartIr, NodeShape, Unsupported};
use crate::lines::{take_id, Stmt};

/// Arrow spellings, most specific first (tie on position -> first in
/// this list wins, which keeps `-->>`-style overlaps deterministic).
const ARROWS: [(&str, EdgeKind); 4] = [
    ("-.->", EdgeKind::Dotted),
    ("-->", EdgeKind::Arrow),
    ("==>", EdgeKind::Thick),
    ("---", EdgeKind::Open),
];

pub(crate) fn parse_flowchart(
    direction: Direction,
    stmts: &[Stmt],
    notices: Vec<String>,
) -> Result<FlowchartIr, Unsupported> {
    let mut fc = FlowchartIr {
        direction,
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

    // Recognized-and-dropped directives (the table's IGNORED row).
    for directive in ["classDef ", "style "] {
        if s.starts_with(directive) {
            fc.notices.push(format!(
                "`{}` directive ignored (line {})",
                directive.trim_end(),
                stmt.line_no
            ));
            return Ok(());
        }
    }
    let first_token = s.split_whitespace().next().unwrap_or("");
    if first_token == "subgraph" || first_token == "end" {
        return Err(Unsupported::new(
            stmt.line_no,
            s,
            "subgraph blocks are not supported (v2 — needs layout clusters)",
        ));
    }

    match find_arrow(s) {
        Some((pos, token, kind)) => {
            let left = &s[..pos];
            let mut rest = &s[pos + token.len()..];
            // Postfix label: optional whitespace, then `|label|`.
            let mut label = None;
            let after = rest.trim_start();
            if let Some(body) = after.strip_prefix('|') {
                let Some((text, tail)) = body.split_once('|') else {
                    return Err(Unsupported::new(
                        stmt.line_no,
                        s,
                        "unterminated `|label|` on an edge",
                    ));
                };
                let text = text.trim();
                if text.is_empty() {
                    return Err(Unsupported::new(stmt.line_no, s, "empty `|label|`"));
                }
                label = Some(text.to_string());
                rest = tail;
            }
            let from = parse_noderef(fc, left)
                .ok_or_else(|| classify_bad_side(stmt, left, rest, Side::Left))?;
            let to = parse_noderef(fc, rest)
                .ok_or_else(|| classify_bad_side(stmt, left, rest, Side::Right))?;
            fc.edges.push(FlowEdge {
                from,
                to,
                label,
                kind,
            });
            Ok(())
        }
        None => match parse_noderef(fc, s) {
            Some(_) => Ok(()),
            None => Err(classify_bad_side(stmt, s, "", Side::Whole)),
        },
    }
}

/// Earliest arrow occurrence (ties: most specific first).
fn find_arrow(s: &str) -> Option<(usize, &'static str, EdgeKind)> {
    let mut best: Option<(usize, &'static str, EdgeKind)> = None;
    for (token, kind) in ARROWS {
        if let Some(pos) = s.find(token) {
            if best.is_none_or(|(bp, _, _)| pos < bp) {
                best = Some((pos, token, kind));
            }
        }
    }
    best
}

enum Side {
    Left,
    Right,
    Whole,
}

/// Name the failure precisely where a v2 construct is recognizable;
/// otherwise the honest generic reason.
fn classify_bad_side(stmt: &Stmt, left: &str, right: &str, side: Side) -> Unsupported {
    let s = stmt.text.as_str();
    if left.contains('&') || right.contains('&') {
        return Unsupported::new(stmt.line_no, s, "`&`-chaining is not supported (v2)");
    }
    // Infix label: the left side of the arrow ends in a `--…` run
    // (mermaid's `A--label-->B` form).
    if matches!(side, Side::Left | Side::Right) {
        let after_id = take_id(left.trim_start()).map(|(_, rest)| rest.trim());
        if after_id.is_some_and(|rest| rest.starts_with("--") || rest.starts_with("==")) {
            return Unsupported::new(
                stmt.line_no,
                s,
                "infix edge labels (`--label-->`) are not supported — use `-->|label|`",
            );
        }
    }
    if matches!(side, Side::Right) && find_arrow(right).is_some() {
        return Unsupported::new(
            stmt.line_no,
            s,
            "edge chaining is not supported — one edge per statement",
        );
    }
    Unsupported::new(stmt.line_no, s, "unrecognized statement")
}

/// Parse a node reference (`id`, `id[text]`, `id(text)`, `id{text}`,
/// `id([text])`, quoted text inside brackets), registering the node.
/// Returns the id, or `None` when the text is not a node reference in
/// the accepted spellings.
fn parse_noderef(fc: &mut FlowchartIr, s: &str) -> Option<String> {
    let s = s.trim();
    let (id, rest) = take_id(s)?;
    let (shape, text) = if rest.is_empty() {
        (NodeShape::Plain, None)
    } else {
        let (shape, inner) = bracket_shape(rest)?;
        (shape, Some(unquote(inner)?))
    };
    register(fc, id, shape, text);
    Some(id.to_string())
}

/// The bracket spelling table. Stadium (`([..])`) checks before
/// rounded (`(..)`) — the longer delimiter is a prefix of the shorter.
fn bracket_shape(rest: &str) -> Option<(NodeShape, &str)> {
    let pairs: [(&str, &str, NodeShape); 4] = [
        ("([", "])", NodeShape::Stadium),
        ("[", "]", NodeShape::Rect),
        ("(", ")", NodeShape::Rounded),
        ("{", "}", NodeShape::Diamond),
    ];
    for (open, close, shape) in pairs {
        if let Some(inner) = rest.strip_prefix(open).and_then(|r| r.strip_suffix(close)) {
            return Some((shape, inner));
        }
    }
    None
}

/// Bracket text: `"…"` accepts anything inside; unquoted text must be
/// bracket-free and non-empty (ambiguous nesting falls back honestly).
fn unquote(inner: &str) -> Option<String> {
    let inner = inner.trim();
    if inner.len() >= 2 && inner.starts_with('"') && inner.ends_with('"') {
        return Some(inner[1..inner.len() - 1].to_string());
    }
    if inner.is_empty() || inner.contains(['[', ']', '(', ')', '{', '}', '|', '"', '&']) {
        return None;
    }
    Some(inner.to_string())
}

/// First-mention order; the first EXPLICIT shape/text declaration
/// wins (a bare mention never resets a declared node).
fn register(fc: &mut FlowchartIr, id: &str, shape: NodeShape, text: Option<String>) {
    match fc.nodes.iter_mut().find(|n| n.id == id) {
        Some(node) => {
            if node.shape == NodeShape::Plain && node.text.is_none() && shape != NodeShape::Plain {
                node.shape = shape;
                node.text = text;
            }
        }
        None => fc.nodes.push(FlowNode {
            id: id.to_string(),
            text,
            shape,
        }),
    }
}
