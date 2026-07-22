//! Application runtime: terminal session + input events + reactive
//! scheduler + layout + compositor pipeline, sequenced per the damage
//! contract (docs/design/01-damage-contract.md).
//!
//! Owner: REACT (loop) with KERNEL (lifecycle edges).
//!
//! ## Shape
//!
//! - [`App`] owns the reactive root and the [`crate::ui::UiTree`];
//!   `mount` installs the user's root component.
//! - [`Driver`] (driver.rs) owns the per-frame pipeline: phase U
//!   (posted jobs + batched input dispatch + effect flush), L (layout),
//!   D (damaged-region redraw under the draw-purity guard), C (flatten),
//!   P (diff -> presenter -> exactly one flush), S (swap). `turn` never
//!   blocks; `wait_for_activity` is the blocking edge.
//! - [`App::run`] = enter raw terminal (panic hook FIRST, so any panic
//!   restores the screen), then `turn`/`wait` until quit. Idle apps sit
//!   in a blocking read: zero wakeups, zero CPU — animations and posted
//!   jobs interrupt it through the terminal waker.
//! - Headless surfaces (`pump`, `draw`) stay: tests and embedders drive
//!   the same reactive/layout pipeline without a terminal.
//!
//! ## Quit policy
//!
//! Ctrl+C quits by default (raw mode disables ISIG, so it arrives as an
//! ordinary key — KERNEL request 3 makes it app policy). An app overrides
//! by CONSUMING the event (any handler/shortcut on the routing path).
//! Programmatic exit: [`App::quitter`] hands out a cloneable handle.
//!
//! ## Theme
//!
//! One app-level theme signal (damage contract §5): [`use_theme`] /
//! [`set_theme`]. `mount` installs a watcher effect that damages the
//! whole tree on switch, so even non-reactive text repaints; widgets
//! that read the signal inside `Dyn` regions re-render fine-grained.

pub mod actions;
pub mod anchored;
mod driver;
mod driver_images;
mod events;
pub mod keymap_help;
mod notices;
pub mod overlays;
pub mod popups;
pub mod select;
pub mod selection;
mod theme;
mod viewport;

#[cfg(test)]
mod acceptance;

pub use actions::{ActionInfo, Actions};
pub use driver::{Driver, RunConfig, Turn};
pub use keymap_help::KeymapHelp;
pub use notices::use_startup_notices;
pub use overlays::{ImageHandle, LayerHandle, Overlays};
pub use popups::{Modal, Toast, MODAL_Z, TOAST_Z};
pub use select::{Combobox, MultiSelect, Select, SelectOption};
pub use theme::{current_theme, set_theme, set_theme_by_id, use_theme};
pub use viewport::{current_viewport, use_viewport};

use std::cell::Cell;
use std::rc::Rc;

use crate::base::{Result, Size};
use crate::reactive::{
    self, create_root, drain_posted, flush_effects, take_frame_request, RootScope, Scope,
    WakeHandle,
};
use crate::term::Terminal;
use crate::theme::TokenId;
use crate::ui::{StyledCanvas, UiTree, View};

/// Cloneable programmatic-quit handle (captured by component closures).
#[derive(Clone)]
pub struct Quitter(Rc<Cell<bool>>);

impl Quitter {
    pub fn quit(&self) {
        self.0.set(true);
    }
}

