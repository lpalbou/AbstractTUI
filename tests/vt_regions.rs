//! DECSTBM scroll-region ground truth (REDTEAM, cycle 4 — the referee
//! surface RENDER's scroll-region optimization is judged against).
//! Semantics per xterm: margins scope LF/RI/SU/SD/IL/DL; ED/EL ignore
//! them; DECSTBM homes the cursor; invalid margins are ignored.

use abstracttui::base::{Point, Size};
use abstracttui::testing::VtScreen;

fn screen(w: i32, h: i32) -> VtScreen {
    VtScreen::new(Size::new(w, h))
}

fn fill_rows(s: &mut VtScreen, h: i32) {
    for r in 0..h {
        s.feed(format!("\x1b[{};1H", r + 1).as_bytes());
        s.feed(format!("row{r}").as_bytes());
    }
}

#[test]
fn decstbm_sets_clamps_and_homes() {
    let mut s = screen(10, 6);
    s.feed(b"\x1b[3;3Hx"); // move away from home
    s.feed(b"\x1b[2;5r");
    assert_eq!(s.margins(), Some((1, 4)));
    assert_eq!(s.cursor(), Point::ZERO, "DECSTBM homes the cursor");
    // Reset forms: no params, and full-screen params.
    s.feed(b"\x1b[r");
    assert_eq!(s.margins(), None);
    s.feed(b"\x1b[1;6r");
    assert_eq!(s.margins(), None, "full-screen margins are the reset state");
    // Invalid: bottom <= top is ignored wholesale.
    s.feed(b"\x1b[4;2Hy"); // cursor lands at (2,3) after writing 'y'
    s.feed(b"\x1b[4;4r");
    assert_eq!(s.margins(), None, "bottom <= top must be ignored");
    assert_eq!(
        s.cursor(),
        Point::new(2, 3),
        "ignored DECSTBM must not home"
    );
    // Out-of-range params clamp.
    s.feed(b"\x1b[2;99r");
    assert_eq!(s.margins(), Some((1, 5)));
    assert_eq!(s.unknown_seq_count(), 0);
}

#[test]
fn lf_at_bottom_margin_scrolls_only_the_region() {
    let mut s = screen(10, 5);
    fill_rows(&mut s, 5);
    s.feed(b"\x1b[2;4r"); // region rows 1..=3 (0-based)
    s.feed(b"\x1b[4;1H"); // cursor to region bottom (row 3)
    s.feed(b"\nNEW"); // LF at margin: region scrolls
    assert_eq!(s.to_text(), "row0\nrow2\nrow3\nNEW\nrow4\n");
    assert_eq!(s.cursor().y, 3, "cursor stays at the bottom margin");
    assert_eq!(s.unknown_seq_count(), 0);
}

#[test]
fn lf_below_region_never_scrolls() {
    let mut s = screen(10, 5);
    fill_rows(&mut s, 5);
    s.feed(b"\x1b[2;3r");
    s.feed(b"\x1b[5;1H\n\n\nZ"); // below the region, at screen bottom
    assert_eq!(s.cursor().y, 4, "cursor sticks at the screen's last row");
    assert!(
        s.to_text().starts_with("row0\nrow1\nrow2\nrow3\n"),
        "{}",
        s.to_text()
    );
    assert!(s.to_text().contains('Z'));
}

#[test]
fn wrap_at_region_bottom_scrolls_region() {
    let mut s = screen(4, 4);
    fill_rows(&mut s, 4);
    s.feed(b"\x1b[1;3r");
    s.feed(b"\x1b[3;1Habcd"); // fills region bottom row, wrap pending
    s.feed(b"ef"); // wrap fires -> region scrolls, rows below untouched
    let text = s.to_text();
    assert!(
        text.ends_with("row3\n"),
        "below-region row must not move: {text}"
    );
    assert!(text.contains("ef"));
}

#[test]
fn reverse_index_at_top_margin_scrolls_region_down() {
    let mut s = screen(10, 5);
    fill_rows(&mut s, 5);
    s.feed(b"\x1b[2;4r");
    s.feed(b"\x1b[2;1H\x1bMTOP"); // RI at the region top
    assert_eq!(s.to_text(), "row0\nTOP\nrow1\nrow2\nrow4\n");
    assert_eq!(s.unknown_seq_count(), 0);
}

#[test]
fn su_sd_scoped_to_region() {
    let mut s = screen(10, 5);
    fill_rows(&mut s, 5);
    s.feed(b"\x1b[2;4r\x1b[2S");
    assert_eq!(s.to_text(), "row0\nrow3\n\n\nrow4\n");
    let mut s = screen(10, 5);
    fill_rows(&mut s, 5);
    s.feed(b"\x1b[2;4r\x1b[1T");
    assert_eq!(s.to_text(), "row0\n\nrow1\nrow2\nrow4\n");
}

