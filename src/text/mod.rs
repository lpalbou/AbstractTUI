//! Text measurement and wrapping over extended grapheme clusters.
//!
//! One width policy, shared by everything. The classic terminal-corruption
//! source is measurement disagreeing with rendering (a widget measures a
//! cluster as 1 column, the surface stores it as 2, the diff emits it and
//! the terminal shifts the rest of the row). `cluster_width` is therefore
//! the *only* width authority in the engine: `Surface::draw_text`, the
//! glyph cache in `render::cell`, wrapping and truncation all consult it.
//!
//! Policy (documented in docs/design/render.md §2.5):
//! - Control clusters (first scalar is a C0/C1 control or DEL) are width 0
//!   and are stripped by drawing/wrapping code, never rendered.
//! - Clusters whose scalar widths sum to 0 (lone combining marks, ZWSP,
//!   a stray variation selector) are width 0: invisible, zero columns.
//! - A cluster containing VS16 (U+FE0F, emoji presentation) is width 2:
//!   modern terminals render emoji presentation sequences double-wide even
//!   when the base character is narrow (e.g. "❤️", keycap sequences).
//! - Everything else is the unicode-width sum capped at 2. The cap folds
//!   multi-scalar clusters (ZWJ families, skin-tone modifiers) whose parts
//!   sum past 2 into the double cell a terminal actually uses. VS15 (text
//!   presentation) is deliberately *not* forced narrow: wide bases keep
//!   their East Asian width, matching the majority of terminals which
//!   ignore VS15 for width purposes.

pub mod highlight;
mod truncate;
mod wrap;

pub use highlight::{CLikeLexer, Highlighter, TokenKind};
pub use truncate::truncate_ellipsis;
pub use wrap::wrap;

use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

/// U+FE0F VARIATION SELECTOR-16: requests emoji presentation.
const VS16: char = '\u{FE0F}';

/// True when the cluster starts with a control scalar (C0, DEL, C1).
/// Grapheme segmentation keeps "\r\n" as one cluster, so checking the
/// first scalar classifies the whole cluster.
pub(crate) fn is_control_cluster(cluster: &str) -> bool {
    cluster.chars().next().is_some_and(char::is_control)
}

/// Display width in terminal columns of one grapheme cluster: 0, 1 or 2.
pub fn cluster_width(cluster: &str) -> i32 {
    if cluster.is_empty() || is_control_cluster(cluster) {
        return 0;
    }
    let base = UnicodeWidthStr::width(cluster) as i32;
    if base <= 0 {
        return 0;
    }
    if cluster.contains(VS16) {
        return 2;
    }
    base.min(2)
}

/// Display width of a string: the sum of its cluster widths. Control
/// clusters (including newlines) measure 0 — wrap first, then measure lines.
pub fn width(s: &str) -> i32 {
    s.graphemes(true).map(cluster_width).sum()
}

/// U+200D ZERO WIDTH JOINER.
const ZWJ: char = '\u{200D}';

/// True when real terminals may disagree with [`cluster_width`] about this
/// cluster (RT1-7): emoji presentation (VS16) and ZWJ sequences render at
/// different widths across terminals (xterm splits families into
/// components; kitty/wezterm render 2), and East-Asian-Ambiguous characters
/// render double-wide under legacy CJK configurations or emoji-font
/// fallback. The presenter invalidates its virtual cursor after emitting
/// one of these, so any width disagreement is confined to the risky
/// cluster itself instead of shifting every glyph after it (the classic
/// mystery-smear).
///
/// Deliberate exception: the TUI-structural blocks U+2500..=U+25FF (box
/// drawing, block elements, geometric shapes) are NOT risky even though
/// UAX #11 classes much of them Ambiguous. Two reasons, both load-bearing:
/// (1) they are the fabric of terminal chrome — flagging them would emit
/// an absolute CUP after nearly every border cell, blowing the byte budget
/// exactly where output is densest; (2) they are native monospace-font
/// glyphs, not subject to the emoji-font fallback that actually widens
/// ambiguous symbols in practice — and a terminal configured ambiguous-wide
/// breaks the CELL LAYOUT of every TUI regardless, which no cursor
/// re-anchoring can repair. Documented in docs/design/render.md §2.4.
///
/// Pure-ASCII clusters are never risky — the fast path costs one scan of
/// bytes the emitter already touched.
///
/// Crate-private [C8 freeze]: this is the presenter's cursor-defense
/// heuristic, not a user-facing width oracle — `width`/`cluster_width`
/// are the public truth; exposing the defense would invite callers to
/// second-guess it.
pub(crate) fn is_risky_cluster(cluster: &str) -> bool {
    if cluster.is_ascii() {
        return false;
    }
    cluster
        .chars()
        .any(|c| c == VS16 || c == ZWJ || is_risky_ambiguous(c))
}