/// Application shell: reactive root + ui tree + quit flag. The terminal
/// pipeline attaches through [`Driver`] (or all at once via [`App::run`]).
///
/// # Testing your app headlessly (RT8-2: the canonical harness)
///
/// No tty needed: drive the SAME pipeline production uses against a
/// captured terminal — `testing::CaptureTerm` records bytes and models
/// the screen, [`Driver::turn`] runs one full frame cycle, and you
/// assert on the rendered text (or the raw bytes). Feed input as the
/// terminal would send it (`push_input`); every dispatch/focus/damage
/// path is the real one.
///
/// ```
/// use abstracttui::prelude::*;
/// use abstracttui::app::Driver;
/// use abstracttui::testing::CaptureTerm;
///
/// let size = Size::new(20, 4);
/// let mut app = App::new(size);
/// app.mount(|cx| {
///     let n = cx.signal(0);
///     Element::new()
///         .shortcut(KeyChord::plain(Key::Char('+')), move |_| n.update(|v| *v += 1))
///         .child(dyn_view(LayoutStyle::line(1), move || text(format!("n = {}", n.get()))))
///         .build()
/// }).unwrap();
///
/// let mut term = CaptureTerm::new(size);
/// let cfg = RunConfig { probe: false, ..RunConfig::default() };
/// let mut driver = Driver::new(&mut app, &mut term, cfg).unwrap();
/// driver.turn(&mut app, &mut term).unwrap();          // first frame
/// assert!(term.screen().to_text().contains("n = 0"));
///
/// term.push_input(b"+");                              // a keypress
/// driver.turn(&mut app, &mut term).unwrap();          // dispatch + repaint
/// assert!(term.screen().to_text().contains("n = 1"));
/// ```
///
/// For pure component tests skip the driver entirely: mount into a
/// `ui::UiTree`, `dispatch` events, draw into a `ui::BufferCanvas`
/// (every widget suite in this crate is written that way).
pub struct App {
    root: Option<RootScope>,
    tree: UiTree,
    viewport: Size,
    quit: Rc<Cell<bool>>,
    overlays: Overlays,
    actions: Actions,
    /// Labeled startup degradations + environment summary (KERNEL's
    /// `degraded()` input fallback, caps one-liner). Apps render these
    /// however they like; the engine only collects.
    notices: Vec<String>,
}

/// What a headless pump pass observed.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct PumpReport {
    pub posted_jobs: usize,
    pub frame_requested: bool,
}

impl App {
    /// Mount a component and run — the whole happy path in one call.
    /// The viewport argument other constructors take is a placeholder
    /// here: the driver replaces it with the real terminal size at
    /// enter, so `simple` picks one for you.
    ///
    /// The canonical first app — a live counter in 16 lines (Tab
    /// focuses, Enter/Space clicks, Ctrl+C quits — all defaults; the
    /// count line re-renders fine-grained through `dyn_view`, the
    /// button resolves the active theme itself via [`Widget::view`
    /// sugar](crate::widgets::Button::view)):
    ///
    /// ```no_run
    /// use abstracttui::prelude::*;
    /// use abstracttui::widgets::Button;
    ///
    /// fn main() -> abstracttui::base::Result<()> {
    ///     App::simple(|cx| {
    ///         let count = cx.signal(0);
    ///         Element::new()
    ///             .style(LayoutStyle::column())
    ///             .child(dyn_view(LayoutStyle::line(1), move || {
    ///                 text(format!("count: {}", count.get()))
    ///             }))
    ///             .child(Button::new("+1").on_click(move || count.update(|c| *c += 1)).view(cx))
    ///             .child(text("Tab focuses · Enter clicks · Ctrl+C quits"))
    ///             .build()
    ///     })
    /// }
    /// ```
    pub fn simple(component: impl FnOnce(Scope) -> View) -> Result<()> {
        let mut app = App::new(Size::new(80, 24));
        app.mount(component)?;
        app.run()
    }

    pub fn new(viewport: Size) -> App {
        App {
            root: None,
            tree: UiTree::new(viewport),
            viewport,
            quit: Rc::new(Cell::new(false)),
            overlays: Overlays::new(),
            actions: Actions::new(),
            notices: Vec::new(),
        }
    }

    /// Record a labeled startup degradation/summary line. The engine
    /// pushes input-path degradations and a caps summary; apps may add
    /// their own. Convention: `"area: state (detail)"`. Publishes into
    /// the REACTIVE store too — components reading
    /// [`use_startup_notices`] re-render on late pushes.
    pub fn push_startup_notice(&mut self, notice: impl Into<String>) {
        let notice = notice.into();
        self.notices.push(notice.clone());
        notices::publish_notice(notice);
    }

