//! SGR, color, mode-flag, hyperlink and dump semantics of the VT model
//! (REDTEAM). Complements tests/vt_screen.rs (geometry/content).

use abstracttui::base::{Rgba, Size};
use abstracttui::testing::{assert_snapshot, xterm_256, Attrs, VtScreen, SYSTEM_16};

fn screen(w: i32, h: i32) -> VtScreen {
    VtScreen::new(Size::new(w, h))
}

fn paint_at(s: &VtScreen, x: i32, y: i32) -> abstracttui::testing::Paint {
    s.cell(x, y).unwrap().paint
}

// -------------------------------------------------------------------- SGR

#[test]
fn sgr_attribute_set_and_reset_pairs() {
    let mut s = screen(20, 1);
    s.feed(b"\x1b[1;2;3;4;7;9ma");
    let p = paint_at(&s, 0, 0);
    for bit in [
        Attrs::BOLD,
        Attrs::DIM,
        Attrs::ITALIC,
        Attrs::UNDERLINE,
        Attrs::REVERSE,
        Attrs::STRIKE,
    ] {
        assert!(p.attrs.contains(bit));
    }
    // Selective resets: 22 clears bold+dim, 23/24/27/29 clear their bit.
    s.feed(b"\x1b[22;23;24;27;29mb");
    assert!(paint_at(&s, 1, 0).attrs.is_empty());
    // The 'a' cell must still carry the original attributes: SGR state
    // changes never rewrite already-painted cells.
    assert!(paint_at(&s, 0, 0).attrs.contains(Attrs::BOLD));
    assert_eq!(s.unknown_seq_count(), 0);
}

#[test]
fn sgr_zero_resets_everything_but_link_state() {
    let mut s = screen(8, 1);
    s.feed(b"\x1b]8;;http://x\x1b\\\x1b[1;31;42ma\x1b[0mb");
    let a = paint_at(&s, 0, 0);
    assert!(a.attrs.contains(Attrs::BOLD) && a.fg.is_some() && a.bg.is_some());
    let b = paint_at(&s, 1, 0);
    assert!(b.attrs.is_empty() && b.fg.is_none() && b.bg.is_none());
    // OSC 8 is closed by OSC 8, not by SGR 0 (kitty/wezterm semantics).
    assert_eq!(b.link, a.link);
    assert!(b.link.is_some());
}

#[test]
fn sgr_empty_param_means_reset() {
    let mut s = screen(4, 1);
    s.feed(b"\x1b[1ma\x1b[mb");
    assert!(paint_at(&s, 1, 0).attrs.is_empty());
}

#[test]
fn sgr_blink_hidden_undercurl_pairs() {
    // The full presenter emission set from render.md §2.4: additions
    // 1,2,3,4,4:3,5,7,8,9 and removals 22,23,24,25,27,28,29.
    let mut s = screen(8, 1);
    s.feed(b"\x1b[5;8ma");
    let p = paint_at(&s, 0, 0);
    assert!(p.attrs.contains(Attrs::BLINK) && p.attrs.contains(Attrs::HIDDEN));
    s.feed(b"\x1b[25;28mb");
    assert!(paint_at(&s, 1, 0).attrs.is_empty());
    // Undercurl via colon sub-param; 24 clears BOTH underline kinds.
    s.feed(b"\x1b[4:3mc");
    assert!(paint_at(&s, 2, 0).attrs.contains(Attrs::UNDERCURL));
    s.feed(b"\x1b[4md");
    let d = paint_at(&s, 3, 0);
    assert!(d.attrs.contains(Attrs::UNDERLINE) && d.attrs.contains(Attrs::UNDERCURL));
    s.feed(b"\x1b[24me");
    assert!(paint_at(&s, 4, 0).attrs.is_empty());
    assert_eq!(s.unknown_seq_count(), 0);
}

#[test]
fn truecolor_semicolon_and_colon_forms() {
    let mut s = screen(8, 1);
    s.feed(b"\x1b[38;2;10;20;30ma");
    assert_eq!(paint_at(&s, 0, 0).fg, Some(Rgba::rgb(10, 20, 30)));
    s.feed(b"\x1b[48;2;1;2;3mb");
    assert_eq!(paint_at(&s, 1, 0).bg, Some(Rgba::rgb(1, 2, 3)));
    // Colon sub-parameter forms, with and without the colorspace slot.
    s.feed(b"\x1b[38:2:40:50:60mc");
    assert_eq!(paint_at(&s, 2, 0).fg, Some(Rgba::rgb(40, 50, 60)));
    s.feed(b"\x1b[38:2::70:80:90md");
    assert_eq!(paint_at(&s, 3, 0).fg, Some(Rgba::rgb(70, 80, 90)));
    s.feed(b"\x1b[38:5:196me");
    assert_eq!(paint_at(&s, 4, 0).fg, Some(Rgba::rgb(255, 0, 0)));
    assert_eq!(s.unknown_seq_count(), 0);
}

