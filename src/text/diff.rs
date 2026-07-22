//! Unified-diff line classification: the diff half of backlog 0140.
//!
//! Diff is LINE-oriented — the meaning of a line is carried by its first
//! bytes (`+`/`-`/`@@`/headers), not by a token grammar — so the lexer
//! classifies whole lines instead of riding [`super::Highlighter`]'s
//! `TokenKind` vocabulary (inserted/deleted/hunk-header are not token
//! kinds a C-like mapping can express, and `TokenKind` is a public
//! exhaustive enum frozen until the 0.3 window — see
//! `docs/backlog/planned/0002_the_0_3_breaking_budget.md`). [`DiffKind`]
//! is the dedicated, additive vocabulary; consumers map it to theme inks
//! in ONE place (`widgets::code::diff_token_color`), exactly like
//! `code_token_color` does for `TokenKind`.
//!
//! Honest limits (documented, not hidden — "approximate by design, never
//! a language authority", the module rule from `highlight.rs`):
//!
//! - Classification is STATELESS per line, deliberately: `CodeView`
//!   renders from a scroll offset (lines above the viewport are never
//!   seen), so a cross-line state machine would tint the same line
//!   differently depending on scroll position. Stateless costs one known
//!   ambiguity, resolved the way classic diff highlighters (vim, git)
//!   resolve it: a removed line whose content begins `-- ` renders as a
//!   file header (`--- ` wins), and symmetrically for `++ `.
//! - Lines that match no diff shape (email prose around a format-patch,
//!   binary-patch base85 bodies) classify as [`DiffKind::Context`] —
//!   untinted, never mis-tinted.
//! - Totality: any `&str` line classifies without panicking; all span
//!   boundaries are ASCII-derived, so ranges always sit on char
//!   boundaries.

use std::ops::Range;

/// Theme-agnostic diff line classes. Coarse on purpose, like
/// [`super::TokenKind`]: six shapes a theme can color without knowing
/// any diff dialect.
///
/// `#[non_exhaustive]`: this vocabulary may grow (word-diff runs,
/// combined-diff columns) — per ADR-0003 §3, enums the engine may grow
/// are born non-exhaustive. Downstream `match`es carry a `_` arm and
/// should render unknown kinds as body text (never invisible).
#[non_exhaustive]
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum DiffKind {
    /// An inserted line (`+` body line).
    Added,
    /// A deleted line (`-` body line).
    Removed,
    /// The `@@ -l,c +l,c @@` hunk range (the trailing function context
    /// after the closing `@@` is a separate [`DiffKind::Context`] span).
    HunkHeader,
    /// `--- a/path` / `+++ b/path` file headers.
    FileHeader,
    /// File-level chrome between hunks: `diff --git`, `index`, mode and
    /// rename lines, `Binary files … differ`, and the
    /// `\ No newline at end of file` marker.
    Meta,
    /// An unchanged body line (space-led), and any line that matches no
    /// diff shape — rendered in the consumer's base style.
    Context,
}

/// Git/diff file-level chrome that is neither a `---`/`+++` header nor
/// hunk body. Matched as PREFIXES against lines whose first byte already
/// rules out body lines (body lines start with ` `, `+`, `-`, or `\`).
/// Anything else that matches none of these is prose → `Context`.
const META_PREFIXES: &[&str] = &[
    "diff ",
    "index ",
    "old mode",
    "new mode",
    "new file mode",
    "deleted file mode",
    "copy from ",
    "copy to ",
    "rename from ",
    "rename to ",
    "similarity index ",
    "dissimilarity index ",
    "Binary files ",
    "GIT binary patch",
    "Only in ",
];

/// The unified-diff line lexer. Stateless and `Copy`-cheap; a struct
/// (not free functions) so per-instance configuration (word-diff,
/// combined-diff columns) can arrive additively later.
#[derive(Copy, Clone, Debug, Default)]
pub struct DiffLexer;

impl DiffLexer {
    /// A diff lexer with the default (unified-diff) rules.
    pub fn new() -> DiffLexer {
        DiffLexer
    }