    /// Everything degraded or noteworthy at startup — DESIGN's examples
    /// surface these in a status line/notice bar. Empty = clean start.
    /// Component code should prefer the reactive read
    /// ([`use_startup_notices`]): engine notices land AFTER mount.
    pub fn startup_notices(&self) -> &[String] {
        &self.notices
    }

    /// The overlay world (modals, toasts, image layers, custom layers).
    /// Cloneable; components capture it at mount time.
    pub fn overlays(&self) -> Overlays {
        self.overlays.clone()
    }

    /// The global action registry (named actions + key chords). Keys
    /// nothing in the UI consumed land here — LAST in the resolution
    /// order, so focused widgets always win over global bindings.
    pub fn actions(&self) -> Actions {
        self.actions.clone()
    }

    /// Mount the root component under the app's reactive root. Also wires
    /// the theme watcher: a theme switch invalidates the whole tree (§5's
    /// escape hatch for non-reactive content like default-colored text;
    /// `Dyn` regions that read the theme re-render fine-grained anyway).
    pub fn mount(&mut self, component: impl FnOnce(Scope) -> View) -> Result<()> {
        if self.root.is_some() {
            return Err(crate::base::Error::App("App::mount called twice".into()));
        }
        let tree = &mut self.tree;
        let overlays = self.overlays.clone();
        let (root, ()) = create_root(|cx| {
            let theme_sig = use_theme(cx);
            // The ACTIVE theme rides reactive CONTEXT so widgets below
            // the app layer can read it without an upward import —
            // `Widget::view(cx)` resolves tokens through this (tracked:
            // a widget built inside a dyn_view re-renders on switch).
            cx.provide_context(theme_sig);
            // The overlay store rides context too: popup-opening
            // controls (`app::select` family) resolve it without prop
            // drilling — `Select::new(..).view(cx)` just works.
            cx.provide_context(overlays);
            let invalidate = tree.invalidator();
            cx.effect_labeled("app-theme-watcher", move || {
                let _ = theme_sig.get(); // subscribe
                invalidate();
            });
            let view = component(cx);
            tree.mount(cx, view);
        });
        self.root = Some(root);
        Ok(())
    }

    /// Programmatic quit handle for components (`quitter.quit()` ends the
    /// next loop turn).
    pub fn quitter(&self) -> Quitter {
        Quitter(self.quit.clone())
    }

    pub fn quit_requested(&self) -> bool {
        self.quit.get()
    }

    /// One scheduler turn without a terminal: run cross-thread posted
    /// work, settle effects, report whether a frame is wanted.
    pub fn pump(&mut self) -> PumpReport {
        let posted_jobs = drain_posted();
        flush_effects();
        PumpReport {
            posted_jobs,
            frame_requested: take_frame_request(),
        }
    }

    /// Headless paint of the whole tree into any canvas (tests, embeds).
    /// The terminal path renders damaged regions only — through
    /// [`Driver`], not this.
    pub fn draw(&mut self, canvas: &mut dyn StyledCanvas) {
        self.tree
            .set_text_fg(current_theme().tokens.get(TokenId::Text));
        self.tree.layout();
        self.tree.draw(canvas);
    }

    pub fn tree(&mut self) -> &mut UiTree {
        &mut self.tree
    }

    pub fn viewport(&self) -> Size {
        self.viewport
    }

    pub fn set_viewport(&mut self, size: Size) {
        self.viewport = size;
        self.tree.set_viewport(size);
        viewport::publish_viewport(size);
    }

    /// Handle other threads use to schedule work on this app's thread.
    pub fn wake_handle(&self) -> WakeHandle {
        reactive::wake_handle()
    }

