//! Table: columns with fixed/percent/flex widths, a styled header,
//! virtualized rows, keyboard/mouse row selection, and a sort-indicator
//! HOOK — sorting itself is the app's job: a header click fires
//! `on_sort_requested(col)`; the app reorders its data and passes the
//! resulting `sorted: (col, ascending)` back for the indicator.
//!
//! SELECTION vs ACTIVATION (the 0250 ruling, Table edition — app-kits
//! 0535): selection FOLLOWS MOVEMENT (arrows/Page/Home/End/click;
//! `on_select` is the notification); activation is the explicit "open
//! this row" event — `on_activate` fires on Enter (always), on Space
//! (single-select Table has no toggle meaning, so Space aliases Enter —
//! the PLATFORM F5 rule; 0530's multi-select mode will claim Space as
//! toggle WITHIN that mode), and on DOUBLE-CLICK: the second press of a
//! click chain landing on the already-selected row (`EventCtx::
//! click_count() >= 2`). Deliberately UNLIKE `List`, a slow second
//! click on the selected row does NOT activate — a table is a browsing
//! surface (re-clicking a row to focus the pane must never open its
//! editor); a List is a picker, where click-the-selected-row-to-commit
//! is the shipped 0250 gesture. Both presses of a double-click deliver
//! normally: click 1 selects, click 2 activates — selection is never
//! suppressed waiting for a second click.
//!
//! Same v1 data model as `List` (snapshot rows, virtualized painting;
//! wrap in a `Dyn` on your data signal to change them) — the scaling
//! note there applies.
//!
//! OWNER: REACT.

use std::cell::RefCell;
use std::rc::Rc;

use crate::layout::Dimension;
use crate::layout::{distribute, Style as LayoutStyle};
use crate::reactive::{Scope, Signal};
use crate::render::{Attrs, Style};
use crate::theme::TokenSet;
use crate::ui::{dyn_view, Element, EventCtx, Key, MouseButton, MouseKind, Phase, UiEvent};

use super::list::draw_scrollbar;

#[derive(Copy, Clone, Debug)]
pub enum ColWidth {
    Cells(i32),
    /// Fraction of the table width, `0.0..=1.0`.
    Percent(f32),
    /// Share of the space left after fixed/percent columns.
    Flex(f32),
}

pub struct Column {
    pub title: String,
    pub width: ColWidth,
}

impl Column {
    pub fn new(title: impl Into<String>, width: ColWidth) -> Column {
        Column {
            title: title.into(),
            width,
        }
    }
}

pub struct Table {
    columns: Vec<Column>,
    rows: Vec<Vec<String>>,
    selection: Option<Signal<usize>>,
    focused: Option<Signal<bool>>,
    sorted: Option<(usize, bool)>,
    layout: Option<LayoutStyle>,
    on_select: Option<Box<dyn FnMut(usize)>>,
    on_activate: Option<Box<dyn FnMut(usize)>>,
    on_sort_requested: Option<Box<dyn FnMut(usize)>>,
}

impl Table {
    pub fn new(columns: Vec<Column>) -> Table {
        Table {
            columns,
            rows: Vec::new(),
            selection: None,
            focused: None,
            sorted: None,
            layout: None,
            on_select: None,
            on_activate: None,
            on_sort_requested: None,
        }
    }

    pub fn rows(mut self, rows: Vec<Vec<String>>) -> Table {
        self.rows = rows;
        self
    }

    pub fn selection(mut self, selection: Signal<usize>) -> Table {
        self.selection = Some(selection);
        self
    }

    /// Which column the DATA is currently sorted by (and ascending?) —
    /// drawn as the header indicator. The app owns the actual ordering.
    pub fn sorted(mut self, col: usize, ascending: bool) -> Table {
        self.sorted = Some((col, ascending));
        self
    }

    pub fn layout(mut self, layout: LayoutStyle) -> Table {
        self.layout = Some(layout);
        self
    }

    pub fn on_select(mut self, f: impl FnMut(usize) + 'static) -> Table {
        self.on_select = Some(Box::new(f));
        self
    }

