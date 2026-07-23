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
//! of them, and widgets with an explicit reactive extent ([`Feed`]'s
//! content-sized mode answers O(1) through its `total_rows` height
//! style — the transcript recipe). A widget that only paints into its
//! rect (a bare `MarkdownView`) has no intrinsic height; wrap it in a
//! one-item [`Feed`] or keep the explicit hint (`MarkdownView::rows` is
//! its exact fold). `content_size(w, h)` remains as the override — when
//! given it WINS and nothing is measured.
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
//! thumb after the pointer leaves it).
//!
//! [`Feed`]: super::Feed
//!
//! OWNER: REACT (follow-tail + measured extent: CONTENT, app-widgets wave).

use std::cell::Cell;
use std::rc::Rc;

use crate::base::Rect;
use crate::layout::{Dimension, Inset, Position, Style as LayoutStyle};
use crate::reactive::{Scope, Signal};
use crate::theme::TokenSet;
use crate::ui::{
    dyn_view, Element, EventCtx, Key, MouseButton, MouseKind, Phase, StyledCanvas, UiEvent, View,
};

use super::list::draw_scrollbar;

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
        let ground = t.surface;

        let hint = self.content_size;
        // The reactive content extent: the hint verbatim, or the solved
        // size of the content wrapper read back by the probe below.
        let extent: Signal<(i32, i32)> = cx.signal(hint.unwrap_or((0, 0)));
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
            let tail_pinned = vertical && follow.map(|f| f.get()).unwrap_or(false) && oy.get() > 0;
            let inset = if tail_pinned {
                Inset {
                    left: Some(-ox.get()),
                    top: None,
                    right: None,
                    bottom: Some(0),
                }
            } else {
                Inset {
                    left: Some(-ox.get()),
                    top: Some(-oy.get()),
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
        cx.effect(move || {
            let (content_w, content_h) = extent.get();
            let (view_w, view_h) = view_box.get();
            if hint.is_none() && content_w == 0 && content_h == 0 {
                // Measured mode before the first measurement: (0,0) is
                // the unmeasured sentinel (a real solve gives the
                // cross axis the viewport's extent). Clamping against
                // it would destroy a restored offset at startup.
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

        let scroll_by = move |dx: i32, dy: i32, view: Rect| {
            let (content_w, content_h) = extent.get_untracked();
            if horizontal && dx != 0 {
                ox.update(|o| *o = (*o + dx).clamp(0, (content_w - view.w).max(0)));
            }
            if vertical && dy != 0 {
                oy.update(|o| *o = (*o + dy).clamp(0, (content_h - view.h).max(0)));
                derive_follow(view.h);
            }
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
                        scroll_by(dx, dy, rect);
                        ctx.stop_propagation();
                    }
                }
                UiEvent::Key(k) => {
                    let content_h = extent.get_untracked().1;
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
        // exactly this strip; drag maps pointer y to offset with capture
        // keeping the drag alive outside the strip.
        let bar = dyn_view(
            LayoutStyle::default()
                .width(Dimension::Cells(1))
                .height(Dimension::Percent(1.0)),
            move || {
                let offset = oy.get();
                let content_h = extent.get().1; // tracked: thumb resizes with growth
                Element::new()
                    .style(
                        LayoutStyle::default()
                            .width(Dimension::Cells(1))
                            .height(Dimension::Percent(1.0)),
                    )
                    .on(Phase::Bubble, move |ctx: &mut EventCtx, ev: &UiEvent| {
                        if let UiEvent::Mouse(m) = ev {
                            let grabbed = matches!(
                                m.kind,
                                MouseKind::Down(MouseButton::Left)
                                    | MouseKind::Drag(MouseButton::Left)
                            );
                            if grabbed {
                                let bar = ctx.current_rect();
                                let usable = (bar.h - 1).max(1);
                                let frac = (m.pos.y - bar.y).clamp(0, usable);
                                let max_off = (content_h - bar.h).max(0);
                                oy.set((frac * max_off) / usable);
                                derive_follow(bar.h);
                                ctx.stop_propagation();
                            }
                        }
                    })
                    .draw(move |canvas, rect| {
                        if rect.is_empty() {
                            return;
                        }
                        draw_scrollbar(canvas, rect, offset, content_h, track, thumb, ground);
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
fn size_probe(sig: Signal<(i32, i32)>) -> impl FnMut(&mut dyn StyledCanvas, Rect) {
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
