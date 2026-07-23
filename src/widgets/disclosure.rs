//! Disclosure: the fold/unfold card (first-app 0260 + field-agora
//! 0850) — a one-row title header that expands a body in place.
//!
//! ```ignore
//! let d = Disclosure::markdown("cycle 3", REASONING_MD)
//!     .detail("12 lines")
//!     .max_body_rows(6)
//!     .view(cx);
//! ```
//!
//! ## Shape
//!
//! - TITLE ROW (always visible): fold glyph (`▸` folded / `▾`
//!   unfolded, `accent` ink), the title (truncate-ellipsis at width),
//!   and an optional trailing DETAIL slot (`text_muted` — timestamps,
//!   "+N lines" hints). The row is one tab stop: Enter/Space toggle
//!   while focused, a click toggles (and focuses, the tree rule).
//!   Chrome is BORDERLESS two-tone — header on `surface_raised`, body
//!   on `surface` (the style guide §3.2 borderless rules: focus =
//!   selection pair over the whole row, hover = accent ink garnish).
//!   Cards stack at transcript scale, where per-card borders read as
//!   noise; wrap one in a [`Block`](super::Block) when a frame is
//!   wanted.
//! - BODY: any `View`, built by a closure per EXPANSION (the
//!   ChoicePrompt body precedent, `FnMut` because a card re-expands).
//!   Folded = the body region rebuilds to an empty, zero-height
//!   element — nothing mounted, nothing drawn, zero idle cost; unfold
//!   REMOUNTS it (per-generation state dies with the fold; durable
//!   state belongs in signals outside the closure). `Disclosure::text`
//!   / `Disclosure::markdown` cover the common bodies through a
//!   one-item [`Feed`] (typeset once, kept across folds).
//! - HEIGHT: `max_body_rows(n)` (default 8 — "limited to a few lines",
//!   the commissioning ask) caps the body region at `min(content, n)`
//!   rows; taller content scrolls inside a [`Scroll`] with a visible
//!   scrollbar (auto-hidden while the content fits).
//!   `max_body_rows(0)` (or any non-positive value) removes the cap:
//!   the body takes its natural height, no scroll region at all.
//!   Capped bodies size themselves from the Scroll's measured extent,
//!   so the region settles one turn after content changes (the
//!   measured-extent contract in `scroll.rs`).
//! - STATE: uncontrolled by default (`initially_folded`, default
//!   FOLDED — progressive disclosure: the header is the summary, the
//!   body is the detail). `folded(Signal<bool>)` switches to
//!   app-owned state (the 0850 "newest expanded, rest folded" policy;
//!   a toggle-all writes every signal); when bound, the signal's
//!   CURRENT value is the state and `initially_folded` is ignored.
//! - A11y: the card is a `region` labeled by the title; the header is
//!   a `button` (the Role enum is frozen until 0.3 — the Select
//!   precedent) whose value reads `"collapsed"`/`"expanded"`.
//!
//! Disposal-safety law (backlog 0297): the fold state is written
//! BEFORE `on_toggle` runs, so the callback may dispose the card's
//! scope synchronously.
//!
//! OWNER: REACT (wave 7, disclosure).

use std::cell::RefCell;
use std::rc::Rc;

use crate::layout::{Dimension, Edges, Style as LayoutStyle};
use crate::reactive::{Scope, Signal};
use crate::render::Style;
use crate::theme::TokenSet;
use crate::ui::{
    dyn_view, dyn_view_scoped, Element, EventCtx, Key, MouseButton, MouseKind, Phase, UiEvent, View,
};

use super::feed::{Feed, FeedItem, FeedState};
use super::scroll::Scroll;

/// The body builder: called once per EXPANSION with the body's
/// generation scope (disposed on fold).
type BodyBuilder = Box<dyn FnMut(Scope) -> View>;

enum Body {
    None,
    Text(String),
    Markdown(String),
    Build(BodyBuilder),
}

pub struct Disclosure {
    title: String,
    detail: Option<String>,
    body: Body,
    folded: Option<Signal<bool>>,
    initially_folded: bool,
    max_body_rows: i32,
    layout: Option<LayoutStyle>,
    on_toggle: Option<Box<dyn FnMut(bool)>>,
}

impl Disclosure {
    /// A card with no body yet: compose with [`Disclosure::body`] (or
    /// leave body-less — the glyph still toggles, honestly reporting
    /// the state).
    pub fn new(title: impl Into<String>) -> Disclosure {
        Disclosure {
            title: title.into(),
            detail: None,
            body: Body::None,
            folded: None,
            initially_folded: true,
            max_body_rows: 8,
            layout: None,
            on_toggle: None,
        }
    }

