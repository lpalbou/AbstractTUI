//! Greedy word wrap over grapheme clusters.
//!
//! Contract (used by widgets and layout for both measuring and painting,
//! so behavior must be stable and boring):
//! - `\n` (and `\r\n`) split logical lines; empty lines are preserved.
//! - Other control clusters (tabs included) are stripped — tab policy
//!   belongs to the widget that owns the text, not the wrapper.
//! - Break points are whitespace runs; the run at a break is consumed
//!   (no trailing/leading spaces around the break). Interior whitespace
//!   that fits is preserved verbatim.
//! - A word wider than the wrap width is broken at grapheme boundaries.
//! - A single cluster wider than the wrap width (a CJK glyph at width 1)
//!   is emitted alone on its own line: overflow is the caller's clipping
//!   problem, silent data loss is not acceptable and refusing would loop.
//! - Zero-width clusters ride along at no cost.

use unicode_segmentation::UnicodeSegmentation;

use super::{cluster_width, is_control_cluster};

/// Options for [`wrap_with`] — the log-pane/chat-bubble knobs.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct WrapOpts {
    /// Columns of indent prepended to CONTINUATION lines (wrapped tails,
    /// not first lines and not lines created by explicit `\n`). The
    /// indent eats into the width budget, so output lines still fit
    /// `max_width`; it clamps to `max_width - 1` (at least one content
    /// column always remains).
    pub hanging_indent: i32,
    /// Maximum output lines (0 = unlimited). When content is cut, the
    /// last kept line ends with `…` within the width budget.
    pub max_lines: usize,
}

/// Wraps `s` to `max_width` columns. `max_width` is clamped to at least 1.
/// Explicit newlines are preserved (each starts a fresh non-indented
/// line); other control clusters are stripped.
pub fn wrap(s: &str, max_width: i32) -> Vec<String> {
    wrap_with(s, max_width, &WrapOpts::default())
}

/// [`wrap`] with hanging indent and a max-lines + ellipsis cap.
pub fn wrap_with(s: &str, max_width: i32, opts: &WrapOpts) -> Vec<String> {
    let max_width = max_width.max(1);
    let hang = opts.hanging_indent.clamp(0, max_width - 1);
    let mut out = Vec::new();
    for line in split_logical_lines(s) {
        let first = out.len();
        if hang == 0 {
            wrap_line(&line, max_width, &mut out);
        } else {
            // First visual line takes the full width; continuations wrap
            // at the reduced budget, then receive the indent prefix.
            wrap_line_hanging(&line, max_width, hang, &mut out);
        }
        debug_assert!(out.len() > first, "every logical line yields a row");
        // Early exit: past the cap, later logical lines can't show.
        if opts.max_lines != 0 && out.len() > opts.max_lines {
            break;
        }
    }
    if out.is_empty() {
        out.push(String::new());
    }
    if opts.max_lines != 0 && out.len() > opts.max_lines {
        out.truncate(opts.max_lines);
        if let Some(last) = out.last_mut() {
            *last = super::truncate_ellipsis(&format!("{last}\u{2026}"), max_width);
        }
    }
    out
}

/// Hanging-indent wrap of one logical line: the first visual line uses
/// the full width, continuations wrap at `max_width - hang` and carry the
/// indent prefix. One greedy pass (no re-wrap approximation).
fn wrap_line_hanging(line: &str, max_width: i32, hang: i32, out: &mut Vec<String>) {
    let start = out.len();
    wrap_line_budgets(line, max_width, max_width - hang, out);
    let indent = " ".repeat(hang as usize);
    for cont in out[start..].iter_mut().skip(1) {
        cont.insert_str(0, &indent);
    }
}

/// Splits on newline clusters and strips other control clusters in one pass.
fn split_logical_lines(s: &str) -> Vec<String> {
    let mut lines = vec![String::new()];
    for cluster in s.graphemes(true) {
        if is_control_cluster(cluster) {
            // Segmentation keeps "\r\n" together, so one cluster = one break.
            if cluster.contains('\n') {
                lines.push(String::new());
            }
            continue;
        }
        lines.last_mut().expect("never empty").push_str(cluster);
    }
    lines
}

fn wrap_line(line: &str, max_width: i32, out: &mut Vec<String>) {
    wrap_line_budgets(line, max_width, max_width, out);
}

