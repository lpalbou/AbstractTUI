//! Owned context-action menus.
//!
//! A [`ContextMenu`] is the reusable popup face for secondary-button
//! actions. It lives in `app`, not `widgets`, because it opens an owned
//! overlay; lower-layer controls such as [`List`](crate::widgets::List)
//! only report a screen-space context request. The owned [`Popup`]
//! substrate supplies modal input routing, viewport clamping/flipping,
//! Escape/outside dismissal, resize dismissal, anchor-scope teardown,
//! and correct top-of-stack z placement.
//!
//! Menus expose stable action keys rather than application-specific
//! commands. A caller captures the target object (preferably a stable id,
//! not a volatile list index), builds the actions that apply to it, and
//! handles the chosen key in [`ContextMenu::on_action`].

use std::cell::RefCell;
use std::rc::Rc;

use crate::base::{Point, Size};
use crate::layout::{Dimension, Style as LayoutStyle};
use crate::reactive::Scope;
use crate::render::Style;
use crate::ui::{Element, EventCtx, Key, Mods, Phase, Role, UiEvent, View};

use super::anchored::{place_panel, DismissReason, PanelAnchor, PanelWidth, Popup};
use super::overlays::Overlays;
use super::select::core::{
    first_enabled, last_enabled, option_rows_view, page_highlight, resolve_overlays,
    step_highlight, OptionRows,
};
use super::select::SelectOption;
use super::{current_theme, current_viewport};

const DEFAULT_MAX_VISIBLE: usize = 12;
const DEFAULT_MIN_WIDTH: i32 = 12;

type ActionBox = Box<dyn FnMut(&str)>;
type DismissBox = Box<dyn FnMut(DismissReason)>;
type ActionCallback = Rc<RefCell<Option<ActionBox>>>;
type DismissCallback = Rc<RefCell<Option<DismissBox>>>;

/// One context-menu action. `key` is the stable value delivered to
/// [`ContextMenu::on_action`]; `label` and optional `hint` are display
/// text. Disabled actions render faint and are skipped by keyboard
/// movement and activation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContextMenuItem {
    pub key: String,
    pub label: String,
    pub hint: Option<String>,
    pub disabled: bool,
}

impl ContextMenuItem {
    pub fn new(key: impl Into<String>, label: impl Into<String>) -> ContextMenuItem {
        ContextMenuItem {
            key: key.into(),
            label: label.into(),
            hint: None,
            disabled: false,
        }
    }

    /// Muted, right-aligned supporting text such as a shortcut.
    pub fn hint(mut self, hint: impl Into<String>) -> ContextMenuItem {
        self.hint = Some(hint.into());
        self
    }

    pub fn disabled(mut self, disabled: bool) -> ContextMenuItem {
        self.disabled = disabled;
        self
    }
}

/// A keyboard-accessible action menu anchored at a screen cell.
///
/// `open` returns `None` for an empty menu, a menu with no enabled
/// action, an unavailable overlay context, or a viewport with no room.
/// Up/Down/Home/End/Page keys move the highlight, Enter or Space commits,
/// Escape abandons, and an outside press dismisses without acting below.
/// Mouse activation is left-button only.
pub struct ContextMenu {
    items: Vec<ContextMenuItem>,
    access_label: String,
    max_visible: usize,
    min_width: i32,
    overlays: Option<Overlays>,
    on_action: Option<ActionBox>,
    on_dismiss: Option<DismissBox>,
}

impl ContextMenu {
    pub fn new(items: impl IntoIterator<Item = ContextMenuItem>) -> ContextMenu {
        ContextMenu {
            items: items.into_iter().collect(),
            access_label: "context actions".into(),
            max_visible: DEFAULT_MAX_VISIBLE,
            min_width: DEFAULT_MIN_WIDTH,
            overlays: None,
            on_action: None,
            on_dismiss: None,
        }
    }

    /// Accessible name for the menu, for example `"Alice actions"`.
    pub fn access_label(mut self, label: impl Into<String>) -> ContextMenu {
        self.access_label = label.into();
        self
    }

    /// Maximum visible action rows before the menu windows around its
    /// highlight (default 12, minimum 1).
    pub fn max_visible(mut self, rows: usize) -> ContextMenu {
        self.max_visible = rows.max(1);
        self
    }

    /// Minimum menu width in terminal cells (default 12, minimum 4).
    /// The viewport still clamps the result.
    pub fn min_width(mut self, cells: i32) -> ContextMenu {
        self.min_width = cells.max(4);
        self
    }

    /// Explicit overlay store for a menu built outside [`App`](super::App).
    /// Inside `App::mount`, the ambient store is used automatically.
    pub fn overlays(mut self, overlays: &Overlays) -> ContextMenu {
        self.overlays = Some(overlays.clone());
        self
    }

    /// Fires once for a committed enabled action, after the popup has
    /// already closed. The borrowed key is valid for the callback.
    pub fn on_action(mut self, f: impl FnMut(&str) + 'static) -> ContextMenu {
        self.on_action = Some(Box::new(f));
        self
    }

    /// Observe any popup ending. Commit is delivered before
    /// [`ContextMenu::on_action`] runs; other reasons never run the action.
    pub fn on_dismiss(mut self, f: impl FnMut(DismissReason) + 'static) -> ContextMenu {
        self.on_dismiss = Some(Box::new(f));
        self
    }

