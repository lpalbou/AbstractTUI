//! Drawer open/close machinery + the one-per-edge registry (private
//! `#[path]`-included CHILD of drawer.rs — the file-size split that
//! keeps `Inner`'s fields private to the drawer's own world).
//!
//! The borrow discipline (the Popup teardown rule, held throughout):
//! user code — `build`, `on_close`, signal writes that flush effects —
//! NEVER runs while the `Inner` RefCell is borrowed. Every verb takes
//! what it needs under a short borrow, releases, then acts.
//!
//! OWNER: DRAWER (0585).

use std::cell::RefCell;
use std::rc::{Rc, Weak};

use crate::anim::Easing;
use crate::base::{Rect, Size};
use crate::reactive::{animate, Signal};
use crate::render::Cell;

use super::super::overlays::{LayerHandle, Overlays};
use super::{
    origin_at, solve_geometry, DrawerCloseReason, DrawerEdge, DrawerFocus, DrawerHandle, Inner,
    Mount,
};

// ---------------------------------------------------------------------------
// One-per-edge registry (per UI thread, like the theme/viewport signals)
// ---------------------------------------------------------------------------

thread_local! {
    static EDGE_REGISTRY: RefCell<[Option<Weak<RefCell<Inner>>>; 4]> =
        const { RefCell::new([None, None, None, None]) };
}

/// Claim `edge` for `me`, instantly finishing any live incumbent with
/// `Replaced` (outside the registry borrow — the incumbent's teardown
/// runs user callbacks).
fn registry_claim(edge: DrawerEdge, me: &Rc<RefCell<Inner>>) {
    let incumbent = EDGE_REGISTRY.with(|slots| {
        let mut slots = slots.borrow_mut();
        let old = slots[edge.index()].take();
        slots[edge.index()] = Some(Rc::downgrade(me));
        old.and_then(|w| w.upgrade())
    });
    if let Some(other) = incumbent {
        if !Rc::ptr_eq(&other, me) {
            close_now(&other, DrawerCloseReason::Replaced, true);
        }
    }
}

/// Release `edge` if it still points at `me` (a replacement may have
/// already re-claimed the slot).
fn registry_release(edge: DrawerEdge, me: &Rc<RefCell<Inner>>) {
    EDGE_REGISTRY.with(|slots| {
        let mut slots = slots.borrow_mut();
        if let Some(w) = &slots[edge.index()] {
            if w.upgrade().is_some_and(|rc| Rc::ptr_eq(&rc, me)) {
                slots[edge.index()] = None;
            }
        }
    });
}

/// Does the registry still name `me` as `edge`'s owner?
fn registry_holds(edge: DrawerEdge, me: &Rc<RefCell<Inner>>) -> bool {
    EDGE_REGISTRY.with(|slots| {
        slots.borrow()[edge.index()]
            .as_ref()
            .and_then(|w| w.upgrade())
            .is_some_and(|rc| Rc::ptr_eq(&rc, me))
    })
}

/// A MODAL drawer owns the keyboard while open — but overlay key
/// dispatch walks topmost-z first and a FOCUSED non-modal tree ABOVE
/// the modal's fixed edge slot wins keys (the cycle-5 focused-overlay
/// rule): a passive drawer on a higher slot kept Esc and every key
/// away from an open modal drawer (REVIEW wave 8, TABS — verified
/// steal, tests/wave_shell_review.rs). Opening (or reopening) a modal
/// drawer therefore BLURS passive drawer trees; clicking back into an
/// unveiled passive panel re-steals deliberately — the engine's
/// click-where-your-keys-go story.
fn blur_passive_drawers(me: &Rc<RefCell<Inner>>) {
    let others: Vec<Rc<RefCell<Inner>>> = EDGE_REGISTRY.with(|slots| {
        slots
            .borrow()
            .iter()
            .flatten()
            .filter_map(|w| w.upgrade())
            .filter(|rc| !Rc::ptr_eq(rc, me))
            .collect()
    });
    for other in others {
        let tree = {
            let b = other.borrow();
            if b.cfg.focus != DrawerFocus::Passive {
                continue;
            }
            b.mount.as_ref().and_then(|m| m.panel.tree())
        };
        // set_focus runs FocusOut handlers (user code): no borrow held.
        if let Some(mut tree) = tree {
            tree.set_focus(None);
        }
    }
}