/// The greedy wrapper with per-row budgets: the FIRST emitted row of this
/// logical line gets `first_w` columns, every later row `rest_w` (the
/// hanging-indent mechanism, budget-side). Both clamp to ≥ 1.
fn wrap_line_budgets(line: &str, first_w: i32, rest_w: i32, out: &mut Vec<String>) {
    let rows_before = out.len();
    let (first_w, rest_w) = (first_w.max(1), rest_w.max(1));
    let budget = |out: &Vec<String>| {
        if out.len() == rows_before {
            first_w
        } else {
            rest_w
        }
    };
    let mut current = String::new();
    let mut current_w = 0i32;

    for token in tokenize(line) {
        match token {
            Token::Space(text, w) => {
                // Whitespace is only kept when it fits; a run that crosses
                // the edge is the break point and is consumed entirely.
                if current_w > 0 && current_w + w <= budget(out) {
                    current.push_str(text);
                    current_w += w;
                } else if current_w > 0 {
                    flush_trimmed(&mut current, &mut current_w, out);
                }
                // Leading whitespace on a fresh line is dropped.
            }
            Token::Word(text, w) => {
                if current_w > 0 && current_w + w > budget(out) {
                    flush_trimmed(&mut current, &mut current_w, out);
                }
                if w <= budget(out) {
                    current.push_str(text);
                    current_w += w;
                } else {
                    break_long_word(text, &budget, &mut current, &mut current_w, out);
                }
            }
        }
    }
    flush_trimmed(&mut current, &mut current_w, out);
    // A logical line that produced no rows (empty, or whitespace-only fully
    // consumed at breaks) still occupies one screen row.
    if out.len() == rows_before {
        out.push(String::new());
    }
}

fn flush_trimmed(current: &mut String, current_w: &mut i32, out: &mut Vec<String>) {
    if *current_w == 0 && current.is_empty() {
        return;
    }
    // Trailing whitespace at a break is consumed.
    let trimmed = current.trim_end();
    out.push(trimmed.to_string());
    current.clear();
    *current_w = 0;
}

/// Fills lines with as many clusters of `word` as fit. A cluster that fits
/// nowhere (wider than the budget) is emitted alone.
fn break_long_word(
    word: &str,
    budget: &impl Fn(&Vec<String>) -> i32,
    current: &mut String,
    current_w: &mut i32,
    out: &mut Vec<String>,
) {
    for cluster in word.graphemes(true) {
        let w = cluster_width(cluster);
        if *current_w > 0 && *current_w + w > budget(out) {
            flush_trimmed(current, current_w, out);
        }
        current.push_str(cluster);
        *current_w += w;
    }
}

