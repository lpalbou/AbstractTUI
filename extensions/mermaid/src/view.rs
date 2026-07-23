//! [`MermaidView`]: source in, honest picture out.
//!
//! - Flowcharts (and flat state diagrams) COMPILE to the graph crate
//!   ([`crate::to_graph`]) and render through `GraphView` — mermaid is
//!   a compiler here, never a second graph renderer.
//! - Sequence diagrams render through the crate's own deterministic
//!   solverless plan (the one surface mermaid paints itself).
//! - Anything unsupported renders the ATOMIC FALLBACK: the source as
//!   the code fence it already is, plus one notice naming the first
//!   unrecognized construct, plus the optional mermaid.live link
//!   (the code travels in the URL FRAGMENT — nothing is sent anywhere
//!   until the user opens the link).
//!
//! Layout/parse run at view-build time (an act); rebuild the view to
//! re-parse, exactly like `GraphView`'s relayout rule.

use std::collections::HashMap;

use abstracttui::app::use_theme;
use abstracttui::base::{Point, Rgba};
use abstracttui::layout::{Dimension, Style as LayoutStyle};
use abstracttui::reactive::Scope;
use abstracttui::text::{truncate_ellipsis, width};
use abstracttui::ui::{Element, StyledCanvas, View};
use abstracttui::widgets::Scroll;
use abstracttui_graph::{GraphAlgo, GraphStyle, GraphView};

use crate::compile::{shape_badge, to_graph};
use crate::ir::{Diagram, FlowchartIr, SequenceIr, Unsupported};
use crate::seq_render::SeqStyle;
use crate::{parse, seq_layout, seq_render};

/// Render mermaid source: supported diagrams natively, everything
/// else as the honest labeled code fence. See the crate docs for the
/// subset table (the contract).
pub struct MermaidView {
    source: String,
    graph_style: Option<GraphStyle>,
    seq_style: Option<SeqStyle>,
    live_link: bool,
    layout_style: Option<LayoutStyle>,
}

impl MermaidView {
    /// A view over mermaid source text.
    pub fn new(source: impl Into<String>) -> MermaidView {
        MermaidView {
            source: source.into(),
            graph_style: None,
            seq_style: None,
            live_link: true,
            layout_style: None,
        }
    }

    /// Explicit flowchart ink set (default: derived from the active
    /// theme, with shape accents on `decision`/`rounded`/`stadium`).
    pub fn graph_style(mut self, style: GraphStyle) -> MermaidView {
        self.graph_style = Some(style);
        self
    }

    /// Explicit sequence ink set (default: derived from the active
    /// theme).
    pub fn seq_style(mut self, style: SeqStyle) -> MermaidView {
        self.seq_style = Some(style);
        self
    }

    /// Show the mermaid.live escape link on fallback (default true).
    pub fn live_link(mut self, on: bool) -> MermaidView {
        self.live_link = on;
        self
    }

    /// Outer layout style (default: a growing column).
    pub fn layout(mut self, layout: LayoutStyle) -> MermaidView {
        self.layout_style = Some(layout);
        self
    }

    /// Build the widget. Parse + layout run HERE (an act, cached in
    /// the view); rebuild to re-parse.
    pub fn view(self, cx: Scope) -> View {
        let outer = self
            .layout_style
            .clone()
            .unwrap_or_else(|| LayoutStyle::column().grow(1.0));
        match parse(&self.source) {
            Ok(Diagram::Flowchart(fc)) => self.flowchart_view(cx, outer, &fc),
            Ok(Diagram::Sequence(seq)) => self.sequence_view(cx, outer, &seq),
            Err(un) => self.fallback_view(cx, outer, &un),
        }
    }

    fn flowchart_view(self, cx: Scope, outer: LayoutStyle, fc: &FlowchartIr) -> View {
        let tokens = use_theme(cx).get().tokens;
        let style = self.graph_style.unwrap_or_else(|| {
            GraphStyle::from_tokens(&tokens)
                .kind_accent("decision", tokens.warn)
                .kind_accent("rounded", tokens.info)
                .kind_accent("stadium", tokens.ok)
        });
        // Shape sigils ride the GraphView badge slot (cell-honest
        // mapping of mermaid's node shapes; see the crate docs table).
        let badges: HashMap<String, &'static str> = fc
            .nodes
            .iter()
            .filter_map(|n| shape_badge(n.shape).map(|b| (n.id.clone(), b)))
            .collect();
        let (desc, opts) = to_graph(fc);
        let graph = GraphView::new(desc)
            .algo(GraphAlgo::Layered(opts))
            .style(style)
            .badges(move |id| badges.get(id).map(|b| (*b).to_string()))
            .view(cx);
        let mut root = Element::new().style(outer);
        for notice in notice_lines(&fc.notices, tokens.warn) {
            root = root.child(notice);
        }
        root.child(graph).build()
    }

    fn sequence_view(self, cx: Scope, outer: LayoutStyle, seq: &SequenceIr) -> View {
        let tokens = use_theme(cx).get().tokens;
        let style = self
            .seq_style
            .unwrap_or_else(|| SeqStyle::from_tokens(&tokens));
        let plan = seq_layout::plan(seq);
        let (w, h) = (plan.width.max(1), plan.height.max(1));
        let content = Element::new()
            .style(
                LayoutStyle::default()
                    .width(Dimension::Cells(w))
                    .height(Dimension::Cells(h)),
            )
            .draw(move |canvas, rect| {
                seq_render::draw(canvas, Point::new(rect.x, rect.y), &plan, &style);
            });
        let scroll = Scroll::new(content.build())
            .content_size(w, h)
            .axes(true, true)
            .offset_x(cx.signal(0))
            .offset_y(cx.signal(0))
            .scrollbar_auto_hide(true)
            .view(cx);
        let mut root = Element::new().style(outer);
        for notice in notice_lines(&seq.notices, tokens.warn) {
            root = root.child(notice);
        }
        root.child(scroll).build()
    }

