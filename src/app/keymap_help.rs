//! KeymapHelp: the free '?' help modal — renders the shortcuts
//! reachable from the current focus (via `UiTree::keymap_of_focus_path`)
//! plus every registered global action, chord-first, one per row.
//!
//! Lives app-side (not `ui`) because it rides `Modal`/the overlay store
//! (the R4-1 layer rule, same as popups/menus). Wiring it costs one
//! action registration:
//!
//! ```ignore
//! let overlays = app.overlays();
//! let actions = app.actions();
//! let tree = app.tree().handle();
//! app.actions().register("help.keys", Some(KeyChord::new(Mods::NONE, Key::Char('?'))), move || {
//!     KeymapHelp::open(&overlays, cx, current_viewport(), &tree, &actions, &current_theme().tokens);
//! });
//! ```
//!
//! OWNER: REACT.

use crate::base::Size;
use crate::layout::{Dimension, Style as LayoutStyle};
use crate::reactive::Scope;
use crate::render::Style;
use crate::theme::TokenSet;
use crate::ui::{Element, Key, KeyChord, Role, UiTree};

use super::actions::Actions;
use super::overlays::Overlays;
use super::popups::Modal;

pub struct KeymapHelp;

impl KeymapHelp {
    /// Collect (chord, description) rows: focused-path shortcuts first
    /// (the resolution order users experience), then global actions.
    /// Public so tests and palettes can reuse the fold.
    pub fn entries(tree: &UiTree, actions: &Actions) -> Vec<(String, String)> {
        let mut out = Vec::new();
        for (chord, label) in tree.keymap_of_focus_path() {
            out.push((
                chord.display(),
                label.unwrap_or_else(|| "(widget shortcut)".into()),
            ));
        }
        for info in actions.list() {
            if let Some(chord) = info.chord {
                out.push((chord.display(), info.name));
            }
        }
        out
    }

    /// Open the help modal. Esc closes it (the modal returned also
    /// closes programmatically).
    pub fn open(
        overlays: &Overlays,
        cx: Scope,
        viewport: Size,
        tree: &UiTree,
        actions: &Actions,
        t: &TokenSet,
    ) -> Modal {
        let entries = Self::entries(tree, actions);
        let chord_w = entries
            .iter()
            .map(|(c, _)| crate::text::width(c))
            .max()
            .unwrap_or(0);
        let desc_w = entries
            .iter()
            .map(|(_, d)| crate::text::width(d))
            .max()
            .unwrap_or(0);
        let w = (chord_w + 2 + desc_w + 4).clamp(24, viewport.w);
        let h = (entries.len() as i32 + 4).clamp(5, viewport.h);
        let ink = t.text;
        let muted = t.text_muted;
        let accent = t.accent;
        let ground = t.overlay;

        let slot: std::rc::Rc<std::cell::RefCell<Option<Modal>>> =
            std::rc::Rc::new(std::cell::RefCell::new(None));
        let closer = slot.clone();
        let modal = Modal::open(overlays, cx, viewport, Size::new(w, h), move |_mcx| {
            Element::new()
                .style(
                    LayoutStyle::column()
                        .width(Dimension::Percent(1.0))
                        .height(Dimension::Percent(1.0)),
                )
                .role(Role::Dialog)
                .access_label("Keyboard shortcuts")
                .shortcut_labeled(
                    KeyChord::new(crate::ui::Mods::NONE, Key::Escape),
                    "Close help",
                    move |_ctx| {
                        if let Some(m) = &*closer.borrow() {
                            m.close();
                        }
                    },
                )
                .draw(move |canvas, rect| {
                    let base = Style::new().fg(ink).bg(ground);
                    canvas.print_styled(
                        crate::base::Point::new(rect.x, rect.y),
                        "Keyboard shortcuts",
                        &Style::new()
                            .fg(accent)
                            .bg(ground)
                            .attrs(crate::render::Attrs::BOLD),
                    );
                    for (i, (chord, desc)) in entries.iter().enumerate() {
                        let y = rect.y + 2 + i as i32;
                        if y >= rect.bottom() - 1 {
                            break;
                        }
                        canvas.print_styled(
                            crate::base::Point::new(rect.x, y),
                            chord,
                            &Style::new().fg(accent).bg(ground),
                        );
                        canvas.print_styled(
                            crate::base::Point::new(rect.x + chord_w + 2, y),
                            desc,
                            &base,
                        );
                    }
                    canvas.print_styled(
                        crate::base::Point::new(rect.x, rect.bottom() - 1),
                        "Esc closes",
                        &Style::new().fg(muted).bg(ground),
                    );
                })
                .build()
        });
        *slot.borrow_mut() = Some(modal.share());
        modal
    }
}
