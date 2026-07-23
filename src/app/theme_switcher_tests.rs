//! ThemeSwitcher tests (split file, `#[path]`-included as
//! `theme_switcher::tests`). The interactive flows run through the
//! REAL `Driver` + `CaptureTerm` (the wave-11 theme-switch precedent
//! and the anchored-layer P1 rig): open the popup with keys, assert
//! LIVE retints on screen bytes, pin the popup's SCREEN-space anchor
//! inside a Modal, and hold the zero-idle claim while closed.
//!
//! Theme state is per-thread (signal + toggle memory are
//! thread-local), so every test anchors its starting theme instead of
//! assuming one.

use std::cell::RefCell;
use std::rc::Rc;

use super::*;
use crate::app::driver::{Driver, RunConfig};
use crate::app::overlays::OverlayContent;
use crate::app::popups::Modal;
use crate::app::{set_theme_by_id, App};
use crate::term::Capabilities;
use crate::testing::CaptureTerm;
use crate::ui::text;

// ------------------------------------------------------------- units

#[test]
fn mode_glyphs_are_single_cell() {
    // The 1-cell contract: both glyphs measure one column under the
    // engine's width authority (they are East-Asian-NEUTRAL and
    // emoji-data-free — the module docs' design note).
    assert_eq!(crate::text::width(mode_glyph(ThemeMode::Dark)), 1);
    assert_eq!(crate::text::width(mode_glyph(ThemeMode::Light)), 1);
    assert_ne!(mode_glyph(ThemeMode::Dark), mode_glyph(ThemeMode::Light));
}

#[test]
fn menu_rows_group_by_mode_with_disabled_headers_and_current_mark() {
    let (options, themes_at, seed) = menu_rows("nord");
    assert_eq!(options.len(), themes_at.len());
    // Two headers, both disabled, dark first, glyphs teaching the
    // vocabulary.
    let headers: Vec<usize> = options
        .iter()
        .enumerate()
        .filter(|(_, o)| o.disabled)
        .map(|(i, _)| i)
        .collect();
    assert_eq!(headers[0], 0, "dark header leads");
    assert!(options[headers[0]].label.contains("Dark"));
    assert!(options[headers[0]]
        .label
        .contains(mode_glyph(ThemeMode::Dark)));
    assert!(options[headers[1]].label.contains("Light"));
    assert!(options[headers[1]]
        .label
        .contains(mode_glyph(ThemeMode::Light)));
    for &h in &headers {
        assert!(themes_at[h].is_none(), "headers map to no theme");
    }
    // Every non-header row maps to a theme of the section's mode.
    let light_start = headers[1];
    for (i, t) in themes_at.iter().enumerate() {
        if let Some(t) = t {
            assert!(!options[i].disabled);
            assert_eq!(options[i].key, t.id, "key is the stable id");
            assert_eq!(
                t.mode(),
                if i < light_start {
                    ThemeMode::Dark
                } else {
                    ThemeMode::Light
                },
                "[{}] filed under the wrong header",
                t.id
            );
        }
    }
    // All visible themes present, house themes first in their groups.
    assert_eq!(themes_at[1].unwrap().id, "abstract-dark");
    assert_eq!(themes_at[light_start + 1].unwrap().id, "abstract-light");
    // The current theme is marked and seeds the highlight.
    let seed = seed.expect("current theme seeds");
    assert_eq!(themes_at[seed].unwrap().id, "nord");
    assert_eq!(options[seed].hint.as_deref(), Some("●"));
    assert_eq!(
        options.iter().filter(|o| o.hint.is_some()).count(),
        1,
        "exactly one current mark"
    );
    // Unknown current: no mark, no seed (the caller falls back to the
    // first enabled row).
    let (options, _, seed) = menu_rows("not-a-theme");
    assert!(seed.is_none());
    assert!(options.iter().all(|o| o.hint.is_none()));
}

// ------------------------------------------------------------ the rig

const VP: Size = Size::new(110, 34);

