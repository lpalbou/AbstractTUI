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
//! focus, no handlers) or a passive card tree, shown after a delay
//! (`after` one-shot — zero wakeups until due) and hidden on
//! `MouseLeave`, `FocusOut`, `Escape`, the anchor moving, or anchor
//! loss. Triggered by hover OR by the anchor taking focus, so the tip
//! is not invisible to a keyboard user or to a terminal with no mouse
//! reporting. Consumer: extensions 0430 hover tips; the 0500 select
//! faces do not use it.
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
    open: Option<OpenTip>,
    anchor: Rect,
    /// A deferred anchor-moved close is already armed — the anchor may
    /// draw several times before the one-shot comes due, and each of
    /// those draws still sees the stale rect.
    stale_armed: bool,
}

struct OpenTip {
    layer: LayerHandle,
    /// Rich cards own a child reactive scope; plain labels do not
    /// mount a tree and therefore have nothing to dispose.
    scope: Option<Scope>,
    /// The "N more" row drawn over a card the viewport could not fit.
    /// Absent when the card fitted whole — see [`show_tip`].
    truncation: Option<LayerHandle>,
}

impl TipState {
    fn close(&mut self) {
        self.generation += 1;
        self.stale_armed = false;
        if let Some(open) = self.open.take() {
            open.layer.remove();
            if let Some(marker) = open.truncation {
                marker.remove();
            }
            if let Some(scope) = open.scope {
                scope.dispose();
            }
        }
    }
}

/// What a hover tip shows.
///
/// The two variants are two different ROUTING costs, not two styles.
/// A [`Label`](TipContent::Label) is a `layer_draw` with no tree, no
/// focus and no handlers — the original tooltip, unchanged. A
/// [`Card`](TipContent::Card) mounts a PASSIVE tree layer so the tip
/// can be an arbitrary widget subtree (a cited message's header,
/// title and body). Passive means keys stay with the anchor owner and
/// the card's content must not contain focusable elements — the
/// non-modal contract in [`super::AnchoredPanel`].
///
/// The card states its own `size` because `place_panel` takes the
/// content extent as an INPUT: geometry is solved before the tree
/// exists, so the caller says how much room to ask for and the
/// solver clamps it to what the viewport can lend.
#[derive(Clone)]
pub enum TipContent {
    /// One line of plain text. Zero-tree path.
    Label(String),
    /// A widget subtree, built lazily at show time so a dormant tip
    /// costs nothing.
    Card {
        /// Desired extent in cells. `place_panel` may lend less.
        size: Size,
        /// Built once per show, in the layer's own scope.
        build: Rc<dyn Fn(Scope) -> View>,
    },
}

impl TipContent {
    /// A card from a view builder, sized in cells.
    pub fn card(size: Size, build: impl Fn(Scope) -> View + 'static) -> TipContent {
        TipContent::Card {
            size,
            build: Rc::new(build),
        }
    }
}

impl std::fmt::Debug for TipContent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TipContent::Label(t) => f.debug_tuple("Label").field(t).finish(),
            TipContent::Card { size, .. } => f.debug_struct("Card").field("size", size).finish(),
        }
    }
}

