//! Flowchart statement parser (the `flowchart`/`graph` rows).
//!
//! The scanner walks a statement left to right, quote- and
//! bracket-aware, splitting it into node references and LINKS. That
//! shape is what makes chaining (`A --> B --> C`), infix labels
//! (`A -- yes --> B`), `&`-groups (`A & B --> C`), and the full arrow
//! vocabulary fall out of one pass instead of four special cases.
//!
//! Link spelling follows mermaid's own lexical rule, which is the only
//! way to tell chaining from an infix label without guessing:
//!
//! - a body of THREE or more dashes with no head (`---`, `----`) is a
//!   complete open link;
//! - a body of exactly TWO (`--`, `==`) with no head OPENS a labelled
//!   link, and the text runs until the closing body (`-->`, `--`);
//! - any body with a head (`-->`, `-.->`, `==>`, `--x`, `--o`) is a
//!   complete link.
//!
//! Whatever the scanner cannot place returns the named [`Unsupported`]
//! verdict — atomic fallback, no partial IR ever escapes.

use abstracttui_graph::Direction;

use crate::ir::{EdgeKind, FlowEdge, FlowNode, FlowchartIr, NodeShape, Unsupported};
use crate::lines::{take_id, Stmt};

/// Recognized-and-dropped directives: they style a diagram we render
/// with tokens, so dropping them changes no structure. Aborting the
/// whole diagram over one `click` line would be the worse trade.
const IGNORED_DIRECTIVES: [&str; 5] = ["classDef", "style", "click", "linkStyle", "class"];

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
    // `subgraph` groups are FLATTENED, not clustered: the layout
    // engine has no cluster contract, and a diagram that renders
    // without its boxes beats a diagram that does not render at all.
    // The grouping loss is a notice, and any edge that names a group
    // still finds a node carrying the group's title.
    let mut group_titles: Vec<(String, String)> = Vec::new();
    let mut depth = 0usize;
    for stmt in stmts {
        let first = stmt.text.split_whitespace().next().unwrap_or("");
        if first == "subgraph" {
            depth += 1;
            if let Some((id, title)) = subgraph_title(&stmt.text) {
                group_titles.push((id, title));
            } else {
                group_titles.push((String::new(), format!("#{}", group_titles.len() + 1)));
            }
            continue;
        }
        if first == "end" && depth > 0 {
            depth -= 1;
            continue;
        }
        // A group-local `direction` is a property of a box that is not
        // drawn: the flattening notice already covers it, and a second
        // line per group is noise a READER cannot act on.
        if first == "direction" && depth > 0 {
            continue;
        }
        parse_statement(&mut fc, stmt)?;
    }
    if depth > 0 {
        return Err(Unsupported::new(
            stmts.last().map(|s| s.line_no).unwrap_or(1),
            "subgraph",
            "a `subgraph` block is never closed by `end`",
        ));
    }
    if !group_titles.is_empty() {
        let names: Vec<&str> = group_titles.iter().map(|(_, t)| t.as_str()).collect();
        let (count, boxes) = match names.len() {
            1 => ("subgraph".to_string(), "the group box is"),
            n => (format!("{n} subgraphs"), "group boxes are"),
        };
        notice_once(
            &mut fc,
            format!(
                "{count} flattened ({}) — {boxes} not drawn",
                names.join(", ")
            ),
        );
    }
    // An edge may name a GROUP as its endpoint. Flattened, that group
    // is an ordinary node; give it the title the source wrote.
    for (id, title) in group_titles {
        if let Some(node) = fc.nodes.iter_mut().find(|n| n.id == id) {
            if node.text.is_none() {
                node.text = Some(title);
            }
        }
    }
    Ok(fc)
}

/// `subgraph one`, `subgraph one [Title]`, `subgraph "Title"` — the id
/// and the text to show for it.
fn subgraph_title(stmt: &str) -> Option<(String, String)> {
    let rest = stmt.strip_prefix("subgraph")?.trim();
    if rest.is_empty() {
        return None;
    }
    if let Some((id, bracket)) = rest.split_once('[') {
        let title = bracket.strip_suffix(']').unwrap_or(bracket);
        return Some((id.trim().to_string(), clean_label(title)));
    }
    let cleaned = clean_label(rest);
    Some((rest.trim_matches('"').to_string(), cleaned))
}

