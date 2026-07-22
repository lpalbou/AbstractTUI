//! Anchored-panel substrate (backlog 0500, passive-panel slice) + the
//! completion controller consuming it (backlog 0120 §7).
//!
//! ## The substrate
//!
//! [`AnchoredPanel`] is the geometry half of 0500's anchored-popup
//! spec, shipped in its PASSIVE routing mode: a NON-modal overlay layer
//! placed against an anchor rect — below-preferred, flipped above when
//! the space below is short AND the space above is longer, clamped into
//! the viewport (the Toast clamp math), width matched to the anchor or
//! sized to content. It rides `app::overlays` with no engine privileges
//! beyond the one 0500-budgeted delta, [`Overlays::top_z`], so a panel
//! opened over any modal stack allocates above it.
//!
//! PASSIVE means keys stay with the ANCHOR OWNER: the panel's tree is
//! never focused (its content must not contain focusable elements) and
//! the engine's non-modal key rule then routes every key to the owner.
//! `on_outside_press` does not exist for non-modal layers (engine
//! fact), so dismissal is OWNER-DRIVEN: focus leaving the owner, Escape
//! in the owner, commit — plus the substrate's own anchor-unmount
//! safety: the opener's scope dying closes the panel (a popup must
//! never outlive the thing it points at; `Modal` deliberately differs —
//! its lifetime is the app's decision). Mouse presses INSIDE the panel
//! are its own (the overlay opacity rule); presses outside fall through
//! to the app, whose focus rules blur the owner and thereby dismiss.
//!
//! The OWNED and TOOLTIP routing modes of 0500 (select family, menus,
//! hover tips) are future consumers of the same `place_panel` geometry;
//! they are NOT built here.
//!
//! ## The completion controller
//!
//! [`Completion`] wires trigger-character providers ('/' commands, '@'
//! mentions, any prefix char) onto a
//! [`TextAreaState`](crate::widgets::TextAreaState): an effect
//! watches value/caret/focus/caret-cell, scans the token behind the
//! caret, asks the matching provider (synchronous, v1), and renders
//! candidates in a passive panel anchored at the caret cell. A
//! capture-phase wrapper intercepts Down/Up (highlight), Enter/Tab
//! (accept = replace token + close), Escape (dismiss; the same token
//! stays muted until the caret leaves it). Zero idle cost while closed:
//! no layers, no timers, one dormant effect.
//!
//! OWNER: REACT.

use std::cell::RefCell;
use std::rc::Rc;

use crate::base::{Point, Rect, Size};
use crate::reactive::Scope;
use crate::ui::View;

use super::overlays::Overlays;

/// Where a panel points: solved SCREEN cells, captured by the opener
/// (inside an event handler via `EventCtx::current_rect`, or from
/// `TextAreaState::caret_cell` — the only rect sources today; 0500
/// records the general rect-query gap).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct PanelAnchor {
    pub rect: Rect,
}

impl PanelAnchor {
    /// Anchor at a single cell (a caret).
    pub fn cell(p: Point) -> PanelAnchor {
        PanelAnchor {
            rect: Rect::new(p.x, p.y, 1, 1),
        }
    }
}

/// Panel width policy (0500 spec v1).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum PanelWidth {
    /// Popup width == anchor width (selects).
    MatchAnchor,
    /// Content-sized (widest row), clamped into `min..=max`.
    Content { min: i32, max: i32 },
}

/// The 0500 placement contract, pure and unit-testable: prefer BELOW
/// the anchor; FLIP above when the rows below are fewer than needed AND
/// the rows above outnumber them; height = min(content, chosen side);
/// x clamped into the viewport (the Toast clamp). The result can be
/// EMPTY (h == 0) when neither side has a row — callers skip opening.
pub fn place_panel(viewport: Size, anchor: Rect, content: Size, width: PanelWidth) -> Rect {
    let w = match width {
        PanelWidth::MatchAnchor => anchor.w,
        PanelWidth::Content { min, max } => content.w.clamp(min, max.max(min)),
    }
    .clamp(1, viewport.w.max(1));
    let below = (viewport.h - anchor.bottom()).max(0);
    let above = anchor.y.max(0);
    let (y, h) = if below >= content.h || below >= above {
        (anchor.bottom(), content.h.min(below))
    } else {
        let h = content.h.min(above);
        (anchor.y - h, h)
    };
    let x = anchor.x.min(viewport.w - w).max(0);
    Rect::new(x, y, w, h.max(0))
}

struct OpenLayer {
    layer: super::overlays::LayerHandle,
    scope: Scope,
    rect: Rect,
}

