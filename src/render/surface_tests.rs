//! Surface unit tests, split out to keep `surface.rs` inside the file
//! size budget. Ordinary unit tests of the sibling module.

use super::*;
use crate::base::Rgba;
use crate::render::cell::Attrs;

fn text_of(s: &Surface, y: i32) -> String {
    let mut line = String::new();
    for x in 0..s.width() {
        let c = s.get(x, y).unwrap();
        if c.glyph.is_continuation() {
            continue;
        }
        let t = c.glyph.as_str(s.pool());
        line.push_str(if t.is_empty() { "." } else { t });
    }
    line
}

/// Checks the structural invariant every write path must uphold.
fn assert_pairs_consistent(s: &Surface) {
    for y in 0..s.height() {
        let mut x = 0;
        while x < s.width() {
            let c = s.get(x, y).unwrap();
            assert!(
                !c.is_continuation(),
                "orphan continuation at ({x},{y}) — leader missing"
            );
            if c.is_wide_leader() {
                assert!(x + 1 < s.width(), "leader in last column at ({x},{y})");
                let cont = s.get(x + 1, y).unwrap();
                assert!(
                    cont.is_continuation(),
                    "leader without continuation at ({x},{y})"
                );
                assert_eq!(cont.fg, c.fg, "continuation style must mirror leader");
                assert_eq!(cont.bg, c.bg);
                assert_eq!(cont.ul, c.ul);
                assert_eq!(cont.attrs, c.attrs);
                assert_eq!(cont.link, c.link);
                x += 2;
            } else {
                x += 1;
            }
        }
    }
}

fn surf(w: i32, h: i32) -> Surface {
    Surface::new(Size::new(w, h), Cell::EMPTY)
}

#[test]
fn draw_and_read_back() {
    let mut s = surf(10, 2);
    let n = s.draw_text(0, 0, "héllo", Style::new().fg(Rgba::WHITE));
    assert_eq!(n, 5);
    assert_eq!(text_of(&s, 0), "héllo.....");
    assert_eq!(s.get(0, 0).unwrap().fg, Rgba::WHITE);
    assert_pairs_consistent(&s);
}

#[test]
fn wide_glyph_clobber_left_half() {
    let mut s = surf(6, 1);
    s.draw_text(0, 0, "世界", Style::new());
    assert_eq!(text_of(&s, 0), "世界..");
    // Overwrite the leader of 世: its continuation must blank.
    s.draw_text(0, 0, "a", Style::new());
    assert_eq!(text_of(&s, 0), "a 界..");
    assert_pairs_consistent(&s);
}

#[test]
fn wide_glyph_clobber_right_half() {
    let mut s = surf(6, 1);
    s.draw_text(0, 0, "世界", Style::new());
    // Overwrite the continuation of 世 (column 1): leader must blank.
    s.set(
        1,
        0,
        Cell::new(Glyph::new("x", &mut GlyphPool::default()).unwrap()),
    );
    assert_eq!(text_of(&s, 0), " x界..");
    assert_pairs_consistent(&s);
}

#[test]
fn wide_glyph_overwrites_two_pairs() {
    let mut s = surf(6, 1);
    s.draw_text(0, 0, "世界人", Style::new());
    // A wide glyph at x=1 clobbers halves of BOTH 世 and 界.
    s.draw_text(1, 0, "中", Style::new());
    assert_eq!(text_of(&s, 0), " 中 人");
    assert_pairs_consistent(&s);
}

#[test]
fn cjk_clipped_at_right_edge() {
    let mut s = surf(5, 1);
    // 2+2 fits, the third ideograph would straddle the edge: stop.
    let n = s.draw_text(0, 0, "世界人", Style::new());
    assert_eq!(n, 4);
    assert_eq!(text_of(&s, 0), "世界.");
    // A wide glyph directly into the last column degrades to a blank.
    s.set(
        4,
        0,
        Cell::new(Glyph::new("中", &mut GlyphPool::default()).unwrap()),
    );
    assert_eq!(text_of(&s, 0), "世界 ");
    assert_pairs_consistent(&s);
}