    /// True when a markdown fence / code-view language label names a
    /// diff (`diff`, `patch`, `udiff` — first word, case-insensitive).
    pub fn matches_lang(label: &str) -> bool {
        let first = label.split_whitespace().next().unwrap_or("");
        first.eq_ignore_ascii_case("diff")
            || first.eq_ignore_ascii_case("patch")
            || first.eq_ignore_ascii_case("udiff")
    }

    /// Classifies one line: non-overlapping, ascending byte ranges with
    /// kinds — the same span contract as [`super::Highlighter::spans`].
    /// Whole lines carry one span; hunk headers split into the
    /// `@@ … @@` range plus a trailing `Context` span for the function
    /// context git appends. Empty lines carry no span (base style).
    pub fn spans(&self, line: &str) -> Vec<(Range<usize>, DiffKind)> {
        if line.is_empty() {
            return Vec::new();
        }
        let whole = |kind: DiffKind| vec![(0..line.len(), kind)];
        let b = line.as_bytes();
        // Hunk range: `@@ -l,c +l,c @@ fn context()`. The closing `@@`
        // bounds the header span; the tail (function context) stays
        // Context so long lines don't become a wall of hunk ink.
        if line.starts_with("@@") {
            match find_close(b) {
                Some(end) if end < line.len() => {
                    return vec![
                        (0..end, DiffKind::HunkHeader),
                        (end..line.len(), DiffKind::Context),
                    ];
                }
                _ => return whole(DiffKind::HunkHeader),
            }
        }
        // File headers BEFORE the +/- body rules (`--- a/f` would
        // otherwise read as a removed line). Requires the separator: a
        // bare `---` is a removed `--` body line, not a header.
        if line.starts_with("--- ") || line.starts_with("---\t") {
            return whole(DiffKind::FileHeader);
        }
        if line.starts_with("+++ ") || line.starts_with("+++\t") {
            return whole(DiffKind::FileHeader);
        }
        match b[0] {
            b'+' => whole(DiffKind::Added),
            b'-' => whole(DiffKind::Removed),
            b' ' => whole(DiffKind::Context),
            // `\ No newline at end of file` (and any `\`-led marker).
            b'\\' => whole(DiffKind::Meta),
            _ => {
                if META_PREFIXES.iter().any(|p| line.starts_with(p)) {
                    whole(DiffKind::Meta)
                } else {
                    // Prose (commit message, email wrapper): untinted.
                    whole(DiffKind::Context)
                }
            }
        }
    }
}

