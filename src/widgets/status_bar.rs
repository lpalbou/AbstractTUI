//! StatusBar: one row of app chrome with three NAMED SLOTS — `left`,
//! `center`, `right` — each holding ordered items, with truncation
//! priority and overflow rules (backlog app-kits 1260). The same widget
//! serves as a header or a bottom status bar; the app never draws.
//!
//! ```ignore
//! use abstracttui::prelude::*;
//! let bar = StatusBar::live(
//!     move || {
//!         StatusBar::new()
//!             .left(BarItem::text("▲ App").ink(t.accent).priority(10))
//!             .left(BarItem::text(workflow.get()).action("pick-workflow").fit(BarFit::TruncateEnd { min: 8 }))
//!             .right(BarItem::new().run("● ", t.ok).run(url.get(), t.text_muted).action("gateway"))
//!     },
//!     move |_ctx, id| match id.as_str() {
//!         "pick-workflow" => open_workflow_picker(),
//!         "gateway" => open_gateway_modal(),
//!         _ => {}
//!     },
//! );
//! ```
//!
//! ## Layout and overflow
//!
//! - Left items sit flush from `padding`; right items flush against
//!   `width - padding`; center items are centered on the bar and slide
//!   sideways to keep `slot_gap` blank cells from the other slots.
//!   Inside a slot, items keep declaration order, joined by `separator`.
//! - When the row is too narrow, items YIELD in ascending `priority`
//!   (ties: the later-declared first, slots in order left, center,
//!   right). A yielding item first truncates with `…` down to its `min`
//!   (only if its fit allows), then drops WHOLE. Whole items never
//!   fragment.
//!
//! One pure plan ([`plan_slots`]) drives both the paint and the hit
//! targets: every item with an [`BarItem::action`] is tagged through
//! [`StyledCanvas::print_action`](crate::ui::StyledCanvas::print_action),
//! so an [`Element::on_action`] host (which [`StatusBar::live`] is)
//! receives clicks and keyboard activations by id.
//!
//! Tokens: ground `surface`, default ink `text_muted`, separator
//! `text_faint`.
//!
//! OWNER: DESIGN.

use crate::base::{Point, Rgba};
use crate::layout::{Dimension, Style as LayoutStyle};
use crate::render::Style;
use crate::text;
use crate::theme::TokenSet;
use crate::ui::{dyn_view_scoped, ActionId, Element, EventCtx, View};

/// Which named slot an item goes in.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Slot {
    Left,
    Center,
    Right,
}

/// How an item yields width under pressure.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Default)]
pub enum BarFit {
    /// Shown whole or not at all (the default: instruments never
    /// self-truncate into fragments).
    #[default]
    Whole,
    /// Keep the HEAD, `…` at the end, down to `min` cells.
    TruncateEnd { min: i32 },
    /// Keep the TAIL, `…` at the start, down to `min` cells (paths,
    /// ids — the part that differs is the end).
    TruncateStart { min: i32 },
}

/// One widget in a slot: styled runs, a priority, a fit rule and an
/// optional action id (the whole item is the hit target).
#[derive(Clone, Debug, Default)]
pub struct BarItem {
    runs: Vec<(String, Option<Style>)>,
    ink: Option<Rgba>,
    priority: i32,
    fit: BarFit,
    action: Option<ActionId>,
}

impl BarItem {
    /// An empty item; add runs with [`BarItem::run`].
    pub fn new() -> BarItem {
        BarItem::default()
    }

    /// One run in the item's default ink ([`BarItem::ink`], else the
    /// theme's `text_muted`).
    pub fn text(s: impl Into<String>) -> BarItem {
        BarItem {
            runs: vec![(s.into(), None)],
            ..BarItem::default()
        }
    }

    /// Append a run in `ink`.
    pub fn run(self, text: impl Into<String>, ink: Rgba) -> BarItem {
        self.run_styled(text, Style::new().fg(ink))
    }

    /// Append a run with a full paint style (bold, underline, …).
    pub fn run_styled(mut self, text: impl Into<String>, style: Style) -> BarItem {
        self.runs.push((text.into(), Some(style)));
        self
    }

    /// Ink for runs added without one ([`BarItem::text`]).
    pub fn ink(mut self, ink: Rgba) -> BarItem {
        self.ink = Some(ink);
        self
    }

    /// Overflow priority: HIGHER survives longer. Default 0.
    pub fn priority(mut self, p: i32) -> BarItem {
        self.priority = p;
        self
    }