struct PanelInner {
    overlays: Overlays,
    owner: Scope,
    width: PanelWidth,
    build: Rc<dyn Fn(Scope) -> View>,
    open: Option<OpenLayer>,
    closed: bool,
}

impl PanelInner {
    fn drop_layer(&mut self) {
        if let Some(open) = self.open.take() {
            open.layer.remove();
            open.scope.dispose();
        }
    }
}

/// A passive anchored panel (0500 routing mode 2). Cloneable handle;
/// `close` is idempotent and also fires when the opener's scope dies.
#[derive(Clone)]
pub struct AnchoredPanel {
    inner: Rc<RefCell<PanelInner>>,
}

impl AnchoredPanel {
    /// Open a passive panel against `anchor`. `content` is the panel's
    /// natural size (rows to show, widest row); `build` produces the
    /// panel tree and re-runs only when geometry forces a remount —
    /// reactive content inside (a `dyn_view`) updates in place.
    ///
    /// CONTRACT: the built tree must contain NO focusable elements — a
    /// focused non-modal overlay would own the keyboard (engine rule),
    /// which is exactly what passive mode exists to avoid.
    ///
    /// Anchor-unmount safety: a cleanup on `cx` closes the panel when
    /// the opener's scope is disposed (dyn_view regeneration, unmount),
    /// so a regenerated opener can never leak an orphan panel.
    pub fn open_passive(
        overlays: &Overlays,
        cx: Scope,
        viewport: Size,
        anchor: PanelAnchor,
        width: PanelWidth,
        content: Size,
        build: impl Fn(Scope) -> View + 'static,
    ) -> AnchoredPanel {
        let panel = AnchoredPanel {
            inner: Rc::new(RefCell::new(PanelInner {
                overlays: overlays.clone(),
                owner: cx,
                width,
                build: Rc::new(build),
                open: None,
                closed: false,
            })),
        };
        let weak = Rc::downgrade(&panel.inner);
        cx.on_cleanup(move || {
            if let Some(inner) = weak.upgrade() {
                let mut inner = inner.borrow_mut();
                inner.closed = true;
                inner.drop_layer();
            }
        });
        panel.apply(viewport, anchor, content);
        panel
    }

    /// Re-place against a moved anchor / changed content size. A pure
    /// move keeps the mounted tree (offset change); a size change
    /// remounts from the stored build closure; no room on either side
    /// hides the panel until room returns. No-op after `close`.
    pub fn update(&self, viewport: Size, anchor: PanelAnchor, content: Size) {
        if self.inner.borrow().closed {
            return;
        }
        self.apply(viewport, anchor, content);
    }

    fn apply(&self, viewport: Size, anchor: PanelAnchor, content: Size) {
        let mut inner = self.inner.borrow_mut();
        let rect = place_panel(viewport, anchor.rect, content, inner.width);
        if rect.h <= 0 || rect.w <= 0 || viewport.w <= 0 || viewport.h <= 0 {
            inner.drop_layer();
            return;
        }
        match &mut inner.open {
            Some(open) if open.rect == rect => {}
            Some(open) if open.rect.size() == rect.size() => {
                open.layer.set_offset(rect.origin());
                open.rect = rect;
            }
            _ => {
                inner.drop_layer();
                let scope = inner.owner.child();
                let view = (inner.build)(scope);
                // Above EVERYTHING live right now — the 0500 stacking
                // rule (a static z cannot survive popup-over-modal).
                let z = inner.overlays.top_z() + 1;
                let layer = inner.overlays.layer_tree(z, rect, false, scope, view);
                inner.open = Some(OpenLayer { layer, scope, rect });
            }
        }
    }

    /// Remove the panel and dispose its content scope. Idempotent; the
    /// handle stays inert afterwards (`update` becomes a no-op).
    pub fn close(&self) {
        let mut inner = self.inner.borrow_mut();
        inner.closed = true;
        inner.drop_layer();
    }

    pub fn is_open(&self) -> bool {
        self.inner.borrow().open.is_some()
    }

    /// The solved panel rect while open.
    pub fn rect(&self) -> Option<Rect> {
        self.inner.borrow().open.as_ref().map(|o| o.rect)
    }

    /// The live overlay layer while open (tests, advanced callers).
    pub fn layer(&self) -> Option<super::overlays::LayerHandle> {
        self.inner.borrow().open.as_ref().map(|o| o.layer.clone())
    }
}

// The completion controller (backlog 0120 §7) lives in a private
// sibling for the file budget; its public types re-export here so the
// app-facing path stays `app::anchored::{Completion, ...}`.
#[path = "anchored_completion.rs"]
mod completion;
pub use completion::{Completion, CompletionCandidate};

#[cfg(test)]
#[path = "anchored_tests.rs"]
mod tests;