/// Byte offset just past the closing `@@` of a hunk header (searching
/// from index 2, past the opening pair). ASCII positions only, so the
/// result is always a char boundary.
fn find_close(b: &[u8]) -> Option<usize> {
    b.get(2..)?
        .windows(2)
        .position(|w| w == b"@@")
        .map(|p| p + 4)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kind_of(line: &str) -> DiffKind {
        let spans = DiffLexer::new().spans(line);
        assert_eq!(
            spans.len(),
            1,
            "expected one whole-line span for {line:?}: {spans:?}"
        );
        assert_eq!(spans[0].0, 0..line.len(), "whole line for {line:?}");
        spans[0].1
    }

    #[test]
    fn body_lines_classify_by_first_byte() {
        assert_eq!(kind_of("+let x = 1;"), DiffKind::Added);
        assert_eq!(kind_of("-let x = 0;"), DiffKind::Removed);
        assert_eq!(kind_of(" unchanged"), DiffKind::Context);
        // A bare `+`/`-` (empty added/removed line) is body, not header.
        assert_eq!(kind_of("+"), DiffKind::Added);
        assert_eq!(kind_of("-"), DiffKind::Removed);
    }

    #[test]
    fn headers_and_meta_lines() {
        assert_eq!(kind_of("--- a/src/main.rs"), DiffKind::FileHeader);
        assert_eq!(kind_of("+++ b/src/main.rs"), DiffKind::FileHeader);
        // POSIX diff separates with a tab.
        assert_eq!(
            kind_of("--- lao\t2002-02-21 23:30:39"),
            DiffKind::FileHeader
        );
        assert_eq!(kind_of("diff --git a/x b/x"), DiffKind::Meta);
        assert_eq!(kind_of("index 83db48f..bf3a1a5 100644"), DiffKind::Meta);
        assert_eq!(kind_of("new file mode 100644"), DiffKind::Meta);
        assert_eq!(kind_of("rename from old.rs"), DiffKind::Meta);
        assert_eq!(
            kind_of("Binary files a/i.png and b/i.png differ"),
            DiffKind::Meta
        );
        // Prose around a format-patch stays untinted.
        assert_eq!(kind_of("Subject: [PATCH] fix the thing"), DiffKind::Context);
        assert_eq!(kind_of("fix the thing"), DiffKind::Context);
    }

    #[test]
    fn no_newline_at_eof_marker_is_meta() {
        assert_eq!(kind_of("\\ No newline at end of file"), DiffKind::Meta);
    }

    #[test]
    fn hunk_header_splits_trailing_function_context() {
        let line = "@@ -1,3 +1,4 @@ fn main() {";
        let spans = DiffLexer::new().spans(line);
        assert_eq!(
            spans,
            vec![
                (0..15, DiffKind::HunkHeader),
                (15..line.len(), DiffKind::Context),
            ]
        );
        assert_eq!(&line[0..15], "@@ -1,3 +1,4 @@");
        // No trailing context: one whole-line span.
        assert_eq!(kind_of("@@ -1,3 +1,4 @@"), DiffKind::HunkHeader);
        // Unclosed `@@`: honest whole-line header, never a panic.
        assert_eq!(kind_of("@@ -1,3 +1,4"), DiffKind::HunkHeader);
    }

    #[test]
    fn documented_ambiguities_resolve_header_first() {
        // A removed line whose content starts `-- ` is indistinguishable
        // from a file header statelessly; header wins (vim/git rule).
        assert_eq!(kind_of("--- struck-through prose"), DiffKind::FileHeader);
        // But a bare `---` (removed `--` line, markdown rules in diffs)
        // requires the separator to be a header, so it stays Removed.
        assert_eq!(kind_of("---"), DiffKind::Removed);
        assert_eq!(kind_of("+++"), DiffKind::Added);
    }

    #[test]
    fn lang_labels_route_diff_and_nothing_else() {
        for yes in ["diff", "Diff", "DIFF", "patch", "udiff", "diff --git"] {
            assert!(DiffLexer::matches_lang(yes), "{yes:?}");
        }
        for no in ["", "rust", "d", "diffs", "patchwork", "c"] {
            assert!(!DiffLexer::matches_lang(no), "{no:?}");
        }
    }

    #[test]
    fn empty_line_carries_no_span() {
        assert!(DiffLexer::new().spans("").is_empty());
    }

    #[test]
    fn non_ascii_content_slices_on_char_boundaries() {
        let lexer = DiffLexer::new();
        for line in ["+héllo 世界", "-🎉", "@@ -1 +1 @@ 名前", "\\ 説明", "文脈"] {
            for (r, _) in lexer.spans(line) {
                let _ = &line[r]; // must not panic
            }
        }
    }

    /// Cheap deterministic totality sweep in the default suite (the
    /// 5k-case hostile campaign lives in `tests/fuzz_big.rs`): every
    /// generated line classifies with valid, ascending, char-boundary
    /// spans.
    #[test]
    fn totality_mini_fuzz() {
        let atoms = [
            "+",
            "-",
            " ",
            "@@",
            "@",
            "---",
            "+++",
            "\\",
            "diff ",
            "index ",
            "@@ -1,2 +3,4 @@",
            "é",
            "世",
            "\u{1b}",
            "\u{0}",
            "text",
            "\t",
        ];
        let lexer = DiffLexer::new();
        let mut seed = 0x5EED_u32;
        let mut rand = move || {
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            (seed >> 16) as usize
        };
        for _ in 0..2_000 {
            let n = 1 + rand() % 8;
            let mut line = String::new();
            for _ in 0..n {
                line.push_str(atoms[rand() % atoms.len()]);
            }
            let mut prev_end = 0usize;
            for (r, _) in lexer.spans(&line) {
                assert!(r.start >= prev_end && r.end <= line.len() && r.start <= r.end);
                assert!(line.is_char_boundary(r.start) && line.is_char_boundary(r.end));
                prev_end = r.end;
            }
        }
    }
}
