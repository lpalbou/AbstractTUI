//! VT interpreter ground-truth suite (REDTEAM). These tests define the
//! semantics the presenter will be measured against in cycle 2: if the
//! model is wrong here, every diff/present property test built on it
//! inherits the lie — so this file is deliberately exhaustive about
//! SGR transitions, wide-glyph pairing, erase semantics, wrap and
//! cursor clamping.

use abstracttui::base::{Point, Rgba, Size};
use abstracttui::testing::{xterm_256, CellContent, VtScreen};

fn screen(w: i32, h: i32) -> VtScreen {
    VtScreen::new(Size::new(w, h))
}

fn ch(s: &VtScreen, x: i32, y: i32) -> char {
    s.cell(x, y).unwrap().ch()
}

// ---------------------------------------------------------------- printing

#[test]
fn plain_text_and_crlf() {
    let mut s = screen(10, 3);
    s.feed(b"hello\r\nwd");
    assert_eq!(s.to_text(), "hello\nwd\n\n");
    assert_eq!(s.cursor(), Point::new(2, 1));
    assert_eq!(s.unknown_seq_count(), 0);
}

#[test]
fn lf_alone_moves_down_keeping_column() {
    let mut s = screen(10, 3);
    s.feed(b"ab\ncd");
    // LF without CR: column preserved (raw-mode discipline: the render
    // layer must emit \r\n; this asserts the model keeps them distinct).
    assert_eq!(ch(&s, 2, 1), 'c');
    assert_eq!(ch(&s, 0, 1), ' ');
}

#[test]
fn backspace_and_tab() {
    let mut s = screen(20, 2);
    s.feed(b"ab\x08X");
    assert_eq!(s.to_text(), "aX\n\n");
    let mut s = screen(20, 2);
    s.feed(b"a\tb");
    assert_eq!(ch(&s, 8, 0), 'b');
    // Tab clamps at the last column instead of wrapping.
    let mut s = screen(10, 2);
    s.feed(b"\t\t\tx");
    assert_eq!(ch(&s, 9, 0), 'x');
}

// ------------------------------------------------------------------- wrap

#[test]
fn deferred_autowrap_matches_xterm() {
    let mut s = screen(5, 3);
    s.feed(b"abcde");
    // Glyph in the last column: cursor HOLDS at the margin (pending wrap).
    assert_eq!(s.cursor(), Point::new(4, 0));
    s.feed(b"f");
    assert_eq!(s.to_text(), "abcde\nf\n\n");
    assert_eq!(s.cursor(), Point::new(1, 1));
}

#[test]
fn cr_cancels_pending_wrap() {
    let mut s = screen(5, 2);
    s.feed(b"abcde\rX");
    // The X overwrites column 0 of the SAME row — no phantom wrap.
    assert_eq!(s.to_text(), "Xbcde\n\n");
}

#[test]
fn cursor_motion_cancels_pending_wrap() {
    let mut s = screen(5, 2);
    s.feed(b"abcde\x1b[1;1HX");
    assert_eq!(s.to_text(), "Xbcde\n\n");
}

#[test]
fn autowrap_off_pins_last_column() {
    let mut s = screen(5, 2);
    s.feed(b"\x1b[?7labcdefg");
    // e, f, g all land on the last column; g wins.
    assert_eq!(s.to_text(), "abcdg\n\n");
    assert_eq!(s.cursor(), Point::new(4, 0));
}

#[test]
fn wrap_at_bottom_scrolls() {
    let mut s = screen(3, 2);
    s.feed(b"abcdef");
    // Row 0 "abc" wraps to row 1 "def"; the f is in the last column with
    // wrap pending, nothing scrolled yet.
    assert_eq!(s.to_text(), "abc\ndef\n");
    s.feed(b"g");
    assert_eq!(s.to_text(), "def\ng\n");
}

// ------------------------------------------------------------- wide glyphs

