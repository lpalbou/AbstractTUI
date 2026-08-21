//! Scroll: a generic clipped viewport over oversized MOUNTED content.
//!
//! The content is mounted ONCE (widget state inside survives scrolling);
//! offsets drive a reactive layout style (`Element::style_signal`) that
//! repositions the content wrapper with negative absolute insets — no
//! remount, real solved rects, so hit testing and focus inside scrolled
//! content keep working. The viewport clips via layout's
//! `clip_overflow`; scrolled-away children are neither painted nor
//! hit-testable (tree-level guarantees).
//!
//! ## Content extent: measured by default, hint optional (0130)
//!
//! Without a hint the content wrapper's scroll axis is `Auto`, so the
//! layout solver answers its intrinsic size on every solve — the size
//! query the module's v1 honesty note used to file as a request.
//! Content that carries an intrinsic height answers exactly: text
//! leaves (wrap-aware measurement at the viewport width), element trees
//! of them, widgets with an explicit reactive extent ([`Feed`]'s
//! content-sized mode answers O(1) through its `total_rows` height
//! style — the transcript recipe), and the content views
//! (`MarkdownView` answers its typeset row count at the viewport
//! width, `CodeView` its line count — wave 13, so
//! `Scroll::new(MarkdownView::new(doc).view(cx))` scrolls out of the
//! box). `content_size(w, h)` remains as the override — when given it
//! WINS and nothing is measured.
//!
//! ## Follow-tail (0130)
//!
//! [`Scroll::follow_tail`] binds the transcript idiom to an app-visible
//! signal: while true, the offset tracks the content's bottom edge
//! across appends AND resizes; any user scroll (wheel, keys, thumb
//! drag) landing above the bottom sets it false; scrolling back to the
//! bottom edge re-arms it. The app may force it true ("jump to latest")
//! and render it ("following / scrolled"). Vertical axis only.
//!
//! ## Follow-tail freeze (first-app/1300)
//!
//! [`freeze_follow_tail`] holds a pinned scroller still without
//! disengaging it: the rows on screen stay on screen while appends grow
//! below the viewport, and unfreezing re-pins to the tail as it stands
//! then. The engine drives it from the screen-selection layer
//! (`app::selection`) — a live drag freezes every follow-tail scroller,
//! because a transcript that scrolls under a selection copies rows the
//! user never highlighted. Apps may drive it for their own freezes.
//!
//! ## Offset repair on content shrink (first-app/0281)
//!
//! A bound offset that a CONTENT shrink (or viewport growth) left
//! beyond the new `max_off` is repaired by the engine: the offset
//! signal clamps down to the new max when the measured extent or the
//! viewport box changes, so the pane never renders void waiting for a
//! gesture. In-range programmatic writes are never touched (offset
//! reads are untracked — only extent/viewport changes trigger the
//! repair), growth never moves a reading user, and `follow` is neither
//! disengaged nor armed by a repair (only gestures write follow). The
//! repair rides the signal, not the pixels: scrollbar, gestures and
//! app reads stay coherent, one settle turn after the shrink.
//!
//! Wheel scrolls vertically (horizontal wheel scrolls x); arrows/PgUp/
//! PgDn/Home/End work while focused; the scrollbar thumb drags with
//! pointer capture (mouse-down auto-captures, so drags keep steering the
//! thumb after the pointer leaves it). The strip's geometry, hit test
//! and pointer→offset inverse are the shared seam
//! (`widgets::scrollbar`, private) every scrolling widget
//! uses, so a press takes hold of the thumb where it was drawn instead
//! of jumping the content out from under it, and
//! [`scrollbar_width`](Scroll::scrollbar_width) widens the gutter for
//! apps with room for a bigger mouse target.
//!
//! [`Feed`]: super::Feed
//!
//! OWNER: REACT (follow-tail + measured extent: CONTENT, app-widgets wave).

use std::cell::Cell;
use std::rc::Rc;

use crate::base::Rect;
use crate::layout::{Dimension, Inset, Position, Style as LayoutStyle};
use crate::reactive::{create_root, Scope, Signal};
use crate::theme::TokenSet;
use crate::ui::{
    dyn_view, Element, EventCtx, Key, MouseButton, MouseKind, Phase, StyledCanvas, UiEvent, View,
};

use super::scrollbar;

// ---------------------------------------------------------------------------
// Follow-tail freeze (first-app/1300)
// ---------------------------------------------------------------------------