struct Rig {
    app: App,
    term: CaptureTerm,
    driver: Driver,
    overlays: super::super::overlays::Overlays,
    scope: crate::reactive::Scope,
}

/// Real App + Driver + CaptureTerm; the root hosts a themed header
/// row (so retints show on screen bytes), the switcher under test at
/// row 1 column 0, and plain text below.
fn rig(vp: Size, switcher: impl FnOnce(crate::reactive::Scope) -> View + 'static) -> Rig {
    let mut term = CaptureTerm::new(vp);
    let mut app = App::new(vp);
    let overlays = app.overlays();
    let holder: Rc<RefCell<Option<crate::reactive::Scope>>> = Default::default();
    let h = holder.clone();
    app.mount(move |cx| {
        *h.borrow_mut() = Some(cx);
        let theme = use_theme(cx);
        Element::new()
            .style(LayoutStyle::column())
            .child(dyn_view(LayoutStyle::line(1), move || {
                let t = theme.get();
                text(format!("active: {}", t.id))
            }))
            .child(
                Element::new()
                    .style(LayoutStyle::line(1))
                    .child(switcher(cx))
                    .build(),
            )
            .child(text("below content"))
            .build()
    })
    .expect("mount");
    let cfg = RunConfig {
        caps: Some(Capabilities::with(|c| {
            c.truecolor = true;
            c.colors_256 = true;
        })),
        enter: None,
        probe: false,
    };
    let driver = Driver::new(&mut app, &mut term, cfg).expect("driver");
    let scope = holder.borrow().expect("mount scope");
    Rig {
        app,
        term,
        driver,
        overlays,
        scope,
    }
}

impl Rig {
    fn settle(&mut self) {
        for _ in 0..64 {
            if self
                .driver
                .turn(&mut self.app, &mut self.term)
                .expect("turn")
                .idle
            {
                break;
            }
        }
    }

    fn input(&mut self, bytes: &[u8]) {
        self.term.push_input(bytes);
        self.settle();
    }

    fn click(&mut self, x: i32, y: i32) {
        // SGR press+release, 1-based coordinates.
        self.input(format!("\x1b[<0;{};{}M", x + 1, y + 1).as_bytes());
        self.input(format!("\x1b[<0;{};{}m", x + 1, y + 1).as_bytes());
    }

    fn screen(&self) -> String {
        self.term.screen().to_text()
    }

    /// The switcher glyph cell (row 1, column 0 in this rig).
    fn glyph(&mut self) -> String {
        self.driver
            .screenshot()
            .cell(0, 1)
            .expect("glyph cell")
            .text()
            .to_string()
    }

    /// The owned popup's layer bounds: the HIGHEST-z modal tree. On a
    /// root-layer rig the popup is the only modal tree (allocated at
    /// `top_z() + 1`, a small z); inside a Modal it outranks the
    /// Modal's own layer — one filter covers both rigs.
    fn popup_bounds(&self) -> Option<Rect> {
        let store = self.overlays.store().borrow();
        store
            .meta
            .iter()
            .zip(&store.layers)
            .filter(|(m, _)| matches!(m.content, OverlayContent::Tree { modal: true, .. }))
            .max_by_key(|(_, l)| l.z())
            .map(|(_, l)| l.bounds())
    }

    /// Accessibility text of the topmost modal overlay tree (the open
    /// popup).
    fn popup_access(&self) -> Option<String> {
        let tree = {
            let store = self.overlays.store().borrow();
            store.meta.iter().rev().find_map(|m| match &m.content {
                OverlayContent::Tree {
                    tree, modal: true, ..
                } => Some(tree.handle()),
                _ => None,
            })?
        };
        let mut tree = tree.handle();
        tree.layout();
        Some(tree.accessibility_tree_text())
    }
}

fn menu_rig() -> Rig {
    rig(VP, |cx| ThemeSwitcher::new().view(cx))
}

// --------------------------------------------------------- menu face

