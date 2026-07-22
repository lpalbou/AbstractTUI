//! VERIFY cycle-6 text robustness: markdown-lite parser tolerance,
//! RichText span-preserving wrap, and highlighter determinism/no-panic.
//!
//! The invariants under test: the parser NEVER panics and NEVER errors
//! (its contract is "degrade to literal text"); wrapping preserves the
//! plain text and never lets one span's style bleed into another's
//! characters; the highlighter is deterministic and total (any input,
//! ascending non-overlapping ranges, no panic).

use abstracttui::base::Rgba;
use abstracttui::render::md::{self, Block, MdStyles};
use abstracttui::render::{RichLine, RichText, Span, Style};
use abstracttui::testing::{hostile_corpus, Rng};
use abstracttui::text::{self, CLikeLexer, Highlighter};

// ---------------------------------------------------------------------------
// Markdown-lite parser tolerance.
// ---------------------------------------------------------------------------

/// Adversarial markdown must never panic and always yield SOME parse
/// (the "always degrades to literal text" contract). Seeds cover the
/// hostile byte corpus plus markdown-shaped soup (unbalanced emphasis,
/// unclosed fences/links, pathological nesting).
#[test]
fn markdown_parser_never_panics_on_adversarial_input() {
    let styles = MdStyles::default();
    // 1) The shared hostile byte corpus (control bytes, truncated UTF-8,
    //    giant runs) — reinterpreted as markdown source.
    for chunk in hostile_corpus(0xC0DE_600D, 200) {
        let src = String::from_utf8_lossy(&chunk);
        let blocks = md::parse(&src, &styles);
        // Round-trip to rich text must not panic either.
        let _ = md::to_rich_text(&blocks, &styles);
    }
    // 2) Markdown-shaped adversarial strings.
    let cases = [
        "```rust\nno closing fence ever",
        "[unclosed link](http://",
        "[text](url) [and](another) ![img](x)",
        "***bold italic mixed** and *unbalanced",
        "> quote\n>> nested\n>>> deeper",
        &"#".repeat(500),
        &"> ".repeat(1000),
        &"- ".repeat(2000),
        &"    ".repeat(500),
        "`code with no close",
        "\u{0}\u{1b}[31m not really ansi",
        "|table|like|\n|---|---|\n|a|b|",
        "1. \n2. \n3. ",
        &format!("{}{}", "  ".repeat(300), "deeply indented"),
    ];
    for src in cases {
        let blocks = md::parse(src, &styles);
        let _ = md::to_rich_text(&blocks, &styles);
        // Never panics; block count is bounded by input size (no
        // amplification).
        assert!(blocks.len() <= src.len() + 1, "block explosion on {src:?}");
    }
}

/// Seeded markdown fuzz: assemble random documents from markdown tokens
/// and assert parse-then-render is total and bounded.
#[test]
fn markdown_seeded_fuzz_is_total_and_bounded() {
    let styles = MdStyles::default();
    let toks = [
        "# ", "## ", "- ", "1. ", "> ", "```", "`", "*", "**", "[", "]", "(", ")", "!", "text ",
        "\n", "\n\n", "    ", "\t", "---", "\u{1b}", "\u{0}",
    ];
    let mut rng = Rng::new(0x00AD_D0C5);
    for _ in 0..500 {
        let n = 1 + rng.below(60);
        let mut src = String::new();
        for _ in 0..n {
            src.push_str(toks[rng.below(toks.len())]);
        }
        let blocks = md::parse(&src, &styles);
        let rt = md::to_rich_text(&blocks, &styles);
        // Total: no panic. Bounded: fence bodies aside, blocks can't
        // exceed line count by much.
        assert!(rt.height() <= (src.matches('\n').count() as i32 + blocks.len() as i32 + 8));
    }
}

/// Unclosed code fence: the parser must still terminate and treat the
/// remaining lines as fence content, not loop or drop them.
#[test]
fn unclosed_fence_captures_remaining_lines() {
    let styles = MdStyles::default();
    let src = "before\n```rust\nfn main() {}\nlet x = 1;\n(no close)";
    let blocks = md::parse(src, &styles);
    let fence = blocks.iter().find_map(|b| match b {
        Block::CodeFence { lang, lines } => Some((lang.clone(), lines.clone())),
        _ => None,
    });
    let (lang, lines) = fence.expect("an unclosed fence still produces a CodeFence block");
    assert_eq!(lang, "rust");
    assert!(
        lines.iter().any(|l| l.contains("fn main")),
        "fence body captured: {lines:?}"
    );
    assert!(
        lines.iter().any(|l| l.contains("no close")),
        "trailing lines captured"
    );
}

// ---------------------------------------------------------------------------
// RichText span-preserving wrap.
// ---------------------------------------------------------------------------