// ---------------------------------------------------------------------------
// Verbs
// ---------------------------------------------------------------------------

pub(super) fn open_now(inner: &Rc<RefCell<Inner>>) {
    enum Plan {
        Nothing,
        Reopen {
            closing: Signal<bool>,
            progress: Signal<f32>,
            bound: Option<Signal<bool>>,
        },
        Fresh,
    }
    let plan = {
        let mut b = inner.borrow_mut();
        if b.opening {
            Plan::Nothing
        } else if let Some(m) = &b.mount {
            let (closing, progress) = (m.closing, m.progress);
            if b.desired_open {
                Plan::Nothing
            } else {
                // Close in flight: reverse it, same mount continues.
                b.desired_open = true;
                b.pending_reason = None;
                Plan::Reopen {
                    closing,
                    progress,
                    bound: b.bound,
                }
            }
        } else {
            b.opening = true;
            Plan::Fresh
        }
    };
    match plan {
        Plan::Nothing => {}
        Plan::Reopen {
            closing,
            progress,
            bound,
        } => {
            if let Some(sig) = bound {
                sig.set_if_changed(true);
            }
            closing.set_if_changed(false);
            progress.set(1.0);
            // A passive drawer may have taken focus during the closing
            // flight; a modal heading open owns the keys again (REVIEW
            // wave 8 — see blur_passive_drawers).
            if inner.borrow().cfg.focus == DrawerFocus::Modal {
                blur_passive_drawers(inner);
            }
        }
        Plan::Fresh => open_fresh(inner),
    }
}

fn open_fresh(inner: &Rc<RefCell<Inner>>) {
    let (overlays, host, build, edge, size, modal, want_scrim, outside_closes, motion, bound) = {
        let b = inner.borrow();
        (
            b.overlays.clone(),
            b.host,
            b.build.clone(),
            b.cfg.edge,
            b.cfg.size,
            b.cfg.focus == DrawerFocus::Modal,
            b.cfg.scrim && b.cfg.focus == DrawerFocus::Modal,
            b.cfg.close_on_outside && b.cfg.focus == DrawerFocus::Modal,
            b.cfg.motion,
            b.bound,
        )
    };
    let viewport = super::super::viewport::current_viewport();
    if viewport.w <= 0 || viewport.h <= 0 {
        // Nothing to place against (bare rig without a published
        // viewport): stay closed rather than mint a zero-size world.
        inner.borrow_mut().opening = false;
        return;
    }
    // One drawer per edge: the incumbent finishes NOW (animated
    // handoffs would stack two panels on one z slot).
    registry_claim(edge, inner);
    let geo = solve_geometry(viewport, edge, size);
    let scope = host.child();
    let content = (build)(scope); // user code — no inner borrow held
                                  // REVIEW FIX (wave 8, TABS — verified defect): the claim can be
                                  // STOLEN while user code runs mid-open — the incumbent's on_close
                                  // (fired inside registry_claim above) or this very build closure
                                  // may open another drawer on the same edge. Two mounts would then
                                  // share one z slot (exactly the equal-z trap the fixed slots
                                  // exist to avoid). The LAST claim owns the slot: an open whose
                                  // claim is gone aborts before creating layers. No close reason
                                  // fires — this open never completed, and a callback that reopens
                                  // on Replaced must terminate, never recurse.
    if !registry_holds(edge, inner) {
        scope.dispose();
        inner.borrow_mut().opening = false;
        return;
    }
    let handle = DrawerHandle {
        inner: inner.clone(),
    };
    let view = {
        let b = inner.borrow();
        super::super::drawer_view::panel_view(&b.cfg, handle, content)
    };
    // The veil resolves its token AT OPEN (the documented Modal rule)
    // and is CAPTURED so the resize re-clamp repaints the same veil —
    // re-reading the current theme there minted a mixed-theme drawer
    // (REVIEW wave 8, TABS — verified defect, pinned in drawer_tests).
    let veil = veil_cell();
    let scrim = want_scrim.then(|| make_scrim(&overlays, edge.scrim_z(), viewport, veil));
    let panel = overlays.layer_tree(
        edge.panel_z(),
        Rect::new(geo.closed.x, geo.closed.y, geo.rect.w, geo.rect.h),
        modal,
        scope,
        view,
    );
    if outside_closes {
        let weak = Rc::downgrade(inner);
        overlays.on_outside_press(&panel, move || {
            if let Some(inner) = weak.upgrade() {
                begin_close(&inner, DrawerCloseReason::OutsidePress);
            }
        });
    }
    let progress = scope.signal(0.0f32);
    let closing = scope.signal(false);
    let geometry = scope.signal(geo);
    let eased = animate(scope, progress, Easing::EaseOut, motion);
    // The slide: offset follows eased progress; a close landing at 0
    // finishes the mount (removes layers, disposes the scope, fires the
    // reason). Layer ops from effects are legal; only draw closures are
    // restricted (overlay borrow discipline).
    let weak = Rc::downgrade(inner);
    let panel_for_slide = panel.clone();
    scope.effect_labeled("drawer-slide", move || {
        let t = eased.get().clamp(0.0, 1.0);
        let geo = geometry.get();
        if !panel_for_slide.is_alive() {
            return;
        }
        panel_for_slide.set_offset(origin_at(geo, t));
        if closing.get() && t <= f32::EPSILON {
            if let Some(inner) = weak.upgrade() {
                finish(&inner, false);
            }
        }
    });
    // Mount-scope cleanup: host death (cascade) or any early disposal
    // tears the overlay material down exactly once (the Mount latch).
    let weak = Rc::downgrade(inner);
    scope.on_cleanup(move || {
        if let Some(inner) = weak.upgrade() {
            finish(&inner, true);
        }
    });
    {
        let mut b = inner.borrow_mut();
        b.mount = Some(Mount {
            scope,
            panel,
            scrim,
            progress,
            closing,
            geometry,
            veil,
        });
        b.desired_open = true;
        b.opening = false;
    }
    if modal {
        // Keys belong to the modal drawer from this instant (REVIEW
        // wave 8 — see blur_passive_drawers).
        blur_passive_drawers(inner);
    }
    if let Some(sig) = bound {
        sig.set_if_changed(true);
    }
    progress.set(1.0); // the flight starts here
}

