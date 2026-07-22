//! OWNED + TOOLTIP routing modes of the anchored-popup substrate
//! (backlog 0500, completing the spec's three-mode contract; the
//! PASSIVE mode shipped first in anchored.rs). Private sibling of
//! anchored.rs (file-size split, the anchored_completion.rs pattern);
//! public types re-export through `app::anchored`.
//!
//! ## OWNED mode ([`Popup`])
//!
//! A MODAL overlay tree above EVERYTHING live (`Overlays::top_z() + 1`
//! — the one 0500 engine delta), so a popup opened over any modal
//! stack layers correctly where a static z constant cannot. Modal
//! means the engine routes every input here while open: keys go to
//! the popup (faces put their navigation on the content; the
//! substrate owns Escape), and a mouse press OUTSIDE the popup's
//! bounds dismisses WITHOUT acting below (deliberate overlay
//! semantics — `on_outside_press` fires only for modal trees).
//!
//! Dismissal is a single idempotent path, [`Popup::dismiss`], and
//! every ending has a name ([`DismissReason`]): `Commit` (a face took
//! the value — [`Popup::close`] is this spelling), `Escape`,
//! `OutsidePress`, `AnchorGone` (the opener's scope died — the
//! anchor-unmount safety contract shared with the passive mode; a
//! popup must never outlive the thing it points at), and `Resize`
//! (the terminal viewport changed while open — see below).
//! `on_dismiss` fires EXACTLY ONCE with the first reason that ended
//! the popup.
//!
//! Stacking note (cycle-3 F2 amendment): "above EVERYTHING live"
//! includes a live `Toast` — a popup opened while a toast shows
//! allocates above `TOAST_Z` and may transiently cover it. That is
//! deliberate: toasts are passive, non-interactive draw layers, so no
//! input conflict exists, and the popup is the surface the user is
//! actively operating (the amended cycle-3 addendum in
//! reviews/study/platform-on-appkits.md records this).
//!
//! Geometry is the same `place_panel` contract (below-preferred, flip
//! above when below is short AND above is longer, viewport clamp);
//! `open_including_anchor_row` extends the popup's bounds to START at
//! the anchor row (the Combobox mounts its editor there — zero visual
//! jump; when flipped, the anchor row is the popup's LAST row and
//! [`Popup::flipped`] tells the face to order its rows accordingly).
//! v1 popups place ONCE at open — the modal owns all input while
//! open, so the anchor cannot move under it (the Modal precedent) —
//! with one exception the modal cannot prevent: a terminal RESIZE
//! (cycle-3 review F9). A resize invalidates both the solved rect
//! (after a shrink the popup can sit off-viewport while still
//! modal-owning every key — an invisible modal) and the captured
//! anchor rect (re-placing would aim at a guess), so the popup
//! dismisses with [`DismissReason::Resize`]; the trigger re-opens it
//! against fresh geometry.
//!
//! ## TOOLTIP mode ([`Tooltip`])
//!
//! Passive AND non-interactive: a `layer_draw` label (no tree, no
//! focus, no handlers) shown after a hover delay (`after` one-shot —
//! zero wakeups until due), hidden on `MouseLeave` or anchor loss.
//! Consumer: extensions 0430 hover tips; the 0500 select faces do not
//! use it.
//!
//! OWNER: SELECT (0500).

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use crate::base::{Rect, Size};
use crate::layout::{Dimension, Style as LayoutStyle};
use crate::reactive::{after, Scope};
use crate::render::Style;
use crate::ui::{Element, Key, Mods, Phase, UiEvent, View};

use super::super::overlays::{LayerHandle, Overlays};
use super::super::theme::current_theme;
use super::super::viewport::current_viewport;
use super::{place_panel, PanelAnchor, PanelWidth};

/// Why an owned popup closed. Delivered to [`Popup::on_dismiss`]
/// exactly once, with the FIRST reason that ended the popup.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum DismissReason {
    /// A face took the value and closed ([`Popup::close`]).
    Commit,
    /// Escape inside the popup (substrate-owned key).
    Escape,
    /// A mouse press outside the popup's bounds (the press never acts
    /// on what is below — deliberate overlay semantics).
    OutsidePress,
    /// The opener's scope died while the popup lived (dyn_view
    /// regeneration, unmount) — the anchor-unmount safety contract.
    AnchorGone,
    /// The terminal viewport changed while the popup was open: both
    /// the solved placement and the captured anchor rect are stale
    /// (the popup could sit off-viewport while still modal-owning all
    /// input), so the popup closes instead of guessing a new place.
    Resize,
}

