//! Outline + slug tests: the GitHub-compatibility golden table
//! (unicode, punctuation, dedup), outline extraction order, and
//! hostile-input fuzz.

use super::*;
use crate::testing::hostile_corpus;

/// The slug golden table. Every row is (heading text, expected slug),
/// checked against GitHub's rendered anchors — except the documented
/// combining-marks deviation (last rows).
#[test]
fn slug_golden_table() {
    let cases: &[(&str, &str)] = &[
        ("Hello", "hello"),
        ("Hello, World!", "hello-world"),
        ("Getting Started", "getting-started"),
        ("multiple   spaces", "multiple---spaces"),
        ("Already-dashed", "already-dashed"),
        ("under_score kept", "under_score-kept"),
        ("Trailing punctuation?!", "trailing-punctuation"),
        ("`code` in heading", "code-in-heading"),
        ("100% coverage", "100-coverage"),
        ("v1.2.3", "v123"),
        ("café society", "café-society"),
        ("Über uns", "über-uns"),
        ("日本語の見出し", "日本語の見出し"),
        ("Mixed 日本語 and English", "mixed-日本語-and-english"),
        ("emoji 🎉 party", "emoji--party"),
        ("ẞharp", "ßharp"),
        ("İstanbul", "i\u{307}stanbul"), // one-to-many lowercase expands
        ("", ""),
        ("---", "---"),
        ("a — em dash", "a--em-dash"),
        // Documented deviation: combining marks drop (GitHub keeps
        // them); precomposed accents above match GitHub exactly.
        ("e\u{301}tude", "etude"),
    ];
    for (text, want) in cases {
        assert_eq!(&slugify(text), want, "slugify({text:?})");
    }
}

#[test]
fn outline_extracts_levels_text_and_dedups_anchors() {
    let doc = "\
# Intro

body

## Setup

### Setup

## Setup

## setup

# **Bold** title `code`
";
    let toc = outline(doc);
    let got: Vec<(u8, &str, &str)> = toc
        .iter()
        .map(|h| (h.level, h.text.as_str(), h.anchor_id.as_str()))
        .collect();
    assert_eq!(
        got,
        vec![
            (1, "Intro", "intro"),
            (2, "Setup", "setup"),
            (3, "Setup", "setup-1"),
            (2, "Setup", "setup-2"),
            (2, "setup", "setup-3"),
            (1, "Bold title code", "bold-title-code"),
        ]
    );
}

#[test]
fn dedup_probing_skips_literal_collisions() {
    // A literal "setup-1" heading occupies the suffix a duplicate
    // "setup" would want; the dedup must probe past it.
    let doc = "## Setup\n\n## Setup 1\n\n## Setup\n";
    let toc = outline(doc);
    let ids: Vec<&str> = toc.iter().map(|h| h.anchor_id.as_str()).collect();
    assert_eq!(ids, vec!["setup", "setup-1", "setup-2"]);
    // All ids unique, always.
    let mut sorted = ids.clone();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(sorted.len(), ids.len());
}

#[test]
fn headings_inside_fences_and_tables_do_not_appear() {
    let doc = "# Real\n\n```\n# not a heading\n```\n\n| # cell |\n|---|\n| # row |\n";
    let toc = outline(doc);
    assert_eq!(toc.len(), 1);
    assert_eq!(toc[0].anchor_id, "real");
}

#[test]
fn outline_survives_hostile_corpus() {
    for chunk in hostile_corpus(0x0146, 250) {
        let s = String::from_utf8_lossy(&chunk);
        let toc = outline(&s);
        // Ids stay unique on every input.
        let mut ids: Vec<&str> = toc.iter().map(|h| h.anchor_id.as_str()).collect();
        let before = ids.len();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), before, "duplicate anchor ids on {s:?}");
        let _ = slugify(&s);
    }
}
