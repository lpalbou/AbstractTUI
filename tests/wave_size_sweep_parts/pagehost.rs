//! Axis (b) — PageHost tab bar with 6 titled tabs at 60 and 40
//! columns: the overflow window (sticky anchor + `‹`/`›` indicator
//! zones), chord navigation to off-window tabs, indicator clicks, and
//! the single-oversized-title ellipsis. Bar rows are goldened at both
//! widths so a regression in the windowing math names itself.

use abstracttui::app::{App, Driver};
use abstracttui::base::Size;
use abstracttui::testing::{assert_snapshot, CaptureTerm, VtScreen};
use abstracttui::ui::text;
use abstracttui::widgets::PageHost;

use crate::harness::{config, drive_to_idle};

/// SGR left click (press + release) at 1-BASED terminal coordinates.
fn sgr_click(col: i32, row: i32) -> Vec<u8> {
    format!("\x1b[<0;{col};{row}M\x1b[<0;{col};{row}m").into_bytes()
}

const CTRL_PGDN: &[u8] = b"\x1b[6;5~";
const CTRL_PGUP: &[u8] = b"\x1b[5;5~";

/// The console fixture: six real page titles (49 columns of natural
/// segment demand — overflow at both 60 and 40).
fn six_tab_host(size: Size) -> App {
    let mut app = App::new(size);
    app.mount(|cx| {
        PageHost::new()
            .page("dash", "Dashboard", |_| text("PAGE-DASHBOARD"))
            .page("sessions", "Sessions", |_| text("PAGE-SESSIONS"))
            .page("artifacts", "Artifacts", |_| text("PAGE-ARTIFACTS"))
            .page("providers", "Providers", |_| text("PAGE-PROVIDERS"))
            .page("logs", "Logs", |_| text("PAGE-LOGS"))
            .page("settings", "Settings", |_| text("PAGE-SETTINGS"))
            .view(cx)
    })
    .expect("mount");
    app
}

struct Rig {
    app: App,
    term: CaptureTerm,
    driver: Driver,
    vt: VtScreen,
}

fn rig(size: Size) -> Rig {
    let mut app = six_tab_host(size);
    let mut term = CaptureTerm::new(size);
    let mut driver = Driver::new(&mut app, &mut term, config()).expect("driver");
    let mut vt = VtScreen::new(size);
    drive_to_idle(&mut driver, &mut app, &mut term, &mut vt);
    Rig {
        app,
        term,
        driver,
        vt,
    }
}

impl Rig {
    fn input(&mut self, bytes: &[u8]) {
        self.term.push_input(bytes);
        drive_to_idle(
            &mut self.driver,
            &mut self.app,
            &mut self.term,
            &mut self.vt,
        );
    }

    fn bar_row(&self) -> String {
        self.vt
            .to_text()
            .lines()
            .next()
            .unwrap_or_default()
            .to_string()
    }

    fn screen(&self) -> String {
        self.vt.to_text()
    }
}

/// 60 columns: the strip windows, the right indicator shows more, the
/// active tab is always inside the window, and chords reach the last
/// tab (window slides, left indicator appears).
#[test]
fn six_tabs_at_60_cols_window_and_navigate() {
    let mut r = rig(Size::new(60, 10));
    let bar = r.bar_row();
    assert!(bar.contains("Dashboard"), "active tab visible: {bar:?}");
    assert!(
        bar.contains('›'),
        "right overflow indicator must show: {bar:?}"
    );
    assert!(
        !bar.contains('‹'),
        "nothing hidden left at the start: {bar:?}"
    );
    assert_snapshot("sweep_pagehost_bar_60_first", &format!("{bar}\n"));

    // Chord to the last tab: the window slides minimally, the left
    // indicator appears, the page switches.
    for _ in 0..5 {
        r.input(CTRL_PGDN);
    }
    let bar = r.bar_row();
    assert!(
        bar.contains("Settings"),
        "active tab must be windowed in: {bar:?}"
    );
    assert!(bar.contains('‹'), "left indicator after sliding: {bar:?}");
    assert!(
        r.screen().contains("PAGE-SETTINGS"),
        "last page mounted:\n{}",
        r.screen()
    );
    assert_snapshot("sweep_pagehost_bar_60_last", &format!("{bar}\n"));

    // And back: Ctrl+PgUp returns; the sticky window follows.
    for _ in 0..5 {
        r.input(CTRL_PGUP);
    }
    assert!(
        r.screen().contains("PAGE-DASHBOARD"),
        "back to the first page:\n{}",
        r.screen()
    );
}

/// 40 columns: still usable — indicators render, the window holds the
/// active tab, clicking `›` advances, clicking a visible tab selects.
#[test]
fn six_tabs_at_40_cols_stay_usable() {
    let mut r = rig(Size::new(40, 10));
    let bar = r.bar_row();
    assert!(bar.contains("Dashboard"), "active visible: {bar:?}");
    assert!(bar.contains('›'), "right indicator: {bar:?}");
    assert_snapshot("sweep_pagehost_bar_40_first", &format!("{bar}\n"));

    // Click the right indicator zone (last column, bar row 1): next.
    r.input(&sgr_click(40, 1));
    assert!(
        r.screen().contains("PAGE-SESSIONS"),
        "indicator click advances:\n{}",
        r.screen()
    );

    // Chord to the far end: the strip must window Settings in.
    for _ in 0..4 {
        r.input(CTRL_PGDN);
    }
    let bar = r.bar_row();
    assert!(
        bar.contains("Settings"),
        "active windowed in at 40 cols: {bar:?}"
    );
    assert!(bar.contains('‹'), "left indicator: {bar:?}");
    assert!(
        r.screen().contains("PAGE-SETTINGS"),
        "page switched:\n{}",
        r.screen()
    );
    assert_snapshot("sweep_pagehost_bar_40_last", &format!("{bar}\n"));

    // Click a VISIBLE tab segment by its painted title position: the
    // hit test consumes the same plan the draw painted (no drift).
    let bar = r.bar_row();
    let col = bar.find("Settings").expect("title on the bar") as i32;
    r.input(&sgr_click(col + 1, 1));
    assert!(
        r.screen().contains("PAGE-SETTINGS"),
        "clicking the active tab keeps it:\n{}",
        r.screen()
    );
}

/// A single tab wider than the whole 40-col budget truncates its title
/// with an ellipsis — never paints into the indicator zone, never
/// panics.
#[test]
fn oversized_single_tab_truncates_with_ellipsis() {
    let size = Size::new(40, 8);
    let mut app = App::new(size);
    app.mount(|cx| {
        PageHost::new()
            .page(
                "huge",
                "An Extremely Long Tab Title That Cannot Possibly Fit Here",
                |_| text("PAGE-HUGE"),
            )
            .page("tiny", "Tiny", |_| text("PAGE-TINY"))
            .view(cx)
    })
    .expect("mount");
    let mut term = CaptureTerm::new(size);
    let mut driver = Driver::new(&mut app, &mut term, config()).expect("driver");
    let mut vt = VtScreen::new(size);
    drive_to_idle(&mut driver, &mut app, &mut term, &mut vt);
    let bar: String = vt.to_text().lines().next().unwrap_or_default().to_string();
    assert!(
        bar.contains('…'),
        "oversized title truncates with ellipsis: {bar:?}"
    );
    assert!(
        vt.to_text().contains("PAGE-HUGE"),
        "page still renders:\n{}",
        vt.to_text()
    );
    assert_snapshot("sweep_pagehost_bar_40_oversized", &format!("{bar}\n"));
}