/// OWNED-mode placement: `place_panel`'s below-prefer/flip/clamp
/// contract, plus the anchor-row inclusion the Combobox needs. `list`
/// is the option-rows extent (EXCLUDING the anchor row); with
/// `include_anchor_row` the returned rect starts at the anchor row
/// (below mode) or ends with it (flipped). Returns `None` when no row
/// fits on either side — callers skip opening.
pub(crate) fn place_owned(
    viewport: Size,
    anchor: Rect,
    list: Size,
    width: PanelWidth,
    include_anchor_row: bool,
) -> Option<(Rect, bool)> {
    if viewport.w <= 0 || viewport.h <= 0 {
        return None;
    }
    if !include_anchor_row {
        let rect = place_panel(viewport, anchor, list, width);
        if rect.h <= 0 || rect.w <= 0 {
            return None;
        }
        return Some((rect, rect.bottom() <= anchor.y));
    }
    let w = match width {
        PanelWidth::MatchAnchor => anchor.w,
        PanelWidth::Content { min, max } => list.w.clamp(min, max.max(min)),
    }
    .clamp(1, viewport.w.max(1));
    let below = (viewport.h - anchor.bottom()).max(0);
    let above = anchor.y.max(0);
    // The same flip rule as place_panel: prefer below unless it is
    // short AND above offers more.
    let (flipped, rows) = if below >= list.h || below >= above {
        (false, list.h.min(below))
    } else {
        (true, list.h.min(above))
    };
    if rows <= 0 {
        return None;
    }
    let x = anchor.x.min(viewport.w - w).max(0);
    let (y, h) = if flipped {
        (anchor.y - rows, rows + anchor.h)
    } else {
        (anchor.y, anchor.h + rows)
    };
    Some((Rect::new(x, y, w, h), flipped))
}

struct PopupInner {
    layer: Option<LayerHandle>,
    scope: Option<Scope>,
    on_dismiss: Option<Box<dyn FnMut(DismissReason)>>,
    rect: Rect,
    flipped: bool,
}

/// An OWNED anchored popup (0500 routing mode 1): a modal overlay tree
/// above the whole live stack. Cloneable handle; [`Popup::dismiss`] is
/// idempotent and also fires when the opener's scope dies.
#[derive(Clone)]
pub struct Popup {
    inner: Rc<RefCell<PopupInner>>,
}

impl Popup {
    /// Open a popup against `anchor` (solved screen cells, captured in
    /// the opener's event handler via `EventCtx::current_rect`). `list`
    /// is the content extent (rows to show, widest row); `build`
    /// produces the popup tree on a child scope of `cx` — state
    /// created there dies with the popup — and receives `flipped`
    /// (true = the popup opened ABOVE the anchor), so faces can order
    /// their rows against gravity. Returns `None` when no row fits on
    /// either side of the anchor.
    pub fn open(
        overlays: &Overlays,
        cx: Scope,
        viewport: Size,
        anchor: PanelAnchor,
        width: PanelWidth,
        list: Size,
        build: impl FnOnce(Scope, bool) -> View,
    ) -> Option<Popup> {
        Popup::open_impl(overlays, cx, viewport, anchor, width, list, false, build)
    }

    /// [`Popup::open`] with the popup's bounds EXTENDED to include the
    /// anchor row: the first row (or last, when [`Popup::flipped`])
    /// sits exactly over the trigger, so a face can mount an editor
    /// there with zero visual jump (the Combobox contract).
    pub fn open_including_anchor_row(
        overlays: &Overlays,
        cx: Scope,
        viewport: Size,
        anchor: PanelAnchor,
        width: PanelWidth,
        list: Size,
        build: impl FnOnce(Scope, bool) -> View,
    ) -> Option<Popup> {
        Popup::open_impl(overlays, cx, viewport, anchor, width, list, true, build)
    }

