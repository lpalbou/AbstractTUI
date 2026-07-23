//! PageHost: full complex pages behind a themed tab bar — the app-shell
//! page host (backlog 0545; the maintainer's "global tab system").
//!
//! `Tabs` (src/widgets/tabs.rs) stays the small in-content strip; a
//! PageHost is the HIGHER-LEVEL container: N pages addressed by id,
//! each a builder `FnMut(Scope) -> View` receiving a per-activation
//! GENERATION scope (`dyn_view_scoped`), a windowed tab bar with
//! badges/counts, container-reserved chords and opt-in digit jumps.
//! It stays inside the cycle-7 router ruling (compose.rs): navigation
//! state IS a signal — the host renders and mutates it, it never owns
//! routing (no history, no deep links).
//!
//! ## State ownership (THE recipe — no keep-alive, by design)
//!
//! Only the ACTIVE page is mounted. Switching disposes the outgoing
//! page's generation scope — its signals, effects, timers and focus
//! die with it — and builds the incoming page fresh. There is
//! deliberately NO keep-alive option: a hidden-but-mounted page keeps
//! its scope alive (its `interval`s tick, its sources ingest), which
//! violates the zero-idle law for invisible content. Durable page
//! state therefore lives in app-owned signals created OUTSIDE the
//! page builders (the compose.rs store pattern), and builders re-read
//! them on remount:
//!
//! ```ignore
//! let draft = cx.signal(String::new());     // survives switches
//! PageHost::new()
//!     .page("write", "Write", move |gcx| {
//!         // gcx dies on switch; `draft` does not.
//!         TextInput::new().value(draft).element(gcx, &t).build()
//!     })
//!     .view(cx)
//! # ;
//! ```
//!
//! ## Navigation contract
//!
//! - Click a tab (or the `‹`/`›` overflow indicators). Left/Right
//!   cycle (with wrap) while the BAR is focused.
//! - CHORDS — default Ctrl+PgUp / Ctrl+PgDn, replaced via
//!   [`PageHost::chords`] — are CONTAINER-RESERVED: intercepted at
//!   Capture phase on the host root, because scrollable widgets match
//!   PageUp/PageDown modifier-blind (scroll.rs/list.rs/table.rs) and
//!   would eat a bubble-layer chord. Plain PgUp/PgDn always stay with
//!   the content. Chords compare NORMALIZED (`KeyChord::normalized`),
//!   so both wire spellings of a shifted letter fire. Chords are live
//!   while focus is anywhere INSIDE the host. With NOTHING focused,
//!   keys target the tree root — a host mounted AS the root element
//!   answers chords from frame one; a host under a wrapper needs
//!   focus established first (click/Tab/`focus_first`, the main tree
//!   is not focus-initialized by the engine).
//! - DIGIT JUMPS 1-9 are OPT-IN ([`PageHost::number_jump`]) and ride
//!   the shortcut table (never capture): a focused TextInput keeps
//!   typing digits; apps own their number keys unless they opt in.
//! - FOCUS: a chord/digit switch re-anchors focus on the host root
//!   (programmatic focus needs no focusability — the focus_init
//!   pattern); the old page's focused node dies with its scope and
//!   an unanchored tree would send the NEXT chord to the tree root,
//!   off the host's path (the 0230 dead-keys class). Clicking keeps
//!   focus on the bar; bar arrows keep the bar focused.
//!
//! `on_change(id)` fires on HOST-driven switches after the active
//! write (disposal-safe, the 0297 law). External writes to a
//! controlled `active` signal switch pages without firing it.
//!
//! OWNER: TABS (wave 8).

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use crate::layout::{Dimension, Style as LayoutStyle};
use crate::reactive::{Scope, Signal};
use crate::theme::TokenSet;
use crate::ui::{
    dyn_view, dyn_view_scoped, Element, EventCtx, Key, KeyChord, Mods, MouseButton, MouseKind,
    Phase, UiEvent, View, ViewId,
};

#[path = "page_host_bar.rs"]
mod bar;