    /// ACTIVATION: the user committed the selected row (the 0250 ruling
    /// applied to Table — "open this row"). Fires with the current index
    /// on Enter, on Space (no toggle meaning in a single-select table),
    /// and on DOUBLE-CLICK — the second press of a click chain on the
    /// already-selected row (see the module docs; a single click, or a
    /// slow second click, only selects). When unbound, Enter and Space
    /// pass through to app shortcuts exactly as before this event
    /// existed. The callback runs after all Table bookkeeping, so it may
    /// dispose the Table's scope synchronously (open-the-editor-modal is
    /// the intended use).
    pub fn on_activate(mut self, f: impl FnMut(usize) + 'static) -> Table {
        self.on_activate = Some(Box::new(f));
        self
    }

    /// Bind an external focus signal (D4-2): true while the table holds
    /// keyboard focus — pane strokes wire to it (§3.2).
    pub fn focus_signal(mut self, focused: Signal<bool>) -> Table {
        self.focused = Some(focused);
        self
    }

    pub fn on_sort_requested(mut self, f: impl FnMut(usize) + 'static) -> Table {
        self.on_sort_requested = Some(Box::new(f));
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
        let text_fg = t.text;
        // D4-1: `accent_alt` on `surface_raised` measured ~2.1:1 on nord;
        // `text_muted` headers with `text` on the SORTED column stay
        // inside the audited vocabulary at readable contrast.
        let header_fg = t.text_muted;
        let header_sorted_fg = t.text;
        let ground = t.surface;
        let header_ground = t.surface_raised;
        let sel_bg = t.selection_bg;
        let sel_fg = t.selection_fg;
        let track = t.border;
        let thumb = t.text_muted;

        let widths: Vec<ColWidth> = self.columns.iter().map(|c| c.width).collect();
        let titles: Rc<Vec<String>> =
            Rc::new(self.columns.iter().map(|c| c.title.clone()).collect());
        let rows = Rc::new(self.rows);
        let len = rows.len();
        let sorted = self.sorted;

        let selection = self.selection.unwrap_or_else(|| cx.signal(0usize));
        let offset = cx.signal(0i32);
        let on_select: crate::widgets::SharedCallback<usize> =
            Rc::new(RefCell::new(self.on_select));
        let on_activate: crate::widgets::SharedCallback<usize> =
            Rc::new(RefCell::new(self.on_activate));
        let on_sort: crate::widgets::SharedCallback<usize> =
            Rc::new(RefCell::new(self.on_sort_requested));
        let layout = self
            .layout
            .unwrap_or_else(|| LayoutStyle::default().grow(1.0));

        let select = {
            let on_select = on_select.clone();
            move |target: usize, body_h: i32| {
                let target = target.min(len.saturating_sub(1));
                let changed = selection.get_untracked() != target;
                if changed {
                    selection.set(target);
                }
                // Ensure-visible BEFORE the user callback (0250 ruling
                // clause 4, disposal-safety law — same inversion as
                // List): an on_select that disposes the Table's scope
                // must find no widget code left to run on dead signals.
                let sel = target as i32;
                offset.update(|o| {
                    if sel < *o {
                        *o = sel;
                    }
                    if body_h > 0 && sel >= *o + body_h {
                        *o = sel - body_h + 1;
                    }
                    *o = (*o).clamp(0, (len as i32 - body_h.max(1)).max(0));
                });
                if changed {
                    if let Some(f) = on_select.borrow_mut().as_mut() {
                        f(target);
                    }
                }
            }
        };

        let handler_widths = widths.clone();
        let activate = on_activate;
        let handler = move |ctx: &mut EventCtx, ev: &UiEvent| {
            let rect = ctx.current_rect();
            let body_h = (rect.h - 1).max(1); // header takes row 0
            match ev {
                UiEvent::Key(k) => {
                    // Activation keys (module docs): Enter always;
                    // Space aliases it while Table is single-select
                    // (F5 — no toggle meaning exists to claim it).
                    // Consumed ONLY when a callback is bound: an
                    // activation-less Table leaves Enter/Space to the
                    // app's own shortcuts (the 0980 lesson — never
                    // claim a key without a consumer).
                    if matches!(k.key, Key::Enter | Key::Char(' ')) {
                        if len > 0 {
                            if let Some(f) = activate.borrow_mut().as_mut() {
                                f(selection.get_untracked().min(len - 1));
                                ctx.stop_propagation();
                            }
                        }
                        return;
                    }
                    // Keyboard sort parity (a11y audit, cycle 6):
                    // 's' requests sorting the NEXT column round-robin
                    // (start after the currently sorted one) — the same
                    // on_sort_requested contract a header click fires;
                    // the app's `sorted` prop moves the indicator.
                    if k.key == Key::Char('s') {
                        let ncols = handler_widths.len();
                        if ncols > 0 {
                            let next = match sorted {
                                Some((col, _)) => (col + 1) % ncols,
                                None => 0,
                            };
                            if let Some(f) = on_sort.borrow_mut().as_mut() {
                                f(next);
                            }
                            ctx.stop_propagation();
                        }
                        return;
                    }
                    let cur = selection.get_untracked();
                    let target = match k.key {
                        Key::Up => cur.saturating_sub(1),
                        Key::Down => cur + 1,
                        Key::PageUp => cur.saturating_sub(body_h as usize),
                        Key::PageDown => cur + body_h as usize,
                        Key::Home => 0,
                        Key::End => len.saturating_sub(1),
                        _ => return,
                    };
                    select(target, body_h);
                    ctx.stop_propagation();
                }
                UiEvent::Mouse(m) => match m.kind {
                    MouseKind::ScrollUp | MouseKind::ScrollDown => {
                        let delta = if m.kind == MouseKind::ScrollUp { -3 } else { 3 };
                        offset.update(|o| {
                            *o = (*o + delta).clamp(0, (len as i32 - body_h).max(0));
                        });
                        ctx.stop_propagation();
                    }
                    MouseKind::Down(MouseButton::Left) => {
                        if m.pos.y == rect.y {
                            // Header click: which column? -> sort hook.
                            let cols = solve_columns(&handler_widths, rect.w - 1);
                            let mut x = rect.x;
                            for (i, w) in cols.iter().enumerate() {
                                if m.pos.x >= x && m.pos.x < x + w {
                                    if let Some(f) = on_sort.borrow_mut().as_mut() {
                                        f(i);
                                    }
                                    break;
                                }
                                x += w + 1;
                            }
                        } else {
                            let row = (m.pos.y - rect.y - 1) + offset.get_untracked();
                            if row >= 0 && (row as usize) < len {
                                let idx = row as usize;
                                // Single-click SELECTS; the second press
                                // of a double-click (click_count ≥ 2)
                                // ACTIVATES — gated on the row being the
                                // ALREADY-selected one, which is the row
                                // guard: a chained press that wiggled
                                // onto a NEIGHBOR row (inside the chain's
                                // cell tolerance) or a fast click-walk
                                // down adjacent rows re-selects but never
                                // activates. select() finishes all Table
                                // bookkeeping first (0250 clause 4), and
                                // on the activating press the selection
                                // did not change, so on_select stays
                                // silent and activate is the LAST call —
                                // it may dispose the Table's scope.
                                let was_selected = selection.get_untracked() == idx;
                                select(idx, body_h);
                                if was_selected && ctx.click_count() >= 2 {
                                    if let Some(f) = activate.borrow_mut().as_mut() {
                                        f(idx);
                                    }
                                }
                            }
                        }
                        ctx.stop_propagation();
                    }
                    _ => {}
                },
                _ => {}
            }
        };

        let ncols = widths.len();
        let mut el = Element::new()
            .style(layout)
            .role(crate::ui::Role::Table)
            .access_value(move || {
                format!(
                    "{} rows x {} cols, selected row {}",
                    len,
                    ncols,
                    selection.get_untracked() + 1
                )
            })
            .focusable();
        if let Some(focused) = self.focused {
            el = el.focus_signal(focused);
        }
        el.on(Phase::Bubble, handler).child(dyn_view(
            LayoutStyle::default()
                .width(Dimension::Percent(1.0))
                .height(Dimension::Percent(1.0)),
            move || {
                let sel = selection.get();
                let first = offset.get().max(0);
                let rows = rows.clone();
                let titles = titles.clone();
                let widths = widths.clone();
                Element::new()
                    .style(
                        LayoutStyle::default()
                            .width(Dimension::Percent(1.0))
                            .height(Dimension::Percent(1.0)),
                    )
                    .draw(move |canvas, rect| {
                        if rect.is_empty() || rect.h < 1 {
                            return;
                        }
                        let len = rows.len() as i32;
                        let body_h = rect.h - 1;
                        let show_bar = len > body_h;
                        let usable = if show_bar { rect.w - 1 } else { rect.w };
                        let cols = solve_columns(&widths, usable);
                        // Header row: raised ground, bold titles, the
                        // sort indicator on the sorted column.
                        let header = Style::new()
                            .fg(header_fg)
                            .bg(header_ground)
                            .attrs(Attrs::BOLD);
                        canvas.fill_styled(
                            crate::base::Rect::new(rect.x, rect.y, rect.w, 1),
                            ' ',
                            &header,
                        );
                        let mut x = rect.x;
                        for (i, w) in cols.iter().enumerate() {
                            let mut title = titles.get(i).cloned().unwrap_or_default();
                            let is_sorted = matches!(sorted, Some((col, _)) if col == i);
                            if let Some((col, asc)) = sorted {
                                if col == i {
                                    title.push(if asc { '▲' } else { '▼' });
                                }
                            }
                            // Sorted column reads in full-strength
                            // `text` (D4-1) — the one header the eye
                            // needs first.
                            let style = if is_sorted {
                                Style::new()
                                    .fg(header_sorted_fg)
                                    .bg(header_ground)
                                    .attrs(Attrs::BOLD)
                            } else {
                                header
                            };
                            let line = crate::text::truncate_ellipsis(&title, *w);
                            canvas.print_styled(crate::base::Point::new(x, rect.y), &line, &style);
                            x += w + 1;
                        }
                        // Body rows, virtualized.
                        let base = Style::new().fg(text_fg).bg(ground);
                        canvas.fill_styled(
                            crate::base::Rect::new(rect.x, rect.y + 1, rect.w, body_h),
                            ' ',
                            &base,
                        );
                        for r in 0..body_h.min(len - first).max(0) {
                            let idx = (first + r) as usize;
                            let y = rect.y + 1 + r;
                            let style = if idx == sel {
                                let s = Style::new().fg(sel_fg).bg(sel_bg);
                                canvas.fill_styled(
                                    crate::base::Rect::new(rect.x, y, usable, 1),
                                    ' ',
                                    &s,
                                );
                                s
                            } else {
                                base
                            };
                            let mut x = rect.x;
                            for (c, w) in cols.iter().enumerate() {
                                if let Some(cell) = rows[idx].get(c) {
                                    let line = crate::text::truncate_ellipsis(cell, *w);
                                    canvas.print_styled(
                                        crate::base::Point::new(x, y),
                                        &line,
                                        &style,
                                    );
                                }
                                x += w + 1;
                            }
                        }
                        if show_bar {
                            let body = crate::base::Rect::new(rect.x, rect.y + 1, rect.w, body_h);
                            draw_scrollbar(canvas, body, first, len, track, thumb, ground);
                        }
                    })
                    .build()
            },
        ))
    }
}