/// Begin an animated close with `reason` (no-op unless heading open).
pub(super) fn begin_close(inner: &Rc<RefCell<Inner>>, reason: DrawerCloseReason) {
    let plan = {
        let mut b = inner.borrow_mut();
        if !b.desired_open || b.mount.is_none() {
            None
        } else {
            b.desired_open = false;
            b.pending_reason = Some(reason);
            let m = b.mount.as_ref().expect("checked above");
            Some((m.progress, m.closing, b.bound))
        }
    };
    let Some((progress, closing, bound)) = plan else {
        return;
    };
    if let Some(sig) = bound {
        sig.set_if_changed(false);
    }
    // Order is load-bearing: progress first (a drawer closed before any
    // frame ran has eased == 0 already — flipping `closing` then lands
    // the close synchronously through the slide effect, which disposes
    // `progress`; writing it afterwards would touch a dead signal).
    progress.set(0.0);
    closing.set(true);
}

/// Close NOW, skipping the slide (replacement, host death).
pub(super) fn close_now(
    inner: &Rc<RefCell<Inner>>,
    reason: DrawerCloseReason,
    dispose_scope: bool,
) {
    {
        let mut b = inner.borrow_mut();
        if b.mount.is_none() {
            return;
        }
        b.pending_reason.get_or_insert(reason);
    }
    finish(inner, !dispose_scope);
}