#[test]
fn wide_glyph_occupies_leader_and_continuation() {
    let mut s = screen(6, 1);
    s.feed("日本".as_bytes());
    assert_eq!(ch(&s, 0, 0), '日');
    assert!(s.cell(1, 0).unwrap().is_continuation());
    assert_eq!(ch(&s, 2, 0), '本');
    assert!(s.cell(3, 0).unwrap().is_continuation());
    assert_eq!(s.cursor(), Point::new(4, 0));
    assert_eq!(s.to_text(), "日本\n");
}

#[test]
fn overwriting_continuation_blanks_leader() {
    let mut s = screen(6, 1);
    s.feed("世".as_bytes());
    s.feed(b"\x1b[1;2Hx"); // cursor onto the continuation cell
    assert_eq!(s.cell(0, 0).unwrap().content, CellContent::Blank);
    assert_eq!(ch(&s, 1, 0), 'x');
}

#[test]
fn overwriting_leader_blanks_continuation() {
    let mut s = screen(6, 1);
    s.feed("世".as_bytes());
    s.feed(b"\x1b[1;1Hx");
    assert_eq!(ch(&s, 0, 0), 'x');
    assert_eq!(s.cell(1, 0).unwrap().content, CellContent::Blank);
}

#[test]
fn wide_over_wide_repairs_both_pairs() {
    let mut s = screen(6, 1);
    s.feed("你好".as_bytes()); // cols 0-1, 2-3
    s.feed("\x1b[1;2H中".as_bytes()); // leader at 1, continuation at 2
    assert_eq!(s.cell(0, 0).unwrap().content, CellContent::Blank);
    assert_eq!(ch(&s, 1, 0), '中');
    assert!(s.cell(2, 0).unwrap().is_continuation());
    assert_eq!(s.cell(3, 0).unwrap().content, CellContent::Blank);
}

#[test]
fn wide_glyph_at_last_column_wraps_and_blanks_orphan() {
    let mut s = screen(5, 2);
    s.feed(b"abcd");
    s.feed("界".as_bytes()); // won't fit in the 1 remaining column
    assert_eq!(s.cell(4, 0).unwrap().content, CellContent::Blank);
    assert_eq!(ch(&s, 0, 1), '界');
    assert!(s.cell(1, 1).unwrap().is_continuation());
}

#[test]
fn wide_glyph_at_last_column_without_autowrap_is_dropped() {
    let mut s = screen(5, 1);
    s.feed(b"\x1b[?7labcd");
    s.feed("界".as_bytes());
    // Model convention: no room, no wrap allowed -> glyph dropped whole
    // (never half a glyph).
    assert_eq!(ch(&s, 4, 0), ' ');
    assert_eq!(s.to_text(), "abcd\n");
}

#[test]
fn combining_mark_attaches_to_previous_glyph() {
    let mut s = screen(5, 1);
    s.feed("e\u{301}".as_bytes());
    assert_eq!(s.cell(0, 0).unwrap().display(), "e\u{301}");
    assert_eq!(s.cursor(), Point::new(1, 0));
}

#[test]
fn vs16_widens_narrow_base_to_emoji_pair() {
    // ☁ (width 1) + VS16 renders double-wide on our target terminals;
    // the model widens the pair and advances the cursor accordingly.
    let mut s = screen(6, 1);
    s.feed("\u{2601}\u{fe0f}x".as_bytes());
    assert!(s.cell(0, 0).unwrap().is_wide_leader());
    assert!(s.cell(1, 0).unwrap().is_continuation());
    assert_eq!(ch(&s, 2, 0), 'x');
}

#[test]
fn zwj_family_joins_into_one_wide_cell() {
    // 👨‍👩‍👦: one cluster, one leader + one continuation (render.md §2.5
    // convention: joined sequences are a single ≤2-wide cell).
    let family = "\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F466}";
    let mut s = screen(8, 1);
    s.feed(family.as_bytes());
    s.feed(b"x");
    assert_eq!(s.cell(0, 0).unwrap().display(), family);
    assert!(s.cell(0, 0).unwrap().is_wide_leader());
    assert!(s.cell(1, 0).unwrap().is_continuation());
    assert_eq!(
        ch(&s, 2, 0),
        'x',
        "cursor advanced exactly 2 for the whole family"
    );
}

