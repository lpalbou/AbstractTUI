//! Drawer: an edge-anchored overlay panel hosting a full page (backlog
//! 0585 — the entity-app drawer, translated to cells).
//!
//! A drawer rides `app::overlays` exactly like `Modal`/`Toast` (it holds
//! no engine privileges): the panel is a TREE layer, the optional scrim
//! a manual veil layer, the slide a `reactive::animate` follower driving
//! `LayerHandle::set_offset` — frames exist ONLY while the flight is
//! live (`Layer::set_origin` bills old ∪ new bounds, so a slide damages
//! its band and nothing else), and a settled or closed drawer costs
//! zero (the Toast idle standard, wave-pinned in `tests/wave_drawers.rs`).
//!
//! ## Focus modes
//!
//! [`DrawerFocus::Modal`] (default): a focus-trapped modal tree — every
//! input routes to the panel while open, Esc closes, a press outside
//! closes when [`Drawer::close_on_outside`] allows (the Modal/Popup
//! precedents). [`DrawerFocus::Passive`]: glanceable — keys stay with
//! the main surface until the user clicks into the panel (the engine's
//! focused-overlay key rule), Esc closes only while the panel holds
//! focus. The web `AfDrawer` this component mirrors is non-modal in a
//! mouse-first browser; in a keyboard-first terminal an unfocused panel
//! cannot even scroll, so Modal is the honest default here.
//!
//! ## Closed = removed (the zero-idle law)
//!
//! A closed drawer removes its layers and disposes its mount scope. A
//! hidden-but-mounted tree would keep accumulating damage that
//! `Overlays::draw_all` never drains (invisible layers skip), pinning
//! `has_pending_work` true forever — a frame spin. State that must
//! survive close therefore lives OUTSIDE the build closure (the Tabs
//! rule): create signals in the installing scope, capture them in
//! `build`, and the next open finds them intact. Long-running work
//! (intervals, sources) belongs to the installing scope too.
//!
//! ## Stacking laws
//!
//! Drawers occupy a fixed band BELOW [`MODAL_Z`](super::popups::MODAL_Z)
//! with per-edge slots (scrim directly under its panel): Left < Right <
//! Top < Bottom. A modal opened from a drawer layers above it; owned
//! popups (`top_z() + 1`) above everything live; toasts above modals.
//! Fixed slots avoid the equal-z stale-order trap AND z creep. ONE
//! drawer per edge: opening on an occupied edge finishes the incumbent
//! instantly with [`DrawerCloseReason::Replaced`] (an animated handoff
//! would stack two panels on one slot).
//!
//! A terminal RESIZE re-clamps instead of dismissing (drawers are
//! long-lived chrome; unlike `Popup` there is no captured anchor to go
//! stale — the edge itself moves deterministically): geometry resolves
//! against the fresh viewport, surfaces and the panel tree resize, and
//! an in-flight slide continues toward the new resting place.
//!
//! OWNER: DRAWER (0585).

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use crate::base::{Point, Rect, Size};
use crate::reactive::{Scope, Signal};
use crate::ui::View;

use super::overlays::{LayerHandle, Overlays};
use super::viewport::use_viewport;

/// Base of the drawer z band. Per-edge slots live at
/// `DRAWER_Z + edge*2` (scrim) and `DRAWER_Z + edge*2 + 1` (panel),
/// all strictly below `MODAL_Z` — a modal opened from a drawer layers
/// above it by construction.
pub const DRAWER_Z: i32 = 800;

/// Which viewport edge the drawer slides from.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum DrawerEdge {
    Left,
    Right,
    Top,
    Bottom,
}

impl DrawerEdge {
    fn index(self) -> usize {
        match self {
            DrawerEdge::Left => 0,
            DrawerEdge::Right => 1,
            DrawerEdge::Top => 2,
            DrawerEdge::Bottom => 3,
        }
    }

    fn panel_z(self) -> i32 {
        DRAWER_Z + (self.index() as i32) * 2 + 1
    }

    fn scrim_z(self) -> i32 {
        self.panel_z() - 1
    }
}

/// Drawer extent along its slide axis (width for Left/Right, height
/// for Top/Bottom). The cross axis always fills the viewport.
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum DrawerSize {
    /// Fixed cells, clamped to the viewport.
    Cells(i32),
    /// Fraction of the viewport axis (clamped 0..=1), rounded.
    Percent(f32),
}