type PageBuilder = Box<dyn FnMut(Scope) -> View>;
type BadgeFn = Box<dyn Fn() -> Option<String>>;
type ChangeBox = Box<dyn FnMut(&str)>;
type ChangeFn = Rc<RefCell<Option<ChangeBox>>>;

struct PageDef {
    id: String,
    title: String,
    badge: Option<BadgeFn>,
    build: PageBuilder,
}

pub struct PageHost {
    pages: Vec<PageDef>,
    active: Option<Signal<String>>,
    initial: Option<String>,
    on_change: Option<ChangeBox>,
    prev_chords: Vec<KeyChord>,
    next_chords: Vec<KeyChord>,
    number_jump: bool,
    layout: Option<LayoutStyle>,
}

/// Unknown/stale ids FOLD to the first page (documented): a controlled
/// signal may transiently hold an id the host does not know; rendering
/// something honest beats a panic in a draw path.
fn idx_of(ids: &[String], id: &str) -> usize {
    ids.iter().position(|p| p == id).unwrap_or(0)
}

impl PageHost {
    pub fn new() -> PageHost {
        PageHost {
            pages: Vec::new(),
            active: None,
            initial: None,
            on_change: None,
            prev_chords: vec![KeyChord::new(Mods::CTRL, Key::PageUp)],
            next_chords: vec![KeyChord::new(Mods::CTRL, Key::PageDown)],
            number_jump: false,
            layout: None,
        }
    }

    /// Add a page: a stable id, the tab title, and the page builder.
    /// The builder receives the GENERATION scope — state created on it
    /// dies when the page deactivates (see the module recipe).
    pub fn page(
        mut self,
        id: impl Into<String>,
        title: impl Into<String>,
        build: impl FnMut(Scope) -> View + 'static,
    ) -> PageHost {
        let id = id.into();
        debug_assert!(
            !self.pages.iter().any(|p| p.id == id),
            "PageHost: duplicate page id {id:?}"
        );
        self.pages.push(PageDef {
            id,
            title: title.into(),
            badge: None,
            build: Box::new(build),
        });
        self
    }

    /// Attach a reactive badge/count to a page's tab. The getter runs
    /// TRACKED inside the bar region: a change to the signals it reads
    /// repaints the BAR only (the page never remounts). `None` hides
    /// the badge.
    pub fn badge(mut self, id: &str, badge: impl Fn() -> Option<String> + 'static) -> PageHost {
        match self.pages.iter_mut().find(|p| p.id == id) {
            Some(p) => p.badge = Some(Box::new(badge)),
            None => debug_assert!(false, "PageHost::badge: unknown page id {id:?}"),
        }
        self
    }

    /// Controlled mode: the app OWNS the active-page signal (id-valued).
    /// External writes switch pages; `on_change` fires only for
    /// host-driven switches.
    pub fn active(mut self, active: Signal<String>) -> PageHost {
        self.active = Some(active);
        self
    }

    /// Uncontrolled mode's start page (ignored when `active` is given).
    pub fn initial(mut self, id: impl Into<String>) -> PageHost {
        self.initial = Some(id.into());
        self
    }

    /// Fires AFTER the active write on host-driven switches — the
    /// callback may dispose the host's scope (the 0297 law).
    pub fn on_change(mut self, f: impl FnMut(&str) + 'static) -> PageHost {
        self.on_change = Some(Box::new(f));
        self
    }

    /// Replace the prev/next chord sets (defaults: Ctrl+PgUp /
    /// Ctrl+PgDn — the wire every terminal delivers, `CSI 5;5~` /
    /// `CSI 6;5~`). Chords are container-reserved (module docs).
    pub fn chords(mut self, prev: &[KeyChord], next: &[KeyChord]) -> PageHost {
        self.prev_chords = prev.to_vec();
        self.next_chords = next.to_vec();
        self
    }

    /// Opt into plain-digit page jumps (1-9, first nine pages). OFF by
    /// default: apps own their number keys.
    pub fn number_jump(mut self, on: bool) -> PageHost {
        self.number_jump = on;
        self
    }

    pub fn layout(mut self, layout: LayoutStyle) -> PageHost {
        self.layout = Some(layout);
        self
    }

