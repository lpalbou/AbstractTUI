//! Cycle-2 cross-review discriminating cases (reviews/study/
//! r2-cross-review.md). Two gaps the wave's own suites left open:
//!
//! 1. The FLIPPED Combobox face ordering: `combobox_popup_content`
//!    orders `status / rows / editor` when the popup opens ABOVE the
//!    anchor (select_combobox.rs), so the editor stays on the trigger
//!    row — the wave tests only exercise the below-mode ordering, and
//!    the flip path of `place_owned` had geometry-only unit coverage.
//! 2. Diff-lexer robustness against real `git diff` shapes the unit
//!    table skips: `/dev/null` file headers (add/delete patches),
//!    rename similarity lines, and CRLF-terminated input.
//!
//! Same harness posture as wave_select_faces.rs (helper duplication
//! across integration files is the house style — each is its own
//! crate).

use std::cell::RefCell;
use std::rc::Rc;

use abstracttui::app::{App, Driver, RunConfig};
use abstracttui::base::Size;
use abstracttui::prelude::*;
use abstracttui::term::Capabilities;
use abstracttui::testing::CaptureTerm;
use abstracttui::text::{DiffKind, DiffLexer};
use abstracttui::ui::text;

const W: i32 = 44;
const H: i32 = 10;

fn config() -> RunConfig {
    RunConfig {
        caps: Some(Capabilities::with(|c| {
            c.truecolor = true;
            c.colors_256 = true;
        })),
        enter: None,
        probe: false,
    }
}

fn settle(driver: &mut Driver, app: &mut App, term: &mut CaptureTerm) {
    for _ in 0..64 {
        let turn = driver.turn(app, term).expect("turn");
        if turn.idle {
            return;
        }
    }
    panic!("loop failed to settle within 64 turns");
}

fn screen_lines(term: &CaptureTerm) -> Vec<String> {
    term.screen()
        .to_text()
        .lines()
        .map(str::to_string)
        .collect()
}

fn row_of(lines: &[String], needle: &str) -> Option<usize> {
    lines.iter().position(|l| l.contains(needle))
}

/// A bottom-anchored Combobox must FLIP its popup above the trigger,
/// keep the editor ON the trigger row (the popup's LAST row when
/// flipped), render the option rows ABOVE it, and still type/commit
/// through the editor exactly as in below mode.
#[test]
fn flipped_combobox_keeps_editor_on_trigger_row_with_options_above() {
    let mut app = App::new(Size::new(W, H));
    let holder: Rc<RefCell<Option<Signal<usize>>>> = Default::default();
    let v2 = holder.clone();
    app.mount(move |cx| {
        let value = cx.signal(usize::MAX);
        *v2.borrow_mut() = Some(value);
        Element::new()
            .style(LayoutStyle::column().grow(1.0))
            .child(text("== header =="))
            // Grow spacer pushes the picker to the bottom of the
            // viewport, so below-space is 1 row and the popup flips.
            .child(
                Element::new()
                    .style(LayoutStyle::default().grow(1.0))
                    .build(),
            )
            .child(
                Element::new()
                    .style(LayoutStyle::row().h(1).shrink(0.0))
                    .child(
                        Combobox::new(vec![
                            SelectOption::new("gpt-tiny"),
                            SelectOption::new("gpt-large"),
                            SelectOption::new("qwen-mini"),
                            SelectOption::new("qwen-max"),
                        ])
                        .value(value)
                        .placeholder("model…")
                        .layout(LayoutStyle::default().w(22).h(1).shrink(0.0))
                        .view(cx),
                    )
                    .build(),
            )
            .child(text(" bottom status"))
            .build()
    })
    .expect("mount");
    let value = holder.borrow().expect("value");

    let mut term = CaptureTerm::new(Size::new(W, H));
    let mut driver = Driver::new(&mut app, &mut term, config()).expect("driver");
    settle(&mut driver, &mut app, &mut term);
    let lines = screen_lines(&term);
    let trigger_row = row_of(&lines, "model…").expect("placeholder rendered");
    assert!(
        trigger_row >= H as usize - 3,
        "trigger sits near the bottom: row {trigger_row} of {lines:?}"
    );

    // Open: Tab focuses the trigger, Enter opens the popup.
    term.push_input(b"\t\r");
    settle(&mut driver, &mut app, &mut term);
    let lines = screen_lines(&term);
    // The editor stays exactly on the trigger row (anchor-row
    // inclusion, flipped: the popup's LAST row) — its frame stroke is
    // the tell.
    assert!(
        lines[trigger_row].contains('▐'),
        "editor frame on the trigger row when flipped: {lines:?}"
    );
    // Every option row renders ABOVE the trigger row (flip), and the
    // status line sits above the options (flipped ordering:
    // status / rows / editor).
    let opt_row = row_of(&lines, "gpt-tiny").expect("options visible");
    assert!(
        opt_row < trigger_row,
        "options above the flipped editor: option row {opt_row}, editor row {trigger_row}: {lines:?}"
    );
    let status_row = row_of(&lines, "4 of 4").expect("status line visible");
    assert!(
        status_row < opt_row,
        "status above the options when flipped: status {status_row}, options {opt_row}: {lines:?}"
    );

    // Typing lands in the flipped editor on the trigger row and
    // refilters the rows above.
    term.push_input(b"qwen");
    settle(&mut driver, &mut app, &mut term);
    let lines = screen_lines(&term);
    assert!(
        lines[trigger_row].contains("qwen"),
        "typed query renders on the trigger row: {lines:?}"
    );
    assert!(
        row_of(&lines, "gpt-tiny").is_none(),
        "non-matches filtered out: {lines:?}"
    );
    assert!(row_of(&lines, "2 of 4").is_some(), "{lines:?}");

    // Enter commits the first match (qwen-mini, index 2) and closes.
    term.push_input(b"\r");
    settle(&mut driver, &mut app, &mut term);
    assert_eq!(value.get_untracked(), 2, "commit through the flipped face");
    let lines = screen_lines(&term);
    assert!(
        lines[trigger_row].contains("qwen-mini"),
        "trigger shows the committed label: {lines:?}"
    );
    assert!(
        row_of(&lines, "4 of 4").is_none(),
        "popup closed and vacated rows repainted: {lines:?}"
    );

    driver.finish(&mut term).expect("leave");
    assert_eq!(term.screen().unknown_seq_count(), 0, "all bytes modeled");
}