/// The one teardown seam: take the mount (exactly-once latch), remove
/// layers, dispose the mount scope (unless already mid-disposal), fire
/// `on_close` with the first reason. Callable from the slide effect,
/// verbs, and scope cleanups — later calls no-op.
fn finish(inner: &Rc<RefCell<Inner>>, from_cleanup: bool) {
    let (mount, reason, bound, overlays) = {
        let mut b = inner.borrow_mut();
        let Some(mount) = b.mount.take() else {
            return;
        };
        b.desired_open = false;
        b.opening = false;
        let reason = b.pending_reason.take().unwrap_or({
            if from_cleanup {
                DrawerCloseReason::HostGone
            } else {
                DrawerCloseReason::Api
            }
        });
        (mount, reason, b.bound, b.overlays.clone())
    };
    // The panel's RESTING rect — the visible cells it occupied — before
    // teardown. A close slides the panel OFF-SCREEN first (progress→0),
    // so `remove()`'s current-bounds damage clips to empty and the
    // visible region would never repaint (F1, cycle-3 acceptance:
    // instant/passive closes left stale pixels; the modal scrim's
    // full-viewport removal hid it for modal drawers only). Name the
    // resting region explicitly so it recomposites from below.
    let resting = mount.geometry.try_get_untracked().map(|g| g.rect);
    mount.panel.remove();
    if let Some(scrim) = mount.scrim {
        scrim.remove();
    }
    if let Some(rect) = resting {
        overlays.damage_root_under_rect(rect);
    }
    if !from_cleanup {
        // Disposal cancels the slide flight through animate's
        // disposal guard; the mount's own cleanup re-enters finish
        // and finds the latch empty.
        mount.scope.dispose();
    }
    {
        let edge = inner.borrow().cfg.edge;
        registry_release(edge, inner);
    }
    if let Some(sig) = bound {
        if sig.is_alive() {
            sig.set_if_changed(false);
        }
    }
    // Take-call-putback: the observer may reopen from inside.
    let taken = inner.borrow_mut().on_close.take();
    if let Some(mut f) = taken {
        f(reason);
        let mut b = inner.borrow_mut();
        if b.on_close.is_none() {
            b.on_close = Some(f);
        }
    }
}

/// Re-solve geometry against a fresh viewport (resize re-clamp).
pub(super) fn reclamp(inner: &Rc<RefCell<Inner>>, viewport: Size) {
    if viewport.w <= 0 || viewport.h <= 0 {
        return; // transient zero-size: keep the last real geometry
    }
    let work = {
        let b = inner.borrow();
        b.mount.as_ref().map(|m| {
            (
                m.panel.clone(),
                m.scrim.clone(),
                m.geometry,
                m.veil,
                b.cfg.edge,
                b.cfg.size,
                b.overlays.clone(),
            )
        })
    };
    let Some((panel, scrim, geometry, veil, edge, size, overlays)) = work else {
        return;
    };
    let old_rect = geometry.try_get_untracked().map(|g| g.rect);
    let geo = solve_geometry(viewport, edge, size);
    if geometry
        .try_get_untracked()
        .is_none_or(|current| current == geo)
    {
        return;
    }
    panel.with_surface(|s| s.resize(geo.rect.size(), Cell::EMPTY));
    if let Some(mut tree) = panel.tree() {
        tree.set_viewport(geo.rect.size());
    }
    panel.damage();
    // A SHRINK vacates part of the old footprint (a right/bottom drawer
    // that got narrower, an edge that moved in): the panel's own
    // move/resize damage covers only the new smaller bounds, so name
    // the OLD rect for the compositor (same F1 class as close). Cheap
    // no-op when the panel grew or stayed put.
    if let Some(old) = old_rect {
        overlays.damage_root_under_rect(old);
    }
    if let Some(scrim) = scrim {
        scrim.with_surface(|s| {
            s.resize(viewport, Cell::EMPTY);
            // The AT-OPEN veil, never the current theme's (tokens
            // resolve at open — REVIEW wave 8 fix).
            s.fill_rect(Rect::from_size(viewport), veil);
        });
        scrim.damage();
    }
    geometry.set(geo); // the slide effect re-places at the current t
}

/// The scrim: one MANUAL layer of glyph-less veil cells — the
/// compositor blends the `overlay` token (which carries alpha by
/// contract) over whatever sits below, veiling kept glyph ink too.
/// Painted at open and resize only; never per frame — always with the
/// AT-OPEN veil cell (`Mount::veil`).
fn make_scrim(overlays: &Overlays, z: i32, viewport: Size, veil: Cell) -> LayerHandle {
    let layer = overlays.layer(z, Rect::from_size(viewport));
    layer.with_surface(|s| s.fill_rect(Rect::from_size(viewport), veil));
    layer
}

/// The veil cell for a fresh open: tokens resolve AT OPEN (the
/// documented Modal rule); the result is captured in `Mount::veil` so
/// later repaints (resize re-clamp) never re-read the theme.
fn veil_cell() -> Cell {
    let mut cell = Cell::EMPTY;
    cell.bg = super::super::theme::current_theme().tokens.overlay;
    cell
}
