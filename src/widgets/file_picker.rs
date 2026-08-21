//! FilePicker: modal-friendly filesystem picker (backlog
//! first-app/0273) — breadcrumb header, live type-to-filter, entry
//! list with kind glyphs + optional sizes, keyboard navigation,
//! opt-in multi-select, `on_pick` commit.
//!
//! PURE WIDGET: entries arrive through the [`FileSource`] seam
//! (file_picker_source.rs) — the widget itself performs NO I/O; the
//! source is called once per NAVIGATION (mount, descend, parent),
//! never per frame, and [`StdFileSource`] is the `std::fs`
//! implementation beside it:
//!
//! ```no_run
//! use abstracttui::prelude::*;
//! use abstracttui::widgets::{FilePicker, StdFileSource};
//! # let _ = |cx: Scope| {
//! FilePicker::new(StdFileSource::default())
//!     .start_in("/Users/me/Documents")
//!     .multi_select(true)
//!     .on_pick(|paths| println!("{paths:?}"))
//!     .view(cx)
//! # };
//! ```
//!
//! KEY ROUTING: the picker's single focus stop is its filter input —
//! printable keys narrow the list live. The picker root intercepts, at
//! CAPTURE phase (the anchored-completion precedent), the keys the
//! input would otherwise swallow: **Enter** activates the selected
//! entry (descend into a directory / pick a file), **Backspace** and
//! **Left** go to the parent WHEN THE FILTER IS EMPTY (otherwise they
//! edit the filter — the palette convention; unconditional-parent
//! would make the filter uneditable), **Space** toggles a mark in
//! multi-select (so a multi-select filter cannot contain spaces —
//! file filters rarely need them), **Up/Down/PageUp/PageDown** move
//! the selection. Home/End stay with the input's caret. **Esc is NOT
//! consumed** — the host modal owns dismissal.
//!
//! MULTI-SELECT: `.multi_select(true)`; Space toggles the selected
//! FILE (directories are navigation, not payload); marks persist
//! across directory changes (the badge counts them) so one commit can
//! carry files from several folders; Enter on a file commits the
//! marked set when non-empty, else the current file. Clicking follows
//! the List convention: click selects, click-on-selected activates.
//!
//! ZERO IDLE: signals only — a parked picker schedules nothing and
//! costs zero bytes (pinned in `tests/wave_attachments.rs`).
//!
//! OWNER: REACT.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use crate::layout::{Dimension, Style as LayoutStyle};
use crate::reactive::{Scope, Signal};
use crate::theme::TokenSet;
use crate::ui::{dyn_view, Element, EventCtx, Key, MouseButton, MouseKind, Phase, UiEvent};
use crate::widgets::TextInput;

#[path = "file_picker_source.rs"]
mod source;
#[path = "file_picker_view.rs"]
mod view;

pub use source::{FileEntry, FileSource, StdFileSource};
use view::{
    draw_breadcrumb, draw_rows, filtered_indices, join_path, parent_path, PickerPalette,
    RowsContent,
};

/// The picker widget. Build with a [`FileSource`], bind
/// [`on_pick`](FilePicker::on_pick), mount (typically inside a
/// [`Modal`](crate::app::Modal)); see the [module docs](self) for key
/// routing and multi-select semantics. The canonical build is
/// `.view(cx)`.
pub struct FilePicker {
    source: Rc<dyn FileSource>,
    start_in: String,
    multi_select: bool,
    show_sizes: bool,
    autofocus: bool,
    layout: Option<LayoutStyle>,
    on_pick: Option<Box<dyn FnMut(Vec<String>)>>,
}

/// Everything the handlers share. One `Rc` so the capture handler,
/// the mouse handler and the filter's `on_change` see one state.
struct PickerState {
    source: Rc<dyn FileSource>,
    dir: Signal<String>,
    listing: Signal<Result<Rc<Vec<FileEntry>>, String>>,
    filter: Signal<String>,
    sel: Signal<usize>,
    offset: Signal<i32>,
    marks: Signal<Vec<String>>,
    multi_select: bool,
    on_pick: crate::widgets::SharedCallback<Vec<String>>,
}

impl PickerState {
    /// Navigate: list `path` through the source (the ONE I/O point),
    /// reset filter/selection/scroll. Errors land in `listing` and
    /// render honestly; Backspace still walks out of an unreadable
    /// directory because `dir` follows the navigation either way.
    fn load(&self, path: String) {
        let result = self.source.read_dir(&path).map(Rc::new);
        crate::reactive::batch(|| {
            self.dir.set(path);
            self.listing.set(result);
            self.filter.set(String::new());
            self.sel.set(0);
            self.offset.set(0);
        });
    }