/// TOOLTIP mode (0500 routing mode 3): a delay-timed, non-interactive
/// tip — above the live stack, and the LAYER is never focused. Zero
/// cost while dormant (one one-shot timer arms per trigger; nothing
/// wakes until due).
///
/// Two triggers, one arming path: the pointer entering the anchor, and
/// the ANCHOR taking focus — see [`Tooltip::attach_content`] for how
/// far the keyboard half reaches and why it cannot reach further.
///
/// A [`TipContent::Label`] rides a `layer_draw` with no tree at all; a
/// [`TipContent::Card`] rides a passive tree layer. Both share ONE
/// trigger implementation — the generation counter that defeats
/// leave-before-due, the anchor-unmount cleanup, and the `place_panel`
/// geometry — because two copies of that logic is two places for the
/// stale-timer bug to come back.
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
        Tooltip::attach_content(cx, overlays, TipContent::Label(text.into()), delay, view)
    }

    /// [`Tooltip::attach`] with rich content: the same triggers,
    /// showing whatever [`TipContent`] describes.
    ///
    /// Every contract of `attach` holds unchanged — hover delay,
    /// hide on `MouseLeave`, close on anchor unmount, screen-space
    /// anchor capture, nothing woken until the timer is due.
    ///
    /// # Keyboard reach, and exactly how far it goes
    ///
    /// A hover-only affordance is invisible to a keyboard user, and to
    /// a terminal with no mouse reporting at all. So the tip also opens
    /// on `FocusIn` and closes on `FocusOut`, and `Escape` dismisses an
    /// open one.
    ///
    /// **The trigger sits on the ROOT of the `view` you pass, and that
    /// root must be the thing that takes focus.** Focus transitions are
    /// delivered TARGET-ONLY (`ui::focus`) — unlike hover, which is
    /// delivered per-node along the hovered path — so a listener on the
    /// wrapper this function builds could hear the mouse and could
    /// never hear focus. Two consequences worth knowing before you rely
    /// on it:
    ///
    /// - `Tooltip::attach(cx, ov, "…", d, Button::new(…))` gets the
    ///   keyboard trigger: the button's own root is the focus target.
    /// - An anchor that merely CONTAINS the focusable thing — an
    ///   `Element` wrapping a button, or a `dyn_view` around one — does
    ///   not. The descendant takes focus and this listener never hears
    ///   it. Attach to the focusable node itself.
    ///
    /// The engine does not make the anchor focusable for you: that
    /// would silently insert a tab stop into the app's traversal order,
    /// which is the app's call and not the tip's. A tip on a
    /// non-interactive decoration stays mouse-only, deliberately.
    ///
    /// Focus reuses the same `delay` as hover rather than opening at
    /// once. Tabbing through a row of chips would otherwise mount and
    /// tear down a tip per stop; the generation counter makes that
    /// correct either way, but one policy is one code path.
    pub fn attach_content(
        cx: Scope,
        overlays: &Overlays,
        content: TipContent,
        delay: Duration,
        view: View,
    ) -> View {
        let text = content;
        let overlays = overlays.clone();
        // A tooltip that never opens on hover is not a degraded
        // tooltip, it is a broken one — and the default session posture
        // (`MouseMode::ButtonDrag`) reports motion only while a button
        // is held, so the tip opened on CLICK and stayed shut on hover.
        // Declaring the need here means mounting one is enough; the
        // app is not asked to know which terminal mode its widgets run
        // on. Costs the 1003 traffic only for apps that have a tip.
        overlays.require_pointer_motion();
        let state = Rc::new(RefCell::new(TipState {
            generation: 0,
            open: None,
            anchor: Rect::ZERO,
            stale_armed: false,
        }));
        {
            // Anchor loss: the wrapped subtree unmounting hides the tip.
            let state = state.clone();
            cx.on_cleanup(move || state.borrow_mut().close());
        }
        // ONE arming path for both triggers. Hover and focus are two
        // ways of saying "the reader is on this element"; giving them
        // an open path each would give the stale-timer bug two homes,
        // which is the same reason `Label` and `Card` share one.
        let arm: Rc<dyn Fn(&mut crate::ui::EventCtx)> = {
            let state = state.clone();
            let overlays = overlays.clone();
            let text = text.clone();
            Rc::new(move |ctx: &mut crate::ui::EventCtx| {
                let generation = {
                    let mut s = state.borrow_mut();
                    s.generation += 1;
                    // SCREEN cells: the tip places in viewport space,
                    // and the anchor may live on a positioned overlay
                    // (a GraphView card inside a Drawer). Captured at
                    // trigger time — a layer still mid-slide when the
                    // delay fires keeps the trigger-time position (the
                    // pre-existing capture-once contract, in the right
                    // space).
                    s.anchor = ctx.current_rect_screen();
                    s.generation
                };
                let state = state.clone();
                let overlays = overlays.clone();
                let text = text.clone();
                after(delay, move || {
                    let anchor = {
                        let s = state.borrow();
                        if s.generation != generation || s.open.is_some() {
                            return; // left (or re-shown) meanwhile
                        }
                        s.anchor
                    };
                    let open = show_tip(&overlays, cx, anchor, &text);
                    state.borrow_mut().open = open;
                });
            })
        };
        // The keyboard trigger, on the caller's own root — the only
        // node that will ever be sent `FocusIn` for this anchor. See
        // the doc comment for what that does and does not reach.
        let mut view = view;
        {
            let state = state.clone();
            let arm = arm.clone();
            view.on_root(Phase::Bubble, move |ctx, ev| match ev {
                UiEvent::FocusIn => arm(ctx),
                UiEvent::FocusOut => state.borrow_mut().close(),
                // Escape dismisses what the reader can see, and is
                // CONSUMED only then: a tip is the innermost
                // dismissible thing on screen, but swallowing every
                // Escape an anchor happens to be focused for would
                // wedge the dialog behind it.
                UiEvent::Key(k) if k.key == Key::Escape && k.mods == Mods::NONE => {
                    let open = state.borrow().open.is_some();
                    if open {
                        state.borrow_mut().close();
                        ctx.stop_propagation();
                    }
                }
                _ => {}
            });
        }
        let handler = {
            let state = state.clone();
            move |ctx: &mut crate::ui::EventCtx, ev: &UiEvent| match ev {
                UiEvent::MouseEnter => arm(ctx),
                UiEvent::MouseLeave => state.borrow_mut().close(),
                _ => {}
            }
        };
        // Anchor-moved guard. The anchor rect is captured ONCE, at
        // hover time, and hover itself is recomputed only from mouse
        // REPORTS — so a feed that scrolls under a stationary pointer
        // moves the anchor with nothing to synthesise a `MouseLeave`
        // from, and the tip is left describing a row that is no longer
        // there. The anchor's own draw is the cheapest truthful signal
        // that it moved: it runs exactly when the anchor repaints,
        // which is what moving IS, and costs nothing while idle. A
        // per-frame task would cost a frame forever; a resize watcher
        // (the `Popup` spelling) would miss scrolling entirely.
        //
        // The close is DEFERRED to the next timer phase rather than
        // done here, because a draw closure runs inside the tree's
        // paint walk and must not mutate the overlay store being
        // walked. Generation makes the one-shot idempotent against a
        // re-hover that lands first.
        let draw_state = state.clone();
        // Content-tight wrapper: `align_self(Start)` opts out of the
        // parent's cross-axis stretch, so the hover box (= the tip's
        // anchor rect) is the wrapped view's own extent, not a
        // stretched row.
        Element::new()
            .style(LayoutStyle::default().align_self(crate::layout::Align::Start))
            .draw(move |_canvas, rect| {
                // SCREEN cells, to compare against a screen-space
                // capture: the draw rect is layer-local.
                let o = crate::ui::layer_origin();
                let live = rect.translate(o.x, o.y);
                let generation = {
                    let mut s = draw_state.borrow_mut();
                    if s.open.is_none() || s.stale_armed || s.anchor == live {
                        return;
                    }
                    s.stale_armed = true;
                    s.generation
                };
                let state = draw_state.clone();
                after(Duration::ZERO, move || {
                    let mut s = state.borrow_mut();
                    s.stale_armed = false;
                    if s.generation == generation {
                        s.close();
                    }
                });
            })
            .on(Phase::Bubble, handler)
            .child(view)
            .build()
    }
}

