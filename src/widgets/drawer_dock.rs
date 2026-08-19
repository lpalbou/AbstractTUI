//! DrawerDock: a right-edge rail of always-visible vertical tabs, each
//! fronting a docked side panel — at most ONE open, and the panel
//! column vanishes entirely while none is (app-kits 1255; the web
//! team-page drawer rail, translated to cells).
//!
//! The reference surface (abstractcontinuum's team page) keeps
//! Assistant / Members / Files / Leaderboard / Desk behind a persistent
//! strip of vertical tabs on the right edge: click a tab and its panel
//! docks open beside the content; click the active tab (or the panel's
//! ✕) and the surface collapses back to the bare rail. This widget is
//! that contract in the layout tree:
//!
//! - **Docked, not overlaid.** The panel is a LAYOUT column between the
//!   content and the rail — content reflows around it, exactly like the
//!   web page. The transient, sliding, scrimmed cousin is
//!   [`app::drawer`](crate::app::drawer); this is persistent chrome.
//! - **Collapsed = a rail, open = one panel.** `open` is a
//!   `Signal<Option<String>>` of the drawer id (`None` = collapsed).
//!   Bind it to drive the dock from keys or restore it across sessions;
//!   the dock renders and mutates it but never owns routing (the
//!   cycle-7 router ruling, PageHost's).
//! - **Closed = disposed** (the PageHost recipe, zero-idle law): only
//!   the open drawer's body is mounted, inside a per-open GENERATION
//!   scope. Durable drawer state lives in app signals created OUTSIDE
//!   the builders, which re-read them on remount.
//! - **Badges** ride the tabs ([`DrawerDock::drawer_badge`]): a themed
//!   dot under the vertical label, resolved reactively — the "something
//!   waits behind this drawer" affordance, readable without opening.
//!
//! ## Interaction contract
//!
//! Click a tab: open that drawer (replacing any other). Click the
//! ACTIVE tab or its header ✕ to collapse. (Esc also collapses, but
//! only while focus sits INSIDE the panel — a text-only drawer that
//! never takes focus closes by mouse alone, like the web reference.)
//! Drawer BUILDERS must not write `open` synchronously — a redirect
//! belongs in `on_change` or an effect; the builder runs inside the
//! panel's own tracked computation, where a same-turn write silently
//! desyncs state from screen (debug builds assert). Tabs render
//! top-down at fixed height: keep titles short — a tab past the
//! viewport bottom is clipped and unreachable by mouse (a
//! height-aware rail is future work). `on_change` fires on
//! DOCK-driven transitions after the state write (disposal-safe, the
//! 0297 law); external writes to a bound `open` signal switch panels
//! without firing it. There are no container-reserved chords: apps
//! bind their own keys by writing `open` — the signal IS the API
//! (PageHost's chords exist because a page host owns navigation; a
//! dock does not).
//!
//! OWNER: TABS (app-kits wave).

use std::cell::RefCell;
use std::rc::Rc;

use crate::base::Point;
use crate::layout::{Dimension, Edges, Style as LayoutStyle};
use crate::reactive::{Scope, Signal};
use crate::render::Style;
use crate::theme::TokenSet;
use crate::ui::{
    dyn_view, dyn_view_scoped, Element, EventCtx, Key, Mods, MouseButton, MouseKind, Phase,
    UiEvent, View,
};
use unicode_segmentation::UnicodeSegmentation;

/// The header close affordance — `✕` U+2715, the engine's established
/// dismiss glyph (Block close, List `on_remove`): East-Asian-NARROW,
/// absent from emoji-data, single-width everywhere.
const CLOSE_GLYPH: &str = "✕";

/// Rail column width in cells: padding, glyph, padding.
const RAIL_W: i32 = 3;

type DrawerBuilder = Box<dyn FnMut(Scope) -> View>;
type BadgeFn = Box<dyn Fn() -> bool>;
type ChangeBox = Box<dyn FnMut(Option<&str>)>;
type ChangeFn = Rc<RefCell<Option<ChangeBox>>>;

struct DrawerDef {
    id: String,
    title: String,
    badge: Option<BadgeFn>,
    build: DrawerBuilder,
}

/// The right-edge drawer rail + docked panel. See the [module
/// docs](self) for the contract; the canonical build is `.view(cx)`.
pub struct DrawerDock {
    content: View,
    drawers: Vec<DrawerDef>,
    open: Option<Signal<Option<String>>>,
    panel_width: i32,
    on_change: Option<ChangeBox>,
    layout: Option<LayoutStyle>,
}