#[test]
fn menu_opens_grouped_below_the_trigger() {
    assert!(set_theme_by_id("abstract-dark"));
    let mut rig = menu_rig();
    rig.settle();
    assert_eq!(rig.glyph(), mode_glyph(ThemeMode::Dark));
    rig.input(b"\t"); // focus the switcher
    rig.input(b"\r"); // open
    let popup = rig.popup_bounds().expect("popup open");
    assert_eq!(
        (popup.x, popup.y),
        (0, 2),
        "below-preferred, anchored at the trigger cell; got {popup:?}"
    );
    let screen = rig.screen();
    assert!(
        screen.contains(&format!("{} Dark", mode_glyph(ThemeMode::Dark))),
        "dark group header visible:\n{screen}"
    );
    assert!(
        screen.contains("Dark (Abstract)"),
        "house dark theme listed first:\n{screen}"
    );
    // The current theme is marked on its row.
    let marked = screen
        .lines()
        .find(|l| l.contains("Dark (Abstract)"))
        .expect("current row");
    assert!(
        marked.contains("●"),
        "current mark beside the label: {marked}"
    );
}

#[test]
fn menu_movement_live_previews_on_screen_bytes_and_enter_commits() {
    assert!(set_theme_by_id("abstract-dark"));
    let mut rig = menu_rig();
    rig.settle();
    rig.input(b"\t");
    rig.input(b"\r");
    let before = rig.driver.screenshot().to_ansi();
    rig.input(b"\x1b[B"); // Down: highlight the next theme
    assert_eq!(
        current_theme().id,
        "observer-night",
        "movement previews LIVE (commit-on-move, the theme-picker semantic)"
    );
    let after = rig.driver.screenshot().to_ansi();
    assert_ne!(before, after, "the preview retints the screen bytes");
    assert!(
        rig.screen().contains("active: observer-night"),
        "the app's own themed region re-rendered:\n{}",
        rig.screen()
    );
    rig.input(b"\r"); // commit
    assert!(rig.popup_bounds().is_none(), "commit closes the popup");
    assert_eq!(current_theme().id, "observer-night", "commit sticks");
}

#[test]
fn menu_escape_restores_the_pre_open_theme() {
    assert!(set_theme_by_id("abstract-dark"));
    let mut rig = menu_rig();
    rig.settle();
    rig.input(b"\t");
    rig.input(b"\r");
    rig.input(b"\x1b[B"); // preview another theme
    assert_ne!(current_theme().id, "abstract-dark");
    rig.input(b"\x1b[27u"); // kitty Escape
    assert!(rig.popup_bounds().is_none(), "Escape closes");
    assert_eq!(
        current_theme().id,
        "abstract-dark",
        "Escape abandons the preview and restores the pre-open theme"
    );
}

#[test]
fn menu_committing_a_light_theme_flips_the_glyph() {
    assert!(set_theme_by_id("abstract-dark"));
    let mut rig = menu_rig();
    rig.settle();
    assert_eq!(rig.glyph(), mode_glyph(ThemeMode::Dark));
    rig.input(b"\t");
    rig.input(b"\r");
    rig.input(b"\x1b[F"); // End: last enabled row = the last light theme
    assert_eq!(current_theme().mode(), ThemeMode::Light);
    rig.input(b"\r"); // commit
    assert_eq!(
        rig.glyph(),
        mode_glyph(ThemeMode::Light),
        "the trigger glyph reflects the mode after the switch"
    );
}

#[test]
fn menu_type_ahead_jumps_by_label_prefix_and_applies() {
    assert!(set_theme_by_id("abstract-dark"));
    let mut rig = menu_rig();
    rig.settle();
    rig.input(b"\t");
    rig.input(b"\r");
    rig.input(b"no"); // the only label starting "no" is Nord
    assert_eq!(
        current_theme().id,
        "nord",
        "type-ahead jumps the highlight and the live preview applies it"
    );
    rig.input(b"\r"); // commit; also proves the buffer clears at reopen
    assert!(rig.popup_bounds().is_none());
    // Identical-prefix family (the adversarial case): repeated 'c'
    // cycles through the Catppuccin group instead of sticking. A fresh
    // open — the accumulation window is 900ms and test keystrokes land
    // microseconds apart, so "no" + "c" in one session would read as
    // the buffer "noc" (correct type-ahead semantics, wrong probe).
    rig.input(b"\r");
    rig.input(b"c");
    let first = current_theme().id;
    assert!(first.starts_with("catppuccin"), "prefix jump: {first}");
    rig.input(b"c");
    let second = current_theme().id;
    assert!(
        second.starts_with("catppuccin"),
        "cycle stays in group: {second}"
    );
    assert_ne!(first, second, "repeated char cycles the matches");
    rig.input(b"\r");
    assert_eq!(current_theme().id, second, "commit keeps the cycled pick");
}