    pub fn fit(mut self, fit: BarFit) -> BarItem {
        self.fit = fit;
        self
    }

    /// Make the whole item a hit target with this id.
    pub fn action(mut self, id: impl Into<ActionId>) -> BarItem {
        self.action = Some(id.into());
        self
    }

    /// Natural width in cells.
    pub fn width(&self) -> i32 {
        self.runs.iter().map(|(s, _)| text::width(s)).sum()
    }

    fn measure(&self) -> SlotMeasure {
        let width = self.width();
        let min = match self.fit {
            BarFit::Whole => width,
            BarFit::TruncateEnd { min } | BarFit::TruncateStart { min } => {
                min.clamp(1, width.max(1))
            }
        };
        SlotMeasure {
            width,
            min: min.min(width),
            priority: self.priority,
        }
    }
}

/// The input of [`plan_slots`]: one item's natural width, the narrowest
/// it may truncate to (`== width` for whole-only items) and its
/// priority (higher survives longer).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct SlotMeasure {
    pub width: i32,
    pub min: i32,
    pub priority: i32,
}

/// Where a planned item lands: column and granted width.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Placement {
    pub x: i32,
    pub w: i32,
}

/// The shared pure slot-layout helper (status bar today, the 0560
/// header next). Returns, per slot `[left, center, right]` and per item,
/// `Some(placement)` or `None` when the item was dropped. See the module
/// docs for the rules; `sep` is the separator width between items in a
/// slot, `slot_gap` the minimum blank between slots.
///
/// ```
/// use abstracttui::widgets::status_bar::{plan_slots, Placement, SlotMeasure};
/// let item = |width, priority| SlotMeasure { width, min: width, priority };
/// let left = [item(5, 1)];
/// let right = [item(4, 0), item(3, 2)];
/// let plan = plan_slots(20, 1, 1, 2, [&left, &[], &right]);
/// assert_eq!(plan[0], vec![Some(Placement { x: 1, w: 5 })]);
/// assert_eq!(plan[2], vec![Some(Placement { x: 11, w: 4 }), Some(Placement { x: 16, w: 3 })]);
/// // Too narrow: the lowest priority (right's first item) drops whole.
/// let plan = plan_slots(12, 1, 1, 2, [&left, &[], &right]);
/// assert_eq!(plan[2][0], None);
/// ```
pub fn plan_slots(
    width: i32,
    padding: i32,
    sep: i32,
    slot_gap: i32,
    slots: [&[SlotMeasure]; 3],
) -> [Vec<Option<Placement>>; 3] {
    // Granted width per item (None = dropped). Zero-width items are
    // dropped up front: they would only cost separators.
    let mut grant: [Vec<Option<i32>>; 3] = [0, 1, 2].map(|s| {
        slots[s]
            .iter()
            .map(|m| (m.width > 0).then_some(m.width))
            .collect()
    });
    let need = |grant: &[Vec<Option<i32>>; 3]| -> i32 {
        let mut total = 2 * padding.max(0);
        let mut nonempty = 0;
        for g in grant.iter() {
            let shown: Vec<i32> = g.iter().flatten().copied().collect();
            if shown.is_empty() {
                continue;
            }
            nonempty += 1;
            total += shown.iter().sum::<i32>() + sep * (shown.len() as i32 - 1);
        }
        total + slot_gap * (nonempty - 1).max(0)
    };
    // Yield order: ascending priority; ties: later-declared first, with
    // slots ordered left, center, right (so (slot, index) descending).
    let mut order: Vec<(usize, usize)> = (0..3)
        .flat_map(|s| (0..slots[s].len()).map(move |i| (s, i)))
        .collect();
    order.sort_by(|a, b| {
        slots[a.0][a.1]
            .priority
            .cmp(&slots[b.0][b.1].priority)
            .then(b.cmp(a))
    });
    for (s, i) in order {
        let over = need(&grant) - width;
        if over <= 0 {
            break;
        }
        let Some(w) = grant[s][i] else { continue };
        let min = slots[s][i].min;
        let shrunk = (w - over).max(min);
        if shrunk < w && w - over >= min {
            grant[s][i] = Some(shrunk);
        } else {
            grant[s][i] = None;
        }
    }
    let total = |g: &Vec<Option<i32>>| -> i32 {
        let shown: Vec<i32> = g.iter().flatten().copied().collect();
        if shown.is_empty() {
            0
        } else {
            shown.iter().sum::<i32>() + sep * (shown.len() as i32 - 1)
        }
    };
    let place = |g: &Vec<Option<i32>>, mut x: i32| -> Vec<Option<Placement>> {
        g.iter()
            .map(|w| {
                w.map(|w| {
                    let p = Placement { x, w };
                    x += w + sep;
                    p
                })
            })
            .collect()
    };
    let (lw, cw, rw) = (total(&grant[0]), total(&grant[1]), total(&grant[2]));
    let left_end = padding + lw;
    let right_start = width - padding - rw;
    let lo = if lw > 0 { left_end + slot_gap } else { padding };
    let hi = if rw > 0 {
        right_start - slot_gap
    } else {
        width - padding
    };
    let cx = ((width - cw) / 2).min(hi - cw).max(lo);
    [
        place(&grant[0], padding),
        place(&grant[1], cx),
        place(&grant[2], right_start),
    ]
}