/// Input policy while open. See the module docs for the default's
/// rationale.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum DrawerFocus {
    /// Focus-trapped modal tree: all input routes to the panel.
    Modal,
    /// Glanceable: keys stay with the main surface until the user
    /// clicks into the panel (click-to-focus); no scrim.
    Passive,
}

/// Why a drawer closed. Delivered to [`Drawer::on_close`] once per
/// close, with the FIRST reason that ended that open.
#[non_exhaustive]
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum DrawerCloseReason {
    /// The handle (`close`/`toggle`), a bound signal write, or the
    /// header's close affordance.
    Api,
    /// Escape inside the panel.
    Escape,
    /// A press outside a modal drawer's bounds (the scrim, when shown).
    OutsidePress,
    /// Another drawer opened on the same edge (one per edge).
    Replaced,
    /// The installing scope died while the drawer lived.
    HostGone,
}

/// Solved placement: the resting (open) rect and the off-screen origin
/// the panel slides from/to.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
struct Geometry {
    rect: Rect,
    closed: Point,
}

/// Resting rect for `edge`+`size` in `viewport` (cross axis fills).
fn solve_rect(viewport: Size, edge: DrawerEdge, size: DrawerSize) -> Rect {
    let axis = match edge {
        DrawerEdge::Left | DrawerEdge::Right => viewport.w,
        DrawerEdge::Top | DrawerEdge::Bottom => viewport.h,
    };
    let want = match size {
        DrawerSize::Cells(n) => n,
        DrawerSize::Percent(p) => ((axis as f32) * p.clamp(0.0, 1.0)).round() as i32,
    }
    .clamp(1, axis.max(1));
    match edge {
        DrawerEdge::Left => Rect::new(0, 0, want, viewport.h),
        DrawerEdge::Right => Rect::new(viewport.w - want, 0, want, viewport.h),
        DrawerEdge::Top => Rect::new(0, 0, viewport.w, want),
        DrawerEdge::Bottom => Rect::new(0, viewport.h - want, viewport.w, want),
    }
}

fn solve_geometry(viewport: Size, edge: DrawerEdge, size: DrawerSize) -> Geometry {
    let rect = solve_rect(viewport, edge, size);
    let closed = match edge {
        DrawerEdge::Left => Point::new(-rect.w, rect.y),
        DrawerEdge::Right => Point::new(viewport.w, rect.y),
        DrawerEdge::Top => Point::new(rect.x, -rect.h),
        DrawerEdge::Bottom => Point::new(rect.x, viewport.h),
    };
    Geometry { rect, closed }
}

/// Panel origin at slide progress `t` (0 = closed, 1 = resting).
fn origin_at(geo: Geometry, t: f32) -> Point {
    let lerp = |a: i32, b: i32| a as f32 + (b - a) as f32 * t;
    Point::new(
        lerp(geo.closed.x, geo.rect.x).round() as i32,
        lerp(geo.closed.y, geo.rect.y).round() as i32,
    )
}

// The open/close/finish machinery + the one-per-edge registry live in
// a private child module (file-size split, the anchored_owned pattern;
// a CHILD so `Inner`'s fields stay private to this file's world).
#[path = "drawer_open.rs"]
mod machinery;
use machinery::{begin_close, close_now, open_now, reclamp};

// ---------------------------------------------------------------------------
// Configuration + builder
// ---------------------------------------------------------------------------

pub(super) struct DrawerConfig {
    pub edge: DrawerEdge,
    pub size: DrawerSize,
    pub focus: DrawerFocus,
    pub scrim: bool,
    pub close_on_outside: bool,
    pub title: Option<String>,
    pub motion: Duration,
}

/// Builder for an installed drawer. Configure, then
/// [`install`](Drawer::install) once; the returned [`DrawerHandle`]
/// opens/closes it for the life of the installing scope.
pub struct Drawer {
    cfg: DrawerConfig,
    overlays: Option<Overlays>,
    bound: Option<Signal<bool>>,
    on_close: Option<Box<dyn FnMut(DrawerCloseReason)>>,
}