fn styled_line() -> RichLine {
    let red = Style::new().fg(Rgba::rgb(220, 60, 60));
    let blue = Style::new().fg(Rgba::rgb(60, 60, 220));
    RichLine::from_spans(vec![
        Span::new("the quick ", Style::EMPTY),
        Span::new("brown fox ", red),
        Span::new("jumps over ", blue),
        Span::new("the lazy dog", Style::EMPTY),
    ])
}

/// Wrapping must PRESERVE the plain text exactly (concatenated across
/// wrapped lines) and never widen a line past the limit.
#[test]
fn wrap_preserves_plain_text_and_width_bound() {
    let rt = RichText::from_lines(vec![styled_line()]);
    let original: String = rt.lines.iter().map(|l| l.plain()).collect();
    for max in 1..=40 {
        let wrapped = rt.wrap(max);
        let joined: String = wrapped
            .lines
            .iter()
            .map(|l| l.plain())
            .collect::<Vec<_>>()
            .join("");
        // Wrap may insert breaks (dropping the break-point space), so
        // compare with spaces removed — no character is invented or lost.
        assert_eq!(
            joined.replace(' ', ""),
            original.replace(' ', ""),
            "wrap@{max} lost/gained characters"
        );
        for line in wrapped.lines.iter() {
            assert!(
                line.width() <= max,
                "wrap@{max}: line too wide: {:?}",
                line.plain()
            );
        }
    }
}

/// A span's characters keep their ORIGINAL style after wrapping — no
/// span cross-contamination. Each source word uses a DISJOINT alphabet
/// tied to a unique color, so any wrapped character maps unambiguously
/// back to the color its source span must carry.
#[test]
fn wrap_never_cross_contaminates_span_styles() {
    // Disjoint letter sets per color (no letter appears in two words).
    let spec: [(&str, Rgba); 4] = [
        ("aaaa ", Rgba::rgb(200, 0, 0)),
        ("bbbb ", Rgba::rgb(0, 200, 0)),
        ("cccc ", Rgba::rgb(0, 0, 200)),
        ("dddd eeee", Rgba::rgb(200, 200, 0)),
    ];
    let mut spans = Vec::new();
    let mut want: std::collections::HashMap<char, Rgba> = std::collections::HashMap::new();
    for (w, c) in spec {
        spans.push(Span::new(w, Style::new().fg(c)));
        for ch in w.chars().filter(|c| !c.is_whitespace()) {
            want.insert(ch, c);
        }
    }
    let rt = RichText::from_lines(vec![RichLine::from_spans(spans)]);
    for max in [3, 5, 7, 11, 20] {
        let wrapped = rt.wrap(max);
        for line in wrapped.lines.iter() {
            for span in line.spans.iter() {
                let fg = span.style.fg;
                for ch in span.text.chars().filter(|c| !c.is_whitespace()) {
                    let expected = want[&ch];
                    assert_eq!(
                        fg,
                        Some(expected),
                        "wrap@{max}: char {ch:?} carries the wrong span color"
                    );
                }
            }
        }
    }
}

/// Wrapping a wide-glyph run must never split a cluster across lines nor
/// exceed the width bound by the cluster's 2 columns.
#[test]
fn wrap_wide_glyphs_respect_cluster_and_width() {
    let line = RichLine::from_spans(vec![Span::plain("漢字テスト日本語ワイド")]);
    let rt = RichText::from_lines(vec![line]);
    for max in 2..=12 {
        let wrapped = rt.wrap(max);
        for l in wrapped.lines.iter() {
            assert!(l.width() <= max, "wide wrap@{max}: {} > {max}", l.width());
            // Each wrapped line's plain text must be whole clusters.
            let plain = l.plain();
            let reseg: String = text::segments(&plain).map(|s| s.cluster).collect();
            assert_eq!(reseg, plain, "wide wrap@{max} tore a cluster");
        }
    }
}

// ---------------------------------------------------------------------------
// Highlighter determinism + totality.
// ---------------------------------------------------------------------------

/// The lexer is deterministic (same input, same tokens) and total (any
/// input, ascending non-overlapping byte ranges, no panic).
#[test]
fn highlighter_is_deterministic_and_total() {
    let lex = CLikeLexer::rust();
    // Determinism + structural validity over the hostile corpus.
    for chunk in hostile_corpus(0x11CE_600D, 200) {
        let line = String::from_utf8_lossy(&chunk);
        // One line at a time (the lexer's documented unit).
        for l in line.split('\n') {
            let a = lex.spans(l);
            let b = lex.spans(l);
            assert_eq!(a, b, "lexer non-deterministic on {l:?}");
            // Ranges ascending, non-overlapping, in bounds.
            let mut prev_end = 0usize;
            for (r, _) in &a {
                assert!(
                    r.start >= prev_end,
                    "overlapping/out-of-order token on {l:?}"
                );
                assert!(r.end <= l.len(), "token past end of line on {l:?}");
                assert!(r.start <= r.end, "inverted range on {l:?}");
                // Ranges must land on char boundaries (no cluster tears).
                assert!(
                    l.is_char_boundary(r.start) && l.is_char_boundary(r.end),
                    "non-boundary token on {l:?}"
                );
                prev_end = r.end;
            }
        }
    }
}