    #[allow(clippy::too_many_arguments)] // one private seam; the two
                                         // public faces above carry the honest signatures.
    fn open_impl(
        overlays: &Overlays,
        cx: Scope,
        viewport: Size,
        anchor: PanelAnchor,
        width: PanelWidth,
        list: Size,
        include_anchor_row: bool,
        build: impl FnOnce(Scope, bool) -> View,
    ) -> Option<Popup> {
        let (rect, flipped) = place_owned(viewport, anchor.rect, list, width, include_anchor_row)?;
        let scope = cx.child();
        let content = build(scope, flipped);
        let popup = Popup {
            inner: Rc::new(RefCell::new(PopupInner {
                layer: None,
                scope: Some(scope),
                on_dismiss: None,
                rect,
                flipped,
            })),
        };
        // The substrate owns Escape: a bubble handler on the wrapper
        // root fires for any key the face content left unconsumed
        // (faces stop_propagation on the keys they handle).
        let wrapper = {
            let p = popup.clone();
            Element::new()
                .style(
                    LayoutStyle::default()
                        .width(Dimension::Percent(1.0))
                        .height(Dimension::Percent(1.0)),
                )
                .on(Phase::Bubble, move |ctx, ev| {
                    if let UiEvent::Key(k) = ev {
                        if k.key == Key::Escape && k.mods == Mods::NONE {
                            p.dismiss(DismissReason::Escape);
                            ctx.stop_propagation();
                        }
                    }
                })
                .child(content)
                .build()
        };
        // Above EVERYTHING live right now — the 0500 stacking rule.
        let z = overlays.top_z() + 1;
        let layer = overlays.layer_tree(z, rect, true, scope, wrapper);
        {
            let p = popup.clone();
            overlays.on_outside_press(&layer, move || p.dismiss(DismissReason::OutsidePress));
        }
        popup.inner.borrow_mut().layer = Some(layer);
        // Anchor-unmount safety: the opener's scope dying closes the
        // popup (same contract as the passive mode; `Modal`
        // deliberately differs — its lifetime is the app's decision).
        // The hook rides the CONTENT scope — a child of the opener, so
        // the opener's disposal cascades into it — which keeps the
        // opener free of accumulating per-open cleanups AND avoids
        // re-disposing a scope from inside its own cleanup: the hook
        // path skips `dispose` (the scope is already dying).
        let weak = Rc::downgrade(&popup.inner);
        scope.on_cleanup(move || {
            if let Some(inner) = weak.upgrade() {
                Popup { inner }.end(DismissReason::AnchorGone, false);
            }
        });
        // Resize-dismiss (cycle-3 F9): a viewport change invalidates
        // both the solved rect and the captured anchor, so the popup
        // ends with `Resize` instead of floating at stale coordinates
        // (possibly off-viewport) while modal-owning every key. The
        // effect rides the CONTENT scope, so it dies with the popup;
        // the baseline is the viewport signal's value AT OPEN, so
        // worlds that never publish a viewport (bare-store unit rigs,
        // `Size::ZERO`) can never observe a change. Dismissing from
        // inside the effect disposes the effect's own scope mid-run —
        // the runtime tolerates that (the closure is Rc-cloned out for
        // the call; post-run bookkeeping shrugs at a freed node), and
        // the AnchorGone cleanup above finds the layer already taken,
        // so exactly-once holds with `Resize` as the first reason.
        let weak = Rc::downgrade(&popup.inner);
        let viewport_now = super::super::viewport::use_viewport(scope);
        let at_open = viewport_now.get_untracked();
        scope.effect_labeled("popup-resize-dismiss", move || {
            if viewport_now.get() != at_open {
                if let Some(inner) = weak.upgrade() {
                    Popup { inner }.dismiss(DismissReason::Resize);
                }
            }
        });
        Some(popup)
    }

    /// Register the dismiss observer: fires EXACTLY ONCE with the
    /// reason that ended the popup. Register right after `open` — a
    /// callback installed after dismissal never fires.
    pub fn on_dismiss(&self, f: impl FnMut(DismissReason) + 'static) {
        let mut inner = self.inner.borrow_mut();
        if inner.layer.is_some() {
            inner.on_dismiss = Some(Box::new(f));
        }
    }

    /// End the popup with `reason`: remove the layer, dispose the
    /// content scope, fire `on_dismiss` once. Idempotent — later calls
    /// (and the anchor-death cleanup) are no-ops. Safe to call from
    /// handlers INSIDE the popup tree (the Modal Esc-close precedent).
    pub fn dismiss(&self, reason: DismissReason) {
        self.end(reason, true);
    }

    /// The one teardown seam. `dispose_scope: false` is the
    /// scope-cleanup path — the content scope is already mid-disposal
    /// and must not be re-disposed from inside its own cleanup.
    fn end(&self, reason: DismissReason, dispose_scope: bool) {
        let (layer, scope, mut callback) = {
            let mut inner = self.inner.borrow_mut();
            let Some(layer) = inner.layer.take() else {
                return; // already dismissed
            };
            (layer, inner.scope.take(), inner.on_dismiss.take())
        };
        layer.remove();
        if dispose_scope {
            if let Some(scope) = scope {
                scope.dispose();
            }
        }
        if let Some(f) = callback.as_mut() {
            f(reason);
        }
    }

    /// Commit-flavored close (the Modal::close shape): the face took
    /// the value and is done — `dismiss(DismissReason::Commit)`.
    pub fn close(&self) {
        self.dismiss(DismissReason::Commit);
    }