    /// The interactive loop against the platform terminal. Blocks until
    /// quit. Installs the emergency-restore panic hook FIRST (KERNEL
    /// request 1): a panic anywhere — draw closure, handler, layout —
    /// restores cooked mode even when the unwind never reaches the
    /// `Terminal`'s Drop.
    pub fn run(mut self) -> Result<()> {
        if !crate::term::have_tty() {
            return Err(crate::base::Error::Unsupported(
                "App::run needs a tty (headless callers use mount + pump + draw, \
                 or Driver against their own Terminal)"
                    .into(),
            ));
        }
        install_panic_hook();
        let mut term = new_platform_terminal()?;
        self.run_prepared(&mut term, RunConfig::default())
    }

    /// `run_on` plus the CONCRETE-terminal startup notices: the input
    /// path's labeled degradation (`UnixTerminal::degraded`, KERNEL's
    /// tty-resolution fallback) is only reachable before the type
    /// erases to `dyn Terminal` — this is that read point.
    #[cfg(unix)]
    fn run_prepared(&mut self, term: &mut crate::term::UnixTerminal, cfg: RunConfig) -> Result<()> {
        let mut driver = Driver::new(self, term, cfg)?;
        if let Some(label) = term.degraded() {
            self.push_startup_notice(format!("input: degraded ({label})"));
        }
        self.push_startup_notice(caps_summary(driver.caps()));
        self.drive_loop(&mut driver, term)
    }

    #[cfg(windows)]
    fn run_prepared(
        &mut self,
        term: &mut crate::term::WindowsTerminal,
        cfg: RunConfig,
    ) -> Result<()> {
        let mut driver = Driver::new(self, term, cfg)?;
        self.push_startup_notice(caps_summary(driver.caps()));
        self.drive_loop(&mut driver, term)
    }

    /// The same loop against any `Terminal` (a scripted one in tests, a
    /// remote one in exotic embeddings). Blocking: only sensible for
    /// terminals whose `read` actually waits — test terminals drive
    /// [`Driver::turn`] directly instead.
    pub fn run_on(&mut self, term: &mut dyn Terminal, cfg: RunConfig) -> Result<()> {
        let mut driver = Driver::new(self, term, cfg)?;
        self.push_startup_notice(caps_summary(driver.caps()));
        self.drive_loop(&mut driver, term)
    }

    fn drive_loop(&mut self, driver: &mut Driver, term: &mut dyn Terminal) -> Result<()> {
        /// Animation frame pacing (~60 fps). Only consulted while signal
        /// transitions are in flight; idle apps block indefinitely.
        const FRAME_INTERVAL: std::time::Duration = std::time::Duration::from_millis(16);
        let result = loop {
            match driver.turn(self, term) {
                Ok(turn) if turn.quit => break Ok(()),
                Ok(_) if reactive::frame_tasks_pending() > 0 => {
                    // Animations in flight: pace frames instead of spinning
                    // (input arriving earlier still wakes the wait).
                    let deadline = std::time::Instant::now() + FRAME_INTERVAL;
                    if let Err(e) = driver.wait_until(term, deadline) {
                        break Err(e);
                    }
                }
                Ok(turn) if turn.idle => {
                    // Zero-wakeup idle: block until input/resize/wake —
                    // bounded by the earliest one-shot timer, if any
                    // (a parked toast's dismissal must fire on time).
                    let result = match reactive::next_timer_deadline() {
                        Some(deadline) => driver.wait_until(term, deadline),
                        None => driver.wait_for_activity(term),
                    };
                    if let Err(e) = result {
                        break Err(e);
                    }
                }
                Ok(_) => {}
                Err(e) => break Err(e),
            }
        };
        let _ = driver.finish(term); // restore even on the error path
                                     // Zero-collapse diagnostics recorded during the run reach stderr
                                     // only HERE — after the terminal is restored — so a live
                                     // alternate screen is never corrupted by diagnostic lines
                                     // (2026-07-22 dashboard incident). In-session visibility rides
                                     // the startup-notices lane instead.
        for note in driver.collapse_log() {
            eprintln!("{note}");
        }
        result
    }