fn parse_statement(fc: &mut FlowchartIr, stmt: &Stmt) -> Result<(), Unsupported> {
    let s = stmt.text.as_str();
    let first_token = s.split_whitespace().next().unwrap_or("");

    if first_token == "direction" {
        fc.notices.push(format!(
            "per-subgraph `direction` ignored (line {})",
            stmt.line_no
        ));
        return Ok(());
    }
    if IGNORED_DIRECTIVES.contains(&first_token) {
        fc.notices.push(format!(
            "`{first_token}` directive ignored (line {})",
            stmt.line_no
        ));
        return Ok(());
    }
    // A statement is a chain: group ( link group )*
    let mut cursor = 0usize;
    let mut pending: Option<(Vec<String>, Link)> = None;
    loop {
        let link = next_link(s, cursor);
        let side_end = link.as_ref().map(|l| l.start).unwrap_or(s.len());
        let group = parse_group(fc, &s[cursor..side_end], stmt)?;
        if let Some((from, meta)) = pending.take() {
            for (a, b) in links_between(&from, &group) {
                fc.edges.push(FlowEdge {
                    from: a,
                    to: b,
                    label: meta.label.clone(),
                    kind: meta.kind,
                });
            }
            if let Some(note) = &meta.notice {
                notice_once(fc, format!("{note} (line {})", stmt.line_no));
            }
        }
        match link {
            None => return Ok(()),
            Some(l) => {
                cursor = l.end;
                pending = Some((group, l));
            }
        }
    }
}

/// Push a notice unless the same one is already there — one line per
/// KIND of degradation, not one per occurrence.
fn notice_once(fc: &mut FlowchartIr, note: String) {
    if !fc.notices.contains(&note) {
        fc.notices.push(note);
    }
}

/// The cross product of two `&`-groups: `A & B --> C & D` is four
/// edges, which is exactly what mermaid draws.
fn links_between(from: &[String], to: &[String]) -> Vec<(String, String)> {
    let mut out = Vec::with_capacity(from.len() * to.len());
    for a in from {
        for b in to {
            out.push((a.clone(), b.clone()));
        }
    }
    out
}

/// One side of a link: one node reference, or several joined by `&`.
fn parse_group(fc: &mut FlowchartIr, text: &str, stmt: &Stmt) -> Result<Vec<String>, Unsupported> {
    let mut ids = Vec::new();
    for part in split_top_level(text, '&') {
        let part = part.trim();
        if part.is_empty() {
            return Err(Unsupported::new(
                stmt.line_no,
                &stmt.text,
                "empty node reference in an `&` group",
            ));
        }
        let id = parse_noderef(fc, part).ok_or_else(|| {
            Unsupported::new(
                stmt.line_no,
                &stmt.text,
                format!("`{part}` is not a node reference"),
            )
        })?;
        ids.push(id);
    }
    if ids.is_empty() {
        return Err(Unsupported::new(
            stmt.line_no,
            &stmt.text,
            "statement has no node reference",
        ));
    }
    Ok(ids)
}

/// Split on `sep` at the TOP level — never inside quotes or brackets,
/// so `A["x & y"] & B` is two parts, not three.
fn split_top_level(s: &str, sep: char) -> Vec<&str> {
    let mut parts = Vec::new();
    let (mut start, mut depth, mut quoted) = (0usize, 0i32, false);
    for (i, c) in s.char_indices() {
        match c {
            '"' => quoted = !quoted,
            '[' | '(' | '{' if !quoted => depth += 1,
            ']' | ')' | '}' if !quoted => depth -= 1,
            c if c == sep && !quoted && depth <= 0 => {
                parts.push(&s[start..i]);
                start = i + c.len_utf8();
            }
            _ => {}
        }
    }
    parts.push(&s[start..]);
    parts
}

/// A link token: where it sits, what it means, and what it cost.
#[derive(Clone, Debug)]
struct Link {
    start: usize,
    end: usize,
    kind: EdgeKind,
    label: Option<String>,
    /// A labeled downgrade (`--x`, `--o`, `<-->` have no cell glyph).
    notice: Option<String>,
}