/// Show one tip at `top_z() + 1`. Returns None when nothing fits (the
/// honest place_panel outcome).
///
/// Both variants solve the SAME geometry and differ only in what they
/// mount into it: a label paints on a draw layer, a card mounts a
/// passive tree.
fn show_tip(overlays: &Overlays, cx: Scope, anchor: Rect, content: &TipContent) -> Option<OpenTip> {
    let viewport = current_viewport();
    let want = match content {
        TipContent::Label(text) => Size::new(crate::text::width(text) + 2, 1),
        TipContent::Card { size, .. } => *size,
    };
    let rect = place_panel(
        viewport,
        anchor,
        want,
        PanelWidth::Content {
            min: want.w.min(3),
            max: want.w,
        },
    );
    if rect.h <= 0 || rect.w <= 0 {
        return None;
    }
    let z = overlays.top_z() + 1;
    match content {
        TipContent::Label(text) => {
            let tokens = &current_theme().tokens;
            let ink = tokens.text;
            let ground = tokens.surface_raised;
            // The one-column pad on each side is the label's frame; what
            // is left is the text's budget. A viewport too narrow for the
            // whole label ELLIPSISES it rather than letting the canvas
            // clip — a clipped label ends on a real word and reads as the
            // whole message.
            let text = crate::text::truncate_ellipsis(text, (rect.w - 2).max(0));
            let layer = overlays.layer_draw(z, rect, move |canvas, rect| {
                let style = Style::new().fg(ink).bg(ground);
                canvas.fill_styled(rect, ' ', &style);
                canvas.print_styled(crate::base::Point::new(rect.x + 1, rect.y), &text, &style);
            });
            Some(OpenTip {
                layer,
                scope: None,
                truncation: None,
            })
        }
        // `modal: false` is the whole routing contract: keys stay with
        // the anchor owner and the tip is never focused.
        TipContent::Card { build, .. } => {
            let scope = cx.child();
            let view = build(scope);
            let layer = overlays.layer_tree(z, rect, false, scope, view);
            Some(OpenTip {
                layer,
                scope: Some(scope),
                truncation: truncation_marker(overlays, z + 1, rect, want.h),
            })
        }
    }
}

/// Draw the "N more" row over the last line of a card the viewport could
/// not fit, or return None when the card fitted whole.
///
/// A browser can clip a preview and stay honest, because the scrollbar is
/// visible evidence there is more. A terminal has no such affordance: a
/// 12-row card cut to 5 is indistinguishable from a 5-row card, so the
/// engine must SAY so. The marker sits one z above the card and covers
/// its bottom row — the row it hides was the partial one, and at
/// `rect.h == 1` the marker is all there is room for, which is the
/// honest report of that geometry.
fn truncation_marker(
    overlays: &Overlays,
    z: i32,
    rect: Rect,
    wanted_h: i32,
) -> Option<LayerHandle> {
    if rect.h >= wanted_h {
        return None;
    }
    // The marker row itself displaces a row of content, so what the
    // reader cannot see is everything below the rows that remain.
    let hidden = wanted_h - (rect.h - 1).max(0);
    let tokens = &current_theme().tokens;
    let ink = tokens.text_faint;
    let ground = tokens.surface_raised;
    let label = crate::text::truncate_ellipsis(&format!("… {hidden} more"), (rect.w - 2).max(0));
    let row = Rect::new(rect.x, rect.bottom() - 1, rect.w, 1);
    Some(overlays.layer_draw(z, row, move |canvas, rect| {
        let style = Style::new().fg(ink).bg(ground);
        canvas.fill_styled(rect, ' ', &style);
        canvas.print_styled(crate::base::Point::new(rect.x + 1, rect.y), &label, &style);
    }))
}

#[cfg(test)]
#[path = "anchored_owned_tests.rs"]
mod tests;