thread_local! {
    /// One process-lifetime signal per thread, under a deliberately
    /// leaked root (the `app::theme` pattern): the freeze outlives every
    /// component scope that reads it, and disposing the root would
    /// invalidate the handle each `Scroll` captured.
    static FOLLOW_FROZEN: Cell<Option<Signal<bool>>> = const { Cell::new(None) };
}

fn follow_frozen_signal() -> Signal<bool> {
    FOLLOW_FROZEN.with(|slot| {
        if let Some(sig) = slot.get() {
            return sig;
        }
        let (root, sig) = create_root(|cx| cx.signal(false));
        std::mem::forget(root);
        slot.set(Some(sig));
        sig
    })
}

/// Hold every follow-tail [`Scroll`] on this thread STILL: while frozen,
/// a pinned scroller keeps showing the rows it is showing now instead of
/// tracking the content's bottom edge across appends. Unfreezing re-pins
/// to the live tail in one settle turn.
///
/// The engine sets this itself while a screen selection is live
/// (`app::selection`): text that slides upward mid-drag is text you
/// cannot copy, so a streaming transcript stops moving the moment a drag
/// paints a region and resumes the instant it clears. Apps may drive it
/// for their own freezes (a copy-mode overlay, a paused inspector) —
/// last writer wins, and the engine writes only on selection edges.
///
/// It changes NOTHING else: `follow` stays armed (chrome still reads
/// "following"), wheel and keys still scroll, and the offset repair
/// still rescues a stranded view. A frozen scroller with no selection
/// and no app writer is the default false — one signal read per pinned
/// scroller per settle, nothing more.
pub fn freeze_follow_tail(on: bool) {
    follow_frozen_signal().set_if_changed(on);
}

/// Whether follow-tail is currently frozen ([`freeze_follow_tail`]).
/// Reads inside a tracked computation re-run it on the next flip.
pub fn follow_tail_frozen() -> bool {
    follow_frozen_signal().get()
}

/// A clipped viewport over oversized mounted content: wheel, arrows,
/// PgUp/PgDn, a drag-able vertical scrollbar (the horizontal axis
/// scrolls by wheel and keys — it draws no bar), and the transcript idiom
/// [`follow_tail`](Scroll::follow_tail) (pin to the bottom until the
/// user scrolls up; re-arm by reaching it again or setting the signal).
///
/// Content mounts ONCE and keeps its widget state while scrolling; its
/// extent is measured by default ([`content_size`](Scroll::content_size)
/// is the optional hint). Bind [`offset_x`](Scroll::offset_x)/
/// [`offset_y`](Scroll::offset_y) to own the position. The canonical
/// build is `.view(cx)`; see the [module docs](crate::widgets::scroll).
pub struct Scroll {
    content: View,
    /// `Some` = explicit extent hint (wins, nothing measured);
    /// `None` = measured from the mounted content (default).
    content_size: Option<(i32, i32)>,
    vertical: bool,
    horizontal: bool,
    offset_y: Option<Signal<i32>>,
    offset_x: Option<Signal<i32>>,
    follow: Option<Signal<bool>>,
    extent_out: Option<Signal<(i32, i32)>>,
    /// Mirror of the viewport's solved size (cells); optional app tap.
    viewport_out: Option<Signal<(i32, i32)>>,
    scrollbar_auto_hide: bool,
    scrollbar_width: i32,
    layout: Option<LayoutStyle>,
}

impl Scroll {
    pub fn new(content: View) -> Scroll {
        Scroll {
            content,
            content_size: None,
            vertical: true,
            horizontal: false,
            offset_y: None,
            offset_x: None,
            follow: None,
            extent_out: None,
            viewport_out: None,
            scrollbar_auto_hide: false,
            scrollbar_width: 1,
            layout: None,
        }
    }

    /// Explicit content extent in cells. Optional since 0130: without it
    /// the extent is MEASURED from the mounted content (see the module
    /// docs for what answers exactly); with it, the hint wins verbatim.
    pub fn content_size(mut self, w: i32, h: i32) -> Scroll {
        self.content_size = Some((w, h));
        self
    }

    pub fn axes(mut self, horizontal: bool, vertical: bool) -> Scroll {
        self.horizontal = horizontal;
        self.vertical = vertical;
        self
    }

    /// Bind external offset signals (dashboards syncing panes).
    pub fn offset_y(mut self, sig: Signal<i32>) -> Scroll {
        self.offset_y = Some(sig);
        self
    }