/// The first link at or after `from`, skipping quotes and brackets so
/// an arrow inside a label is text, not structure. (The old scanner
/// searched raw text, which made `A["a --> b"] --> B` unparseable
/// while `A --> B["a --> b"]` worked.)
fn next_link(s: &str, from: usize) -> Option<Link> {
    let bytes = s.as_bytes();
    let (mut i, mut depth, mut quoted) = (from, 0i32, false);
    while i < bytes.len() {
        match bytes[i] {
            b'"' => quoted = !quoted,
            b'[' | b'(' | b'{' if !quoted => depth += 1,
            b']' | b')' | b'}' if !quoted => depth -= 1,
            b'<' | b'-' | b'=' if !quoted && depth <= 0 => {
                if let Some(link) = scan_link(s, i) {
                    return Some(link);
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

/// Body character classes.
#[derive(Copy, Clone, PartialEq, Eq)]
enum Body {
    Dash,
    Thick,
    Dotted,
}

/// Parse a link starting at `at`, or `None` when these bytes are not
/// one (a lone `-` in a label, a `<` in text).
fn scan_link(s: &str, at: usize) -> Option<Link> {
    let bytes = s.as_bytes();
    let mut i = at;
    let mut notice = None;
    if bytes[i] == b'<' {
        // `<-->` / `<==>`: parsed, then rendered one-way with a notice.
        if !matches!(bytes.get(i + 1), Some(b'-') | Some(b'=')) {
            return None;
        }
        notice = Some("bidirectional links render as one-way arrows".to_string());
        i += 1;
    }
    let (body, mut end) = scan_body(s, i)?;
    let head = scan_head(s, end);
    if head.is_some() {
        end += 1;
    }
    let kind = match body.class {
        Body::Dash => EdgeKind::Arrow,
        Body::Thick => EdgeKind::Thick,
        Body::Dotted => EdgeKind::Dotted,
    };
    match head {
        Some(h) => {
            if h != b'>' {
                notice.get_or_insert(format!("`{}` arrowheads render as plain arrows", h as char));
            }
            let (label, end) = take_postfix_label(s, end);
            Some(Link {
                start: at,
                end,
                kind,
                label,
                notice,
            })
        }
        // No head. A long body is a complete OPEN link; a short one
        // opens a labelled link whose text runs to the closing body.
        None if body.complete => {
            let (label, end) = take_postfix_label(s, end);
            Some(Link {
                start: at,
                end,
                kind: if body.class == Body::Dotted {
                    EdgeKind::Dotted
                } else {
                    EdgeKind::Open
                },
                label,
                notice,
            })
        }
        None => scan_labelled(s, at, end, body.class, notice),
    }
}

struct BodyRun {
    class: Body,
    /// True when this body is a complete link on its own (`---`,
    /// `===`, `-.-`); false when it opens a labelled link (`--`, `==`,
    /// `-.`).
    complete: bool,
}

/// Consume a run of link body characters and classify it.
fn scan_body(s: &str, at: usize) -> Option<(BodyRun, usize)> {
    let bytes = s.as_bytes();
    let mut i = at;
    while i < bytes.len() && matches!(bytes[i], b'-' | b'=' | b'.') {
        i += 1;
    }
    let run = &s[at..i];
    if run.len() < 2 {
        return None;
    }
    let dashes = run.bytes().filter(|b| *b == b'-').count();
    let equals = run.bytes().filter(|b| *b == b'=').count();
    let dots = run.bytes().filter(|b| *b == b'.').count();
    let class = if dots > 0 {
        if dashes == 0 {
            return None; // a bare `..` is not a link
        }
        Body::Dotted
    } else if equals > 0 {
        if dashes > 0 {
            return None; // `-=-` is not a spelling
        }
        Body::Thick
    } else {
        Body::Dash
    };
    // `---`+ and `===`+ are complete; `-.-` is complete, `-.` is not.
    let complete = match class {
        Body::Dotted => run.ends_with('-'),
        _ => run.len() >= 3,
    };
    Some((BodyRun { class, complete }, i))
}

/// An arrowhead immediately after the body. `x`/`o` count only when
/// they do not start an identifier (`A --- xyz` is a link to `xyz`).
fn scan_head(s: &str, at: usize) -> Option<u8> {
    let bytes = s.as_bytes();
    let h = *bytes.get(at)?;
    if h == b'>' {
        return Some(h);
    }
    if h == b'x' || h == b'o' {
        let next = bytes.get(at + 1).copied();
        let starts_id = next.is_some_and(|c| c.is_ascii_alphanumeric() || c == b'_');
        if !starts_id {
            return Some(h);
        }
    }
    None
}

/// `A -->|yes| B`: the postfix label belongs to the link it follows.
/// An unterminated `|` is left alone — the group parser then names it,
/// which keeps one error path instead of two.
fn take_postfix_label(s: &str, end: usize) -> (Option<String>, usize) {
    let rest = &s[end..];
    let trimmed = rest.trim_start();
    let lead = rest.len() - trimmed.len();
    let Some(body) = trimmed.strip_prefix('|') else {
        return (None, end);
    };
    let Some(close) = body.find('|') else {
        return (None, end);
    };
    let text = body[..close].trim();
    if text.is_empty() {
        return (None, end);
    }
    (Some(clean_label(text)), end + lead + 1 + close + 1)
}

/// `A -- yes --> B`: the label runs from the opening body to the
/// closing one, and the closing body's head decides the direction.
fn scan_labelled(
    s: &str,
    start: usize,
    text_from: usize,
    class: Body,
    notice: Option<String>,
) -> Option<Link> {
    let bytes = s.as_bytes();
    let mut i = text_from;
    while i < bytes.len() {
        if matches!(bytes[i], b'-' | b'=' | b'.') {
            if let Some((body, end)) = scan_body(s, i) {
                if body.class != class {
                    return None; // `-- text ==>` is not a spelling
                }
                let mut end = end;
                let head = scan_head(s, end);
                if head.is_some() {
                    end += 1;
                }
                let label = s[text_from..i].trim();
                if label.is_empty() {
                    return None;
                }
                let mut notice = notice;
                if let Some(h) = head {
                    if h != b'>' {
                        notice.get_or_insert(format!(
                            "`{}` arrowheads render as plain arrows",
                            h as char
                        ));
                    }
                }
                return Some(Link {
                    start,
                    end,
                    kind: match (class, head) {
                        (Body::Dotted, _) => EdgeKind::Dotted,
                        (Body::Thick, _) => EdgeKind::Thick,
                        (Body::Dash, Some(_)) => EdgeKind::Arrow,
                        (Body::Dash, None) => EdgeKind::Open,
                    },
                    label: Some(clean_label(label)),
                    notice,
                });
            }
        }
        i += 1;
    }
    None
}

/// Parse a node reference (`id`, `id[text]`, `id(text)`, `id{text}`,
/// `id([text])`, `id((text))`, `id[[text]]`, `id[(text)]`,
/// `id{{text}}`, `id>text]`), registering the node.
fn parse_noderef(fc: &mut FlowchartIr, s: &str) -> Option<String> {
    let s = s.trim();
    let (id, rest) = take_id(s)?;
    let rest = rest.trim();
    let (shape, text) = if rest.is_empty() {
        (NodeShape::Plain, None)
    } else {
        let (shape, inner) = bracket_shape(rest)?;
        let raw = unquote(inner)?;
        if has_line_break(&raw) {
            notice_once(
                fc,
                "`<br/>` line breaks flattened — cards are one line".to_string(),
            );
        }
        (shape, Some(clean_label(&raw)))
    };
    register(fc, id, shape, text);
    Some(id.to_string())
}

/// The bracket spelling table, LONGEST delimiters first — every short
/// pair is a prefix of a longer one.
fn bracket_shape(rest: &str) -> Option<(NodeShape, &str)> {
    let pairs: [(&str, &str, NodeShape); 9] = [
        ("([", "])", NodeShape::Stadium),
        ("[[", "]]", NodeShape::Subroutine),
        ("[(", ")]", NodeShape::Cylinder),
        ("((", "))", NodeShape::Circle),
        ("{{", "}}", NodeShape::Hexagon),
        ("[", "]", NodeShape::Rect),
        ("(", ")", NodeShape::Rounded),
        ("{", "}", NodeShape::Diamond),
        (">", "]", NodeShape::Asymmetric),
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

/// Label text as the card will carry it: quotes stripped (they are
/// mermaid's escape, not content) and `<br/>` flattened to a space.
/// A card is one line today, so a line break becomes a word break —
/// visible in the label, never a literal `<br/>` on screen.
pub(crate) fn clean_label(text: &str) -> String {
    let text = text.trim();
    let text = if text.len() >= 2 && text.starts_with('"') && text.ends_with('"') {
        &text[1..text.len() - 1]
    } else {
        text
    };
    let mut out = text.to_string();
    for spelling in ["<br/>", "<br />", "<br>", "<BR/>", "<BR>"] {
        out = out.replace(spelling, " ");
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Did this label carry a line break the single-line card flattened?
pub(crate) fn has_line_break(text: &str) -> bool {
    ["<br/>", "<br />", "<br>", "<BR/>", "<BR>"]
        .iter()
        .any(|s| text.contains(s))
}

/// First-mention order; the LAST explicit shape/text declaration wins,
/// which is what mermaid renders when a node is mentioned twice.
fn register(fc: &mut FlowchartIr, id: &str, shape: NodeShape, text: Option<String>) {
    match fc.nodes.iter_mut().find(|n| n.id == id) {
        Some(node) => {
            if shape != NodeShape::Plain || text.is_some() {
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