    /// Convenience: a plain-text body (wrapped verbatim at the card
    /// width — log output, tool results). Typeset once in a one-item
    /// feed that survives fold cycles.
    pub fn text(title: impl Into<String>, body: impl Into<String>) -> Disclosure {
        let mut d = Disclosure::new(title);
        d.body = Body::Text(body.into());
        d
    }

    /// Convenience: a markdown body (the full DOC vocabulary — tables,
    /// task lists, lazy in-flow images — through the same typeset
    /// recipe as [`Feed`]/`MarkdownView`).
    pub fn markdown(title: impl Into<String>, src: impl Into<String>) -> Disclosure {
        let mut d = Disclosure::new(title);
        d.body = Body::Markdown(src.into());
        d
    }

    /// Any-`View` body, built per EXPANSION on the body's generation
    /// scope (`FnMut` — a card re-expands; the previous generation is
    /// disposed on fold). Durable state belongs in signals created
    /// OUTSIDE the closure; per-expansion internals (a scroll offset,
    /// hover state) belong inside and die with the fold. Replaces any
    /// text/markdown body.
    pub fn body(mut self, build: impl FnMut(Scope) -> View + 'static) -> Disclosure {
        self.body = Body::Build(Box::new(build));
        self
    }

    /// Trailing muted slot on the title row (timestamps, "+N lines",
    /// status words). Renders whole, right-aligned — or not at all
    /// when the row is too tight (the title always wins the space;
    /// it truncates, the detail drops).
    pub fn detail(mut self, detail: impl Into<String>) -> Disclosure {
        self.detail = Some(detail.into());
        self
    }

    /// Uncontrolled initial state (default `true` = FOLDED: progressive
    /// disclosure — the header is the summary, opening is the user's
    /// act). Ignored when [`Disclosure::folded`] binds a signal.
    pub fn initially_folded(mut self, folded: bool) -> Disclosure {
        self.initially_folded = folded;
        self
    }

    /// Controlled mode: bind the fold state to an app-owned signal
    /// (two-way — toggling writes it, writing it re-renders the card).
    /// The signal's current value IS the state; `initially_folded` is
    /// ignored. This is the 0850 policy hook: keep fold state in a
    /// `Signal<bool>` per card (or a keyed map feeding them) and a
    /// "collapse all" is a loop of writes.
    pub fn folded(mut self, folded: Signal<bool>) -> Disclosure {
        self.folded = Some(folded);
        self
    }

    /// Cap the unfolded body at `rows` cell rows (default 8). Shorter
    /// content takes its natural height ("limited to", not "padded
    /// to"); taller content scrolls inside the capped region with a
    /// visible scrollbar. `rows <= 0` removes the cap entirely — the
    /// body takes its natural height with no scroll region.
    pub fn max_body_rows(mut self, rows: i32) -> Disclosure {
        self.max_body_rows = rows;
        self
    }

    /// Toggle notification: `f(folded_now)` — the NEW state, `true` =
    /// the card just folded. Runs AFTER the state write (a controlled
    /// signal already holds the new value), so the callback may
    /// dispose the card's scope synchronously (backlog 0297).
    pub fn on_toggle(mut self, f: impl FnMut(bool) + 'static) -> Disclosure {
        self.on_toggle = Some(Box::new(f));
        self
    }