/// Seeded code-like fuzz: random mixes of keywords, strings, comments,
/// numbers, punctuation — no panic, tokens always valid.
#[test]
fn highlighter_seeded_code_fuzz() {
    let lex = CLikeLexer::rust();
    let toks = [
        "fn ",
        "let ",
        "mut ",
        "struct ",
        "// comment",
        "/* block",
        "*/",
        "\"str",
        "\"closed\"",
        "'c'",
        "0x1F",
        "3.14",
        "_ident",
        "foo",
        "+",
        "-",
        "->",
        "{",
        "}",
        "(",
        ")",
        ";",
        "\\",
        "é",
        "漢",
    ];
    let mut rng = Rng::new(0x00C0_DEF0);
    for _ in 0..500 {
        let n = 1 + rng.below(40);
        let mut line = String::new();
        for _ in 0..n {
            line.push_str(toks[rng.below(toks.len())]);
        }
        let spans = lex.spans(&line);
        let mut prev_end = 0usize;
        for (r, _) in &spans {
            assert!(r.start >= prev_end && r.end <= line.len() && r.start <= r.end);
            assert!(line.is_char_boundary(r.start) && line.is_char_boundary(r.end));
            prev_end = r.end;
        }
    }
}

/// The RichLine bridge from highlighted tokens must preserve the source
/// text exactly (tinting is a style overlay, never a text rewrite).
#[test]
fn from_highlighted_preserves_source_text() {
    let lex = CLikeLexer::rust();
    let styles = |_k: text::TokenKind| Style::new().attrs(abstracttui::render::Attrs::BOLD);
    let base = Style::EMPTY;
    for src in [
        "fn main() { let x = 42; }",
        "// just a comment",
        "\"unterminated string",
        "",
        "漢字 // wide then comment",
    ] {
        let line = RichLine::from_highlighted(src, &lex, base, styles);
        assert_eq!(line.plain(), src, "highlight bridge altered the text");
    }
}

// ---------------------------------------------------------------------------
// Diff lexer (backlog 0140, additive slice) — the downstream view.
// ---------------------------------------------------------------------------

/// From a foreign crate (this test target), the diff vocabulary works
/// with the documented downstream idiom: `DiffKind` is
/// `#[non_exhaustive]`, so a foreign `match` carries a `_` arm mapping
/// unknown future kinds to body text (never invisible). This test is
/// the compile-time proof the contract holds (ADR-0003 §4).
#[test]
fn diff_lexer_public_api_and_non_exhaustive_idiom() {
    use abstracttui::text::{DiffKind, DiffLexer};

    let lexer = DiffLexer::new();
    let classify = |line: &str| lexer.spans(line).first().map(|(_, k)| *k);
    assert_eq!(classify("+added"), Some(DiffKind::Added));
    assert_eq!(classify("-removed"), Some(DiffKind::Removed));
    assert_eq!(classify("@@ -1 +1 @@"), Some(DiffKind::HunkHeader));
    assert_eq!(classify("--- a/f"), Some(DiffKind::FileHeader));
    assert_eq!(
        classify("\\ No newline at end of file"),
        Some(DiffKind::Meta)
    );
    assert_eq!(classify(" ctx"), Some(DiffKind::Context));

    // The downstream mapping idiom: `_` catches kinds this crate may
    // grow, rendered as body text — the arm this match REQUIRES to
    // compile is the non_exhaustive proof.
    let label = |k: DiffKind| match k {
        DiffKind::Added => "add",
        DiffKind::Removed => "del",
        _ => "body",
    };
    assert_eq!(label(DiffKind::Added), "add");
    assert_eq!(label(DiffKind::Context), "body");

    // Totality over the shared hostile byte corpus (same bar as the
    // C-like lexer above): valid ascending char-boundary ranges, no
    // panic, on lossy-decoded hostile bytes reinterpreted as diff lines.
    for chunk in hostile_corpus(0xD1FF_600D, 200) {
        let src = String::from_utf8_lossy(&chunk);
        for line in src.lines() {
            let mut prev_end = 0usize;
            for (r, _) in lexer.spans(line) {
                assert!(r.start >= prev_end && r.end <= line.len() && r.start <= r.end);
                assert!(line.is_char_boundary(r.start) && line.is_char_boundary(r.end));
                prev_end = r.end;
            }
        }
    }
}