    /// Current filtered indices + clamped selection.
    fn filtered_and_sel(&self) -> (Rc<Vec<FileEntry>>, Vec<usize>, usize) {
        let entries = match self.listing.get_untracked() {
            Ok(e) => e,
            Err(_) => Rc::new(Vec::new()),
        };
        let filtered = self
            .filter
            .with_untracked(|f| filtered_indices(&entries, f));
        let sel = self
            .sel
            .get_untracked()
            .min(filtered.len().saturating_sub(1));
        (entries, filtered, sel)
    }

    /// Move the selection to `target` (index into the filtered view)
    /// and keep it visible in `view_h` rows.
    fn move_sel(&self, target: usize, view_h: i32) {
        let (_, filtered, _) = self.filtered_and_sel();
        if filtered.is_empty() {
            return;
        }
        let target = target.min(filtered.len() - 1);
        self.sel.set(target);
        let total = filtered.len() as i32;
        self.offset.update(|o| {
            let row = target as i32;
            if row < *o {
                *o = row;
            }
            if view_h > 0 && row >= *o + view_h {
                *o = row - view_h + 1;
            }
            *o = (*o).clamp(0, (total - view_h.max(1)).max(0));
        });
    }

    /// Activate the selected entry: descend into a directory, or
    /// commit files through `on_pick` (marked set when non-empty,
    /// else the current file). ALL widget writes happen before the
    /// callback (disposal-safety law) — `on_pick` may dispose the
    /// picker's scope synchronously (close-the-modal is the point).
    fn activate(&self) {
        let (entries, filtered, sel) = self.filtered_and_sel();
        let Some(&idx) = filtered.get(sel) else {
            return; // empty / error / no matches: nothing to activate
        };
        let entry = &entries[idx];
        let full = join_path(&self.dir.get_untracked(), &entry.name);
        if entry.is_dir {
            self.load(full);
            return;
        }
        let marked = self.marks.get_untracked();
        let picked = if self.multi_select && !marked.is_empty() {
            marked
        } else {
            vec![full]
        };
        // Held borrow across user code: dispatch-only slot (the
        // SharedCallback held-borrow contract). Runs LAST.
        if let Some(f) = self.on_pick.borrow_mut().as_mut() {
            f(picked);
        }
    }

    /// Toggle the mark on the selected FILE (multi-select only;
    /// directories are navigation, not payload). Marks store full
    /// paths, so they survive navigation across directories.
    fn toggle_mark(&self) {
        let (entries, filtered, sel) = self.filtered_and_sel();
        let Some(&idx) = filtered.get(sel) else {
            return;
        };
        let entry = &entries[idx];
        if entry.is_dir {
            return;
        }
        let full = join_path(&self.dir.get_untracked(), &entry.name);
        self.marks.update(|m| {
            if let Some(pos) = m.iter().position(|p| *p == full) {
                m.remove(pos);
            } else {
                m.push(full);
            }
        });
    }

    /// Go to the parent directory (no-op at a root).
    fn go_parent(&self) {
        if let Some(parent) = parent_path(&self.dir.get_untracked()) {
            self.load(parent);
        }
    }
}

impl FilePicker {
    /// A picker over `source` (apps: [`StdFileSource`]; tests/custom
    /// backends: any [`FileSource`]).
    pub fn new(source: impl FileSource + 'static) -> FilePicker {
        FilePicker {
            source: Rc::new(source),
            start_in: ".".to_string(),
            multi_select: false,
            show_sizes: true,
            autofocus: true,
            layout: None,
            on_pick: None,
        }
    }

    /// Initial directory (default `"."`). Pass an absolute path for a
    /// readable breadcrumb — the widget navigates whatever the source
    /// accepts and never resolves paths itself.
    pub fn start_in(mut self, path: impl Into<String>) -> FilePicker {
        self.start_in = path.into();
        self
    }

    /// Space toggles marks; Enter commits the marked set (module
    /// docs). Default off: Enter picks the single selected file.
    pub fn multi_select(mut self, on: bool) -> FilePicker {
        self.multi_select = on;
        self
    }

    /// Right-aligned size column for file entries whose source
    /// reported a size (default on).
    pub fn show_sizes(mut self, on: bool) -> FilePicker {
        self.show_sizes = on;
        self
    }