/// East Asian Ambiguous detection without a second width table: the two
/// unicode-width opinions differ exactly on the ambiguous set. The
/// TUI-structural ranges are carved out (see [`is_risky_cluster`]).
fn is_risky_ambiguous(c: char) -> bool {
    use unicode_width::UnicodeWidthChar;
    if ('\u{2500}'..='\u{25FF}').contains(&c) {
        return false;
    }
    c.width() != c.width_cjk()
}

/// One measured grapheme cluster of a string: byte range + display width.
/// The cursor-math currency for input fields (REACT consumes it):
/// caret positions are cluster boundaries, and column <-> byte-offset
/// conversions fold over these segments.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Segment<'a> {
    /// The cluster text (borrowing the input).
    pub cluster: &'a str,
    /// Byte offset of the cluster within the measured string.
    pub offset: usize,
    /// Display columns (0 for control/zero-width clusters, 1 or 2
    /// otherwise) — the same policy as [`cluster_width`], by construction.
    pub width: i32,
}

/// Iterates `s` as measured grapheme clusters. Nothing is skipped —
/// control clusters appear with width 0 so byte offsets stay exhaustive
/// (an input-field caret must be able to sit after ANY byte boundary the
/// user can reach); rendering paths keep stripping controls themselves.
pub fn segments(s: &str) -> impl Iterator<Item = Segment<'_>> {
    s.grapheme_indices(true).map(|(offset, cluster)| Segment {
        cluster,
        offset,
        width: cluster_width(cluster),
    })
}

/// The cluster boundary strictly after `byte_idx` (caret "step right").
/// Out-of-range or non-boundary inputs behave like the caret they imply:
/// mid-cluster snaps to that cluster's END, `byte_idx >= s.len()` stays
/// at `s.len()`.
pub fn next_boundary(s: &str, byte_idx: usize) -> usize {
    for seg in segments(s) {
        let end = seg.offset + seg.cluster.len();
        if end > byte_idx {
            return end;
        }
    }
    s.len()
}

/// The cluster boundary strictly before `byte_idx` (caret "step left" /
/// backspace target: deleting `prev_boundary(s, i)..i` removes exactly
/// one whole cluster). Mid-cluster inputs snap to that cluster's START;
/// `byte_idx == 0` stays 0.
pub fn prev_boundary(s: &str, byte_idx: usize) -> usize {
    let mut prev = 0;
    for seg in segments(s) {
        if seg.offset >= byte_idx {
            return prev;
        }
        prev = seg.offset;
    }
    prev
}

/// Wrapping window used when the caller passes a non-positive available
/// width ("unconstrained"). Large enough that no real content wraps, small
/// enough that width accumulation can never overflow `i32`.
const UNBOUNDED_WIDTH: i32 = 1 << 24;