#[test]
fn palette_256_maps_through_xterm_table() {
    let mut s = screen(8, 1);
    s.feed(b"\x1b[38;5;196ma\x1b[48;5;21mb\x1b[38;5;244mc\x1b[38;5;231md");
    assert_eq!(paint_at(&s, 0, 0).fg, Some(Rgba::rgb(255, 0, 0)));
    assert_eq!(paint_at(&s, 1, 0).bg, Some(Rgba::rgb(0, 0, 255)));
    assert_eq!(paint_at(&s, 2, 0).fg, Some(Rgba::rgb(128, 128, 128)));
    assert_eq!(paint_at(&s, 3, 0).fg, Some(Rgba::WHITE));
}

#[test]
fn basic_and_bright_colors_map_to_system_palette() {
    let mut s = screen(8, 1);
    s.feed(b"\x1b[31ma\x1b[94mb\x1b[42mc\x1b[103md");
    assert_eq!(paint_at(&s, 0, 0).fg, Some(SYSTEM_16[1]));
    assert_eq!(paint_at(&s, 1, 0).fg, Some(SYSTEM_16[12]));
    assert_eq!(paint_at(&s, 2, 0).bg, Some(SYSTEM_16[2]));
    assert_eq!(paint_at(&s, 3, 0).bg, Some(SYSTEM_16[11]));
    // 38;5;N for N<16 agrees with the direct forms.
    assert_eq!(SYSTEM_16[1], xterm_256(1));
}

#[test]
fn default_colors_39_49_are_distinct_from_any_rgb() {
    let mut s = screen(6, 1);
    s.feed(b"\x1b[31;41ma\x1b[39mb\x1b[49mc");
    assert_eq!(paint_at(&s, 1, 0).fg, None);
    assert_eq!(paint_at(&s, 1, 0).bg, Some(SYSTEM_16[1]));
    assert_eq!(paint_at(&s, 2, 0).bg, None);
}

#[test]
fn multiple_sgr_codes_in_one_sequence_apply_in_order() {
    let mut s = screen(4, 1);
    s.feed(b"\x1b[0;1;38;2;5;6;7;4ma");
    let p = paint_at(&s, 0, 0);
    assert!(p.attrs.contains(Attrs::BOLD));
    assert!(p.attrs.contains(Attrs::UNDERLINE));
    assert_eq!(p.fg, Some(Rgba::rgb(5, 6, 7)));
    // Codes AFTER an embedded extended color still parse (consumption
    // arithmetic — the classic off-by-one in SGR readers).
    assert_eq!(s.unknown_seq_count(), 0);
}

#[test]
fn malformed_extended_color_is_counted_not_misread() {
    for bytes in [
        b"\x1b[38m".as_slice(),
        b"\x1b[38;5m".as_slice(),
        b"\x1b[38;2;1;2m".as_slice(),
        b"\x1b[38;9;1;2;3m".as_slice(),
    ] {
        let mut s = screen(4, 1);
        s.feed(bytes);
        s.feed(b"a");
        assert_eq!(s.unknown_seq_count(), 1, "for {:?}", bytes);
        // Whatever happened, no color was invented.
        assert_eq!(paint_at(&s, 0, 0).fg, None, "for {:?}", bytes);
    }
}

#[test]
fn out_of_range_color_components_clamp() {
    let mut s = screen(4, 1);
    s.feed(b"\x1b[38;2;999;0;300ma\x1b[48;5;9999mb");
    assert_eq!(paint_at(&s, 0, 0).fg, Some(Rgba::rgb(255, 0, 255)));
    assert_eq!(paint_at(&s, 1, 0).bg, Some(xterm_256(255)));
}

// ------------------------------------------------------------------ modes

#[test]
fn decset_decrst_track_mode_flags() {
    let mut s = screen(4, 1);
    assert!(s.modes().cursor_visible() && s.modes().autowrap());
    s.feed(b"\x1b[?2026h\x1b[?25l\x1b[?2004h\x1b[?1004h\x1b[?1006h");
    assert!(s.modes().synchronized_output());
    assert!(!s.modes().cursor_visible());
    assert!(s.modes().bracketed_paste());
    assert!(s.modes().focus_reporting());
    assert!(s.modes().sgr_mouse());
    s.feed(b"\x1b[?2026l\x1b[?25h");
    assert!(!s.modes().synchronized_output());
    assert!(s.modes().cursor_visible());
    assert_eq!(s.counters().sync_begins, 1);
    assert_eq!(s.counters().sync_ends, 1);
    assert_eq!(s.unknown_seq_count(), 0);
}