/// Fit an item's runs into `w` cells (already planned), honoring its
/// fit rule. Returns the runs to print.
fn fitted_runs(item: &BarItem, w: i32) -> Vec<(String, Option<Style>)> {
    if item.width() <= w {
        return item.runs.clone();
    }
    let keep_tail = matches!(item.fit, BarFit::TruncateStart { .. });
    let mut budget = (w - 1).max(0);
    let mut out: Vec<(String, Option<Style>)> = Vec::new();
    let runs: Vec<&(String, Option<Style>)> = if keep_tail {
        item.runs.iter().rev().collect()
    } else {
        item.runs.iter().collect()
    };
    let ellipsis_style = if keep_tail {
        runs.first().and_then(|r| r.1)
    } else {
        None
    };
    for (s, st) in runs {
        if budget <= 0 {
            break;
        }
        let rw = text::width(s);
        if rw <= budget {
            out.push((s.clone(), *st));
            budget -= rw;
        } else {
            let part = if keep_tail {
                text_tail(s, budget)
            } else {
                text_head(s, budget)
            };
            out.push((part, *st));
            break;
        }
    }
    if keep_tail {
        out.reverse();
        out.insert(
            0,
            (
                "…".to_string(),
                ellipsis_style.or(out.first().and_then(|r| r.1)),
            ),
        );
    } else {
        let st = out.last().and_then(|r| r.1);
        out.push(("…".to_string(), st));
    }
    out
}

fn text_head(s: &str, cols: i32) -> String {
    let mut out = String::new();
    let mut used = 0;
    for ch in s.chars() {
        let w = text::width(ch.encode_utf8(&mut [0u8; 4]));
        if used + w > cols {
            break;
        }
        used += w;
        out.push(ch);
    }
    out
}

fn text_tail(s: &str, cols: i32) -> String {
    let mut kept: Vec<char> = Vec::new();
    let mut used = 0;
    for ch in s.chars().rev() {
        let w = text::width(ch.encode_utf8(&mut [0u8; 4]));
        if used + w > cols {
            break;
        }
        used += w;
        kept.push(ch);
    }
    kept.into_iter().rev().collect()
}

/// A one-row bar of named slots. Build it, then [`StatusBar::element`]
/// (static) or [`StatusBar::live`] (the reactive, focusable app-chrome
/// door).
#[derive(Clone, Debug)]
pub struct StatusBar {
    slots: [Vec<BarItem>; 3],
    separator: String,
    slot_gap: i32,
    padding: i32,
    ground: Option<Rgba>,
}

impl Default for StatusBar {
    fn default() -> Self {
        StatusBar::new()
    }
}

impl StatusBar {
    pub fn new() -> StatusBar {
        StatusBar {
            slots: [Vec::new(), Vec::new(), Vec::new()],
            separator: "  ".to_string(),
            slot_gap: 2,
            padding: 1,
            ground: None,
        }
    }

    /// Append an item to the left slot (slot order = declaration order).
    pub fn left(self, item: BarItem) -> StatusBar {
        self.item(Slot::Left, item)
    }

    pub fn center(self, item: BarItem) -> StatusBar {
        self.item(Slot::Center, item)
    }

    pub fn right(self, item: BarItem) -> StatusBar {
        self.item(Slot::Right, item)
    }

    pub fn item(mut self, slot: Slot, item: BarItem) -> StatusBar {
        let ix = match slot {
            Slot::Left => 0,
            Slot::Center => 1,
            Slot::Right => 2,
        };
        self.slots[ix].push(item);
        self
    }