    /// Tear down: dispose the root scope (cleanups unmount the tree).
    pub fn shutdown(&mut self) {
        if let Some(root) = self.root.take() {
            root.dispose();
        }
    }
}

impl Drop for App {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// One-line environment summary for the startup notices — capabilities
/// an app author cares about at a glance, honest about what is OFF.
fn caps_summary(caps: &crate::term::Capabilities) -> String {
    let mut on: Vec<&str> = Vec::new();
    if caps.truecolor {
        on.push("truecolor");
    } else if caps.colors_256 {
        on.push("256color");
    } else {
        on.push("16color");
    }
    if caps.kitty_keyboard {
        on.push("kitty-kbd");
    }
    if caps.sync_output_2026 {
        on.push("sync");
    }
    if caps.kitty_graphics {
        on.push("kitty-gfx");
    } else if caps.iterm2_images {
        on.push("iterm2-gfx");
    } else if caps.sixel {
        on.push("sixel");
    }
    if caps.in_tmux {
        on.push("tmux");
    }
    format!("caps: {}", on.join("+"))
}

#[cfg(unix)]
fn new_platform_terminal() -> Result<crate::term::UnixTerminal> {
    crate::term::UnixTerminal::new()
}

#[cfg(windows)]
fn new_platform_terminal() -> Result<crate::term::WindowsTerminal> {
    crate::term::WindowsTerminal::new()
}

/// Emergency-restore panic hook, installed once per process. Chains the
/// previous hook so panic messages still print — AFTER the terminal is
/// back in cooked mode, so they are actually readable.
fn install_panic_hook() {
    use std::sync::Once;
    static HOOK: Once = Once::new();
    HOOK.call_once(|| {
        let prev = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            crate::term::emergency_restore();
            prev(info);
        }));
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::base::Point;
    use crate::layout::Style;
    use crate::ui::{dyn_view, text, BufferCanvas, Element};

    #[test]
    fn headless_mount_pump_draw_cycle() {
        let mut app = App::new(Size::new(16, 2));
        let handle = app.wake_handle();
        app.mount(|cx| {
            let n = cx.signal(1i64);
            std::thread::spawn(move || handle.post(move || n.set(42)))
                .join()
                .expect("join");
            Element::new()
                .child(dyn_view(Style::default(), move || {
                    text(format!("v={}", n.get()))
                }))
                .build()
        })
        .expect("mount");
        let report = app.pump();
        assert_eq!(report.posted_jobs, 1);
        assert!(report.frame_requested);
        let mut canvas = BufferCanvas::new(Size::new(16, 2));
        app.draw(&mut canvas);
        assert_eq!(canvas.row_text(0).trim_end(), "v=42");
        assert_eq!(canvas.cell(Point::new(0, 0)).expect("cell").0, 'v');
        // Idle: nothing pending, no frame wanted — the zero-work claim.
        let idle = app.pump();
        assert_eq!(
            idle,
            PumpReport {
                posted_jobs: 0,
                frame_requested: false
            }
        );
        app.shutdown();
        assert_eq!(
            app.tree().instance_count(),
            0,
            "shutdown unmounts everything"
        );
    }

    #[test]
    fn quitter_sets_the_flag() {
        let app = App::new(Size::new(4, 4));
        assert!(!app.quit_requested());
        app.quitter().quit();
        assert!(app.quit_requested());
    }

    #[test]
    fn theme_switch_invalidates_the_tree() {
        let mut app = App::new(Size::new(8, 2));
        app.mount(|_cx| Element::new().child(text("hi")).build())
            .expect("mount");
        app.pump();
        let _ = app.tree().take_damage();
        let prev = set_theme(crate::theme::get("nord").expect("nord"));
        assert!(
            !app.tree().take_damage().is_empty(),
            "theme watcher must damage the tree on switch"
        );
        set_theme(prev); // restore for sibling tests on this thread
    }
}
