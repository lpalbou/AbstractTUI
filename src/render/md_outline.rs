//! Heading outline + anchor slugs (0146): `outline(source)` extracts
//! every heading with a GitHub-compatible `anchor_id`, so readers can
//! build TOC panels and resolve `[text](#anchor)` links.
//!
//! Row positions are deliberately NOT computed here: which visual row a
//! heading starts at depends on the typeset fold (wrap width, spacing
//! policy, the level-1 underline), which is the widget layer's — see
//! `MarkdownView::outline_rows` for the width-resolved form. This
//! module owns the SOURCE-level facts: order, level, text, anchor.
//!
//! ## Slug algorithm (documented deviations)
//!
//! GitHub's anchor rule, approximated with std-only Unicode: lowercase
//! everything; keep alphanumerics (Unicode letters/digits) and `_`;
//! map spaces to `-`; keep existing `-`; DROP everything else
//! (punctuation, emoji, symbols); duplicate slugs within one document
//! get `-1`, `-2`, … suffixes in reading order. Known deviation:
//! COMBINING MARKS are dropped (std has no Unicode-category-M test),
//! so decomposed accents slug differently from GitHub ("e\u{301}" →
//! "e"); precomposed accents ("é") match GitHub exactly.

use super::{parse_doc, Block, DocBlock, MdStyles};

/// One outline entry, in reading order.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Heading {
    /// Hash count, 1..=6.
    pub level: u8,
    /// The heading's plain text (inline markers resolved).
    pub text: String,
    /// GitHub-compatible anchor id, deduplicated within the document
    /// (`intro`, `intro-1`, `intro-2`, …). Match `[x](#anchor)` link
    /// targets against this (the leading `#` stripped).
    pub anchor_id: String,
}

/// Extracts the document outline: every heading, in order, with level,
/// plain text and a deduplicated anchor id.
///
/// ```
/// use abstracttui::render::md;
///
/// let doc = "# Intro\n\n## Setup\n\n## Setup\n";
/// let toc = md::outline(doc);
/// assert_eq!(toc.len(), 3);
/// assert_eq!(toc[1].anchor_id, "setup");
/// assert_eq!(toc[2].anchor_id, "setup-1", "duplicates suffix in order");
/// ```
pub fn outline(source: &str) -> Vec<Heading> {
    // Styles do not affect heading TEXT (plain() strips styling), so
    // the default set serves every caller.
    let styles = MdStyles::default();
    let mut used: Vec<String> = Vec::new();
    let mut out = Vec::new();
    for block in parse_doc(source, &styles) {
        let DocBlock::Core(Block::Heading { level, content }) = block else {
            continue;
        };
        let text = content.plain();
        let base = slugify(&text);
        let anchor_id = dedup(&base, &mut used);
        out.push(Heading {
            level,
            text,
            anchor_id,
        });
    }
    out
}

/// The single-heading slug (no dedup — [`outline`] applies suffixes).
/// See the module docs for the exact algorithm and deviations.
///
/// ```
/// use abstracttui::render::md::slugify;
/// assert_eq!(slugify("Hello, World!"), "hello-world");
/// assert_eq!(slugify("café società"), "café-società");
/// ```
pub fn slugify(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        if c.is_alphanumeric() || c == '_' {
            // char::to_lowercase may expand (ẞ → ss); extend fully.
            out.extend(c.to_lowercase());
        } else if c == ' ' || c == '-' {
            out.push('-');
        }
        // Everything else (punctuation, symbols, emoji, controls,
        // combining marks) drops — GitHub's rule, minus marks.
    }
    out
}

/// Deduplicate against `used`, GitHub-style: first occurrence keeps the
/// base, later ones append `-1`, `-2`, … (probing skips suffixes that
/// are themselves taken by literal headings).
fn dedup(base: &str, used: &mut Vec<String>) -> String {
    let mut candidate = base.to_string();
    let mut n = 0usize;
    while used.iter().any(|u| u == &candidate) {
        n += 1;
        candidate = format!("{base}-{n}");
    }
    used.push(candidate.clone());
    candidate
}

#[cfg(test)]
#[path = "md_outline_tests.rs"]
mod tests;