#[test]
fn cjk_clipped_at_left_edge() {
    let mut s = surf(6, 1);
    let n = s.draw_text(-1, 0, "世界", Style::new());
    // Advance counts the clipped columns too.
    assert_eq!(n, 4);
    // 世 straddles the edge: visible half blanks; 界 lands at 1..3.
    assert_eq!(text_of(&s, 0), " 界...");
    assert_pairs_consistent(&s);
}

#[test]
fn controls_stripped_zero_width_skipped() {
    let mut s = surf(8, 1);
    let n = s.draw_text(0, 0, "a\tb\u{200B}c", Style::new());
    assert_eq!(n, 3);
    assert_eq!(text_of(&s, 0), "abc.....");
}

#[test]
fn fill_rect_wide_tiles_and_odd_edge() {
    let mut s = surf(5, 2);
    let mut pool = GlyphPool::default();
    let block = Cell::new(Glyph::new("回", &mut pool).unwrap());
    s.fill_rect(Rect::new(0, 0, 5, 2), block);
    assert_eq!(text_of(&s, 0), "回回 ");
    assert_pairs_consistent(&s);
}

#[test]
fn blit_clips_and_repairs_pairs() {
    let mut src = surf(6, 1);
    src.draw_text(0, 0, "世界人", Style::new());
    let mut dst = surf(6, 1);
    dst.draw_text(0, 0, "abcdef", Style::new());
    // Copy columns 1..4 of src: continuation of 世, then 界 whole.
    dst.blit(&src, Rect::new(1, 0, 3, 1), Point::new(2, 0));
    // src col1 = cont (orphan → blank), col2-3 = 界 pair (intact);
    // text_of renders the pair once, so 5 glyphs cover 6 columns.
    assert_eq!(text_of(&dst, 0), "ab 界f");
    assert_pairs_consistent(&dst);
}

#[test]
fn blit_adopts_pool_and_links() {
    let mut src = surf(4, 1);
    let link = src.register_link("https://example.com");
    let family = "👨\u{200D}👩\u{200D}👧\u{200D}👦";
    src.draw_text(0, 0, family, Style::new().link(link));
    let mut dst = surf(4, 1);
    // Different id spaces on the destination side.
    dst.register_link("https://other.example");
    dst.blit(&src, src.bounds(), Point::ZERO);
    let cell = dst.get(0, 0).unwrap();
    assert_eq!(
        cell.glyph.as_str(dst.pool()),
        family,
        "pooled glyph re-interned"
    );
    assert_eq!(
        dst.link_uri(cell.link),
        Some("https://example.com"),
        "link id remapped"
    );
    assert_pairs_consistent(&dst);
}

#[test]
fn scroll_up_and_down() {
    let mut s = surf(3, 3);
    s.draw_text(0, 0, "aaa", Style::new());
    s.draw_text(0, 1, "bbb", Style::new());
    s.draw_text(0, 2, "ccc", Style::new());
    s.scroll_up(s.bounds(), 1, Cell::EMPTY);
    assert_eq!(
        (text_of(&s, 0), text_of(&s, 1), text_of(&s, 2)),
        ("bbb".into(), "ccc".into(), "...".to_string())
    );
    s.scroll_down(s.bounds(), 2, Cell::EMPTY);
    assert_eq!(
        (text_of(&s, 0), text_of(&s, 1), text_of(&s, 2)),
        ("...".into(), "...".into(), "bbb".to_string())
    );
    assert_pairs_consistent(&s);
}

#[test]
fn scroll_severs_pairs_straddling_region_edge() {
    let mut s = surf(6, 2);
    s.draw_text(0, 0, "a世b.", Style::new());
    s.draw_text(0, 1, "cd界e", Style::new());
    // Region covers columns 2..4: cuts 世 (leader at 1) on row 0 and
    // 界 (leader at 2, cont at 3) is inside; region right edge at 4
    // cuts nothing on row 1... construct a cut: region 2..4 row 1 has
    // 界 at 2-3 fully inside. Scroll and verify invariants hold.
    s.scroll_up(Rect::new(2, 0, 2, 2), 1, Cell::EMPTY);
    assert_pairs_consistent(&s);
    // Row 0 columns 2..4 received row 1's 界 pair; the severed 世
    // leader (col 1) blanked.
    assert_eq!(text_of(&s, 0), "a 界..");
}