    /// Drawn between items inside a slot (default two blanks).
    pub fn separator(mut self, sep: impl Into<String>) -> StatusBar {
        self.separator = sep.into();
        self
    }

    /// Minimum blank cells between slots (default 2).
    pub fn slot_gap(mut self, cells: i32) -> StatusBar {
        self.slot_gap = cells.max(0);
        self
    }

    /// Left/right inset (default 1).
    pub fn padding(mut self, cells: i32) -> StatusBar {
        self.padding = cells.max(0);
        self
    }

    /// Ground colour (default the theme's `surface`).
    pub fn ground(mut self, ground: Rgba) -> StatusBar {
        self.ground = Some(ground);
        self
    }

    /// The plan for a row `width` cells wide (what [`StatusBar::element`]
    /// paints): per slot, per item, `Some(placement)` or `None`.
    pub fn plan(&self, width: i32) -> [Vec<Option<Placement>>; 3] {
        let measures: [Vec<SlotMeasure>; 3] =
            [0, 1, 2].map(|s| self.slots[s].iter().map(BarItem::measure).collect());
        plan_slots(
            width,
            self.padding,
            text::width(&self.separator),
            self.slot_gap,
            [&measures[0], &measures[1], &measures[2]],
        )
    }

    /// A static one-row, full-width draw element. Items with an action
    /// are tagged; wrap the element in an [`Element::on_action`] host (or
    /// use [`StatusBar::live`]) to receive them.
    pub fn element(self, t: &TokenSet) -> Element {
        let ground = self.ground.unwrap_or(t.surface);
        let muted = t.text_muted;
        let faint = t.text_faint;
        Element::new()
            .style(
                LayoutStyle::default()
                    .width(Dimension::Percent(1.0))
                    .height(Dimension::Cells(1))
                    .shrink(0.0),
            )
            .draw(move |canvas, rect| {
                if rect.w <= 0 || rect.h <= 0 {
                    return;
                }
                canvas.fill(rect, ' ', muted, ground);
                let plan = self.plan(rect.w);
                let y = rect.y;
                let sep_style = Style::new().fg(faint).bg(ground);
                for (s, items) in self.slots.iter().enumerate() {
                    let mut prev_end: Option<i32> = None;
                    for (item, place) in items.iter().zip(plan[s].iter()) {
                        let Some(place) = place else { continue };
                        let x0 = rect.x + place.x;
                        if let Some(end) = prev_end {
                            canvas.print_styled(Point::new(end, y), &self.separator, &sep_style);
                        }
                        let default = Style::new().fg(item.ink.unwrap_or(muted));
                        let mut x = x0;
                        for (run, st) in fitted_runs(item, place.w) {
                            let style = st.unwrap_or(default).bg(ground);
                            x += match &item.action {
                                Some(id) => canvas.print_action(Point::new(x, y), &run, &style, id),
                                None => canvas.print_styled(Point::new(x, y), &run, &style),
                            };
                        }
                        prev_end = Some(x0 + place.w);
                    }
                }
            })
    }

    /// The app-chrome door: a STABLE, focusable action host (keyboard
    /// focus survives content changes) around a reactive region that
    /// re-runs `build` whenever the signals it reads change — and on a
    /// theme switch. `on_action` receives clicks and keyboard
    /// activations (Tab to the bar, Left/Right, Enter/Space) of every
    /// item built with [`BarItem::action`].
    ///
    /// For a mouse-only bar, compose the same thing without
    /// `.focusable()`: `Element::new().on_action(f).child(dyn_view(..,
    /// move || bar().element(&t).build()))`.
    pub fn live(
        mut build: impl FnMut() -> StatusBar + 'static,
        on_action: impl FnMut(&mut EventCtx, &ActionId) + 'static,
    ) -> View {
        let line = || {
            LayoutStyle::default()
                .width(Dimension::Percent(1.0))
                .height(Dimension::Cells(1))
                .shrink(0.0)
        };
        Element::new()
            .style(line())
            .focusable()
            .on_action(on_action)
            .child(dyn_view_scoped(line(), move |cx| {
                let t = crate::widgets::theme_tokens(cx);
                build().element(&t).build()
            }))
            .build()
    }
}

#[cfg(test)]
#[path = "status_bar_tests.rs"]
mod tests;