#[test]
fn menu_popup_inside_a_modal_anchors_at_the_screen_cell() {
    // The P1 regression class: anchors are SCREEN cells even when the
    // opener lives on a positioned overlay layer.
    assert!(set_theme_by_id("abstract-dark"));
    let mut rig = rig(VP, |_cx| text("root under modal"));
    rig.settle();
    let _modal = Modal::open(&rig.overlays, rig.scope, VP, Size::new(40, 7), |mcx| {
        Element::new()
            .style(
                LayoutStyle::column()
                    .width(Dimension::Percent(1.0))
                    .height(Dimension::Percent(1.0)),
            )
            .child(text("appearance"))
            .child(ThemeSwitcher::new().view(mcx))
            .build()
    });
    rig.settle();
    // Locate the glyph on the actual screen — no assumption about the
    // modal's padding.
    let shot = rig.driver.screenshot();
    let glyph = mode_glyph(ThemeMode::Dark);
    let mut at = None;
    for y in 0..VP.h {
        for x in 0..VP.w {
            if shot.cell(x, y).is_some_and(|c| c.text() == glyph) {
                at = Some((x, y));
            }
        }
    }
    let (gx, gy) = at.expect("glyph rendered inside the modal");
    assert!(gx > 30 && gy > 10, "glyph sits inside the centered modal");
    rig.input(b"\r"); // modal focuses its first focusable: the switcher
    let popup = rig.popup_bounds().expect("popup open above the modal");
    assert_eq!(
        (popup.x, popup.y),
        (gx, gy + 1),
        "popup adjacent to the trigger in SCREEN space; got {popup:?}"
    );
}

#[test]
fn menu_on_tiny_terminal_fits_the_viewport_and_still_commits() {
    assert!(set_theme_by_id("abstract-dark"));
    let vp = Size::new(38, 6);
    let mut rig = rig(vp, |cx| ThemeSwitcher::new().view(cx));
    rig.settle();
    rig.input(b"\t");
    rig.input(b"\r");
    let popup = rig.popup_bounds().expect("popup open");
    assert!(
        popup.x >= 0 && popup.y >= 0 && popup.right() <= vp.w && popup.bottom() <= vp.h,
        "popup clamped into the viewport: {popup:?}"
    );
    assert!(popup.h <= 4, "window capped by the room below the trigger");
    rig.input(b"\x1b[B");
    rig.input(b"\r");
    assert!(rig.popup_bounds().is_none());
    assert_ne!(current_theme().id, "abstract-dark", "commit worked");
}

#[test]
fn menu_outside_press_keeps_the_preview_and_reports_once() {
    assert!(set_theme_by_id("abstract-dark"));
    let seen: Rc<RefCell<Vec<&'static str>>> = Default::default();
    let s = seen.clone();
    let mut rig = rig(VP, move |cx| {
        ThemeSwitcher::new()
            .on_change(move |t| s.borrow_mut().push(t.id))
            .view(cx)
    });
    rig.settle();
    rig.input(b"\t");
    rig.input(b"\r");
    rig.input(b"\x1b[B"); // preview observer-night
    rig.click(90, 30); // outside the popup
    assert!(rig.popup_bounds().is_none(), "outside press dismisses");
    assert_eq!(
        current_theme().id,
        "observer-night",
        "outside press keeps the preview (the select-family contract)"
    );
    assert_eq!(
        *seen.borrow(),
        vec!["observer-night"],
        "the stuck switch reported exactly once"
    );
    // Escape after a preview: restored, and NOT reported.
    rig.input(b"\r");
    rig.input(b"\x1b[B");
    rig.input(b"\x1b[27u");
    assert_eq!(current_theme().id, "observer-night");
    assert_eq!(
        seen.borrow().len(),
        1,
        "Escape-restore never fires on_change"
    );
    // Committing the already-current theme: closed, nothing reported.
    rig.input(b"\r");
    rig.input(b"\r");
    assert_eq!(seen.borrow().len(), 1, "no-change commit stays silent");
}