#[test]
fn skin_tone_modifier_fuses_when_adjacent() {
    let mut s = screen(8, 1);
    s.feed("\u{1F44D}\u{1F3FD}x".as_bytes()); // thumbs-up + medium tone
    assert_eq!(s.cell(0, 0).unwrap().display(), "\u{1F44D}\u{1F3FD}");
    assert!(s.cell(1, 0).unwrap().is_continuation());
    assert_eq!(ch(&s, 2, 0), 'x');
    // Non-adjacent modifier (cursor moved between): standalone glyph.
    let mut s = screen(8, 1);
    s.feed("\u{1F44D}".as_bytes());
    s.feed(b"\x1b[1;5H");
    s.feed("\u{1F3FD}".as_bytes());
    assert!(s.cell(4, 0).unwrap().is_wide_leader());
}

#[test]
fn zwj_broken_by_escape_does_not_join() {
    // A cursor motion between ZWJ and the next emoji breaks the cluster:
    // the second emoji prints where the cursor is, standalone.
    let mut s = screen(10, 1);
    s.feed("\u{1F468}\u{200D}".as_bytes());
    s.feed(b"\x1b[1;7H");
    s.feed("\u{1F469}".as_bytes());
    assert!(s.cell(0, 0).unwrap().is_wide_leader());
    assert!(s.cell(6, 0).unwrap().is_wide_leader());
    assert_eq!(s.cell(6, 0).unwrap().display(), "\u{1F469}");
}

// ---------------------------------------------------------------- erasing

#[test]
fn el_variants() {
    let mut s = screen(6, 1);
    s.feed(b"abcdef\x1b[1;4H"); // cursor on 'd' (x=3)
    s.feed(b"\x1b[K"); // EL 0: cursor to end
    assert_eq!(s.to_text(), "abc\n");
    let mut s = screen(6, 1);
    s.feed(b"abcdef\x1b[1;4H\x1b[1K"); // EL 1: start THROUGH cursor
    assert_eq!(s.to_text(), "    ef\n");
    let mut s = screen(6, 1);
    s.feed(b"abcdef\x1b[2K");
    assert_eq!(s.to_text(), "\n");
}

#[test]
fn ed_variants() {
    let make = || {
        let mut s = screen(3, 3);
        s.feed(b"abc\r\ndef\r\nghi\x1b[2;2H"); // cursor on 'e'
        s
    };
    let mut s = make();
    s.feed(b"\x1b[J"); // ED 0: cursor to end of screen
    assert_eq!(s.to_text(), "abc\nd\n\n");
    let mut s = make();
    s.feed(b"\x1b[1J"); // ED 1: start of screen THROUGH cursor
    assert_eq!(s.to_text(), "\n  f\nghi\n");
    let mut s = make();
    s.feed(b"\x1b[2J"); // ED 2: everything (cursor unmoved)
    assert_eq!(s.to_text(), "\n\n\n");
    assert_eq!(s.cursor(), Point::new(1, 1));
}

#[test]
fn erase_uses_bce_background() {
    let mut s = screen(4, 1);
    s.feed(b"\x1b[48;2;9;8;7mab\x1b[1;1H\x1b[K");
    let cell = s.cell(0, 0).unwrap();
    assert_eq!(cell.content, CellContent::Blank);
    assert_eq!(cell.paint.bg, Some(Rgba::rgb(9, 8, 7)));
    // BCE keeps ONLY bg: fg/attrs reset in the erased cells.
    assert_eq!(cell.paint.fg, None);
    assert!(cell.paint.attrs.is_empty());
}

#[test]
fn el_through_wide_pair_repairs_boundary() {
    let mut s = screen(6, 1);
    s.feed("ab你cd".as_bytes()); // 你 occupies cols 2-3
    s.feed(b"\x1b[1;4H\x1b[K"); // erase from the continuation cell
                                // Leader (col 2) must not survive as half a glyph.
    assert_eq!(s.cell(2, 0).unwrap().content, CellContent::Blank);
    assert_eq!(s.to_text(), "ab\n");
}