    pub fn offset_x(mut self, sig: Signal<i32>) -> Scroll {
        self.offset_x = Some(sig);
        self
    }

    /// Bind the follow-tail policy (0130): while `sig` is true the
    /// offset stays pinned to the content bottom across appends and
    /// resizes; a user scroll above the bottom sets it false; reaching
    /// the bottom again (wheel, End, thumb) re-arms it. The signal is
    /// app-visible both ways — read it for "following / scrolled"
    /// chrome, set it true to jump to the latest. Vertical only.
    pub fn follow_tail(mut self, sig: Signal<bool>) -> Scroll {
        self.follow = Some(sig);
        self
    }

    /// Bind the CONTENT EXTENT `(w, h)` in cells to an app-visible
    /// signal (first-app 0260 / field-agora 0850 enabler): in measured
    /// mode the solver's answer lands here one settle turn after it
    /// changes (`(0, 0)` = not measured yet — a supplied signal's
    /// current value is kept until then, so a remounting caller warm-
    /// starts from its last measurement); in hint mode the hint lands
    /// verbatim at build. The Scroll OWNS the writes while mounted —
    /// read it for "N more rows" chrome or height-capped wrappers
    /// (`Disclosure::max_body_rows` sizes its body region from it);
    /// writing it yourself desynchronizes clamps and the thumb.
    ///
    /// One caveat the warm start does NOT cover, so chrome reading this
    /// signal is not surprised by it: content whose height depends on a
    /// width discovered in draw (`Feed`, `MarkdownView`) solves in TWO
    /// steps, and the FIRST is a placeholder — `(w, 1)` — which
    /// overwrites the warm value for one turn before the real count
    /// lands. Bound offsets are protected from that (field-agora/0895);
    /// a reader rendering "N more rows" straight off this signal will
    /// still see one frame of nonsense on mount. Debounce it, or drive
    /// that chrome off a settled copy.
    pub fn extent_signal(mut self, sig: Signal<(i32, i32)>) -> Scroll {
        self.extent_out = Some(sig);
        self
    }

    /// Mirror the viewport's solved width/height (cells) into an app
    /// signal — use with [`extent_signal`](Scroll::extent_signal) to
    /// compute `max_off = content_h - view_h` outside the widget.
    pub fn viewport_size_signal(mut self, sig: Signal<(i32, i32)>) -> Scroll {
        self.viewport_out = Some(sig);
        self
    }

    /// Auto-hide the vertical scrollbar while the content FITS the
    /// viewport (default `false`: the bar always renders, full-height
    /// thumb when nothing overflows — the pre-0.2.11 behavior,
    /// byte-stable). When hidden, the bar's column is still RESERVED
    /// (painted as bare ground) so content width never re-wraps when
    /// the bar appears, and the invisible strip ignores drags — an
    /// invisible target must never jump the offset.
    pub fn scrollbar_auto_hide(mut self, auto_hide: bool) -> Scroll {
        self.scrollbar_auto_hide = auto_hide;
        self
    }

    /// Width of the vertical scrollbar's column, in cells (default 1,
    /// clamped to 1..=4). The strip is a RESERVED gutter — widening it
    /// takes the cells from the content, so a pane that measures its own
    /// text against the viewport width must widen with it. Two cells is
    /// the comfortable mouse target on a dense screen; one is the
    /// terminal-native default that costs content nothing.
    pub fn scrollbar_width(mut self, cells: i32) -> Scroll {
        self.scrollbar_width = cells.clamp(1, 4);
        self
    }

    pub fn layout(mut self, layout: LayoutStyle) -> Scroll {
        self.layout = Some(layout);
        self
    }

    /// Canonical one-call build (cycle 8): tokens resolve from the
    /// app's THEME CONTEXT (a tracked read — building inside a
    /// `dyn_view` re-renders on theme switch) and the finished `View`
    /// comes back ready for `.child(..)`. Use `element(cx, &tokens)`
    /// when you need explicit theming or extra Element customization.
    pub fn view(self, cx: Scope) -> crate::ui::View {
        let t = crate::widgets::theme_tokens(cx);
        self.element(cx, &t).build()
    }

