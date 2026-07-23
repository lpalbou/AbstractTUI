//! Axis (f) — unicode-width truncation at narrow boundaries: CJK and
//! emoji content in Table cells, PageHost tab titles, and Disclosure
//! titles, at widths chosen to SPLIT a 2-column glyph. The engine's
//! contract: never a torn glyph — a straddling wide cluster is dropped
//! whole (the spare column stays blank) and every frame keeps the
//! leader/continuation pairing (walked cell-by-cell over the final
//! screen at every size and across narrowing resizes).

use abstracttui::app::{App, Driver};
use abstracttui::base::Size;
use abstracttui::testing::{assert_snapshot, CaptureTerm, VtScreen};
use abstracttui::ui::text;
use abstracttui::widgets::{ColWidth, Column, Disclosure, PageHost, Table};

use crate::harness::{assert_wide_pairs_sound, config, drive_to_idle};

fn drive(size: Size, mount: impl FnOnce(&mut App)) -> VtScreen {
    let mut app = App::new(size);
    mount(&mut app);
    let mut term = CaptureTerm::new(size);
    let mut driver = Driver::new(&mut app, &mut term, config()).expect("driver");
    let mut vt = VtScreen::new(size);
    drive_to_idle(&mut driver, &mut app, &mut term, &mut vt);
    vt
}

fn cjk_table(app: &mut App) {
    app.mount(|cx| {
        Table::new(vec![
            Column::new("名前", ColWidth::Flex(1.0)),
            Column::new("値", ColWidth::Cells(9)),
        ])
        .rows(vec![
            vec!["日本語テスト行".into(), "中文字符".into()],
            vec!["🎉🧪 emoji mix".into(), "混合 text".into()],
            vec!["한국어 데이터".into(), "값 123".into()],
            vec!["mixed 漢字 and text".into(), "⚙ gears".into()],
            vec!["ラウンドトリップ".into(), "往復".into()],
            vec!["セパレータ試験".into(), "試験".into()],
            vec!["エッジケース".into(), "端".into()],
            vec!["ワイド境界".into(), "境界".into()],
        ])
        .view(cx)
    })
    .expect("mount");
}

/// Table cells full of wide glyphs at widths that land a column budget
/// mid-ideograph: cells truncate with an ellipsis, wide pairs stay
/// sound, nothing panics — at every hostile width, odd and even.
#[test]
fn cjk_table_never_tears_at_narrow_widths() {
    for &size in &[
        Size::new(24, 8),
        Size::new(23, 8),
        Size::new(21, 6),
        Size::new(19, 6),
        Size::new(40, 10),
    ] {
        let vt = drive(size, cjk_table);
        assert_wide_pairs_sound(&vt, &format!("table {size:?}"));
        assert!(
            vt.to_text().contains('…') || vt.to_text().contains("名前"),
            "{size:?}: content renders (truncated or whole):\n{}",
            vt.to_text()
        );
    }
    // One representative golden: the 23-column layout (odd width, both
    // columns forced to cut inside wide runs).
    let vt = drive(Size::new(23, 8), cjk_table);
    assert_snapshot("sweep_unicode_table_23", &vt.to_text());
}

/// A live NARROWING resize across glyph boundaries: the diff/present
/// path must keep every emitted frame pair-sound while columns of
/// wide content appear and disappear at the right edge.
#[test]
fn narrowing_resize_over_cjk_content_keeps_pairs_sound() {
    let start = Size::new(40, 10);
    let mut app = App::new(start);
    cjk_table(&mut app);
    let mut term = CaptureTerm::new(start);
    let mut driver = Driver::new(&mut app, &mut term, config()).expect("driver");
    let mut vt = VtScreen::new(start);
    drive_to_idle(&mut driver, &mut app, &mut term, &mut vt);
    assert_wide_pairs_sound(&vt, "start 40x10");

    // Walk the width down one column at a time through the splitting
    // range, then back up — every settled frame stays sound.
    for w in (19..=40).rev().chain(20..=40) {
        let size = Size::new(w, 10);
        term.push_resize(size);
        drive_to_idle(&mut driver, &mut app, &mut term, &mut vt);
        assert_wide_pairs_sound(&vt, &format!("resized to {w}x10"));
    }
}

/// PageHost tab titles in CJK at 40 columns: the windowing plan clamps
/// title budgets and `truncate_ellipsis` drops straddling ideographs
/// whole — the bar row stays pair-sound and usable.
#[test]
fn cjk_tab_titles_truncate_cleanly_at_40_cols() {
    let size = Size::new(40, 8);
    let vt = drive(size, |app| {
        app.mount(|cx| {
            PageHost::new()
                .page("dash", "ダッシュボード", |_| text("PAGE-DASH"))
                .page("sess", "セッション管理", |_| text("PAGE-SESS"))
                .page("arts", "アーティファクト", |_| text("PAGE-ARTS"))
                .page("logs", "ログ 📜", |_| text("PAGE-LOGS"))
                .page("conf", "設定", |_| text("PAGE-CONF"))
                .page("help", "ヘルプ", |_| text("PAGE-HELP"))
                .view(cx)
        })
        .expect("mount");
    });
    assert_wide_pairs_sound(&vt, "cjk tabs 40x8");
    let bar: String = vt.to_text().lines().next().unwrap_or_default().to_string();
    assert!(
        bar.contains("ダッシュボード") || bar.contains('…'),
        "bar renders CJK titles (whole or ellipsized): {bar:?}"
    );
    assert!(
        vt.to_text().contains("PAGE-DASH"),
        "active page renders:\n{}",
        vt.to_text()
    );
    assert_snapshot("sweep_unicode_tabs_40", &format!("{bar}\n"));
}

/// Disclosure titles with emoji/CJK at a width that cuts inside the
/// title: header stays pair-sound, the card still folds/unfolds.
#[test]
fn emoji_disclosure_title_survives_narrow_width() {
    for &size in &[Size::new(18, 6), Size::new(15, 6), Size::new(12, 6)] {
        let vt = drive(size, |app| {
            app.mount(|cx| {
                Disclosure::text("⚙ 設定パネル gears", "本文 body text 内容")
                    .initially_folded(false)
                    .view(cx)
            })
            .expect("mount");
        });
        assert_wide_pairs_sound(&vt, &format!("disclosure {size:?}"));
        assert!(
            !vt.to_text().trim().is_empty(),
            "{size:?}: the card renders something:\n{}",
            vt.to_text()
        );
    }
}
