//! Block close-affordance acceptance (app-kits 0605): the panel ✕
//! through the REAL driver pipeline against CaptureTerm — SGR mouse
//! bytes in, screen text + emission economy out.
//!
//! The operator's order, verbatim: "sometimes we need to close them to
//! free up space … a cross (probably upper right?) on each discussion
//! panel so I can close them and the other panels get more space."
//! The charter pins:
//! - click ✕ on the middle of three panels → the app's callback
//!   removes it and the SIBLINGS RE-FLEX into the space;
//! - activation is press + release INSIDE (drag-out cancels — the
//!   Button 0.2.20 convention, never fire-on-down);
//! - a dead panel's callback can never re-fire (close-spam at the same
//!   cell lands on whatever LIVE panel owns it now — browser-tab
//!   semantics);
//! - the affordance composes: inside a PageHost page, inside a Drawer
//!   overlay, inside a Scroll mid-scroll;
//! - closable blocks PARKED cost zero: an idle turn emits zero bytes.

use std::cell::RefCell;
use std::rc::Rc;

use abstracttui::app::drawer::{Drawer, DrawerEdge, DrawerFocus, DrawerSize};
use abstracttui::app::{App, Driver, RunConfig};
use abstracttui::base::Size;
use abstracttui::layout::Style as LayoutStyle;
use abstracttui::prelude::use_theme;
use abstracttui::reactive::{Scope, Signal};
use abstracttui::term::Capabilities;
use abstracttui::testing::CaptureTerm;
use abstracttui::ui::{dyn_view, text, Element, View};
use abstracttui::widgets::{Block, BorderKind, PageHost, Scroll};

fn test_config() -> RunConfig {
    RunConfig {
        caps: Some(Capabilities::with(|c| {
            c.truecolor = true;
            c.colors_256 = true;
            c.unicode_ok = true;
        })),
        enter: None,
        probe: false,
        ..RunConfig::default()
    }
}

struct Rig {
    app: App,
    term: CaptureTerm,
    driver: Driver,
}

impl Rig {
    fn new(vp: Size, root: impl FnOnce(Scope) -> View + 'static) -> Rig {
        let mut term = CaptureTerm::new(vp);
        let mut app = App::new(vp);
        app.mount(root).expect("mount");
        let driver = Driver::new(&mut app, &mut term, test_config()).expect("driver");
        let mut rig = Rig { app, term, driver };
        rig.settle();
        rig
    }

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

    /// SGR press + release at one cell (0-based coords).
    fn click(&mut self, x: i32, y: i32) {
        self.input(format!("\x1b[<0;{};{}M", x + 1, y + 1).as_bytes());
        self.input(format!("\x1b[<0;{};{}m", x + 1, y + 1).as_bytes());
    }

    fn screen(&self) -> String {
        self.term.screen().to_text()
    }

    /// Column positions of every ✕ on `row` (all glyphs single-width,
    /// so char index == column).
    fn close_cells(&self, row: usize) -> Vec<i32> {
        let line = self
            .screen()
            .lines()
            .nth(row)
            .map(str::to_string)
            .unwrap_or_default();
        line.chars()
            .enumerate()
            .filter(|(_, ch)| *ch == '✕')
            .map(|(x, _)| x as i32)
            .collect()
    }
}

type Fired = Rc<RefCell<Vec<&'static str>>>;

/// A row of closable panels driven by an app-side alive-mask — the
/// operator's exact wiring: `on_close` clears the panel's bit, the
/// row's dyn re-renders, siblings re-flex.
fn panel_row(cx: Scope, alive: Signal<u32>, fired: Fired) -> View {
    let theme = use_theme(cx);
    dyn_view(
        LayoutStyle::default()
            .grow(1.0)
            .basis(abstracttui::layout::Dimension::Cells(0)),
        move || {
            let t = theme.get().tokens;
            let mask = alive.get();
            let mut row = Element::new().style(
                LayoutStyle::row()
                    .width(abstracttui::layout::Dimension::Percent(1.0))
                    .height(abstracttui::layout::Dimension::Percent(1.0)),
            );
            for (i, name) in ["Alpha", "Beta", "Gamma"].iter().enumerate() {
                if mask & (1 << i) == 0 {
                    continue;
                }
                let fired = fired.clone();
                row = row.child(
                    Block::new()
                        .border(BorderKind::Rounded)
                        .title(*name)
                        .fill(t.surface)
                        .on_close(move || {
                            fired.borrow_mut().push(name);
                            alive.update(|m| *m &= !(1 << i));
                        })
                        .layout(LayoutStyle::column().grow(1.0))
                        .child(text(format!("{name} body")))
                        .element(&t)
                        .build(),
                );
            }
            row.build()
        },
    )
}