/// Resolve column widths: fixed cells first, percent of total, then flex
/// shares over the remainder by largest-remainder (columns tile exactly;
/// 1-cell gaps between columns).
///
/// `pub(crate)`: the markdown table typesetter (0142) shares THIS
/// solver — one column-width policy for the Table widget and md-table
/// rows, never a duplicate (the 1-cell-gap assumption is part of the
/// contract both sides render against).
pub(crate) fn solve_columns(widths: &[ColWidth], total: i32) -> Vec<i32> {
    let n = widths.len() as i32;
    if n == 0 || total <= 0 {
        return vec![0; widths.len()];
    }
    let usable = (total - (n - 1)).max(0); // inter-column gaps
    let mut out = vec![0i32; widths.len()];
    let mut remaining = usable;
    let mut flex_weights = vec![0.0f64; widths.len()];
    let mut any_flex = false;
    for (i, w) in widths.iter().enumerate() {
        match *w {
            ColWidth::Cells(c) => {
                out[i] = c.clamp(0, remaining.max(0));
                remaining -= out[i];
            }
            ColWidth::Percent(p) => {
                let c = ((usable as f32) * p.clamp(0.0, 1.0)).round() as i32;
                out[i] = c.clamp(0, remaining.max(0));
                remaining -= out[i];
            }
            ColWidth::Flex(f) => {
                flex_weights[i] = f.max(0.0) as f64;
                any_flex = true;
            }
        }
    }
    if any_flex && remaining > 0 {
        let shares = distribute(remaining, &flex_weights);
        for (i, s) in shares.iter().enumerate() {
            out[i] += s;
        }
    }
    out
}

#[cfg(test)]
#[path = "table_tests.rs"]
mod tests;
