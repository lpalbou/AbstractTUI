//! The in-engine SEMANTIC model (cycle-6 accessibility substrate).
//!
//! Every element can carry a ROLE plus an accessible LABEL and a
//! dynamic VALUE; `UiTree::accessibility_tree()` snapshots the annotated
//! tree (role, label, value, focus, bounds, depth) and
//! `accessibility_tree_text()` serializes it for assertions and
//! debugging. This is deliberately NOT a platform bridge (no AT-SPI/
//! UIA/VoiceOver wiring — that work is platform-specific and out of
//! scope); it is the model such a bridge would read, and the surface
//! REDTEAM can hold widgets accountable against: if a widget's state
//! is not in this tree, a screen reader could never say it.
//!
//! Shape rules:
//! - Only ANNOTATED nodes and text leaves appear; unannotated
//!   containers are structural noise and are flattened out (their
//!   children hang from the nearest annotated ancestor).
//! - `value` is a closure sampled AT SNAPSHOT TIME (untracked): live
//!   widget state (input text, checkbox on/off, list selection) without
//!   the snapshot subscribing to anything.
//! - Text leaves surface as `Role::Text` with their content as label —
//!   free reading order for plain content.

use std::rc::Rc;

use crate::base::Rect;

/// Semantic role vocabulary. Deliberately flat and terminal-sized —
/// grows as widgets need it; unknown UI defaults to `Generic` and is
/// skipped by the snapshot unless labeled.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Role {
    Generic,
    Text,
    Heading,
    Region,
    Button,
    Checkbox,
    RadioGroup,
    Input,
    TextArea,
    List,
    ListItem,
    Table,
    Cell,
    Tabs,
    Tab,
    Dialog,
    Menu,
    MenuItem,
    ScrollArea,
    // SEMVER (budget 0002 entry 1): this enum is PUBLIC and EXHAUSTIVE
    // in the published 0.2.0 — adding a variant is a major break, and a
    // mid-enum insertion additionally shifts later discriminants (the
    // Role::Select live catch, 2026-07-22). New variants (Select, Tree,
    // TreeItem, ...) land batched inside the 0.3 window, AT THE END of
    // the enum, together with `#[non_exhaustive]`. Until then, widgets
    // reuse the closest existing role (a select trigger reports
    // `Button` + its choice as the access value).
}

impl Role {
    pub fn as_str(self) -> &'static str {
        match self {
            Role::Generic => "generic",
            Role::Text => "text",
            Role::Heading => "heading",
            Role::Region => "region",
            Role::Button => "button",
            Role::Checkbox => "checkbox",
            Role::RadioGroup => "radiogroup",
            Role::Input => "input",
            Role::TextArea => "textarea",
            Role::List => "list",
            Role::ListItem => "listitem",
            Role::Table => "table",
            Role::Cell => "cell",
            Role::Tabs => "tabs",
            Role::Tab => "tab",
            Role::Dialog => "dialog",
            Role::Menu => "menu",
            Role::MenuItem => "menuitem",
            Role::ScrollArea => "scrollarea",
        }
    }
}

/// Element-attached semantics (builder side). The `value` closure reads
/// widget signals UNTRACKED at snapshot time.
#[derive(Clone, Default)]
pub(super) struct AccessProps {
    pub role: Option<Role>,
    pub label: Option<String>,
    pub value: Option<Rc<dyn Fn() -> String>>,
}

impl AccessProps {
    pub fn is_empty(&self) -> bool {
        self.role.is_none() && self.label.is_none() && self.value.is_none()
    }
}

/// One row of the accessibility snapshot (preorder, `depth` = nesting
/// among ANNOTATED nodes only).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AccessEntry {
    pub role: Role,
    pub label: String,
    pub value: Option<String>,
    pub focused: bool,
    pub bounds: Rect,
    pub depth: usize,
}

/// The snapshot: a flattened preorder list (a tree walk without the
/// pointer soup — depth reconstructs structure, which is all a reader
/// or an assertion needs).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AccessSnapshot {
    pub entries: Vec<AccessEntry>,
}

impl AccessSnapshot {
    /// Stable text form, one node per line:
    /// `  role "label" = "value" [focused] @ x,y wxh`
    /// (value/focused only when present). Tests assert against this.
    pub fn to_text(&self) -> String {
        let mut out = String::new();
        for e in &self.entries {
            for _ in 0..e.depth {
                out.push_str("  ");
            }
            out.push_str(e.role.as_str());
            if !e.label.is_empty() {
                out.push_str(" \"");
                out.push_str(&e.label);
                out.push('"');
            }
            if let Some(v) = &e.value {
                out.push_str(" = \"");
                out.push_str(v);
                out.push('"');
            }
            if e.focused {
                out.push_str(" [focused]");
            }
            out.push_str(&format!(
                " @ {},{} {}x{}",
                e.bounds.x, e.bounds.y, e.bounds.w, e.bounds.h
            ));
            out.push('\n');
        }
        out
    }

    /// First entry with `role` (assert helper).
    pub fn find(&self, role: Role) -> Option<&AccessEntry> {
        self.entries.iter().find(|e| e.role == role)
    }

    /// The focused entry, if any annotated node has focus.
    pub fn focused(&self) -> Option<&AccessEntry> {
        self.entries.iter().find(|e| e.focused)
    }
}

/// Verify the FOCUS-VISIBLE guarantee for the currently focused node:
/// rendering with focus must differ from rendering without it somewhere
/// INSIDE the focused node's rect (DESIGN §3: selection pair or
/// border_focus stroke — any visible affordance passes, invisibility
/// fails). Returns true when no node is focused (nothing owed).
///
/// Test/debug hook — it draws the tree twice into scratch buffers.
pub fn focus_affordance_visible(tree: &mut super::UiTree) -> bool {
    let Some(focused) = tree.focused() else {
        return true;
    };
    let rect = tree.rect_of(focused);
    if rect.is_empty() {
        return true; // zero-area focus target: nothing CAN show
    }
    let size = tree.viewport_size();
    let mut with_focus = super::BufferCanvas::new(size);
    tree.draw(&mut with_focus);
    tree.set_focus(None);
    // EXPLICIT flush (RT6 risk 12): focus-driven Dyn rebuilds and
    // focus_signal effects must land before the comparison draw — the
    // hook no longer leans on set_focus's internal batch alone. The
    // CONTRACT stands regardless: focus visuals must be synchronous
    // (signal -> Dyn/draw); a widget deferring them through timers or
    // frame tasks fails this check BY DESIGN and should be redesigned.
    crate::reactive::flush_effects();
    let mut without = super::BufferCanvas::new(size);
    tree.draw(&mut without);
    tree.set_focus(Some(focused));
    crate::reactive::flush_effects();
    // Any visible difference inside the focused rect counts — glyph,
    // colors, or attributes (BOLD alone is a legal affordance).
    for y in rect.y..rect.bottom().min(size.h) {
        for x in rect.x..rect.right().min(size.w) {
            let p = crate::base::Point::new(x, y);
            if with_focus.cell(p) != without.cell(p)
                || with_focus.attrs_at(p) != without.attrs_at(p)
            {
                return true;
            }
        }
    }
    false
}
