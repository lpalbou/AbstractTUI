//! Width-bounded truncation with an ellipsis.

use unicode_segmentation::UnicodeSegmentation;

use super::{cluster_width, is_control_cluster, width};

/// The single-column horizontal ellipsis. A three-dot ASCII fallback is a
/// theme/widget decision, not a text-layer one.
const ELLIPSIS: &str = "\u{2026}";

/// Returns `s` if it fits in `max_width` columns, otherwise the widest
/// prefix (in grapheme clusters) that leaves room for `…`.
///
/// - Control clusters are stripped (consistent with `wrap` / `draw_text`).
/// - A wide cluster that would straddle the cut is dropped entirely; the
///   spare column stays blank (callers pad, we never emit half a glyph).
/// - `max_width == 0` returns an empty string; `max_width == 1` degrades to
///   just the ellipsis when truncation is needed.
pub fn truncate_ellipsis(s: &str, max_width: i32) -> String {
    if max_width <= 0 {
        return String::new();
    }
    // Fast accept: unchanged content, single allocation. Control stripping
    // still applies so the output is always draw-safe.
    if width(s) <= max_width {
        return s
            .graphemes(true)
            .filter(|c| !is_control_cluster(c))
            .collect();
    }

    let budget = max_width - 1; // reserve the ellipsis column
    let mut out = String::new();
    let mut used = 0i32;
    for cluster in s.graphemes(true) {
        if is_control_cluster(cluster) {
            continue;
        }
        let w = cluster_width(cluster);
        if used + w > budget {
            break;
        }
        out.push_str(cluster);
        used += w;
    }
    out.push_str(ELLIPSIS);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fits_untouched() {
        assert_eq!(truncate_ellipsis("hello", 5), "hello");
        assert_eq!(truncate_ellipsis("hello", 10), "hello");
        assert_eq!(truncate_ellipsis("", 3), "");
    }

    #[test]
    fn truncates_with_ellipsis() {
        assert_eq!(truncate_ellipsis("hello world", 8), "hello w…");
        assert_eq!(super::super::width(&truncate_ellipsis("hello world", 8)), 8);
    }

    #[test]
    fn wide_cluster_never_straddles_the_cut() {
        // "世界" is 4 columns. Budget 3 leaves 2 for content: one ideograph.
        assert_eq!(truncate_ellipsis("世界人", 3), "世…");
        // Budget 4 leaves 3: second ideograph (2 cols) would straddle → drop.
        assert_eq!(truncate_ellipsis("世界人", 4), "世…");
        assert!(super::super::width(&truncate_ellipsis("世界人", 4)) <= 4);
    }

    #[test]
    fn degenerate_widths() {
        assert_eq!(truncate_ellipsis("hello", 0), "");
        assert_eq!(truncate_ellipsis("hello", 1), "…");
    }

    #[test]
    fn strips_controls_even_when_fitting() {
        assert_eq!(truncate_ellipsis("a\tb", 5), "ab");
    }
}