impl Drawer {
    /// A drawer on `edge`, sized 40% of the axis, Modal focus, scrim on,
    /// outside-press closes, ~160ms slide.
    pub fn new(edge: DrawerEdge) -> Drawer {
        Drawer {
            cfg: DrawerConfig {
                edge,
                size: DrawerSize::Percent(0.4),
                focus: DrawerFocus::Modal,
                scrim: true,
                close_on_outside: true,
                title: None,
                motion: Duration::from_millis(160),
            },
            overlays: None,
            bound: None,
            on_close: None,
        }
    }

    /// Extent along the slide axis (default 40% of the viewport axis).
    pub fn size(mut self, size: DrawerSize) -> Drawer {
        self.cfg.size = size;
        self
    }

    /// Input policy while open (default [`DrawerFocus::Modal`]).
    pub fn focus(mut self, focus: DrawerFocus) -> Drawer {
        self.cfg.focus = focus;
        self
    }

    /// Veil the content behind a MODAL drawer with the theme's `overlay`
    /// token (default true). Ignored in Passive mode — dimming content
    /// that stays interactive would lie.
    pub fn scrim(mut self, on: bool) -> Drawer {
        self.cfg.scrim = on;
        self
    }

    /// Close when a press lands outside a MODAL drawer's panel (default
    /// true). Passive drawers never own outside presses.
    pub fn close_on_outside(mut self, on: bool) -> Drawer {
        self.cfg.close_on_outside = on;
        self
    }

    /// Show a themed header row: the title, a muted Esc hint (modal
    /// only) and a close affordance. Without a title, no header —
    /// provide your own chrome inside the content.
    pub fn title(mut self, title: impl Into<String>) -> Drawer {
        self.cfg.title = Some(title.into());
        self
    }

    /// Slide duration (default ~160ms). `Duration::ZERO` is the instant
    /// mode — terminals cannot report a reduced-motion preference, so
    /// the knob is deliberately app-owned.
    pub fn motion(mut self, motion: Duration) -> Drawer {
        self.cfg.motion = motion;
        self
    }

    /// Explicit overlay store (bare rigs). Inside an `App`, the store
    /// arrives through reactive context automatically.
    pub fn overlays(mut self, overlays: &Overlays) -> Drawer {
        self.overlays = Some(overlays.clone());
        self
    }

    /// Controlled mode: the drawer follows `open` (an external `set`
    /// opens/closes it) and the handle verbs write it back — one source
    /// of truth. A signal already true at install opens immediately.
    pub fn bind(mut self, open: Signal<bool>) -> Drawer {
        self.bound = Some(open);
        self
    }

    /// Observer for every close, with the reason that ended that open.
    pub fn on_close(mut self, f: impl FnMut(DrawerCloseReason) + 'static) -> Drawer {
        self.on_close = Some(Box::new(f));
        self
    }

    /// Install under `cx` (closed). `build` runs per OPEN on a fresh
    /// mount scope — state that must survive close lives outside it
    /// (the Tabs rule; see the module docs). Returns the handle; the
    /// drawer lives until `cx` dies (then closes with `HostGone`).
    pub fn install(self, cx: Scope, build: impl Fn(Scope) -> View + 'static) -> DrawerHandle {
        let overlays = self
            .overlays
            .or_else(|| cx.use_context::<Overlays>())
            .unwrap_or_else(|| {
                debug_assert!(
                    false,
                    "abstracttui drawer: no overlay store — install inside an App \
                     (context) or pass Drawer::overlays(..) explicitly"
                );
                Overlays::new() // release: a detached store; the drawer is inert
            });
        let host = cx.child();
        let inner = Rc::new(RefCell::new(Inner {
            cfg: self.cfg,
            overlays,
            host,
            build: Rc::new(build),
            bound: self.bound,
            on_close: self.on_close,
            mount: None,
            desired_open: false,
            opening: false,
            pending_reason: None,
        }));
        // Host death closes with HostGone: the hook rides a scope-level
        // cleanup; per-open mount cleanups do the actual teardown (the
        // Popup pattern) — this one only names the reason first.
        let weak = Rc::downgrade(&inner);
        host.on_cleanup(move || {
            if let Some(inner) = weak.upgrade() {
                close_now(&inner, DrawerCloseReason::HostGone, false);
            }
        });
        // Controlled mode: follow the bound signal (both directions).
        // Copy the signal out FIRST — effects run synchronously at
        // creation, and a signal already true would open (and borrow
        // the inner) during that first run.
        let bound = inner.borrow().bound;
        if let Some(sig) = bound {
            let weak = Rc::downgrade(&inner);
            host.effect_labeled("drawer-bound", move || {
                let want = sig.get();
                let Some(inner) = weak.upgrade() else { return };
                let desired = inner.borrow().desired_open;
                if want && !desired {
                    open_now(&inner);
                } else if !want && desired {
                    begin_close(&inner, DrawerCloseReason::Api);
                }
            });
        }
        // Resize re-clamps (never dismisses): solve against the fresh
        // viewport, resize surfaces + panel tree, let the slide effect
        // re-place at the current progress.
        let weak = Rc::downgrade(&inner);
        let viewport = use_viewport(host);
        host.effect_labeled("drawer-reclamp", move || {
            let vp = viewport.get(); // tracked: re-runs per resize
            let Some(inner) = weak.upgrade() else { return };
            reclamp(&inner, vp);
        });
        DrawerHandle { inner }
    }
}