#[test]
fn click_close_middle_panel_siblings_reflex_and_idle_is_zero() {
    let fired: Fired = Default::default();
    let f = fired.clone();
    let mut rig = Rig::new(Size::new(48, 8), move |cx| {
        let alive = cx.signal(0b111u32);
        Element::new()
            .style(
                LayoutStyle::column()
                    .width(abstracttui::layout::Dimension::Percent(1.0))
                    .height(abstracttui::layout::Dimension::Percent(1.0)),
            )
            .child(panel_row(cx, alive, f))
            .build()
    });

    let screen = rig.screen();
    assert!(
        screen.contains("Alpha") && screen.contains("Beta") && screen.contains("Gamma"),
        "three panels up:\n{screen}"
    );
    let closes = rig.close_cells(0);
    assert_eq!(closes.len(), 3, "one ✕ per panel: {closes:?}\n{screen}");

    // The operator's click: the MIDDLE panel's ✕.
    rig.click(closes[1], 0);
    let screen = rig.screen();
    assert_eq!(*fired.borrow(), vec!["Beta"], "exactly one close fired");
    assert!(!screen.contains("Beta"), "Beta is gone:\n{screen}");
    assert!(
        screen.contains("Alpha") && screen.contains("Gamma"),
        "siblings live:\n{screen}"
    );
    // Re-flex: the survivors' ✕ sit WIDER apart than before (each
    // panel grew into the freed space).
    let after = rig.close_cells(0);
    assert_eq!(after.len(), 2, "two panels, two ✕:\n{screen}");
    assert!(
        after[1] - after[0] > closes[1] - closes[0],
        "panels re-flexed into the space: {closes:?} -> {after:?}"
    );

    // Parked closable panels cost ZERO: an idle turn emits no bytes.
    let _ = rig.term.take_bytes();
    let turn = rig.driver.turn(&mut rig.app, &mut rig.term).expect("idle");
    assert!(!turn.emitted, "{turn:?}");
    assert!(rig.term.take_bytes().is_empty(), "idle emits zero bytes");

    // Even HOVERED (hot restyle live under the pointer), the affordance
    // is a settled generation — idle stays zero.
    rig.input(format!("\x1b[<35;{};1M", after[0] + 1).as_bytes());
    let _ = rig.term.take_bytes();
    let turn = rig.driver.turn(&mut rig.app, &mut rig.term).expect("idle2");
    assert!(!turn.emitted, "{turn:?}");
    assert!(rig.term.take_bytes().is_empty(), "hovered idle is zero too");
}

#[test]
fn press_drag_out_release_never_closes() {
    let fired: Fired = Default::default();
    let f = fired.clone();
    let mut rig = Rig::new(Size::new(48, 8), move |cx| {
        let alive = cx.signal(0b111u32);
        Element::new()
            .style(
                LayoutStyle::column()
                    .width(abstracttui::layout::Dimension::Percent(1.0))
                    .height(abstracttui::layout::Dimension::Percent(1.0)),
            )
            .child(panel_row(cx, alive, f))
            .build()
    });
    let closes = rig.close_cells(0);
    let x = closes[1] + 1; // 1-based for SGR
                           // Press on Beta's ✕, drag away (left held), release outside.
    rig.input(format!("\x1b[<0;{x};1M").as_bytes());
    rig.input(b"\x1b[<32;5;5M");
    rig.input(b"\x1b[<0;5;5m");
    assert!(fired.borrow().is_empty(), "drag-out cancels the close");
    assert!(rig.screen().contains("Beta"), "Beta survives");
    // And a real click afterwards still works.
    rig.click(closes[1], 0);
    assert_eq!(*fired.borrow(), vec!["Beta"]);
}