#[test]
fn ech_erases_without_moving() {
    let mut s = screen(6, 1);
    s.feed(b"abcdef\x1b[1;2H\x1b[3X");
    assert_eq!(s.to_text(), "a   ef\n");
    assert_eq!(s.cursor(), Point::new(1, 0));
}

// ---------------------------------------------------------- cursor motion

#[test]
fn cup_is_one_based_and_clamped() {
    let mut s = screen(10, 5);
    s.feed(b"\x1b[3;4H");
    assert_eq!(s.cursor(), Point::new(3, 2));
    s.feed(b"\x1b[999;999H");
    assert_eq!(s.cursor(), Point::new(9, 4));
    s.feed(b"\x1b[H");
    assert_eq!(s.cursor(), Point::ZERO);
    s.feed(b"\x1b[0;0H"); // explicit 0 == default == 1
    assert_eq!(s.cursor(), Point::ZERO);
}

#[test]
fn relative_motion_clamps_at_edges() {
    let mut s = screen(4, 3);
    s.feed(b"\x1b[99A\x1b[99D");
    assert_eq!(s.cursor(), Point::ZERO);
    s.feed(b"\x1b[99B\x1b[99C");
    assert_eq!(s.cursor(), Point::new(3, 2));
    s.feed(b"\x1b[A\x1b[D");
    assert_eq!(s.cursor(), Point::new(2, 1));
    // Zero-count motion means one (xterm normalization).
    s.feed(b"\x1b[0A");
    assert_eq!(s.cursor(), Point::new(2, 0));
}

#[test]
fn cha_vpa_cnl_cpl() {
    let mut s = screen(8, 4);
    s.feed(b"\x1b[2;2H\x1b[5G");
    assert_eq!(s.cursor(), Point::new(4, 1));
    s.feed(b"\x1b[3d");
    assert_eq!(s.cursor(), Point::new(4, 2));
    s.feed(b"\x1b[E");
    assert_eq!(s.cursor(), Point::new(0, 3));
    s.feed(b"\x1b[2F");
    assert_eq!(s.cursor(), Point::new(0, 1));
}

#[test]
fn save_restore_cursor_and_paint() {
    let mut s = screen(10, 3);
    s.feed(b"\x1b[31m\x1b[2;5H\x1b7\x1b[0m\x1b[1;1Hx\x1b8y");
    assert_eq!(ch(&s, 0, 0), 'x');
    // Restored position (4,1) AND restored red paint.
    assert_eq!(ch(&s, 4, 1), 'y');
    assert_eq!(s.cell(4, 1).unwrap().paint.fg, Some(xterm_256(1)));
    assert_eq!(s.cell(0, 0).unwrap().paint.fg, None);
}

// ------------------------------------------------------------- scrolling

#[test]
fn linefeed_at_bottom_scrolls_up() {
    let mut s = screen(3, 2);
    s.feed(b"ab\r\ncd\r\nef");
    assert_eq!(s.to_text(), "cd\nef\n");
    assert_eq!(s.cursor(), Point::new(2, 1));
}

#[test]
fn reverse_index_at_top_scrolls_down() {
    let mut s = screen(3, 2);
    s.feed(b"ab\x1b[1;1H\x1bMx");
    assert_eq!(s.to_text(), "x\nab\n");
}

#[test]
fn explicit_scroll_regions_su_sd() {
    let mut s = screen(3, 3);
    s.feed(b"a\r\nb\r\nc\x1b[2S");
    assert_eq!(s.to_text(), "c\n\n\n");
    let mut s = screen(3, 3);
    s.feed(b"a\r\nb\r\nc\x1b[1T");
    assert_eq!(s.to_text(), "\na\nb\n");
}

#[test]
fn scroll_fill_uses_bce() {
    let mut s = screen(2, 2);
    s.feed(b"\x1b[48;5;196mab\r\ncd\r\n"); // scrolls once at the bottom
    let fill = s.cell(0, 1).unwrap();
    assert_eq!(fill.content, CellContent::Blank);
    assert_eq!(fill.paint.bg, Some(xterm_256(196)));
}