// ------------------------------------------------------------- diff lexer

fn kind_of(line: &str) -> DiffKind {
    let spans = DiffLexer::new().spans(line);
    assert_eq!(
        spans.len(),
        1,
        "one whole-line span for {line:?}: {spans:?}"
    );
    assert_eq!(spans[0].0, 0..line.len(), "whole line for {line:?}");
    spans[0].1
}

/// Real `git diff` shapes the in-module unit table skips: add/delete
/// patches header against `/dev/null`, and pure-rename patches carry
/// `similarity index` + `rename from/to` with no hunks at all.
#[test]
fn diff_lexer_classifies_dev_null_headers_and_rename_chrome() {
    assert_eq!(kind_of("--- /dev/null"), DiffKind::FileHeader);
    assert_eq!(kind_of("+++ /dev/null"), DiffKind::FileHeader);
    assert_eq!(kind_of("similarity index 100%"), DiffKind::Meta);
    assert_eq!(kind_of("dissimilarity index 3%"), DiffKind::Meta);
    assert_eq!(kind_of("copy from src/a.rs"), DiffKind::Meta);
    assert_eq!(kind_of("copy to src/b.rs"), DiffKind::Meta);
    assert_eq!(kind_of("old mode 100644"), DiffKind::Meta);
    assert_eq!(kind_of("GIT binary patch"), DiffKind::Meta);
    assert_eq!(kind_of("Only in a: notes.txt"), DiffKind::Meta);
}

/// CRLF input: consumers that split on `\n` hand the lexer lines with a
/// trailing `\r`. Classification must hold (prefix rules are unharmed
/// by a trailing byte) and every span must stay on char boundaries.
#[test]
fn diff_lexer_tolerates_crlf_terminated_lines() {
    assert_eq!(kind_of("+added\r"), DiffKind::Added);
    assert_eq!(kind_of("-removed\r"), DiffKind::Removed);
    assert_eq!(kind_of(" context\r"), DiffKind::Context);
    assert_eq!(kind_of("--- a/x.rs\r"), DiffKind::FileHeader);
    assert_eq!(kind_of("diff --git a/x b/x\r"), DiffKind::Meta);
    // Hunk header: the `\r` falls into the trailing Context span, never
    // inside the `@@ … @@` range, and boundaries stay valid.
    let line = "@@ -1,2 +1,2 @@\r";
    let spans = DiffLexer::new().spans(line);
    assert_eq!(spans[0].1, DiffKind::HunkHeader);
    assert_eq!(&line[spans[0].0.clone()], "@@ -1,2 +1,2 @@");
    let mut prev_end = 0;
    for (r, _) in &spans {
        assert!(r.start >= prev_end && r.end <= line.len());
        assert!(line.is_char_boundary(r.start) && line.is_char_boundary(r.end));
        prev_end = r.end;
    }
    // A lone `\r` line (blank line in a CRLF file) is untinted context.
    assert_eq!(kind_of("\r"), DiffKind::Context);
}