/// Close-spam at one cell: after a panel dies, the SAME cell belongs to
/// whatever live panel re-flexed under it — its ✕ (or nothing) decides.
/// The dead panel's callback can never re-fire (instances disposed).
#[test]
fn close_spam_at_the_same_cell_never_double_fires_the_dead_panel() {
    let fired: Fired = Default::default();
    let f = fired.clone();
    let mut rig = Rig::new(Size::new(48, 8), move |cx| {
        let alive = cx.signal(0b111u32);
        Element::new()
            .style(
                LayoutStyle::column()
                    .width(abstracttui::layout::Dimension::Percent(1.0))
                    .height(abstracttui::layout::Dimension::Percent(1.0)),
            )
            .child(panel_row(cx, alive, f))
            .build()
    });
    let closes = rig.close_cells(0);
    let cell = closes[1];
    rig.click(cell, 0);
    rig.click(cell, 0);
    rig.click(cell, 0);
    let log = fired.borrow().clone();
    assert_eq!(
        log.iter().filter(|n| **n == "Beta").count(),
        1,
        "the dead panel fired exactly once: {log:?}"
    );
    // Every additional fire came from a LIVE panel that owned the cell,
    // and each of those fired at most once too (they die when they fire).
    for name in ["Alpha", "Gamma"] {
        assert!(log.iter().filter(|n| **n == name).count() <= 1, "{log:?}");
    }
    // Panels on screen == panels never fired.
    let screen = rig.screen();
    for name in ["Alpha", "Beta", "Gamma"] {
        assert_eq!(
            screen.contains(name),
            !log.contains(&name),
            "screen agrees with the fire log {log:?}:\n{screen}"
        );
    }
}

#[test]
fn closable_block_composes_inside_a_page_host_page() {
    let fired: Fired = Default::default();
    let f = fired.clone();
    let mut rig = Rig::new(Size::new(40, 10), move |cx| {
        let theme = use_theme(cx);
        let open = cx.signal(true);
        PageHost::new()
            .page("home", "Home", move |_gcx| {
                let t = theme.get().tokens;
                let f = f.clone();
                dyn_view(LayoutStyle::default().grow(1.0), move || {
                    if !open.get() {
                        return text("panel closed");
                    }
                    let f = f.clone();
                    Block::new()
                        .border(BorderKind::Rounded)
                        .title("Pane")
                        .on_close(move || {
                            f.borrow_mut().push("pane");
                            open.set(false);
                        })
                        .layout(LayoutStyle::column().grow(1.0))
                        .child(text("hosted body"))
                        .element(&t)
                        .build()
                })
            })
            .view(cx)
    });
    let screen = rig.screen();
    assert!(screen.contains("hosted body"), "{screen}");
    // The ✕ lives on the block's border row INSIDE the page (row 1 is
    // the tab bar's neighbor — find it wherever the page put it).
    let (row, col) = find_close(&rig).expect("a ✕ on screen");
    rig.click(col, row);
    assert_eq!(*fired.borrow(), vec!["pane"]);
    assert!(rig.screen().contains("panel closed"), "{}", rig.screen());
}