    /// Open at `screen_position`, normally
    /// [`ListContext::screen_position`](crate::widgets::ListContext::screen_position).
    /// Placement prefers the row below the pointer, flips above when
    /// cramped, and clamps horizontally into the viewport.
    pub fn open(self, cx: Scope, screen_position: Point) -> Option<Popup> {
        let overlays = resolve_overlays(cx, self.overlays)?;
        let viewport = current_viewport();
        if viewport.w <= 0 || viewport.h <= 0 || self.items.is_empty() {
            return None;
        }

        let display: Vec<usize> = (0..self.items.len()).collect();
        let select_options: Vec<SelectOption> = self
            .items
            .iter()
            .map(|item| SelectOption {
                key: item.key.clone(),
                label: item.label.clone(),
                hint: item.hint.clone(),
                disabled: item.disabled,
            })
            .collect();
        let options = Rc::new(select_options);
        let seed = first_enabled(&options, &display)?;
        let wanted_rows = self.max_visible.min(display.len()).max(1);

        // Width is measured in display cells, not bytes or scalar chars.
        // Hints reserve a two-cell gap when present; rows own one cell of
        // inset on each edge.
        let natural_width = self
            .items
            .iter()
            .map(|item| {
                let label = crate::text::width(&item.label);
                let hint = item
                    .hint
                    .as_ref()
                    .map_or(0, |hint| 2 + crate::text::width(hint));
                label + hint + 2
            })
            .max()
            .unwrap_or(0)
            .max(self.min_width)
            .max(4);
        let width = PanelWidth::Content {
            min: self.min_width.min(viewport.w.max(1)),
            max: natural_width,
        };
        let anchor = PanelAnchor::cell(screen_position);
        // `Popup::open` may shorten a panel near a viewport edge. Feed
        // that solved row count to the windowing logic so keyboard
        // movement never highlights an off-panel row.
        let placed = place_panel(
            viewport,
            anchor.rect,
            Size::new(natural_width, wanted_rows as i32),
            width,
        );
        if placed.is_empty() {
            return None;
        }
        let visible = placed.h.max(1) as usize;

        let items = Rc::new(self.items);
        let action: ActionCallback = Rc::new(RefCell::new(self.on_action));
        let dismiss: DismissCallback = Rc::new(RefCell::new(self.on_dismiss));
        let session: Rc<RefCell<Option<Popup>>> = Rc::new(RefCell::new(None));
        let access_label = self.access_label;
        let theme = current_theme().tokens;

        let build = {
            let options = options.clone();
            let display_values = display.clone();
            let items = items.clone();
            let session = session.clone();
            let action = action.clone();
            move |pcx: Scope, _flipped: bool| -> View {
                let display = pcx.signal(display_values.clone());
                let highlight = pcx.signal(seed);
                let activate: Rc<dyn Fn(usize)> = Rc::new({
                    let items = items.clone();
                    let session = session.clone();
                    let action = action.clone();
                    move |position| {
                        let Some(item) = items.get(position) else {
                            return;
                        };
                        if item.disabled {
                            return;
                        }
                        let key = item.key.clone();
                        // Teardown precedes application code: the action
                        // may dispose its opener or open a replacement
                        // without overlapping modal layers.
                        let popup = session.borrow().clone();
                        if let Some(popup) = popup {
                            popup.dismiss(DismissReason::Commit);
                        }
                        if let Some(f) = action.borrow_mut().as_mut() {
                            f(&key);
                        }
                    }
                });
                let key_handler = {
                    let options = options.clone();
                    let activate = activate.clone();
                    move |ctx: &mut EventCtx, ev: &UiEvent| {
                        let UiEvent::Key(k) = ev else { return };
                        if k.mods != Mods::NONE {
                            return;
                        }
                        let current = highlight
                            .get_untracked()
                            .min(display_values.len().saturating_sub(1));
                        let target = match k.key {
                            Key::Down => {
                                Some(step_highlight(&options, &display_values, current, 1))
                            }
                            Key::Up => Some(step_highlight(&options, &display_values, current, -1)),
                            Key::Home => first_enabled(&options, &display_values),
                            Key::End => last_enabled(&options, &display_values),
                            Key::PageDown => Some(page_highlight(
                                &options,
                                &display_values,
                                current,
                                1,
                                visible,
                            )),
                            Key::PageUp => Some(page_highlight(
                                &options,
                                &display_values,
                                current,
                                -1,
                                visible,
                            )),
                            Key::Enter | Key::Char(' ') => {
                                activate(current);
                                ctx.stop_propagation();
                                return;
                            }
                            _ => return, // Escape belongs to Popup.
                        };
                        if let Some(target) = target {
                            highlight.set_if_changed(target);
                        }
                        ctx.stop_propagation();
                    }
                };
                let ink = theme.text;
                let ground = theme.surface_raised;
                Element::new()
                    .style(
                        LayoutStyle::column()
                            .width(Dimension::Percent(1.0))
                            .height(Dimension::Percent(1.0)),
                    )
                    .role(Role::Menu)
                    .access_label(access_label.clone())
                    .draw(move |canvas, rect| {
                        canvas.fill_styled(rect, ' ', &Style::new().fg(ink).bg(ground));
                    })
                    .on(Phase::Bubble, key_handler)
                    .child(option_rows_view(
                        &theme,
                        OptionRows {
                            options: options.clone(),
                            display,
                            highlight,
                            checks: None,
                            max_visible: visible,
                            on_activate: activate,
                        },
                    ))
                    .build()
            }
        };

        let popup = Popup::open(
            &overlays,
            cx,
            viewport,
            anchor,
            width,
            Size::new(natural_width, visible as i32),
            build,
        )?;
        popup.on_dismiss({
            let session = session.clone();
            move |reason| {
                session.borrow_mut().take();
                if let Some(f) = dismiss.borrow_mut().as_mut() {
                    f(reason);
                }
            }
        });
        *session.borrow_mut() = Some(popup.clone());
        Some(popup)
    }
}

#[cfg(test)]
#[path = "context_menu_tests.rs"]
mod tests;