    pub fn element(self, cx: Scope, t: &TokenSet) -> Element {
        let track = t.border;
        let thumb = t.text_muted;
        // The hot ink: what the thumb wears while the pointer is on the
        // strip or a drag is live — the same accent `List` lights its
        // own strip with.
        let hot_ink = t.accent;
        let ground = t.surface;

        let hint = self.content_size;
        let viewport_out = self.viewport_out;
        // The reactive content extent: the hint verbatim, or the solved
        // size of the content wrapper read back by the probe below. A
        // caller-bound signal (`extent_signal`) REPLACES the internal
        // one — same writes, app-visible; its pre-existing value is
        // kept in measured mode (warm start for remounting callers),
        // while a hint overwrites it (the hint IS the truth).
        let extent: Signal<(i32, i32)> = match self.extent_out {
            Some(sig) => {
                if let Some(h) = hint {
                    sig.set_if_changed(h);
                }
                sig
            }
            None => cx.signal(hint.unwrap_or((0, 0))),
        };
        // Viewport box — reactive only for the follow-tail pin (resize
        // re-pins); gestures read their own rect from the event ctx.
        let view_box: Signal<(i32, i32)> = cx.signal((0, 0));
        let ox = self.offset_x.unwrap_or_else(|| cx.signal(0i32));
        let oy = self.offset_y.unwrap_or_else(|| cx.signal(0i32));
        let follow = self.follow;
        let (vertical, horizontal) = (self.vertical, self.horizontal);
        let layout = self.layout.unwrap_or_else(|| {
            // basis 0 beside grow: inside a flex parent the scroll takes
            // LEFTOVER space instead of demanding its content-derived
            // basis — a long transcript can no longer starve fixed
            // sibling rows to zero (the 0240 modal-overflow class;
            // follow-up #1 from its completion report).
            LayoutStyle::default().grow(1.0).basis(Dimension::Cells(0))
        });

        // The mounted-once content wrapper: negative insets = scrolling.
        // Hint mode: explicit size, so absolute layout never consults
        // intrinsics for huge content. Measured mode: the scroll axis
        // stays Auto and the SOLVER answers it per solve (the 0130 size
        // query — `place_absolute` measures intrinsics for Auto axes);
        // the cross axis fills the viewport.
        //
        // While FOLLOWING with a scrolled pane, the wrapper anchors its
        // BOTTOM inset to the viewport instead of top-offsetting: the
        // solver keeps the tail glued through appends, shrinks and
        // resizes with ZERO extent knowledge (pixel-exact the same
        // frame), and the wrapper can never scroll out of the clip —
        // which would starve the size probe (a rebuilt/shrunken feed
        // used to deadlock exactly there). The offset signal is synced
        // by the pin effect a turn later for scrollbar/gesture
        // coherence.
        let wrapper_style = move || {
            let (w, h) = match hint {
                Some((w, h)) => (Dimension::Cells(w.max(1)), Dimension::Cells(h.max(1))),
                None => (
                    if horizontal {
                        Dimension::Auto
                    } else {
                        Dimension::Percent(1.0)
                    },
                    if vertical {
                        Dimension::Auto
                    } else {
                        Dimension::Percent(1.0)
                    },
                ),
            };
            // A freeze (1300: a live screen selection, or an app's own
            // call) drops the bottom anchor and hands the wrapper back
            // to `top: -oy` — the offset the pin last wrote. Appends
            // then grow BELOW the viewport instead of pushing the
            // visible rows up, which is the whole point: the cells
            // under a drag stay the cells the copy will read.
            let tail_pinned = vertical
                && follow.map(|f| f.get()).unwrap_or(false)
                && oy.get() > 0
                && !follow_frozen_signal().get();
            // RENDER-side clamp (field-agora/0895). The repair effect
            // below owns the app's signal and is deliberately slow to
            // write it; this owns where the content is DRAWN and is
            // always immediate. They are not redundant: an offset past
            // the end must never park the content outside the clip,
            // because a culled child never draws, a `Feed` that never
            // draws never discovers its width, and a width it never
            // discovers is an extent it never corrects. The pane would
            // stay void forever waiting for a measurement that can only
            // happen if it is visible. Clamping the inset breaks that
            // deadlock without touching the caller's signal.
            let (content_w, content_h) = extent.get();
            let (view_w, view_h) = view_box.get();
            // The unmeasured sentinel is exempt: a restored offset must
            // not flash at the top for one frame before the first solve.
            let unmeasured = hint.is_none() && content_w == 0 && content_h == 0;
            let draw_at = |off: i32, content: i32, view: i32, on: bool| {
                if unmeasured || !on || view <= 0 {
                    off
                } else {
                    off.clamp(0, (content - view).max(0))
                }
            };
            let left = -draw_at(ox.get(), content_w, view_w, horizontal);
            let inset = if tail_pinned {
                Inset {
                    left: Some(left),
                    top: None,
                    right: None,
                    bottom: Some(0),
                }
            } else {
                Inset {
                    left: Some(left),
                    top: Some(-draw_at(oy.get(), content_h, view_h, vertical)),
                    right: None,
                    bottom: None,
                }
            };
            LayoutStyle {
                position: Position::Absolute,
                inset,
                width: w,
                height: h,
                ..LayoutStyle::default()
            }
        };
        let mut wrapper = Element::new().style_signal(wrapper_style);
        if hint.is_none() {
            // Measured mode: read the solver's answer back into the
            // extent signal (clamps, thumb, follow pin). The probe
            // draws even when the wrapper is fully scrolled out of the
            // clip (`probe_when_culled`, first-app/0281): a content
            // SHRINK below the held offset puts the wrapper entirely
            // above the viewport, where a culled probe would starve —
            // the extent would freeze at the pre-shrink value and the
            // offset repair below could never see the shrink.
            wrapper = wrapper.draw(size_probe(extent)).probe_when_culled();
        }
        let wrapper = wrapper.child(self.content);

        let viewport = Element::new()
            .style(
                LayoutStyle::default()
                    .grow(1.0)
                    // Scroll (not just Clip): scrolled-away content
                    // neither paints nor hits, AND the node advertises
                    // itself to wheel routing / ensure-visible.
                    .scroll(),
            )
            .role(crate::ui::Role::ScrollArea)
            .child(wrapper.build())
            // The viewport box feeds the follow pin AND the offset
            // repair (0281), so the probe is unconditional now. Steady
            // frames record an unchanged size and schedule nothing.
            .draw(size_probe(view_box));

        if let Some(out) = viewport_out {
            cx.effect(move || {
                out.set(view_box.get());
            });
        }

        // Follow-tail pin: while following, the offset tracks the
        // content bottom across appends (extent growth) and resizes
        // (view_box). The effect writes a signal it never reads — no
        // cycle — and only GESTURES write `follow` from geometry, so a
        // programmatic offset write never disengages the user.
        if let Some(f) = follow {
            cx.effect(move || {
                if !f.get() {
                    return; // extent/view re-track when re-armed
                }
                // Frozen (1300): the offset holds where it is, so the
                // view holds too. The freeze signal is tracked BEFORE
                // the early return — thawing re-runs this effect, which
                // re-tracks extent/view and pins to the tail as it
                // stands then (one settle turn, no jump backwards).
                if follow_frozen_signal().get() {
                    return;
                }
                let content_h = extent.get().1;
                let view_h = view_box.get().1;
                if view_h > 0 {
                    let pinned = (content_h - view_h).max(0);
                    if oy.try_get_untracked() != Some(pinned) {
                        oy.set(pinned);
                    }
                }
            });
        }

        // Offset repair (first-app/0281): a CONTENT shrink (details
        // fold, session switch) or a viewport growth can strand a bound
        // offset beyond the new max — the pane rendered void until a
        // gesture rescued it. Track the two truths of max_off (extent +
        // viewport) and clamp the offset DOWN when they change; offset
        // reads stay untracked, so in-range programmatic writes are
        // never touched and growth never moves a reading user (max_off
        // only grows). `follow` is never written here: a repair is not
        // a gesture, so it neither disengages nor arms the follow —
        // and while following, the pin above computes the same value,
        // so the two effects can never fight.
        // The FIRST measurement to ARRIVE after mount is provisional
        // (field-agora/0895) — see the effect below. Hint mode has no
        // measurement at all, so it is trusted from the first run.
        //
        // `at_build` is whatever the extent signal already held when
        // this Scroll was constructed. On a fresh mount that is the
        // (0,0) sentinel; with a caller-bound `extent_signal` it is the
        // WARM value carried over from the previous mount. Neither is a
        // measurement THIS mount took, so observing one must not spend
        // the provisional exemption below — a warm extent doing exactly
        // that is the whole of the warm-start bug.
        let at_build = extent.get_untracked();
        let measured_once = Cell::new(hint.is_some());
        cx.effect(move || {
            let (content_w, content_h) = extent.get();
            let (view_w, view_h) = view_box.get();
            if hint.is_none() && (content_w, content_h) == at_build {
                // Nothing has arrived yet this mount. The fresh-mount
                // spelling is the (0,0) unmeasured sentinel (a real
                // solve gives the cross axis the viewport's extent);
                // the warm spelling is a remembered extent a remounting
                // caller supplied through `extent_signal`. Clamping
                // against either would destroy a restored or
                // carried-over offset before one solve has run.
                return;
            }
            if !measured_once.replace(true) {
                // field-agora/0895: (0,0) is not the only untrustworthy
                // extent. A widget whose height depends on a width it
                // only learns inside DRAW publishes its size in two
                // steps — a one-row placeholder first, the real count on
                // the next turn. `Feed` does exactly this, deliberately:
                // typesetting needs the width, and RT1-2 forbids writing
                // the reactive height from a paint closure, so the
                // correction rides an `after(0)` fixup (feed.rs). The
                // placeholder is a REAL-LOOKING measurement — (w, 1),
                // cross axis already correct — so the clamp below
                // computed max_off = 0 and wrote the app's bound offset
                // to zero. Remounting a bound `Scroll` over a `Feed`
                // therefore rewound the reader to the top and destroyed
                // the app's own state, every time.
                //
                // So the first measurement only establishes that we HAVE
                // one; it never clamps. Every later change is trusted,
                // which is where a genuine shrink (0281) lands. The
                // narrow, deliberate gap: an offset restored beyond a
                // content extent that never changes again stays out of
                // range until the next extent or viewport change. That
                // is strictly better than destroying a valid offset on
                // every remount, and unlike a deferred re-check it costs
                // no frame and cannot race the publisher's own fixup.
                return;
            }
            if vertical && view_h > 0 {
                let max_off = (content_h - view_h).max(0);
                if oy.try_get_untracked().is_some_and(|o| o > max_off) {
                    oy.set(max_off);
                }
            }
            if horizontal && view_w > 0 {
                let max_off = (content_w - view_w).max(0);
                if ox.try_get_untracked().is_some_and(|o| o > max_off) {
                    ox.set(max_off);
                }
            }
        });

        // After a user gesture: landing on the bottom edge re-arms the
        // follow, landing above it releases it (0130 semantics).
        let derive_follow = move |view_h: i32| {
            if let Some(f) = follow {
                let max_off = (extent.get_untracked().1 - view_h).max(0);
                f.set_if_changed(oy.get_untracked() >= max_off);
            }
        };

        // Returns whether this scroller actually consumed the delta —
        // the caller stops propagation on true, so a wheel at the edge
        // chains to a parent scroller instead of dying here.
        let scroll_by = move |dx: i32, dy: i32, view: Rect| -> bool {
            let (content_w, content_h) = extent.get_untracked();
            let mut moved = false;
            if horizontal && dx != 0 {
                let max_off = (content_w - view.w).max(0);
                moved |= ox.set_if_changed((ox.get_untracked() + dx).clamp(0, max_off));
            }
            if vertical && dy != 0 {
                let max_off = (content_h - view.h).max(0);
                // While the tail is pinned the wrapper is bottom-anchored
                // and `oy` is only synced a turn later, so the stale value
                // is not where the reader is — the pin IS the position.
                // Stepping from it is what keeps a wheel-up off the head.
                // FROZEN (1300), the wrapper is top-anchored again and the
                // pin writes nothing: `oy` IS the position, and stepping
                // from the live bottom would jump the reader to the tail
                // they are mid-drag over. Read untracked — a gesture is
                // not a computation.
                let base = if follow.is_some_and(|f| f.get_untracked())
                    && !follow_frozen_signal().get_untracked()
                {
                    max_off
                } else {
                    oy.get_untracked()
                };
                moved |= oy.set_if_changed((base + dy).clamp(0, max_off));
                derive_follow(view.h);
            }
            moved
        };

        let handler = move |ctx: &mut EventCtx, ev: &UiEvent| {
            let rect = ctx.current_rect();
            match ev {
                UiEvent::Mouse(m) => {
                    let (dx, dy) = match m.kind {
                        MouseKind::ScrollUp => (0, -3),
                        MouseKind::ScrollDown => (0, 3),
                        MouseKind::ScrollLeft => (-3, 0),
                        MouseKind::ScrollRight => (3, 0),
                        _ => (0, 0),
                    };
                    if dx != 0 || dy != 0 {
                        // A scroller that has not been measured yet still
                        // OWNS the gesture: reporting "did not move"
                        // would chain the wheel to an ancestor and scroll
                        // the wrong pane for the first frames after mount.
                        let unmeasured = hint.is_none() && extent.get_untracked() == (0, 0);
                        if scroll_by(dx, dy, rect) || unmeasured {
                            ctx.stop_propagation();
                        }
                    }
                }
                UiEvent::Key(k) => {
                    let (_content_w, content_h) = extent.get_untracked();
                    let (dx, dy) = match k.key {
                        Key::Up => (0, -1),
                        Key::Down => (0, 1),
                        Key::Left => (-1, 0),
                        Key::Right => (1, 0),
                        Key::PageUp => (0, -rect.h.max(1)),
                        Key::PageDown => (0, rect.h.max(1)),
                        Key::Home => (0, -content_h),
                        Key::End => (0, content_h),
                        _ => return,
                    };
                    scroll_by(dx, dy, rect);
                    ctx.stop_propagation();
                }
                _ => {}
            }
        };

        // Scrollbar: its own Dyn column so offset/extent changes damage
        // exactly this strip. Geometry, hit test and the pointer->offset
        // inverse all come from the shared seam (`widgets::scrollbar`),
        // so what the drag computes is what the paint drew — the thumb
        // stays under the cursor for the whole gesture.
        //
        // The gesture is stateful on purpose: `Down` REMEMBERS where
        // inside the thumb the pointer grabbed and moves nothing; only
        // `Drag` scrolls, and only while that grab is live. A bare
        // `Drag` with no grab (terminals emit motion-with-button after a
        // lost `Up`) is somebody else's gesture and must never teleport
        // the offset. Screen select mode used to produce that state
        // here too — the layer claimed the drag and the driver dropped
        // this tree's capture with it — until the strip declared itself
        // a `drag_zone` below (first-app/1335) and the layer learned to
        // stand down over one.
        //
        // Under `scrollbar_auto_hide`, a fitting content hides the bar:
        // the strip paints bare ground (deterministic pixels — skipping
        // the draw would leave the previous frame's glyphs in a damaged
        // region) and ignores drags (an invisible target must never
        // steer the offset).
        let auto_hide = self.scrollbar_auto_hide;
        let bar_w = self.scrollbar_width.clamp(1, 4);
        // Grab state (rows below the thumb's top edge): a plain cell, so
        // taking hold of the thumb re-renders nothing. Hover IS a signal
        // — the strip has to repaint to show its hot ink, and only a
        // tracked read damages it (the `List` hover pattern).
        let grab: Rc<Cell<Option<i32>>> = Rc::new(Cell::new(None));
        let hot: Signal<bool> = cx.signal(false);
        let grab_h = grab.clone();
        let bar = dyn_view(
            LayoutStyle::default()
                .width(Dimension::Cells(bar_w))
                .height(Dimension::Percent(1.0)),
            move || {
                let offset = oy.get();
                let content_h = extent.get().1; // tracked: thumb resizes with growth
                let is_hot = hot.get(); // tracked: hot ink repaints the strip
                let grab = grab_h.clone();
                Element::new()
                    .style(
                        LayoutStyle::default()
                            .width(Dimension::Cells(bar_w))
                            .height(Dimension::Percent(1.0)),
                    )
                    .on(Phase::Bubble, move |ctx: &mut EventCtx, ev: &UiEvent| {
                        // Hover is the affordance that answers "can I
                        // grab this?" before the user has to guess. It
                        // rides `MouseEnter`/`MouseLeave`, so it lights
                        // only where the app opted into hover motion
                        // (`RunConfig::hover_ink`); a live drag lights it
                        // everywhere, since drag motion always reports.
                        match ev {
                            UiEvent::MouseEnter => {
                                hot.set_if_changed(true);
                                return;
                            }
                            UiEvent::MouseLeave => {
                                // A live drag keeps the ink: the pointer
                                // leaves the strip constantly mid-drag.
                                if grab.get().is_none() {
                                    hot.set_if_changed(false);
                                }
                                return;
                            }
                            _ => {}
                        }
                        let UiEvent::Mouse(m) = ev else { return };
                        if matches!(m.kind, MouseKind::Up(MouseButton::Left)) {
                            grab.set(None);
                            if !ctx.current_rect().contains(m.pos) {
                                hot.set_if_changed(false); // released off-strip
                            }
                            return;
                        }
                        let strip = ctx.current_rect();
                        if auto_hide && content_h <= strip.h {
                            return; // hidden bar: inert strip
                        }
                        let bar = scrollbar::metrics(strip, strip.w, offset, content_h);
                        if !bar.overflows() {
                            return; // nothing to steer
                        }
                        match m.kind {
                            MouseKind::Down(MouseButton::Left) => {
                                let Some(zone) = scrollbar::hit(&bar, m.pos) else {
                                    return;
                                };
                                let dy = scrollbar::grab_for(&bar, zone);
                                grab.set(Some(dy));
                                hot.set_if_changed(true);
                                // A press ON the thumb moves nothing —
                                // it only takes hold. Bare track is the
                                // teleport (thumb centers on the
                                // pointer) the macOS convention ships.
                                if matches!(zone, scrollbar::Zone::Track) {
                                    oy.set(scrollbar::offset_at(&bar, m.pos.y, dy));
                                    derive_follow(strip.h);
                                }
                                ctx.stop_propagation();
                            }
                            MouseKind::Drag(MouseButton::Left) => {
                                // Only OUR gesture scrolls (see above).
                                let Some(dy) = grab.get() else { return };
                                oy.set(scrollbar::offset_at(&bar, m.pos.y, dy));
                                derive_follow(strip.h);
                                ctx.stop_propagation();
                            }
                            _ => {}
                        }
                    })
                    // The strip owns left drags (first-app/1335): screen
                    // select mode stands down here, so the thumb keeps the
                    // gesture instead of losing its capture to a
                    // highlight. Exactly the conditions the handler above
                    // uses to decide it is inert — an invisible or
                    // non-overflowing bar claims nothing.
                    .drag_zone(move |rect| {
                        if rect.is_empty() || (auto_hide && content_h <= rect.h) {
                            return None;
                        }
                        scrollbar::metrics(rect, rect.w, offset, content_h)
                            .overflows()
                            .then_some(rect)
                    })
                    .draw(move |canvas, rect| {
                        if rect.is_empty() {
                            return;
                        }
                        if auto_hide && content_h <= rect.h {
                            let blank = crate::render::Style::new().fg(ground).bg(ground);
                            canvas.fill_styled(rect, ' ', &blank);
                            return;
                        }
                        let bar = scrollbar::metrics(rect, rect.w, offset, content_h);
                        scrollbar::draw(canvas, &bar, is_hot, track, thumb, hot_ink, ground);
                    })
                    .build()
            },
        );

        let mut root = Element::new()
            .style(layout)
            .focusable()
            .on(Phase::Bubble, handler)
            .child(viewport.build());
        if vertical {
            root = root.child(bar);
        }
        root
    }
}