    pub fn is_open(&self) -> bool {
        self.inner.borrow().layer.is_some()
    }

    /// The solved popup rect (screen cells) chosen at open.
    pub fn rect(&self) -> Rect {
        self.inner.borrow().rect
    }

    /// True when the popup opened ABOVE the anchor (cramped below):
    /// with `open_including_anchor_row`, the anchor row is then the
    /// popup's LAST row and faces order their content accordingly.
    pub fn flipped(&self) -> bool {
        self.inner.borrow().flipped
    }

    /// The live overlay layer while open (tests, advanced callers).
    pub fn layer(&self) -> Option<LayerHandle> {
        self.inner.borrow().layer.clone()
    }
}

struct TipState {
    /// Bumped by every enter/leave: a due one-shot from a stale
    /// generation must not open (leave-before-due).
    generation: u64,
    layer: Option<LayerHandle>,
    anchor: Rect,
}

impl TipState {
    fn close(&mut self) {
        self.generation += 1;
        if let Some(layer) = self.layer.take() {
            layer.remove();
        }
    }
}

/// TOOLTIP mode (0500 routing mode 3): a hover-timed, non-interactive
/// label — a `layer_draw` above the live stack, no tree, no focus, no
/// handlers. Zero cost while dormant (one one-shot timer arms per
/// hover; nothing wakes until due).
pub struct Tooltip;

impl Tooltip {
    /// Wrap `view` so hovering it for `delay` shows `text` in a panel
    /// placed against the hovered element's solved rect (the
    /// `place_panel` contract). Hides on `MouseLeave`; closes with
    /// `cx` (anchor loss). Tokens resolve from the ACTIVE theme at
    /// show time.
    pub fn attach(
        cx: Scope,
        overlays: &Overlays,
        text: impl Into<String>,
        delay: Duration,
        view: View,
    ) -> View {
        let text = text.into();
        let overlays = overlays.clone();
        let state = Rc::new(RefCell::new(TipState {
            generation: 0,
            layer: None,
            anchor: Rect::ZERO,
        }));
        {
            // Anchor loss: the wrapped subtree unmounting hides the tip.
            let state = state.clone();
            cx.on_cleanup(move || state.borrow_mut().close());
        }
        let handler = {
            let state = state.clone();
            move |ctx: &mut crate::ui::EventCtx, ev: &UiEvent| match ev {
                UiEvent::MouseEnter => {
                    let generation = {
                        let mut s = state.borrow_mut();
                        s.generation += 1;
                        s.anchor = ctx.current_rect();
                        s.generation
                    };
                    let state = state.clone();
                    let overlays = overlays.clone();
                    let text = text.clone();
                    after(delay, move || {
                        let anchor = {
                            let s = state.borrow();
                            if s.generation != generation || s.layer.is_some() {
                                return; // left (or re-shown) meanwhile
                            }
                            s.anchor
                        };
                        let layer = show_tip(&overlays, anchor, &text);
                        state.borrow_mut().layer = layer;
                    });
                }
                UiEvent::MouseLeave => state.borrow_mut().close(),
                _ => {}
            }
        };
        // Content-tight wrapper: `align_self(Start)` opts out of the
        // parent's cross-axis stretch, so the hover box (= the tip's
        // anchor rect) is the wrapped view's own extent, not a
        // stretched row.
        Element::new()
            .style(LayoutStyle::default().align_self(crate::layout::Align::Start))
            .on(Phase::Bubble, handler)
            .child(view)
            .build()
    }
}

/// Paint one tip label on a draw layer at `top_z() + 1`. Returns None
/// when no row fits (the honest place_panel outcome).
fn show_tip(overlays: &Overlays, anchor: Rect, text: &str) -> Option<LayerHandle> {
    let viewport = current_viewport();
    let content = Size::new(crate::text::width(text) + 2, 1);
    let rect = place_panel(
        viewport,
        anchor,
        content,
        PanelWidth::Content {
            min: content.w.min(3),
            max: content.w,
        },
    );
    if rect.h <= 0 || rect.w <= 0 {
        return None;
    }
    let tokens = &current_theme().tokens;
    let ink = tokens.text;
    let ground = tokens.surface_raised;
    let text = text.to_string();
    let layer = overlays.layer_draw(overlays.top_z() + 1, rect, move |canvas, rect| {
        let style = Style::new().fg(ink).bg(ground);
        canvas.fill_styled(rect, ' ', &style);
        canvas.print_styled(crate::base::Point::new(rect.x + 1, rect.y), &text, &style);
    });
    Some(layer)
}

#[cfg(test)]
#[path = "anchored_owned_tests.rs"]
mod tests;