impl DrawerDock {
    /// A dock around `content` (the main surface; it keeps every cell
    /// the rail and panel do not use).
    pub fn new(content: View) -> DrawerDock {
        DrawerDock {
            content,
            drawers: Vec::new(),
            open: None,
            panel_width: 36,
            on_change: None,
            layout: None,
        }
    }

    /// Add a drawer: `id` (state key), `title` (vertical tab label AND
    /// panel header), and the body builder. The builder runs on OPEN
    /// inside a generation scope that dies on close/switch — durable
    /// state belongs to signals created outside it (module docs).
    pub fn drawer(
        mut self,
        id: impl Into<String>,
        title: impl Into<String>,
        build: impl FnMut(Scope) -> View + 'static,
    ) -> DrawerDock {
        let id = id.into();
        debug_assert!(
            self.drawers.iter().all(|d| d.id != id),
            "DrawerDock: duplicate drawer id {id:?} — the first registration \
             wins every lookup, the second is unreachable",
        );
        self.drawers.push(DrawerDef {
            id,
            title: title.into(),
            badge: None,
            build: Box::new(build),
        });
        self
    }

    /// Attach a reactive badge to the LAST added drawer: while `f`
    /// answers true, a themed dot renders under that tab's label.
    /// Resolved inside the rail's reactive region — read signals freely.
    pub fn drawer_badge(mut self, f: impl Fn() -> bool + 'static) -> DrawerDock {
        debug_assert!(
            !self.drawers.is_empty(),
            "DrawerDock::drawer_badge attaches to the LAST added drawer — \
             call it after `.drawer(..)`, never before the first one",
        );
        if let Some(last) = self.drawers.last_mut() {
            last.badge = Some(Box::new(f));
        }
        self
    }

    /// Bind the open-drawer state (`None` = collapsed). The dock writes
    /// it on tab/✕/Esc interactions; the app may write it any time —
    /// external writes switch panels without firing `on_change`.
    pub fn open(mut self, sig: Signal<Option<String>>) -> DrawerDock {
        self.open = Some(sig);
        self
    }

    /// Panel column width in cells (default 36).
    pub fn panel_width(mut self, w: i32) -> DrawerDock {
        self.panel_width = w.max(8);
        self
    }

    /// Observe DOCK-driven transitions (`Some(id)` opened, `None`
    /// collapsed). Fires after the state write; a handler may dispose
    /// the dock's scope (0297).
    pub fn on_change(mut self, f: impl FnMut(Option<&str>) + 'static) -> DrawerDock {
        self.on_change = Some(Box::new(f));
        self
    }

    /// Root layout override (default: fill the parent).
    pub fn layout(mut self, layout: LayoutStyle) -> DrawerDock {
        self.layout = Some(layout);
        self
    }

    /// Build with the ambient theme's tokens.
    pub fn view(self, cx: Scope) -> View {
        let t = super::theme_tokens(cx);
        self.element(cx, &t).build()
    }