// ------------------------------------------------------- toggle face

#[test]
fn toggle_face_flips_mode_and_remembers_the_theme() {
    assert!(set_theme_by_id("nord"));
    let mut rig = rig(VP, |cx| ThemeSwitcher::toggle().view(cx));
    rig.settle();
    assert_eq!(rig.glyph(), mode_glyph(ThemeMode::Dark));
    rig.input(b"\t");
    rig.input(b"\r");
    assert_eq!(
        current_theme().id,
        "abstract-light",
        "cold light flip lands on the house palette"
    );
    assert_eq!(rig.glyph(), mode_glyph(ThemeMode::Light), "glyph follows");
    assert!(
        rig.popup_bounds().is_none(),
        "the toggle face opens nothing"
    );
    rig.input(b"\r");
    assert_eq!(current_theme().id, "nord", "flip back restores the CHOICE");
    assert_eq!(rig.glyph(), mode_glyph(ThemeMode::Dark));
}

#[test]
fn rapid_toggle_spam_round_trips_cleanly() {
    assert!(set_theme_by_id("gruvbox"));
    let mut rig = rig(VP, |cx| ThemeSwitcher::toggle().view(cx));
    rig.settle();
    rig.input(b"\t");
    for _ in 0..5 {
        rig.input(b"\r\r"); // two flips per settle: burst pressure
    }
    assert_eq!(
        current_theme().id,
        "gruvbox",
        "an even number of flips lands exactly where it started"
    );
    assert_eq!(rig.glyph(), mode_glyph(ThemeMode::Dark));
}

// ------------------------------------------------------ idle + a11y

#[test]
fn closed_switcher_is_idle() {
    assert!(set_theme_by_id("abstract-dark"));
    let mut rig = menu_rig();
    rig.settle();
    let turn = rig.driver.turn(&mut rig.app, &mut rig.term).expect("turn");
    assert!(turn.idle, "closed switcher schedules no work");
    // Open + Escape, then idle again: the popup's subscriptions died
    // with its scope.
    rig.input(b"\t");
    rig.input(b"\r");
    rig.input(b"\x1b[27u");
    rig.settle();
    let turn = rig.driver.turn(&mut rig.app, &mut rig.term).expect("turn");
    assert!(turn.idle, "open/close leaves nothing running");
}

#[test]
fn a11y_reports_button_with_value_and_menu_popup() {
    assert!(set_theme_by_id("abstract-dark"));
    let mut menu = menu_rig();
    menu.settle();
    let access = menu.app.tree().accessibility_tree_text();
    assert!(
        access.contains("button \"theme\"") && access.contains("Dark (Abstract)"),
        "trigger reports the button role, its label, and the current \
         theme as value:\n{access}"
    );
    menu.input(b"\t");
    menu.input(b"\r");
    let popup = menu.popup_access().expect("popup access tree");
    assert!(
        popup.contains("menu \"themes\"") && popup.contains("menuitem"),
        "popup reports menu/menuitem roles:\n{popup}"
    );
    // The toggle face names its ACTION (the glyph shows state, the
    // label carries what a press does — the module docs' split).
    let mut toggle_rig = rig(VP, |cx| ThemeSwitcher::toggle().view(cx));
    toggle_rig.settle();
    let access = toggle_rig.app.tree().accessibility_tree_text();
    assert!(
        access.contains("button \"toggle theme mode\""),
        "toggle face labels the action:\n{access}"
    );
}