#[test]
fn il_dl_inside_region_bounded_by_bottom_margin() {
    let mut s = screen(10, 6);
    fill_rows(&mut s, 6);
    s.feed(b"\x1b[2;5r");
    s.feed(b"\x1b[3;4H\x1b[1L"); // IL at row 2 (inside region)
    assert_eq!(s.to_text(), "row0\nrow1\n\nrow2\nrow3\nrow5\n");
    assert_eq!(s.cursor(), Point::new(0, 2), "IL homes the column");
    s.feed(b"\x1b[3;1H\x1b[1M"); // DL pulls rows back up
    assert_eq!(s.to_text(), "row0\nrow1\nrow2\nrow3\n\nrow5\n");
    // Outside the region: no-op.
    s.feed(b"\x1b[6;2H\x1b[3L");
    assert_eq!(s.to_text(), "row0\nrow1\nrow2\nrow3\n\nrow5\n");
    assert_eq!(s.unknown_seq_count(), 0);
}

#[test]
fn il_dl_without_margins_operate_on_full_screen() {
    let mut s = screen(8, 4);
    fill_rows(&mut s, 4);
    s.feed(b"\x1b[2;1H\x1b[1L");
    assert_eq!(s.to_text(), "row0\n\nrow1\nrow2\n");
    s.feed(b"\x1b[1;1H\x1b[2M");
    assert_eq!(s.to_text(), "row1\nrow2\n\n\n");
}

#[test]
fn ed_el_ignore_margins() {
    let mut s = screen(8, 4);
    fill_rows(&mut s, 4);
    s.feed(b"\x1b[2;3r\x1b[1;1H\x1b[2J");
    assert_eq!(
        s.to_text(),
        "\n\n\n\n",
        "ED 2 erases the WHOLE screen despite margins"
    );
}

#[test]
fn cup_is_absolute_regardless_of_margins() {
    // Origin mode (DECOM) is off: CUP addresses the screen, not the
    // region — the presenter never sets DECOM.
    let mut s = screen(8, 5);
    s.feed(b"\x1b[2;4r\x1b[1;2Hx\x1b[5;1Hy");
    assert_eq!(s.cell(1, 0).unwrap().ch(), 'x');
    assert_eq!(s.cell(0, 4).unwrap().ch(), 'y');
}

#[test]
fn region_scroll_preserves_wide_pairs_and_styles() {
    let mut s = screen(8, 4);
    s.feed("\x1b[1;1H\x1b[31m日本\x1b[0m".as_bytes());
    s.feed(b"\x1b[2;1Hplain");
    s.feed(b"\x1b[1;3r\x1b[3;1H\n"); // scroll region up once
                                     // The wide row moved up out of view; row with "plain" is now top.
    assert_eq!(s.to_text(), "plain\n\n\n\n");
    // New content into the region with wide glyphs survives scrolling.
    s.feed("\x1b[3;1H漢字\n".as_bytes());
    for y in 0..4 {
        for x in 0..8 {
            let c = s.cell(x, y).unwrap();
            if c.is_continuation() {
                assert!(s.cell(x - 1, y).unwrap().is_wide_leader());
            }
            if c.is_wide_leader() {
                assert!(s.cell(x + 1, y).unwrap().is_continuation());
            }
        }
    }
    assert_eq!(s.unknown_seq_count(), 0);
}

#[test]
fn decscusr_and_osc52_are_tracked_not_unknown() {
    let mut s = screen(8, 3);
    s.feed(b"\x1b[4 q"); // DECSCUSR: steady underline
    assert_eq!(s.cursor_style(), 4);
    s.feed(b"\x1b[0 q");
    assert_eq!(s.cursor_style(), 0);
    s.feed(b"\x1b]52;c;aGVsbG8=\x07"); // OSC 52 clipboard write
    assert_eq!(s.clipboard(), Some(("c", "aGVsbG8=")));
    s.feed(b"\x1b]52;c;?\x1b\\"); // query form
    assert_eq!(s.clipboard(), Some(("c", "?")));
    assert_eq!(s.unknown_seq_count(), 0);
}

#[test]
fn full_reset_clears_margins_and_style() {
    let mut s = screen(8, 4);
    s.feed(b"\x1b[2;3r\x1b[3 q\x1bc");
    assert_eq!(s.margins(), None);
    assert_eq!(s.cursor_style(), 0);
}