// ---------------------------------------------------------------------------
// Live state
// ---------------------------------------------------------------------------

/// One open's overlay material. `Option<Mount>` in `Inner` is the
/// exactly-once teardown latch (first taker wins, like Popup's layer).
struct Mount {
    scope: Scope,
    panel: LayerHandle,
    scrim: Option<LayerHandle>,
    progress: Signal<f32>,
    closing: Signal<bool>,
    geometry: Signal<Geometry>,
    /// The AT-OPEN scrim veil cell: resize repaints reuse it — tokens
    /// resolve at open, never mid-life (REVIEW wave 8 fix).
    veil: crate::render::Cell,
}

struct Inner {
    cfg: DrawerConfig,
    overlays: Overlays,
    host: Scope,
    build: Rc<dyn Fn(Scope) -> View>,
    bound: Option<Signal<bool>>,
    on_close: Option<Box<dyn FnMut(DrawerCloseReason)>>,
    mount: Option<Mount>,
    /// Heading open (true from an open verb until a close verb) —
    /// [`DrawerHandle::is_open`]'s answer during flights.
    desired_open: bool,
    /// Re-entrancy latch: `build` runs user code mid-open.
    opening: bool,
    pending_reason: Option<DrawerCloseReason>,
}

/// Cloneable handle to an installed drawer: open/close/toggle it for
/// the life of the installing scope. Dropping handles never closes
/// (lifetime is the scope's, the Modal rule).
#[derive(Clone)]
pub struct DrawerHandle {
    inner: Rc<RefCell<Inner>>,
}

impl DrawerHandle {
    /// Slide open (no-op while open/opening; reverses a close in
    /// flight). With a bound signal, writes it too.
    pub fn open(&self) {
        open_now(&self.inner);
    }

    /// Slide closed with [`DrawerCloseReason::Api`] (no-op while
    /// closed/closing). With a bound signal, writes it too.
    pub fn close(&self) {
        begin_close(&self.inner, DrawerCloseReason::Api);
    }

    /// [`open`](DrawerHandle::open) or [`close`](DrawerHandle::close)
    /// by current heading.
    pub fn toggle(&self) {
        if self.is_open() {
            self.close();
        } else {
            self.open();
        }
    }

    /// True while the drawer heads open (open or opening; false the
    /// moment a close begins).
    pub fn is_open(&self) -> bool {
        self.inner.borrow().desired_open
    }

    /// The drawer's edge.
    pub fn edge(&self) -> DrawerEdge {
        self.inner.borrow().cfg.edge
    }

    /// The panel's overlay layer while open (tests, advanced callers).
    pub fn layer(&self) -> Option<LayerHandle> {
        self.inner.borrow().mount.as_ref().map(|m| m.panel.clone())
    }

    /// Close with an internally-named reason (the panel chrome's Esc
    /// and ✕ paths).
    pub(super) fn close_with(&self, reason: DrawerCloseReason) {
        begin_close(&self.inner, reason);
    }
}

#[cfg(test)]
#[path = "drawer_tests.rs"]
mod tests;