#[test]
fn closable_block_composes_inside_a_drawer_overlay() {
    let fired: Fired = Default::default();
    let f = fired.clone();
    let holder: Rc<RefCell<Option<Scope>>> = Default::default();
    let h = holder.clone();
    let mut rig = Rig::new(Size::new(46, 10), move |cx| {
        *h.borrow_mut() = Some(cx);
        Element::new()
            .style(
                LayoutStyle::column()
                    .width(abstracttui::layout::Dimension::Percent(1.0))
                    .height(abstracttui::layout::Dimension::Percent(1.0)),
            )
            .child(text("main page content"))
            .build()
    });
    let overlays = rig.app.overlays();
    let cx = holder.borrow().expect("scope");
    let theme_tokens = abstracttui::theme::default_theme().tokens;
    let handle = Drawer::new(DrawerEdge::Right)
        .size(DrawerSize::Cells(22))
        .title("Files")
        .focus(DrawerFocus::Modal)
        // Instant mode: this rig runs on the wall clock — a timed slide
        // would never land inside the settle loop.
        .motion(std::time::Duration::ZERO)
        .overlays(&overlays)
        .install(cx, move |_| {
            let f = f.clone();
            Block::new()
                .border(BorderKind::Rounded)
                .title("Pane")
                .on_close(move || f.borrow_mut().push("drawer-pane"))
                .layout(LayoutStyle::column().grow(1.0))
                .child(text("drawer body"))
                .element(&theme_tokens)
                .build()
        });
    handle.open();
    rig.settle();
    let screen = rig.screen();
    assert!(screen.contains("drawer body"), "{screen}");
    // TWO close affordances are on screen now: the drawer header's own
    // ✕ (row 0) and the block's (its border row, named by its title).
    // Click the BLOCK's — the drawer must stay open, the pane callback
    // must fire (screen-space SGR → layer-local math, end to end).
    let (row, col) = find_close_on_row_containing(&rig, "Pane").expect("the pane ✕");
    rig.click(col, row);
    assert_eq!(
        *fired.borrow(),
        vec!["drawer-pane"],
        "screen-space click reached the overlay's ✕ (layer-local math)"
    );
    assert!(
        rig.screen().contains("Files"),
        "the drawer's own ✕ was not the one clicked:\n{}",
        rig.screen()
    );
}

#[test]
fn closable_blocks_inside_a_scroll_close_correctly_mid_scroll() {
    let fired: Fired = Default::default();
    let f = fired.clone();
    let mut rig = Rig::new(Size::new(30, 7), move |cx| {
        let theme = use_theme(cx);
        let t = theme.get().tokens;
        let mut column = Element::new()
            .style(LayoutStyle::column().width(abstracttui::layout::Dimension::Percent(1.0)));
        for name in ["One", "Two", "Three", "Four", "Five"] {
            let f = f.clone();
            column = column.child(
                Block::new()
                    .border(BorderKind::Rounded)
                    .title(name)
                    .on_close(move || f.borrow_mut().push(name))
                    .layout(
                        LayoutStyle::column()
                            .width(abstracttui::layout::Dimension::Percent(1.0))
                            .height(abstracttui::layout::Dimension::Cells(3))
                            .shrink(0.0),
                    )
                    .child(text(format!("{name} body")))
                    .element(&t)
                    .build(),
            );
        }
        Scroll::new(column.build())
            .layout(LayoutStyle::fill())
            .element(cx, &t)
            .build()
    });
    // Wheel down so a LATER panel's border row is visible mid-viewport.
    rig.input(b"\x1b[<65;5;3M");
    rig.input(b"\x1b[<65;5;3M");
    rig.input(b"\x1b[<65;5;3M");
    let screen = rig.screen();
    let (row, col) = find_close(&rig).expect("a ✕ visible mid-scroll");
    let title_row: String = screen.lines().nth(row as usize).unwrap_or("").to_string();
    let expected = ["One", "Two", "Three", "Four", "Five"]
        .into_iter()
        .find(|n| title_row.contains(n))
        .expect("the ✕ row names its panel");
    rig.click(col, row);
    assert_eq!(
        *fired.borrow(),
        vec![expected],
        "the scrolled hit landed on the panel the frame shows"
    );
}

/// First ✕ on screen, as (row, col) — scanning the rendered text keeps
/// the tests honest: we click what the FRAME shows, never a computed
/// layout guess.
fn find_close(rig: &Rig) -> Option<(i32, i32)> {
    for (y, line) in rig.screen().lines().enumerate() {
        if let Some((x, _)) = line.chars().enumerate().find(|(_, ch)| *ch == '✕') {
            return Some((y as i32, x as i32));
        }
    }
    None
}

/// The ✕ on the row whose text contains `marker` — disambiguates when
/// several close affordances are on screen (a Block inside a titled
/// Drawer shows the drawer's own header ✕ too).
fn find_close_on_row_containing(rig: &Rig, marker: &str) -> Option<(i32, i32)> {
    for (y, line) in rig.screen().lines().enumerate() {
        if !line.contains(marker) {
            continue;
        }
        if let Some((x, _)) = line.chars().enumerate().find(|(_, ch)| *ch == '✕') {
            return Some((y as i32, x as i32));
        }
    }
    None
}