    /// Build against explicit tokens (theming recipes / tests).
    pub fn element(self, cx: Scope, t: &TokenSet) -> Element {
        let DrawerDock {
            content,
            drawers,
            open,
            panel_width,
            on_change,
            layout,
        } = self;
        let open = open.unwrap_or_else(|| cx.signal(None));
        let on_change: ChangeFn = Rc::new(RefCell::new(on_change));

        // Shared defs: the rail reads titles/badges, the panel slot
        // takes the builders. The RefCell borrow spans the USER's
        // builder in the panel closure — safe because effect re-runs
        // are never inline (reactive law), so no reentry path exists;
        // the debug_assert below keeps the builder honest instead.
        let defs = Rc::new(RefCell::new(drawers));

        // The dock's ONE transition choke point: every interaction
        // (tab click, ✕, Esc) funnels here so `on_change` can never
        // miss a dock-driven write. Same-value writes are elided.
        let transition = Rc::new(move |next: Option<String>| {
            if open.get_untracked() == next {
                return;
            }
            open.set(next.clone());
            // Take the callback OUT while it runs (reentrancy: a
            // handler opening another drawer must not hit the RefCell).
            let taken = on_change.borrow_mut().take();
            if let Some(mut f) = taken {
                f(next.as_deref());
                if on_change.borrow().is_none() {
                    *on_change.borrow_mut() = Some(f);
                }
            }
        });

        // ---- panel: a generation-scoped slot keyed on the open id ----
        let panel_defs = defs.clone();
        let panel_transition = transition.clone();
        let panel_t = *t;
        let panel = dyn_view_scoped(
            LayoutStyle::default().height(Dimension::Percent(1.0)),
            move |gcx| {
                let Some(id) = open.get() else {
                    // Collapsed: the slot solves to zero width — the
                    // content column absorbs every cell the panel had.
                    return Element::new()
                        .style(LayoutStyle::default().width(Dimension::Cells(0)))
                        .build();
                };
                let mut defs = panel_defs.borrow_mut();
                let Some(def) = defs.iter_mut().find(|d| d.id == id) else {
                    // Unknown id (a stale restore): render collapsed but
                    // do NOT rewrite the app's signal from render — a
                    // later registration may redeem it.
                    return Element::new()
                        .style(LayoutStyle::default().width(Dimension::Cells(0)))
                        .build();
                };
                let title = def.title.clone();
                let body = (def.build)(gcx);
                drop(defs);
                // A builder that writes `open` SYNCHRONOUSLY writes a
                // dependency of the computation running it. The engine's
                // law makes a self-invalidating computation a named
                // panic, but through the Dyn flush path the re-run is
                // swallowed — state and screen silently desync
                // (adversarial review P1). Make it loud where the law
                // could not: redirect from `on_change` or an effect,
                // never from inside the builder.
                debug_assert!(
                    open.get_untracked().as_deref() == Some(id.as_str()),
                    "DrawerDock: drawer builder {id:?} wrote `open` while its \
                     panel was being built — redirect from `on_change` or an \
                     effect instead",
                );
                let close = panel_transition.clone();
                let close_esc = panel_transition.clone();
                let bg = panel_t.surface_raised;
                // Header: title + a ✕ whose hit region is the whole
                // trailing corner (the drawer close-corner rule, 0.3.1:
                // a one-cell target at the panel edge is exactly where
                // real terminals quantize an edge click into the
                // neighbouring column — give it jitter room).
                let header_title = title.clone();
                let header = Element::new()
                    .style(
                        LayoutStyle::default()
                            .height(Dimension::Cells(1))
                            .width(Dimension::Percent(1.0))
                            .shrink(0.0),
                    )
                    .draw(move |canvas, rect| {
                        if rect.is_empty() {
                            return;
                        }
                        // Title truncated CLEAR of the ✕ corner: the
                        // close hot zone is the trailing 4 cells, so the
                        // title may use at most `w - 7` columns (start
                        // x+2, one-cell gap before the zone). Without
                        // this, a long title's clickable cells fell
                        // INSIDE the zone and clicking the title closed
                        // the panel (adversarial review P2-1/2).
                        let avail = rect.w - 7;
                        if avail > 0 {
                            let ink = Style::new().fg(panel_t.text).bg(bg).bold();
                            let mut shown = String::new();
                            let mut used = 0;
                            for g in header_title.graphemes(true) {
                                let gw = crate::text::width(g);
                                if used + gw > avail {
                                    break;
                                }
                                shown.push_str(g);
                                used += gw;
                            }
                            canvas.print_styled(Point::new(rect.x + 2, rect.y), &shown, &ink);
                        }
                        if rect.w >= 6 {
                            let dim = Style::new().fg(panel_t.text_faint).bg(bg);
                            canvas.print_styled(
                                Point::new(rect.right() - 2, rect.y),
                                CLOSE_GLYPH,
                                &dim,
                            );
                        }
                    })
                    .on(Phase::Bubble, move |ctx: &mut EventCtx, ev: &UiEvent| {
                        if let UiEvent::Mouse(m) = ev {
                            if m.kind == MouseKind::Down(MouseButton::Left) {
                                let rect = ctx.current_rect();
                                // Trailing REGION: the glyph cell, the
                                // margin beside it, two cells of jitter.
                                if m.pos.x >= rect.right() - 4 {
                                    close(None);
                                    ctx.stop_propagation();
                                }
                            }
                        }
                    })
                    .build();
                Element::new()
                    .style(
                        LayoutStyle::column()
                            .width(Dimension::Cells(panel_width))
                            .height(Dimension::Percent(1.0)),
                    )
                    // Ground fill + chrome, before children paint:
                    // a left border seam (content | panel reads as two
                    // surfaces, not one smear) and a header underline.
                    .draw(move |canvas, rect| {
                        if rect.is_empty() {
                            return;
                        }
                        let ink = Style::new().fg(panel_t.text).bg(bg);
                        canvas.fill_styled(rect, ' ', &ink);
                        let seam = Style::new().fg(panel_t.border).bg(bg);
                        for y in rect.y..rect.bottom() {
                            canvas.print_styled(Point::new(rect.x, y), "\u{2502}", &seam);
                        }
                        if rect.h > 1 {
                            for x in (rect.x + 1)..rect.right() {
                                canvas.print_styled(Point::new(x, rect.y + 1), "\u{2500}", &seam);
                            }
                        }
                    })
                    // Esc collapses while the key routes through the
                    // panel subtree (focus inside it). Bubble phase: an
                    // inner editor that consumed Esc wins.
                    .on(Phase::Bubble, move |ctx: &mut EventCtx, ev: &UiEvent| {
                        if let UiEvent::Key(k) = ev {
                            if k.key == Key::Escape && k.mods == Mods::NONE {
                                close_esc(None);
                                ctx.stop_propagation();
                            }
                        }
                    })
                    .child(header)
                    // One fixed row over the underline the ground drew.
                    .child(
                        Element::new()
                            .style(
                                LayoutStyle::default()
                                    .height(Dimension::Cells(1))
                                    .shrink(0.0),
                            )
                            .build(),
                    )
                    .child(
                        Element::new()
                            .style(LayoutStyle::column().grow(1.0).padding(Edges {
                                left: 2,
                                right: 1,
                                top: 0,
                                bottom: 0,
                            }))
                            .child(body)
                            .build(),
                    )
                    .build()
            },
        );

        // ---- rail: always-visible vertical tabs ----------------------
        let rail_defs = defs;
        let rail_transition = transition;
        let rail_t = *t;
        let rail = dyn_view(
            LayoutStyle::column()
                .width(Dimension::Cells(RAIL_W))
                .height(Dimension::Percent(1.0))
                .shrink(0.0),
            move || {
                let active = open.get();
                let mut col = Element::new()
                    .style(
                        LayoutStyle::column()
                            .width(Dimension::Cells(RAIL_W))
                            .height(Dimension::Percent(1.0)),
                    )
                    // The rail strip's own ground (below the tab blocks).
                    .draw(move |canvas, rect| {
                        let ink = Style::new().fg(rail_t.text_faint).bg(rail_t.surface);
                        canvas.fill_styled(rect, ' ', &ink);
                    });
                let defs = rail_defs.borrow();
                for def in defs.iter() {
                    let id = def.id.clone();
                    let is_active = active.as_deref() == Some(id.as_str());
                    // Grapheme clusters, one per rail row: a combining
                    // accent or ZWJ family renders as ONE glyph, never
                    // shredded across rows (adversarial review P2-3).
                    let label: Vec<String> =
                        def.title.graphemes(true).map(str::to_string).collect();
                    let badge = def.badge.as_ref().is_some_and(|f| f());
                    // Tab block: one blank row, the label vertically,
                    // an optional badge dot, one blank row.
                    let rows = label.len() as i32 + 2 + i32::from(badge);
                    let (fg, bg) = if is_active {
                        (rail_t.accent, rail_t.surface_raised)
                    } else {
                        (rail_t.text_faint, rail_t.surface)
                    };
                    let accent = rail_t.accent;
                    let toggle = rail_transition.clone();
                    let next = if is_active { None } else { Some(id) };
                    col = col.child(
                        Element::new()
                            .style(
                                LayoutStyle::default()
                                    .width(Dimension::Cells(RAIL_W))
                                    .height(Dimension::Cells(rows))
                                    .shrink(0.0),
                            )
                            .draw(move |canvas, rect| {
                                if rect.is_empty() {
                                    return;
                                }
                                let ink = Style::new().fg(fg).bg(bg);
                                canvas.fill_styled(rect, ' ', &ink);
                                let x = rect.x + 1;
                                for (i, g) in label.iter().enumerate() {
                                    let y = rect.y + 1 + i as i32;
                                    if y >= rect.bottom() {
                                        break;
                                    }
                                    canvas.print_styled(Point::new(x, y), g, &ink);
                                }
                                if badge {
                                    let y = rect.y + 1 + label.len() as i32;
                                    if y < rect.bottom() {
                                        let dot = Style::new().fg(accent).bg(bg);
                                        canvas.print_styled(Point::new(x, y), "●", &dot);
                                    }
                                }
                            })
                            .on(Phase::Bubble, move |ctx: &mut EventCtx, ev: &UiEvent| {
                                if let UiEvent::Mouse(m) = ev {
                                    if m.kind == MouseKind::Down(MouseButton::Left) {
                                        toggle(next.clone());
                                        ctx.stop_propagation();
                                    }
                                }
                            })
                            .build(),
                    );
                }
                col.build()
            },
        );

        Element::new()
            .style(layout.unwrap_or_else(|| {
                LayoutStyle::row()
                    .width(Dimension::Percent(1.0))
                    .height(Dimension::Percent(1.0))
            }))
            .child(
                Element::new()
                    .style(
                        LayoutStyle::column()
                            .grow(1.0)
                            .height(Dimension::Percent(1.0)),
                    )
                    .child(content)
                    .build(),
            )
            .child(panel)
            .child(rail)
    }
}

#[cfg(test)]
#[path = "drawer_dock_tests.rs"]
mod tests;