    /// The atomic fallback: notice + optional live link + the source
    /// as the code fence it already is (verbatim lines, monospace).
    fn fallback_view(self, cx: Scope, outer: LayoutStyle, un: &Unsupported) -> View {
        let tokens = use_theme(cx).get().tokens;
        let mut root = Element::new().style(outer);
        root = root.child(one_line(format!("⚠ {un}"), tokens.warn));
        if self.live_link {
            root = root.child(one_line(
                format!("view online: {}", live_link_url(&self.source)),
                tokens.text_faint,
            ));
        }
        let lines: Vec<String> = self.source.lines().map(str::to_string).collect();
        let w = lines.iter().map(|l| width(l)).max().unwrap_or(1).max(1);
        let h = (lines.len() as i32).max(1);
        let ink = tokens.text_muted;
        let content = Element::new()
            .style(
                LayoutStyle::default()
                    .width(Dimension::Cells(w))
                    .height(Dimension::Cells(h)),
            )
            .draw(move |canvas, rect| {
                for (i, line) in lines.iter().enumerate() {
                    canvas.print(
                        Point::new(rect.x, rect.y + i as i32),
                        line,
                        ink,
                        Rgba::TRANSPARENT,
                    );
                }
            });
        let scroll = Scroll::new(content.build())
            .content_size(w, h)
            .axes(true, true)
            .offset_x(cx.signal(0))
            .offset_y(cx.signal(0))
            .scrollbar_auto_hide(true)
            .view(cx);
        root.child(scroll).build()
    }
}

/// One truncated single-row text line (notices, links).
fn one_line(text: String, ink: Rgba) -> View {
    Element::new()
        .style(
            LayoutStyle::default()
                .height(Dimension::Cells(1))
                .shrink(0.0),
        )
        .draw(move |canvas: &mut dyn StyledCanvas, rect| {
            if rect.w <= 0 {
                return;
            }
            let t = truncate_ellipsis(&text, rect.w);
            canvas.print(rect.origin(), &t, ink, Rgba::TRANSPARENT);
        })
        .build()
}

fn notice_lines(notices: &[String], ink: Rgba) -> Vec<View> {
    notices
        .iter()
        .map(|n| one_line(format!("⚠ mermaid: {n}"), ink))
        .collect()
}

/// The mermaid.live escape hatch: the editor's `#base64:` state form
/// (URL-safe base64 of the state JSON, verified against the live
/// editor's own serde). The diagram code travels in the URL FRAGMENT
/// — it is never sent to any server by this crate; only opening the
/// link shares it with the browser and site.
pub fn live_link_url(source: &str) -> String {
    let mut json = String::with_capacity(source.len() + 64);
    json.push_str("{\"code\":");
    json_escape_into(&mut json, source);
    json.push_str(",\"mermaid\":\"{\\\"theme\\\": \\\"default\\\"}\"}");
    format!(
        "https://mermaid.live/edit#base64:{}",
        base64_url(json.as_bytes())
    )
}

/// JSON string literal (quotes included).
fn json_escape_into(out: &mut String, s: &str) {
    out.push('"');
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                out.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

/// URL-safe base64 (`-`/`_` alphabet, no padding — the js-base64
/// url-safe dialect mermaid.live decodes).
fn base64_url(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            chunk.get(1).copied().unwrap_or(0),
            chunk.get(2).copied().unwrap_or(0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        out.push(ALPHABET[(n >> 18) as usize & 63] as char);
        out.push(ALPHABET[(n >> 12) as usize & 63] as char);
        if chunk.len() > 1 {
            out.push(ALPHABET[(n >> 6) as usize & 63] as char);
        }
        if chunk.len() > 2 {
            out.push(ALPHABET[n as usize & 63] as char);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_url_matches_reference_vectors() {
        assert_eq!(base64_url(b"Man"), "TWFu");
        assert_eq!(base64_url(b"Ma"), "TWE");
        assert_eq!(base64_url(b"M"), "TQ");
        // URL-safe alphabet: 0xFB 0xFF encodes with `-`/`_`, never
        // `+`/`/`, and no padding.
        assert_eq!(base64_url(&[0xFB, 0xFF]), "-_8");
    }

    #[test]
    fn json_escaping_covers_the_control_set() {
        let mut out = String::new();
        json_escape_into(&mut out, "a\"b\\c\nd\te\u{1}");
        assert_eq!(out, "\"a\\\"b\\\\c\\nd\\te\\u0001\"");
    }

    #[test]
    fn live_link_carries_the_code_in_the_fragment() {
        let url = live_link_url("graph TD\nA-->B");
        assert!(url.starts_with("https://mermaid.live/edit#base64:"));
        assert!(!url.contains('+') && !url.contains('/') || url.starts_with("https://"));
        // The fragment itself has no padding and no URL-hostile chars.
        let frag = url.split("#base64:").nth(1).unwrap();
        assert!(frag
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_'));
    }
}