    /// Canonical one-call build: tokens resolve from the app's THEME
    /// CONTEXT inside the bar's own dyn region, so the bar retints on
    /// theme switch without remounting the active page.
    pub fn view(self, cx: Scope) -> View {
        self.assemble(cx, None).build()
    }

    /// Explicit-token build (tests, custom theming): the bar captures
    /// `t` — the caller owns retint policy (rebuild to retint).
    pub fn element(self, cx: Scope, t: &TokenSet) -> Element {
        self.assemble(cx, Some(*t))
    }

    fn assemble(self, cx: Scope, fixed: Option<TokenSet>) -> Element {
        let n = self.pages.len();
        let mut ids = Vec::with_capacity(n);
        let mut titles = Vec::with_capacity(n);
        let mut badges = Vec::with_capacity(n);
        let mut builders = Vec::with_capacity(n);
        for p in self.pages {
            ids.push(p.id);
            titles.push(p.title);
            badges.push(p.badge);
            builders.push(p.build);
        }
        let ids: Rc<Vec<String>> = Rc::new(ids);
        let titles: Rc<Vec<String>> = Rc::new(titles);
        let badges: Rc<Vec<Option<BadgeFn>>> = Rc::new(badges);
        let builders = Rc::new(RefCell::new(builders));

        // Active-page signal: controlled (app-owned) or uncontrolled.
        if let (Some(want), None) = (&self.initial, &self.active) {
            debug_assert!(
                ids.contains(want),
                "PageHost::initial: unknown page id {want:?}"
            );
        }
        let start = self
            .initial
            .filter(|want| ids.contains(want))
            .or_else(|| ids.first().cloned())
            .unwrap_or_default();
        let active = self.active.unwrap_or_else(|| cx.signal(start));
        let on_change: ChangeFn = Rc::new(RefCell::new(self.on_change));

        // Host-driven switch: write first, callback second (0297 — the
        // callback may dispose everything, including this host).
        let switch: Rc<dyn Fn(usize)> = {
            let ids = ids.clone();
            let on_change = on_change.clone();
            Rc::new(move |target: usize| {
                if ids.is_empty() {
                    return;
                }
                let target = target.min(ids.len() - 1);
                let cur = active.with_untracked(|id| idx_of(&ids, id));
                if cur == target {
                    return;
                }
                active.set(ids[target].clone());
                if let Some(f) = on_change.borrow_mut().as_mut() {
                    f(ids[target].as_str());
                }
            })
        };
        // Prev/next with WRAP (a cycling gesture — tmux precedent).
        let switch_rel: Rc<dyn Fn(i32)> = {
            let ids = ids.clone();
            let switch = switch.clone();
            Rc::new(move |dir: i32| {
                let n = ids.len();
                if n == 0 {
                    return;
                }
                let cur = active.with_untracked(|id| idx_of(&ids, id)) as i32;
                switch((cur + dir).rem_euclid(n as i32) as usize);
            })
        };

        // Shared bar state: the dyn build refreshes it (tracked reads);
        // the draw closure and the mouse handler both consume it through
        // the ONE pure plan (`bar::plan_bar`) — no mirrored arithmetic.
        let bar_state = Rc::new(RefCell::new(bar::BarModel {
            items: Vec::new(),
            active: 0,
        }));
        // Sticky window anchor — render bookkeeping, not reactive state.
        let window_first = Rc::new(Cell::new(0usize));
        // The plan AS DRAWN (plus the width it was planned for): the
        // mouse handler hit-tests against what the user actually SEES.
        // Recomputing from the live model raced same-batch model
        // changes — a badge widening between the last draw and a click
        // shifted the segments under the pointer, so the press landed
        // on the wrong tab (review2 F1, DRAWER cross-review; pinned by
        // wave_shell_review2::click_resolves_against_the_drawn_bar_*).
        let drawn_plan: Rc<RefCell<Option<(bar::BarPlan, i32)>>> = Rc::new(RefCell::new(None));

        let bar_handler = {
            let state = bar_state.clone();
            let first = window_first.clone();
            let drawn = drawn_plan.clone();
            let switch = switch.clone();
            let switch_rel = switch_rel.clone();
            move |ctx: &mut EventCtx, ev: &UiEvent| match ev {
                UiEvent::Key(k) => {
                    // Plain arrows only: modified arrows belong to the
                    // app (spatial nav chords etc.).
                    if k.mods != Mods::NONE {
                        return;
                    }
                    match k.key {
                        Key::Left => switch_rel(-1),
                        Key::Right => switch_rel(1),
                        _ => return,
                    }
                    ctx.stop_propagation();
                }
                UiEvent::Mouse(m) => {
                    if let MouseKind::Down(MouseButton::Left) = m.kind {
                        let rect = ctx.current_rect();
                        let hit = {
                            // Prefer the drawn plan (pixel truth); fall
                            // back to a fresh plan only before the first
                            // draw (nothing visible to aim at yet).
                            let stashed = drawn.borrow().clone();
                            let (plan, avail) = stashed.unwrap_or_else(|| {
                                let model = state.borrow();
                                (bar::plan_bar(&model, first.get(), rect.w), rect.w)
                            });
                            bar::hit_bar(&plan, avail, m.pos.x - rect.x)
                        };
                        match hit {
                            bar::BarHit::Prev => switch_rel(-1),
                            bar::BarHit::Next => switch_rel(1),
                            bar::BarHit::Tab(i) => switch(i),
                            bar::BarHit::Miss => return,
                        }
                        ctx.stop_propagation();
                    }
                }
                _ => {}
            }
        };

        let access_value = {
            let ids = ids.clone();
            let titles = titles.clone();
            let badges = badges.clone();
            move || {
                if titles.is_empty() {
                    return String::new();
                }
                let idx = active.with_untracked(|id| idx_of(&ids, id));
                let mut s = format!("{} ({}/{})", titles[idx], idx + 1, titles.len());
                if let Some(getter) = badges.get(idx).and_then(|g| g.as_ref()) {
                    // The snapshot samples untracked; badge getters read
                    // tracked signals, so shield them explicitly.
                    if let Some(b) = crate::reactive::untrack(getter) {
                        s.push_str(" [");
                        s.push_str(&b);
                        s.push(']');
                    }
                }
                s
            }
        };

        let bar_dyn = {
            let ids = ids.clone();
            let titles = titles.clone();
            let badges = badges.clone();
            let state = bar_state.clone();
            let first = window_first.clone();
            let drawn = drawn_plan.clone();
            dyn_view(
                LayoutStyle::default()
                    .width(Dimension::Percent(1.0))
                    .height(Dimension::Cells(2)),
                move || {
                    // Tracked: active id, every badge getter, and (view
                    // path) the theme context — a count or theme change
                    // repaints the BAR only; the page never remounts.
                    let t = fixed.unwrap_or_else(|| crate::widgets::theme_tokens(cx));
                    let active_idx = idx_of(&ids, &active.get());
                    let items: Vec<bar::BarItem> = titles
                        .iter()
                        .enumerate()
                        .map(|(i, title)| bar::BarItem {
                            title: title.clone(),
                            badge: badges[i].as_ref().and_then(|f| f()),
                        })
                        .collect();
                    *state.borrow_mut() = bar::BarModel {
                        items,
                        active: active_idx,
                    };
                    let ink = bar::ink_from(&t);
                    let state = state.clone();
                    let first = first.clone();
                    let drawn = drawn.clone();
                    Element::new()
                        .style(
                            LayoutStyle::default()
                                .width(Dimension::Percent(1.0))
                                .height(Dimension::Cells(2)),
                        )
                        .draw(move |canvas, rect| {
                            let m = state.borrow();
                            let plan = bar::plan_bar(&m, first.get(), rect.w);
                            first.set(plan.first);
                            // Publish pixel truth for the hit-test
                            // (review2 F1) — plain-cell bookkeeping,
                            // no reactive access (RT1-2 holds).
                            *drawn.borrow_mut() = Some((plan.clone(), rect.w));
                            bar::draw_bar(canvas, rect, &m, &plan, &ink);
                        })
                        .build()
                },
            )
        };

        // shrink 0: the bar is the widget's control surface — a tight
        // box crushes the PAGE, never the tabs (0240 #2).
        let bar_el = Element::new()
            .style(
                LayoutStyle::default()
                    .height(Dimension::Cells(2))
                    .shrink(0.0),
            )
            .role(crate::ui::Role::Tabs)
            .access_value(access_value)
            .focusable()
            .on(Phase::Bubble, bar_handler)
            .child(bar_dyn);

        // The page region: exactly the active builder mounts, on a
        // generation scope disposed at the next switch.
        let page_dyn = {
            let ids = ids.clone();
            dyn_view_scoped(
                LayoutStyle::default()
                    .width(Dimension::Percent(1.0))
                    .grow(1.0),
                move |gen_cx| {
                    let idx = idx_of(&ids, &active.get());
                    match builders.borrow_mut().get_mut(idx) {
                        Some(build) => build(gen_cx),
                        None => crate::ui::text(""),
                    }
                },
            )
        };

        // Focus anchor: a Capture-phase recorder notes the host's own
        // ViewId on every key routed through it. Registered BEFORE the
        // chord interceptor on the same node, so even the very first
        // chord can re-anchor (handlers run in registration order).
        let anchor: Rc<Cell<Option<ViewId>>> = Rc::new(Cell::new(None));
        let mut root = Element::new()
            .style(self.layout.unwrap_or_else(LayoutStyle::column))
            .on(Phase::Capture, {
                let anchor = anchor.clone();
                move |ctx: &mut EventCtx, ev: &UiEvent| {
                    if matches!(ev, UiEvent::Key(_)) {
                        anchor.set(ctx.current());
                    }
                }
            })
            .on(Phase::Capture, {
                let prev = self.prev_chords.clone();
                let next = self.next_chords.clone();
                let switch_rel = switch_rel.clone();
                let anchor = anchor.clone();
                move |ctx: &mut EventCtx, ev: &UiEvent| {
                    let UiEvent::Key(k) = ev else { return };
                    let chord = k.chord().normalized();
                    let dir = if next.iter().any(|c| c.normalized() == chord) {
                        1
                    } else if prev.iter().any(|c| c.normalized() == chord) {
                        -1
                    } else {
                        return;
                    };
                    switch_rel(dir);
                    if let Some(id) = anchor.get() {
                        ctx.request_focus(id);
                    }
                    ctx.stop_propagation();
                }
            });
        // Labeled twins in the shortcut table: keymap-help surfaces the
        // vocabulary, and the action stays correct even if the capture
        // interceptor ever stops consuming first.
        for c in &self.next_chords {
            let switch_rel = switch_rel.clone();
            let anchor = anchor.clone();
            root = root.shortcut_labeled(*c, "next page", move |ctx| {
                switch_rel(1);
                if let Some(id) = anchor.get() {
                    ctx.request_focus(id);
                }
            });
        }
        for c in &self.prev_chords {
            let switch_rel = switch_rel.clone();
            let anchor = anchor.clone();
            root = root.shortcut_labeled(*c, "previous page", move |ctx| {
                switch_rel(-1);
                if let Some(id) = anchor.get() {
                    ctx.request_focus(id);
                }
            });
        }
        if self.number_jump {
            for i in 0..n.min(9) {
                let digit = char::from_digit(i as u32 + 1, 10).expect("digits 1-9");
                let switch = switch.clone();
                let anchor = anchor.clone();
                root = root.shortcut_labeled(
                    KeyChord::plain(Key::Char(digit)),
                    format!("page {}: {}", i + 1, titles[i]),
                    move |ctx| {
                        switch(i);
                        if let Some(id) = anchor.get() {
                            ctx.request_focus(id);
                        }
                    },
                );
            }
        }
        root.child(bar_el.build()).child(page_dyn)
    }
}

impl Default for PageHost {
    fn default() -> Self {
        PageHost::new()
    }
}

#[cfg(test)]
#[path = "page_host_tests.rs"]
mod tests;