enum Token<'a> {
    Word(&'a str, i32),
    Space(&'a str, i32),
}

/// Splits a control-free line into alternating word / whitespace tokens,
/// measured once. Borrow-only; allocation happens when rows are produced.
fn tokenize(line: &str) -> impl Iterator<Item = Token<'_>> {
    let mut clusters = line.grapheme_indices(true).peekable();
    std::iter::from_fn(move || {
        let (start, first) = clusters.next()?;
        let is_space = first.chars().next().is_some_and(char::is_whitespace);
        let mut end = start + first.len();
        let mut w = cluster_width(first);
        while let Some((_, c)) = clusters.peek() {
            let c_space = c.chars().next().is_some_and(char::is_whitespace);
            if c_space != is_space {
                break;
            }
            w += cluster_width(c);
            let (i, c) = clusters.next().expect("peeked");
            end = i + c.len();
        }
        let text = &line[start..end];
        Some(if is_space {
            Token::Space(text, w)
        } else {
            Token::Word(text, w)
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_word_wrap() {
        assert_eq!(
            wrap("the quick brown fox", 10),
            vec!["the quick", "brown fox"]
        );
        assert_eq!(wrap("hello", 10), vec!["hello"]);
        assert_eq!(wrap("", 10), vec![""]);
    }

    #[test]
    fn break_consumes_whitespace() {
        assert_eq!(wrap("aa   bb", 4), vec!["aa", "bb"]);
        // Interior whitespace that fits is preserved.
        assert_eq!(wrap("a  b", 6), vec!["a  b"]);
    }

    #[test]
    fn long_word_breaks_at_grapheme_boundaries() {
        assert_eq!(wrap("abcdefgh", 3), vec!["abc", "def", "gh"]);
        // Accented cluster stays whole.
        assert_eq!(wrap("héllo", 2), vec!["hé", "ll", "o"]);
    }

    #[test]
    fn cjk_wraps_by_columns() {
        // Each ideograph is 2 columns: 3 per 6-column line.
        assert_eq!(wrap("城市化进程加快", 6), vec!["城市化", "进程加", "快"]);
        // Odd width: only 2 fit, no half glyphs.
        assert_eq!(wrap("城市化", 5), vec!["城市", "化"]);
    }

    #[test]
    fn oversized_cluster_emitted_alone() {
        assert_eq!(wrap("界", 1), vec!["界"]);
        assert_eq!(wrap("a界b", 1), vec!["a", "界", "b"]);
    }

    #[test]
    fn newlines_and_empty_lines() {
        assert_eq!(wrap("a\n\nb", 10), vec!["a", "", "b"]);
        assert_eq!(wrap("a\r\nb", 10), vec!["a", "b"]);
        assert_eq!(wrap("ab\n", 5), vec!["ab", ""]);
    }

    #[test]
    fn zwj_sequences_do_not_crash_or_split() {
        let family = "👨\u{200D}👩\u{200D}👧\u{200D}👦";
        let text = format!("{family} {family}");
        let wrapped = wrap(&text, 2);
        assert_eq!(wrapped, vec![family.to_string(), family.to_string()]);
        // ZWJ cluster never splits internally even under hard break.
        assert_eq!(wrap(family, 1), vec![family.to_string()]);
    }

    #[test]
    fn controls_stripped() {
        assert_eq!(wrap("a\tb", 10), vec!["ab"]);
    }

    #[test]
    fn hanging_indent_indents_continuations_only() {
        let opts = WrapOpts {
            hanging_indent: 2,
            max_lines: 0,
        };
        let lines = wrap_with("alpha beta gamma delta", 8, &opts);
        assert_eq!(lines[0], "alpha", "first line unindented at full width");
        for cont in &lines[1..] {
            assert!(cont.starts_with("  "), "continuation indented: {cont:?}");
            assert!(crate::text::width(cont) <= 8, "fits incl. indent: {cont:?}");
        }
        // Explicit newlines start fresh (no indent).
        let lines = wrap_with("one two three\nnew", 8, &opts);
        assert!(lines.contains(&"new".to_string()));
        // Indent clamps: at width 3, hang 10 leaves 1 content column min.
        let lines = wrap_with(
            "abcd",
            3,
            &WrapOpts {
                hanging_indent: 10,
                max_lines: 0,
            },
        );
        for l in &lines {
            assert!(crate::text::width(l) <= 3);
        }
    }

    #[test]
    fn max_lines_caps_with_ellipsis() {
        let opts = WrapOpts {
            hanging_indent: 0,
            max_lines: 2,
        };
        let lines = wrap_with("aa bb cc dd ee ff", 5, &opts);
        assert_eq!(lines.len(), 2);
        assert!(lines[1].ends_with('…'), "cut is visible: {lines:?}");
        assert!(crate::text::width(&lines[1]) <= 5);
        // Content that FITS the cap gains no ellipsis.
        let lines = wrap_with("aa bb", 5, &opts);
        assert_eq!(lines, vec!["aa bb"]);
        // Cap of 1 still shows something.
        let lines = wrap_with(
            "word word word",
            6,
            &WrapOpts {
                hanging_indent: 0,
                max_lines: 1,
            },
        );
        assert_eq!(lines.len(), 1);
        assert!(lines[0].ends_with('…'));
        // max_lines=0 = unlimited (the default path).
        assert_eq!(wrap_with("a b c", 1, &WrapOpts::default()).len(), 3);
    }

    #[test]
    fn hanging_and_cap_compose_for_chat_bubbles() {
        let opts = WrapOpts {
            hanging_indent: 3,
            max_lines: 3,
        };
        let lines = wrap_with(
            "user: the quick brown fox jumps over the lazy dog",
            12,
            &opts,
        );
        assert!(lines.len() <= 3);
        assert!(lines.last().unwrap().ends_with('…'));
        for l in &lines {
            assert!(crate::text::width(l) <= 12, "{l:?}");
        }
    }
}