/// Wrapping-aware measurement for layout leaves (REACT request 4): the
/// size `s` needs when wrapped into `avail.w` columns.
///
/// - `avail.w <= 0` means "unconstrained": logical lines measure at their
///   natural width.
/// - The returned width can exceed `avail.w` only when a single cluster is
///   wider than the whole window (a CJK glyph at width 1) — truth over
///   comfort; the layout clips.
/// - Height is the wrapped line count, NOT clamped to `avail.h`: the
///   solver decides how much to show. Empty input still occupies one line
///   (a text leaf is never zero-height).
pub fn measure(s: &str, avail: crate::base::Size) -> crate::base::Size {
    let window = if avail.w <= 0 {
        UNBOUNDED_WIDTH
    } else {
        avail.w
    };
    let lines = wrap(s, window);
    let w = lines.iter().map(|l| width(l)).max().unwrap_or(0);
    crate::base::Size::new(w, lines.len() as i32)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Cycle-7 measurement (integrator ask 5): is grapheme segmentation
    /// worth caching for per-keystroke caret math? Run explicitly:
    /// `cargo test --release --lib text::tests::profile -- --ignored --nocapture`
    /// Measured (release, this box): 1.88 µs per segments() walk of a
    /// mixed 76-byte line, 6.3 µs per measure() of a wrapped paragraph —
    /// hundreds of keystrokes per millisecond of budget, and caret math
    /// runs ONCE per keystroke, not per cell. A cache (with eviction,
    /// invalidation on edit, per-widget ownership) would cost more in
    /// complexity than it saves; declined until a profile shows
    /// segmentation in a real hot path.
    #[test]
    #[ignore]
    fn profile_segments_and_measure_per_keystroke_cost() {
        use std::time::Instant;
        let line = "let value = compute(width, \"héllo 世界 👍🏽 done\"); // trailing comment...";
        let para = "The quick brown fox 跳过 the lazy dog 🐕 while counting 一二三 clusters.";
        const N: u32 = 20_000;
        let s = Instant::now();
        let mut sink = 0usize;
        for _ in 0..N {
            sink += segments(line).map(|seg| seg.width as usize).sum::<usize>();
        }
        let seg_us = s.elapsed().as_secs_f64() * 1e6 / N as f64;
        let s = Instant::now();
        for _ in 0..N {
            sink += measure(para, crate::base::Size::new(40, 0)).h as usize;
        }
        let meas_us = s.elapsed().as_secs_f64() * 1e6 / N as f64;
        eprintln!("segments(120-col line): {seg_us:.2} us/call; measure(paragraph @40): {meas_us:.2} us/call; sink {sink}");
    }

    #[test]
    fn ascii_and_cjk() {
        assert_eq!(width("hello"), 5);
        assert_eq!(width("世界"), 4);
        assert_eq!(width("aé"), 2); // combining accent folds into the base
        assert_eq!(width(""), 0);
    }

    #[test]
    fn emoji_presentation_is_wide() {
        assert_eq!(cluster_width("❤\u{FE0F}"), 2); // narrow base + VS16
        assert_eq!(cluster_width("1\u{FE0F}\u{20E3}"), 2); // keycap
        assert_eq!(cluster_width("👍"), 2);
        // ZWJ family: parts sum past 2, cap folds to one double cell.
        assert_eq!(cluster_width("👨\u{200D}👩\u{200D}👧\u{200D}👦"), 2);
        // Skin tone modifier: base 2 + modifier 2, capped.
        assert_eq!(cluster_width("👍🏽"), 2);
    }

    #[test]
    fn controls_and_zero_width_measure_zero() {
        assert_eq!(cluster_width("\t"), 0);
        assert_eq!(cluster_width("\r\n"), 0);
        assert_eq!(cluster_width("\u{200B}"), 0); // ZWSP
        assert_eq!(cluster_width("\u{FE0F}"), 0); // lone VS16: no visible base
        assert_eq!(width("a\nb"), 2);
    }

    #[test]
    fn zwj_between_letters_does_not_inflate() {
        // "a" + ZWJ forms one cluster with "a"; it must stay width 1.
        assert_eq!(width("a\u{200D}b"), 2);
    }

    #[test]
    fn risky_cluster_classification() {
        assert!(!is_risky_cluster("a"));
        assert!(!is_risky_cluster(" "));
        assert!(!is_risky_cluster("世"), "plain CJK is unambiguous wide");
        assert!(!is_risky_cluster("é"), "combining accent is settled narrow");
        assert!(is_risky_cluster("❤\u{FE0F}"), "VS16 presentation");
        assert!(is_risky_cluster("👨\u{200D}👩\u{200D}👧"), "ZWJ sequence");
        // Ambiguous-width symbols (unicode-width's two opinions differ).
        assert!(is_risky_cluster("§"));
        assert!(is_risky_cluster("°"));
        assert!(is_risky_cluster("☆"));
        // TUI-structural carve-out: chrome glyphs must stay cheap.
        assert!(!is_risky_cluster("─"), "box drawing excluded by design");
        assert!(!is_risky_cluster("│"));
        assert!(!is_risky_cluster("█"), "block elements excluded");
        assert!(!is_risky_cluster("▲"), "geometric shapes excluded");
    }

    #[test]
    fn segments_cover_every_byte_with_widths() {
        let s = "a世\t👍🏽é";
        let segs: Vec<_> = segments(s).collect();
        // Exhaustive coverage: offsets tile the string.
        let mut expected_offset = 0;
        for seg in &segs {
            assert_eq!(seg.offset, expected_offset);
            expected_offset += seg.cluster.len();
        }
        assert_eq!(expected_offset, s.len());
        let widths: Vec<i32> = segs.iter().map(|s| s.width).collect();
        assert_eq!(widths, vec![1, 2, 0, 2, 1], "control kept at width 0");
        // Widths agree with the one policy by construction.
        assert_eq!(widths.iter().sum::<i32>(), width(s));
        assert_eq!(segments("").count(), 0);
    }

    #[test]
    fn boundaries_step_whole_clusters() {
        let s = "a👍🏽b"; // 'a' 1B, thumbs+tone 8B, 'b' 1B
        assert_eq!(next_boundary(s, 0), 1);
        assert_eq!(
            next_boundary(s, 1),
            9,
            "steps over the whole ZWJ-ish cluster"
        );
        assert_eq!(next_boundary(s, 9), 10);
        assert_eq!(next_boundary(s, 10), 10, "clamped at the end");
        assert_eq!(prev_boundary(s, 10), 9);
        assert_eq!(
            prev_boundary(s, 9),
            1,
            "backspace target is the cluster start"
        );
        assert_eq!(prev_boundary(s, 1), 0);
        assert_eq!(prev_boundary(s, 0), 0, "floored at 0");
    }

    #[test]
    fn boundaries_snap_mid_cluster_inputs() {
        let s = "x👍🏽y";
        // Byte 3 sits inside the emoji cluster (1..9).
        assert_eq!(next_boundary(s, 3), 9, "mid-cluster snaps to cluster end");
        assert_eq!(prev_boundary(s, 3), 1, "mid-cluster snaps to cluster start");
        // Past-the-end input behaves like a caret at the end.
        assert_eq!(prev_boundary(s, 400), 9);
        assert_eq!(next_boundary(s, 400), s.len());
        // Empty string is inert.
        assert_eq!(next_boundary("", 0), 0);
        assert_eq!(prev_boundary("", 0), 0);
        // Backspace deletes exactly one cluster: the RT3-2 shape.
        let caret = s.len();
        let cut = prev_boundary(s, caret);
        let mut owned = s.to_string();
        owned.replace_range(cut..caret, "");
        assert_eq!(owned, "x👍🏽");
    }

    #[test]
    fn measure_wraps_and_measures() {
        use crate::base::Size;
        assert_eq!(measure("hello", Size::new(10, 5)), Size::new(5, 1));
        assert_eq!(
            measure("the quick brown fox", Size::new(10, 5)),
            Size::new(9, 2)
        );
        // Unconstrained: natural line widths.
        assert_eq!(measure("ab\ncdef", Size::new(0, 0)), Size::new(4, 2));
        // Wide glyphs measured in columns, wrapped by columns.
        assert_eq!(measure("世界人", Size::new(4, 9)), Size::new(4, 2));
        // Empty text still occupies one line.
        assert_eq!(measure("", Size::new(10, 5)), Size::new(0, 1));
        // Oversized single cluster: honest overflow, height not clamped.
        assert_eq!(measure("界", Size::new(1, 1)), Size::new(2, 1));
    }
}