    /// Layout for the card root (a COLUMN: header row + body region).
    /// Default: full width, natural height, `shrink(0.0)` — a card in
    /// a tight column overflows honestly instead of being crushed
    /// (the 0240 rule).
    pub fn layout(mut self, layout: LayoutStyle) -> Disclosure {
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
        let tokens = *t;
        let text_fg = t.text;
        let muted = t.text_muted;
        let accent = t.accent;
        let raised = t.surface_raised;
        let ground = t.surface;
        let sel_fg = t.selection_fg;
        let sel_bg = t.selection_bg;

        let title = self.title;
        let detail = self.detail;
        let folded = self
            .folded
            .unwrap_or_else(|| cx.signal(self.initially_folded));
        let hovered = cx.signal(false);
        let focused = cx.signal(false);
        let on_toggle: crate::widgets::SharedCallback<bool> = Rc::new(RefCell::new(self.on_toggle));

        // State write BEFORE the callback (the 0297 disposal law).
        let toggle = move || {
            let now = !folded.get_untracked();
            folded.set(now);
            if let Some(f) = on_toggle.borrow_mut().as_mut() {
                f(now);
            }
        };
        let handler = move |ctx: &mut EventCtx, ev: &UiEvent| match ev {
            UiEvent::Key(k) if k.key == Key::Enter || k.key == Key::Char(' ') => {
                if focused.get_untracked() {
                    toggle();
                    ctx.stop_propagation();
                }
            }
            UiEvent::Mouse(m) if matches!(m.kind, MouseKind::Down(MouseButton::Left)) => {
                toggle();
                ctx.stop_propagation();
            }
            _ => {}
        };

        // --- title row ----------------------------------------------------
        let title_for_dyn = title.clone();
        let header = Element::new()
            .style(
                LayoutStyle::default()
                    .width(Dimension::Percent(1.0))
                    .height(Dimension::Cells(1))
                    .shrink(0.0),
            )
            // Role vocabulary: the enum is frozen until 0.3, so the
            // toggle reports `Button` + its state as value (the Select
            // precedent recorded in access.rs).
            .role(crate::ui::Role::Button)
            .access_label(title.clone())
            .access_value(move || {
                if folded.get_untracked() {
                    "collapsed"
                } else {
                    "expanded"
                }
                .into()
            })
            .focusable()
            .hover_signal(hovered)
            .focus_signal(focused)
            .on(Phase::Bubble, handler)
            .child(dyn_view(
                LayoutStyle::default()
                    .width(Dimension::Percent(1.0))
                    .height(Dimension::Cells(1)),
                move || {
                    let is_folded = folded.get();
                    let hover = hovered.get();
                    let focus = focused.get();
                    let title = title_for_dyn.clone();
                    let detail = detail.clone();
                    Element::new()
                        .style(
                            LayoutStyle::default()
                                .width(Dimension::Percent(1.0))
                                .height(Dimension::Cells(1)),
                        )
                        .draw(move |canvas, rect| {
                            if rect.is_empty() {
                                return;
                            }
                            draw_header(
                                canvas,
                                rect,
                                &title,
                                detail.as_deref(),
                                is_folded,
                                hover,
                                focus,
                                HeaderInks {
                                    text_fg,
                                    muted,
                                    accent,
                                    raised,
                                    sel_fg,
                                    sel_bg,
                                },
                            );
                        })
                        .build()
                },
            ));

        // --- body region ----------------------------------------------------
        // Text/markdown bodies typeset in a one-item feed created ONCE
        // on the mount scope: segments and the discovered width survive
        // fold cycles, so a re-expand costs no re-typeset.
        let mut make_body: Option<BodyBuilder> = match self.body {
            Body::None => None,
            Body::Text(s) => {
                let fs = FeedState::new(cx);
                fs.push("body", FeedItem::text(s));
                Some(Box::new(move |gcx: Scope| {
                    Feed::new(&fs).gap(0).element(gcx, &tokens).build()
                }))
            }
            Body::Markdown(s) => {
                let fs = FeedState::new(cx);
                fs.push("body", FeedItem::markdown(s));
                Some(Box::new(move |gcx: Scope| {
                    Feed::new(&fs).gap(0).element(gcx, &tokens).build()
                }))
            }
            Body::Build(f) => Some(f),
        };

        let cap = self.max_body_rows;
        // The measured body extent, durable across fold cycles (a
        // re-expand warm-starts at its last height instead of the cap).
        let extent: Signal<(i32, i32)> = cx.signal((0, 0));
        let body_region = make_body.take().map(|mut build| {
            dyn_view_scoped(
                LayoutStyle::default()
                    .width(Dimension::Percent(1.0))
                    .shrink(0.0),
                move |gcx| {
                    if folded.get() {
                        // Folded = UNMOUNTED: an empty auto-sized element
                        // measures zero rows, draws nothing, handles
                        // nothing. Unfolding rebuilds from `build`.
                        return Element::new().build();
                    }
                    let body = build(gcx);
                    let fill = Style::new().fg(text_fg).bg(ground);
                    if cap <= 0 {
                        // Uncapped: natural height, no scroll region.
                        return Element::new()
                            .style(
                                LayoutStyle::column()
                                    .width(Dimension::Percent(1.0))
                                    .padding(Edges::hv(1, 0))
                                    .shrink(0.0),
                            )
                            .draw(move |canvas, rect| {
                                canvas.fill_styled(rect, ' ', &fill);
                            })
                            .child(body)
                            .build();
                    }
                    // Capped: the region is min(measured, cap) rows —
                    // "limited to", never "padded to". Before the first
                    // measurement ((0,0)) it opens AT the cap and
                    // settles down one turn later for short content
                    // (opening short avoids clipping tall bodies, the
                    // common case under a cap).
                    let scroll = Scroll::new(body)
                        .extent_signal(extent)
                        .scrollbar_auto_hide(true)
                        .element(gcx, &tokens)
                        .build();
                    Element::new()
                        .style_signal(move || {
                            let (mw, mh) = extent.get();
                            // ONLY (0, 0) is the unmeasured sentinel
                            // (the same reading as Scroll's offset
                            // repair): a measured zero-row body is
                            // real and takes the 1-row floor below —
                            // "limited to", never "padded to" cap
                            // rows of blank.
                            let shown = if mw == 0 && mh == 0 { cap } else { mh.min(cap) };
                            LayoutStyle::column()
                                .width(Dimension::Percent(1.0))
                                .height(Dimension::Cells(shown.max(1)))
                                .padding(Edges::hv(1, 0))
                                .shrink(0.0)
                        })
                        .draw(move |canvas, rect| {
                            canvas.fill_styled(rect, ' ', &fill);
                        })
                        .child(scroll)
                        .build()
                },
            )
        });

        // --- the card -------------------------------------------------------
        let layout = self.layout.unwrap_or_else(|| {
            LayoutStyle::column()
                .width(Dimension::Percent(1.0))
                .shrink(0.0)
        });
        let mut root = Element::new()
            .style(layout)
            // The ARIA disclosure pair: a region labeled by the title
            // wrapping the button that controls it.
            .role(crate::ui::Role::Region)
            .access_label(title)
            .child(header.build());
        if let Some(region) = body_region {
            root = root.child(region);
        }
        root
    }
}