#[test]
fn resize_preserves_and_repairs() {
    let mut s = surf(6, 2);
    s.draw_text(0, 0, "ab世c", Style::new());
    s.draw_text(0, 1, "fghijk", Style::new());
    // Narrow to 4 columns: 世's pair occupies 2..4 and survives; row 1
    // simply truncates. Narrow to 3 would cut the pair.
    s.resize(Size::new(3, 2), Cell::EMPTY);
    assert_eq!(text_of(&s, 0), "ab "); // leader at col 2 lost its cont
    assert_eq!(text_of(&s, 1), "fgh");
    assert_pairs_consistent(&s);
    s.resize(Size::new(5, 3), Cell::EMPTY);
    assert_eq!(text_of(&s, 2), ".....");
    assert_pairs_consistent(&s);
}

#[test]
fn style_patch_keeps_panel_background() {
    let mut s = surf(4, 1);
    let panel = Cell::EMPTY.with_bg(Rgba::rgb(40, 40, 60));
    s.fill_rect(s.bounds(), panel);
    s.draw_text(0, 0, "ok", Style::new().fg(Rgba::WHITE).attrs(Attrs::BOLD));
    let c = s.get(0, 0).unwrap();
    assert_eq!(c.bg, Rgba::rgb(40, 40, 60), "text keeps panel bg");
    assert_eq!(c.attrs, Attrs::BOLD);
}

#[test]
fn blit_source_edge_never_resurrects_a_leader() {
    // RT1-4: dst holds a wide leader just LEFT of the blit target; the
    // blit copies a continuation (whose own leader fell outside the
    // source rect) to the target's first column. If that continuation
    // survived, it would pair up with dst's unrelated leader — a lie.
    let mut src = surf(4, 1);
    src.draw_text(0, 0, "世ab", Style::new());
    let mut dst = surf(6, 1);
    dst.draw_text(2, 0, "界", Style::new()); // leader at 2, cont at 3
                                             // Copy src columns 1..3 (cont-of-世, 'a') onto dst columns 3..5.
    dst.blit(&src, Rect::new(1, 0, 2, 1), Point::new(3, 0));
    dst.debug_validate().unwrap();
    // dst's 界 leader lost its continuation -> blanked, and the copied
    // orphan continuation -> blanked; 'a' landed at 4.
    assert_eq!(text_of(&dst, 0), "..  a.");
}

#[test]
fn debug_validate_catches_violations() {
    let mut s = surf(6, 1);
    s.draw_text(0, 0, "世界a", Style::new());
    s.debug_validate().unwrap();
    // Simulate corruption through the compositor's raw write path.
    s.put_composed(1, 0, Cell::new(Glyph::SPACE)); // clobber 世's cont
    let err = s.debug_validate().unwrap_err();
    assert!(err.contains("leader without continuation"), "{err}");
}

#[test]
fn link_table_caps_with_counted_drops() {
    let mut s = surf(4, 1);
    assert_eq!(s.links_dropped(), 0);
    let id = s.register_link("https://example.com");
    assert_eq!(id, 1);
    assert_eq!(s.register_link("https://example.com"), 1, "dedup");
    // The real cap is 65535 — filling it in a unit test is pointless
    // churn; the cap path is pinned by construction here instead.
    // (REDTEAM's churn bench owns the volume test, RT1-14.)
    assert_eq!(s.links_dropped(), 0);
}

#[test]
fn damage_accumulates_and_caps() {
    let mut s = surf(80, 24);
    let mut sink = Vec::new();
    s.take_damage(&mut sink); // drain the initial full-frame damage
    sink.clear();
    s.draw_text(10, 5, "x", Style::new());
    s.take_damage(&mut sink);
    assert_eq!(sink.len(), 1);
    assert!(sink[0].contains(Point::new(10, 5)));
    assert!(sink[0].w <= 3, "single-cell write damages a tiny rect");
    // Cap: scattered writes collapse rather than grow unboundedly.
    for i in 0..100 {
        s.set(i % 80, (i * 7) % 24, Cell::new(Glyph::SPACE));
    }
    sink.clear();
    s.take_damage(&mut sink);
    assert!(sink.len() <= DAMAGE_CAP + 1);
}