    /// Focus the filter input when the picker mounts (default ON: a
    /// picker exists to be typed at, and modal content keys are dead
    /// until focus enters the modal tree — the 0230 finding). Turn
    /// off when embedding the picker beside other focusables.
    pub fn autofocus(mut self, on: bool) -> FilePicker {
        self.autofocus = on;
        self
    }

    /// Layout override for the picker root (default: column, grow).
    pub fn layout(mut self, layout: LayoutStyle) -> FilePicker {
        self.layout = Some(layout);
        self
    }

    /// The commit: full paths in pick order — the marked set when
    /// multi-select marks exist, else the one activated file.
    /// Disposal-safe: all picker bookkeeping completes first, so the
    /// callback may dispose the picker's scope synchronously (the
    /// modal close).
    pub fn on_pick(mut self, f: impl FnMut(Vec<String>) + 'static) -> FilePicker {
        self.on_pick = Some(Box::new(f));
        self
    }

    /// One-call build with tokens from the app's theme context.
    pub fn view(self, cx: Scope) -> crate::ui::View {
        let t = crate::widgets::theme_tokens(cx);
        self.element(cx, &t).build()
    }

    /// Explicit-theming build (see [`FilePicker::view`]).
    pub fn element(self, cx: Scope, t: &TokenSet) -> Element {
        let palette = PickerPalette {
            text: t.text,
            muted: t.text_muted,
            faint: t.text_faint,
            accent: t.accent,
            error: t.error,
            sel_bg: t.selection_bg,
            sel_fg: t.selection_fg,
            ground: t.surface,
            track: t.border,
            thumb: t.text_muted,
            mark: t.ok,
        };
        let multi_select = self.multi_select;
        let show_sizes = self.show_sizes;

        let state = Rc::new(PickerState {
            source: self.source,
            dir: cx.signal(String::new()),
            listing: cx.signal(Ok(Rc::new(Vec::new()))),
            filter: cx.signal(String::new()),
            sel: cx.signal(0usize),
            offset: cx.signal(0i32),
            marks: cx.signal(Vec::new()),
            multi_select,
            on_pick: Rc::new(RefCell::new(self.on_pick)),
        });
        state.load(self.start_in);

        // ---- filter input (the single focus stop) -------------------
        let filter_sig = state.filter;
        let reset_state = state.clone();
        let mut input_el = TextInput::new()
            .value(filter_sig)
            .placeholder("type to filter")
            .placeholder_while_focused(true)
            .on_change(move |_| {
                // Live narrowing restarts the selection (palette
                // convention). These are widget-internal writes, not
                // a user callback — the disposal law is not in play.
                reset_state.sel.set(0);
                reset_state.offset.set(0);
            })
            .element(cx, t);
        if self.autofocus {
            input_el = input_el.autofocus();
        }

        // ---- capture-phase key routing ------------------------------
        // The input is focused, so keys target it; the picker root
        // sees them FIRST at capture phase (anchored-completion
        // precedent) and takes only what the picker owns.
        let keys = {
            let state = state.clone();
            move |ctx: &mut EventCtx, ev: &UiEvent| {
                let UiEvent::Key(k) = ev else { return };
                // List area = root minus breadcrumb + filter rows.
                let view_h = (ctx.current_rect().h - 2).max(1);
                let filter_empty = state.filter.with_untracked(|f| f.is_empty());
                let (_, _, sel) = state.filtered_and_sel();
                match k.key {
                    Key::Enter => {
                        state.activate();
                        ctx.stop_propagation();
                    }
                    Key::Char(' ') if multi_select => {
                        state.toggle_mark();
                        ctx.stop_propagation();
                    }
                    Key::Backspace | Key::Left if filter_empty => {
                        state.go_parent();
                        ctx.stop_propagation();
                    }
                    Key::Up => {
                        state.move_sel(sel.saturating_sub(1), view_h);
                        ctx.stop_propagation();
                    }
                    Key::Down => {
                        state.move_sel(sel + 1, view_h);
                        ctx.stop_propagation();
                    }
                    Key::PageUp => {
                        state.move_sel(sel.saturating_sub(view_h.max(1) as usize), view_h);
                        ctx.stop_propagation();
                    }
                    Key::PageDown => {
                        state.move_sel(sel + view_h.max(1) as usize, view_h);
                        ctx.stop_propagation();
                    }
                    _ => {}
                }
            }
        };

        // Pointer over the scrollbar strip (or a live grab): a signal,
        // because the strip must REPAINT to show its hot ink — only a
        // tracked read damages the region.
        let bar_hot = cx.signal(false);
        // The body row under the pointer (the strip owns its own cells,
        // so it never lights a row): §3.2 hover ink, the same rule
        // `List` paints rows with.
        let hover_row = cx.signal(None::<usize>);

        // ---- mouse on the rows area ---------------------------------
        let mouse = {
            let state = state.clone();
            // Scrollbar grab (rows below the thumb's top edge); `None` =
            // no drag of ours is live.
            let bar_grab: Rc<Cell<Option<i32>>> = Rc::new(Cell::new(None));
            move |ctx: &mut EventCtx, ev: &UiEvent| {
                if matches!(ev, UiEvent::MouseLeave) {
                    hover_row.set_if_changed(None);
                    // A live grab keeps the ink (the pointer leaves the
                    // strip constantly mid-drag).
                    if bar_grab.get().is_none() {
                        bar_hot.set_if_changed(false);
                    }
                    return;
                }
                let UiEvent::Mouse(m) = ev else { return };
                let rect = ctx.current_rect();
                // The strip, when the listing overflows: hover ink and
                // gestures both resolve against it (`widgets::scrollbar`).
                let bar = {
                    let (_, filtered, _) = state.filtered_and_sel();
                    let total = filtered.len() as i32;
                    (total > rect.h).then(|| {
                        crate::widgets::scrollbar::metrics(
                            rect,
                            1,
                            state.offset.get_untracked(),
                            total,
                        )
                    })
                };
                match m.kind {
                    MouseKind::Move => {
                        // Motion with no button reports only where the
                        // app opted in (`RunConfig::hover_ink`).
                        let over = bar
                            .as_ref()
                            .is_some_and(|b| b.overflows() && b.track.contains(m.pos));
                        bar_hot.set_if_changed(over);
                        // What lights up is what a press would act on.
                        let (_, filtered, _) = state.filtered_and_sel();
                        let row = (!over)
                            .then(|| (m.pos.y - rect.y) + state.offset.get_untracked())
                            .filter(|r| *r >= 0 && (*r as usize) < filtered.len())
                            .map(|r| r as usize);
                        hover_row.set_if_changed(row);
                    }
                    MouseKind::Up(MouseButton::Left) => {
                        bar_grab.set(None);
                        if !bar.as_ref().is_some_and(|b| b.track.contains(m.pos)) {
                            bar_hot.set_if_changed(false); // released off-strip
                        }
                    }
                    MouseKind::Drag(MouseButton::Left) => {
                        // Only OUR grab scrolls (see `widgets::scrollbar`).
                        let (Some(dy), Some(bar)) = (bar_grab.get(), bar.as_ref()) else {
                            return;
                        };
                        if bar.overflows() {
                            state
                                .offset
                                .set_if_changed(crate::widgets::scrollbar::offset_at(
                                    bar, m.pos.y, dy,
                                ));
                            ctx.stop_propagation();
                        }
                    }
                    MouseKind::ScrollUp | MouseKind::ScrollDown => {
                        let delta = if m.kind == MouseKind::ScrollUp { -3 } else { 3 };
                        let (_, filtered, _) = state.filtered_and_sel();
                        let total = filtered.len() as i32;
                        state.offset.update(|o| {
                            *o = (*o + delta).clamp(0, (total - rect.h).max(0));
                        });
                        ctx.stop_propagation();
                    }
                    MouseKind::Down(MouseButton::Left) => {
                        let row = (m.pos.y - rect.y) + state.offset.get_untracked();
                        let (_, filtered, sel) = state.filtered_and_sel();
                        // The scrollbar column is the bar's, not a row's:
                        // without this guard a press on the strip selects
                        // the row beside it — and a second press on an
                        // already-selected one ACTIVATES, opening a
                        // directory the user never aimed at.
                        let total = filtered.len() as i32;
                        if total > rect.h {
                            let bar = crate::widgets::scrollbar::metrics(
                                rect,
                                1,
                                state.offset.get_untracked(),
                                total,
                            );
                            if let Some(zone) = crate::widgets::scrollbar::hit(&bar, m.pos) {
                                if bar.overflows() {
                                    let dy = crate::widgets::scrollbar::grab_for(&bar, zone);
                                    bar_grab.set(Some(dy));
                                    if matches!(zone, crate::widgets::scrollbar::Zone::Track) {
                                        state.offset.set_if_changed(
                                            crate::widgets::scrollbar::offset_at(&bar, m.pos.y, dy),
                                        );
                                    }
                                }
                                ctx.stop_propagation();
                                return;
                            }
                        }
                        if row >= 0 && (row as usize) < filtered.len() {
                            // The List convention: click selects,
                            // click-on-selected activates (double-click
                            // by subsumption, timing-free).
                            let was_selected = sel == row as usize;
                            state.move_sel(row as usize, rect.h.max(1));
                            if was_selected {
                                state.activate();
                            }
                        }
                        ctx.stop_propagation();
                    }
                    _ => {}
                }
            }
        };

        // ---- reactive views -----------------------------------------
        let crumb_state = state.clone();
        let breadcrumb = dyn_view(
            LayoutStyle::default()
                .width(Dimension::Percent(1.0))
                .height(Dimension::Cells(1))
                .shrink(0.0),
            move || {
                let dir = crumb_state.dir.get();
                let marked = if multi_select {
                    crumb_state.marks.with(Vec::len)
                } else {
                    0
                };
                Element::new()
                    .style(LayoutStyle::default().width(Dimension::Percent(1.0)))
                    .draw(move |canvas, rect| {
                        draw_breadcrumb(canvas, rect, &dir, marked, &palette);
                    })
                    .build()
            },
        );

        let rows_state = state.clone();
        let rows = dyn_view(
            LayoutStyle::default()
                .width(Dimension::Percent(1.0))
                .grow(1.0),
            move || {
                // Tracked reads: navigation, filtering, selection,
                // scrolling and marking each re-render exactly this
                // subtree.
                let listing = rows_state.listing.get();
                let filter = rows_state.filter.get();
                let sel = rows_state.sel.get();
                let offset = rows_state.offset.get();
                let marks = rows_state.marks.get();
                let dir = rows_state.dir.get();
                let content = match listing {
                    Err(msg) => RowsContent::Error(msg),
                    Ok(entries) if entries.is_empty() => RowsContent::Empty,
                    Ok(entries) => {
                        let filtered = filtered_indices(&entries, &filter);
                        if filtered.is_empty() {
                            RowsContent::NoMatches
                        } else {
                            let marked = if multi_select {
                                filtered
                                    .iter()
                                    .map(|&i| {
                                        let full = join_path(&dir, &entries[i].name);
                                        marks.contains(&full)
                                    })
                                    .collect()
                            } else {
                                Vec::new()
                            };
                            let sel = sel.min(filtered.len() - 1);
                            RowsContent::Rows {
                                entries,
                                filtered,
                                sel,
                                offset,
                                marked,
                            }
                        }
                    }
                };
                let entry_count = match &content {
                    RowsContent::Rows { filtered, .. } => filtered.len(),
                    _ => 0,
                };
                let sel_now = match &content {
                    RowsContent::Rows { sel, .. } => *sel + 1,
                    _ => 0,
                };
                let hot = bar_hot.get(); // tracked: hot ink repaints the strip
                let hot_row = hover_row.get(); // tracked: ditto for the row
                Element::new()
                    .style(
                        LayoutStyle::default()
                            .width(Dimension::Percent(1.0))
                            .height(Dimension::Percent(1.0)),
                    )
                    .role(crate::ui::Role::List)
                    .access_value(move || format!("{entry_count} entries, selected {sel_now}"))
                    .draw(move |canvas, rect| {
                        draw_rows(canvas, rect, &content, &palette, show_sizes, hot, hot_row);
                    })
                    .build()
            },
        );

        let layout = self
            .layout
            .unwrap_or_else(|| LayoutStyle::column().grow(1.0));
        Element::new()
            .style(layout)
            .on(Phase::Capture, keys)
            .child(breadcrumb)
            .child(input_el.build())
            .child(
                Element::new()
                    .style(
                        LayoutStyle::default()
                            .width(Dimension::Percent(1.0))
                            .grow(1.0),
                    )
                    .on(Phase::Bubble, mouse)
                    // The bar strip owns left drags (first-app/1335):
                    // screen select mode stands down over the STRIP only,
                    // so the thumb keeps its gesture while the listing
                    // stays selectable. Same geometry as the handler.
                    .drag_zone({
                        let state = state.clone();
                        move |rect| {
                            let (_, filtered, _) = state.filtered_and_sel();
                            let total = filtered.len() as i32;
                            (total > rect.h).then(|| {
                                crate::widgets::scrollbar::metrics(
                                    rect,
                                    1,
                                    state.offset.get_untracked(),
                                    total,
                                )
                                .track
                            })
                        }
                    })
                    .child(rows)
                    .build(),
            )
    }
}

#[cfg(test)]
#[path = "file_picker_tests.rs"]
mod tests;