/// Resolved header inks (damage contract §5: plain `Rgba` into the
/// draw closure). One struct so `draw_header` stays under clippy's
/// argument limit without losing names.
#[derive(Copy, Clone)]
struct HeaderInks {
    text_fg: crate::base::Rgba,
    muted: crate::base::Rgba,
    accent: crate::base::Rgba,
    raised: crate::base::Rgba,
    sel_fg: crate::base::Rgba,
    sel_bg: crate::base::Rgba,
}

/// Paint the title row: `[pad][glyph] [title…] … [detail][pad]`.
/// Focus wears the selection pair over the whole row (§3.2); hover is
/// accent garnish on the title; the glyph keeps `accent` ink. The
/// title truncates with an ellipsis; the detail renders whole and
/// right-aligned, or drops entirely when fewer than 4 title cells
/// would remain (the title always wins).
#[allow(clippy::too_many_arguments)]
fn draw_header(
    canvas: &mut dyn crate::ui::StyledCanvas,
    rect: crate::base::Rect,
    title: &str,
    detail: Option<&str>,
    folded: bool,
    hover: bool,
    focus: bool,
    inks: HeaderInks,
) {
    let bg = if focus { inks.sel_bg } else { inks.raised };
    let base_fg = if focus { inks.sel_fg } else { inks.text_fg };
    let base = Style::new().fg(base_fg).bg(bg);
    canvas.fill_styled(rect, ' ', &base);

    let glyph = if folded { "▸" } else { "▾" };
    let glyph_ink = if focus { inks.sel_fg } else { inks.accent };
    if rect.w >= 2 {
        // The glyph sits at x+1 (one-cell left pad): a 1-cell rect has
        // no room for it, and the canvas is NOT clipped to the element
        // (damage contract §5) — printing anyway would paint the
        // neighbor's cell.
        canvas.print_styled(
            crate::base::Point::new(rect.x + 1, rect.y),
            glyph,
            &Style::new().fg(glyph_ink).bg(bg),
        );
    }

    let title_x = rect.x + 1 + crate::text::width(glyph) + 1;
    let right = rect.right() - 1; // one-cell right pad
    let mut title_max = right - title_x;
    let mut detail_at: Option<(i32, &str)> = None;
    if let Some(d) = detail {
        let dw = crate::text::width(d);
        let remaining = right - title_x - dw - 1; // one-cell gap
        if remaining >= 4 {
            title_max = remaining;
            detail_at = Some((right - dw, d));
        }
    }
    let title_ink = if focus {
        inks.sel_fg
    } else if hover {
        inks.accent
    } else {
        inks.text_fg
    };
    let shown = crate::text::truncate_ellipsis(title, title_max.max(0));
    canvas.print_styled(
        crate::base::Point::new(title_x, rect.y),
        &shown,
        &Style::new().fg(title_ink).bg(bg),
    );
    if let Some((dx, d)) = detail_at {
        let detail_ink = if focus { inks.sel_fg } else { inks.muted };
        canvas.print_styled(
            crate::base::Point::new(dx, rect.y),
            d,
            &Style::new().fg(detail_ink).bg(bg),
        );
    }
}

#[cfg(test)]
#[path = "disclosure_tests.rs"]
mod tests;