/// Solved-size readback (the 0130 measured-extent seam, RT1-2 lawful):
/// the draw closure records its rect's size into a plain cell; when it
/// changed, ONE latched `after(0)` publishes the latest value to `sig`
/// next turn — paint itself never writes signals (the Feed width-fixup
/// pattern). Steady frames record an unchanged size and schedule
/// nothing, so an idle scroll costs zero timers.
pub(crate) fn size_probe(sig: Signal<(i32, i32)>) -> impl FnMut(&mut dyn StyledCanvas, Rect) {
    let seen: Rc<Cell<(i32, i32)>> = Rc::new(Cell::new((-1, -1)));
    let pending: Rc<Cell<bool>> = Rc::new(Cell::new(false));
    move |_canvas, rect| {
        let size = (rect.w, rect.h);
        if seen.get() == size {
            return;
        }
        seen.set(size);
        if pending.replace(true) {
            return; // one deferred publish at a time; it reads `seen` late
        }
        let (seen, pending) = (seen.clone(), pending.clone());
        crate::reactive::after(std::time::Duration::ZERO, move || {
            pending.set(false);
            // A disposed UI scope leaves the signal dead: stay inert
            // (an outliving timer must never panic the app).
            if sig.try_get_untracked().is_some() {
                sig.set_if_changed(seen.get());
            }
        });
    }
}

#[cfg(test)]
#[path = "scroll_tests.rs"]
mod tests;

// The 0260/0850 enabler tests (extent_signal + scrollbar_auto_hide),
// split for the file-size discipline.
#[cfg(test)]
#[path = "scroll_extent_tests.rs"]
mod extent_tests;