#[test]
fn multi_param_decset_sets_all() {
    let mut s = screen(4, 1);
    s.feed(b"\x1b[?1049;25;2004h");
    assert!(s.modes().alt_screen());
    assert!(s.modes().cursor_visible());
    assert!(s.modes().bracketed_paste());
}

#[test]
fn alt_screen_entry_clears_and_homes() {
    let mut s = screen(4, 2);
    s.feed(b"junk\x1b[?1049h");
    assert_eq!(s.to_text(), "\n\n");
    assert_eq!(s.cursor(), abstracttui::base::Point::ZERO);
}

#[test]
fn kitty_push_pop_depth_balances() {
    let mut s = screen(4, 1);
    s.feed(b"\x1b[>3u");
    assert_eq!(s.counters().kitty_push_depth, 1);
    s.feed(b"\x1b[<u");
    assert_eq!(s.counters().kitty_push_depth, 0);
    s.feed(b"\x1b[<u"); // over-pop clamps, never underflows
    assert_eq!(s.counters().kitty_push_depth, 0);
    assert_eq!(s.unknown_seq_count(), 0);
}

// -------------------------------------------------------------- hyperlinks

#[test]
fn osc8_links_are_tracked_per_cell() {
    let mut s = screen(10, 1);
    s.feed(b"\x1b]8;;https://a\x1b\\A\x1b]8;;\x1b\\B\x1b]8;id=7;https://c\x07C");
    let a = paint_at(&s, 0, 0).link.expect("A linked");
    assert_eq!(paint_at(&s, 1, 0).link, None);
    let c = paint_at(&s, 2, 0).link.expect("C linked");
    assert_ne!(a, c);
    assert_eq!(s.link_target(a), Some(("", "https://a")));
    assert_eq!(s.link_target(c), Some(("id=7", "https://c")));
    assert_eq!(s.unknown_seq_count(), 0);
}

#[test]
fn same_uri_reuses_link_id() {
    let mut s = screen(10, 1);
    s.feed(b"\x1b]8;;u\x1b\\a\x1b]8;;\x1b\\\x1b]8;;u\x1b\\b");
    assert_eq!(paint_at(&s, 0, 0).link, paint_at(&s, 1, 0).link);
}

// ------------------------------------------------------------------ dumps

#[test]
fn styled_dump_is_deterministic_and_snapshotted() {
    let mut s = screen(12, 4);
    s.feed(b"\x1b[?25l\x1b[2J\x1b[1;1H\x1b[1;38;2;233;69;96mTitle\x1b[0m");
    s.feed("\x1b[2;1H\x1b[48;5;24m 日本 \x1b[0m".as_bytes());
    s.feed(b"\x1b[3;1H\x1b]8;;https://x\x1b\\link\x1b]8;;\x1b\\ plain");
    let dump = s.to_styled_dump();
    let again = s.to_styled_dump();
    assert_eq!(dump, again);
    assert_snapshot("vt_styled_dump_basic", &dump);
    assert_eq!(s.unknown_seq_count(), 0);
}

#[test]
fn to_text_trims_trailing_blanks_only() {
    let mut s = screen(6, 2);
    s.feed(b"a b  \r\n  c");
    assert_eq!(s.to_text(), "a b\n  c\n");
}

// ------------------------------------------------------------ cleanliness

#[test]
fn full_reset_esc_c() {
    let mut s = screen(4, 2);
    s.feed(b"\x1b[31mhi\x1bc");
    assert_eq!(s.to_text(), "\n\n");
    assert!(s.modes().cursor_visible());
    s.feed(b"x");
    assert_eq!(paint_at(&s, 0, 0).fg, None);
}

#[test]
fn unknown_sequences_are_counted_with_samples() {
    let mut s = screen(4, 1);
    s.feed(b"\x1b[99z"); // unknown final
    s.feed(b"\x1b[53m"); // overline: deliberately outside the modeled set
    s.feed(b"\x07"); // BEL in ground
    assert_eq!(s.unknown_seq_count(), 3);
    assert!(!s.unknown_samples().is_empty());
    assert!(s.unknown_samples()[0].contains('z'));
}

#[test]
fn queries_are_not_unknown() {
    // The presenter/probe may legally emit queries; they paint nothing
    // and must not count as dirt.
    let mut s = screen(4, 1);
    s.feed(b"\x1b[c\x1b[>0q\x1b[6n\x1b[?2026$p");
    assert_eq!(s.unknown_seq_count(), 0);
    assert_eq!(s.to_text(), "\n");
}

#[test]
fn osc_title_is_tracked() {
    let mut s = screen(4, 1);
    s.feed(b"\x1b]0;My App\x07");
    assert_eq!(s.title(), Some("My App"));
    assert_eq!(s.unknown_seq_count(), 0);
}
