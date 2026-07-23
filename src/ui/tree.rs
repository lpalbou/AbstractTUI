//! Live instance tree: damage collection, layout mirroring, hit testing,
//! focus and event routing. Mounting/unmounting (including the `Dyn`
//! reactive-region lifecycle) lives in `ui::mount`.
//!
//! ## Borrow discipline
//!
//! The instance store (`TreeCore`) sits behind `Rc<RefCell>` shared with
//! Dyn effects. NO borrow is held across user code: mounts borrow in
//! short bursts; dispatch collects handler `Rc`s first, releases, then
//! invokes. A handler may set signals, which synchronously remounts some
//! `Dyn` — the routing path re-validates instance liveness (generational
//! ids) after every handler call.

use std::cell::RefCell;
use std::rc::Rc;

use crate::base::{Point, Rect, Rgba, Size};
use crate::layout::{solve, LayoutId, LayoutTree};
use crate::reactive::{request_frame, GenArena, Key as ArenaKey, Scope};

use super::mount::{mount_view, remove_subtree};
use super::view::{DrawFn, Handler, Shortcut, View};

/// Generational handle to a mounted view instance.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct ViewId(pub(crate) ArenaKey);

pub(super) enum InstPayload {
    Element {
        draw: Option<Rc<RefCell<DrawFn>>>,
        handlers: Rc<RefCell<Vec<Handler>>>,
        shortcuts: Rc<RefCell<Vec<Shortcut>>>,
    },
    Text {
        content: String,
    },
    /// Marker node owning a reactive subtree (its single child).
    Dyn,
}

pub(super) struct Inst {
    pub(super) parent: Option<ViewId>,
    pub(super) children: Vec<ViewId>,
    pub(super) layout: LayoutId,
    pub(super) focusable: bool,
    pub(super) focus_trap: bool,
    pub(super) focus_memory: bool,
    /// Run this node's own draw even when its rect is fully outside
    /// the clip (measurement-readback probes — see
    /// [`Element::probe_when_culled`](super::Element::probe_when_culled)).
    /// Children still cull individually.
    pub(super) probe_when_culled: bool,
    pub(super) access: super::access::AccessProps,
    pub(super) payload: InstPayload,
}

pub(super) struct TreeCore {
    pub(super) insts: GenArena<Inst>,
    pub(super) layout: LayoutTree,
    pub(super) root: Option<ViewId>,
    pub(super) viewport: Size,
    pub(super) damage: Vec<Rect>,
    pub(super) needs_layout: bool,
    pub(super) focus: Option<ViewId>,
    /// Default color for text leaves; the app sets it from the active
    /// theme's `Text` token (widgets with opinions style themselves).
    pub(super) text_fg: Rgba,
    /// Root-to-deepest path of instances currently under the pointer.
    /// Membership = "hovered" (ancestors included, DOM mouseenter model).
    pub(super) hovered_path: Vec<ViewId>,
    /// Pointer capture: all mouse events route here until release.
    pub(super) capture: Option<ViewId>,
    /// The screen cell of the press that armed the capture — the HEAL
    /// anchor: a pressed widget's own visual re-render can dispose the
    /// captured instance (Button's pressed `dyn_view` regenerates its
    /// hit leaf on the very Down that captured it); the capture then
    /// re-points at this cell's current occupant instead of silently
    /// dying (which stranded the widget's pressed state whenever the
    /// release landed outside it).
    pub(super) capture_pos: Option<Point>,
    /// Hover memo: last pointer position + the layout epoch it was
    /// hit-tested against. Any-motion mouse streams (mode 1003) repeat
    /// positions heavily; skipping the hit-test walk when neither moved
    /// makes hover O(1) for repeats.
    pub(super) last_hover: Option<(Point, u64)>,
    /// Bumped every time layout actually re-solves (same-position hits
    /// can change when geometry did).
    pub(super) layout_epoch: u64,
    /// Incremental re-solve anchors (style_signal changes): each entry's
    /// SUBTREE re-solves within its current box — a scroll drag pays for
    /// its own container, not the screen. A full `needs_layout` solve
    /// supersedes them.
    pub(super) dirty_subtrees: Vec<LayoutId>,
    /// Last-focused descendant per memory container (focus restore).
    pub(super) focus_memory: std::collections::HashMap<ViewId, ViewId>,
    /// Autofocus node recorded during mount, consumed OUTSIDE every
    /// computation: by `UiTree::mount` after the initial mount returns,
    /// or by `UiTree::layout` (frame phase L) for nodes mounted inside a
    /// `Dyn` effect run. Focus delivery runs user handlers whose signal
    /// writes would re-enter a running computation if fired inline
    /// (the 0220 mount-time "dependency cycle" panic).
    pub(super) pending_autofocus: Option<ViewId>,
    /// Multi-click synthesis (`ui::click`): one chain per tree, folded
    /// on every dispatched mouse event; handlers read the result via
    /// `EventCtx::click_count`. Positional identity, deliberately not
    /// ViewId identity — selection-driven re-renders dispose and remount
    /// the very instance a click hit (List/Table regenerate their
    /// `dyn_view` on click 1), so instance comparison would reset the
    /// chain on exactly the presses that should chain.
    pub(super) click_chain: super::click::ClickChain,
}

impl TreeCore {
    /// Push a damage rect, deduplicating containment both ways (RT2-4:
    /// one Dyn remount used to feed three identical rects — dispose
    /// damage, remount damage and the new leaf's geometry damage all
    /// cover the same region). The list is small between takes, so the
    /// linear scan costs less than the triple translation it saves.
    pub(super) fn damage_rect(&mut self, rect: Rect) {
        if rect.is_empty() {
            return;
        }
        if self.damage.iter().any(|r| r.intersect(rect) == rect) {
            return; // already covered
        }
        self.damage.retain(|r| rect.intersect(*r) != *r); // drop swallowed
        self.damage.push(rect);
    }

    pub(super) fn damage_all(&mut self) {
        let full = Rect::from_size(self.viewport);
        self.damage.push(full);
    }
}

/// The mounted UI. One per app window/screen.
pub struct UiTree {
    /// Shared with Dyn effects and the `ui::focus` split (same type,
    /// second file) — never borrowed across user code.
    pub(super) core: Rc<RefCell<TreeCore>>,
}

impl UiTree {
    pub fn new(viewport: Size) -> UiTree {
        UiTree {
            core: Rc::new(RefCell::new(TreeCore {
                insts: GenArena::new(),
                layout: LayoutTree::new(),
                root: None,
                viewport,
                damage: Vec::new(),
                needs_layout: false,
                focus: None,
                text_fg: Rgba::WHITE,
                hovered_path: Vec::new(),
                capture: None,
                capture_pos: None,
                last_hover: None,
                layout_epoch: 0,
                dirty_subtrees: Vec::new(),
                focus_memory: std::collections::HashMap::new(),
                pending_autofocus: None,
                click_chain: super::click::ClickChain::new(),
            })),
        }
    }

    /// Default text color (theme `Text` token). The app re-sets this when
    /// the theme signal changes and damages the whole tree.
    pub fn set_text_fg(&mut self, fg: Rgba) {
        self.core.borrow_mut().text_fg = fg;
    }

    /// A second handle onto the SAME tree (shared core) — the overlay
    /// store keeps trees while the driver drives them without moving
    /// ownership around. Not a copy: both handles see every mutation.
    pub fn handle(&self) -> UiTree {
        UiTree {
            core: self.core.clone(),
        }
    }

    /// Viewport size (accessibility hook + diagnostics).
    pub fn viewport_size(&self) -> Size {
        self.core.borrow().viewport
    }

    /// Snapshot the SEMANTIC tree: annotated nodes (role/label/value)
    /// and text leaves, preorder, with focus and solved bounds. This is
    /// the accessibility model — see `ui::access` for the honesty
    /// contract (in-engine substrate; no platform bridge yet).
    pub fn accessibility_tree(&mut self) -> super::access::AccessSnapshot {
        self.layout(); // bounds must be truthful
        let core = self.core.borrow();
        let mut snapshot = super::access::AccessSnapshot::default();
        let Some(root) = core.root else {
            return snapshot;
        };
        // The focused node's ANNOTATED self-or-ancestor carries the
        // focus mark (a focused inner leaf announces as its widget).
        let focus_carrier = core.focus.map(|f| {
            let mut cur = f;
            loop {
                let Some(inst) = core.insts.get(cur.0) else {
                    break cur;
                };
                let annotated =
                    !inst.access.is_empty() || matches!(inst.payload, InstPayload::Text { .. });
                if annotated {
                    break cur;
                }
                match inst.parent {
                    Some(p) => cur = p,
                    None => break cur,
                }
            }
        });
        // Iterative preorder with annotated-only depth.
        let mut stack: Vec<(ViewId, usize)> = vec![(root, 0)];
        while let Some((id, depth)) = stack.pop() {
            let Some(inst) = core.insts.get(id.0) else {
                continue;
            };
            let mut child_depth = depth;
            let entry = match &inst.payload {
                InstPayload::Text { content } if !content.is_empty() => {
                    Some(super::access::AccessEntry {
                        role: super::access::Role::Text,
                        label: content.clone(),
                        value: None,
                        focused: focus_carrier == Some(id),
                        bounds: core.layout.rect(inst.layout),
                        depth,
                    })
                }
                _ if !inst.access.is_empty() => {
                    let a = &inst.access;
                    // Value closures are app code over live signals; a
                    // closure whose data was disposed must not kill the
                    // snapshot (RT6 risk 11). `try_get_untracked` is the
                    // endorsed read; the unwind guard is the backstop
                    // for closures that panicked anyway.
                    let value = a.value.as_ref().map(|f| {
                        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| f()))
                            .unwrap_or_else(|_| "<stale>".into())
                    });
                    Some(super::access::AccessEntry {
                        role: a.role.unwrap_or(super::access::Role::Region),
                        label: a.label.clone().unwrap_or_default(),
                        value,
                        focused: focus_carrier == Some(id),
                        bounds: core.layout.rect(inst.layout),
                        depth,
                    })
                }
                _ => None,
            };
            if let Some(e) = entry {
                snapshot.entries.push(e);
                child_depth += 1;
            }
            // Reverse push keeps document order under the pop.
            for &child in inst.children.iter().rev() {
                stack.push((child, child_depth));
            }
        }
        snapshot
    }

    /// Text serialization of [`UiTree::accessibility_tree`] — the
    /// assertable/debug-dump form (`--a11y`-style dumps print this).
    pub fn accessibility_tree_text(&mut self) -> String {
        self.accessibility_tree().to_text()
    }

    /// Short alias of [`UiTree::accessibility_tree`].
    pub fn a11y_tree(&mut self) -> super::access::AccessSnapshot {
        self.accessibility_tree()
    }

    /// What a focus change should ANNOUNCE: the focused entry's role +
    /// label + value ("button \"Save\"", "input \"Search\" = \"foo\"").
    /// None when nothing is focused or the focused subtree carries no
    /// semantics (which the a11y audit should treat as a finding).
    pub fn focus_announcement(&mut self) -> Option<String> {
        let snapshot = self.accessibility_tree();
        let e = snapshot.focused()?;
        let mut out = e.role.as_str().to_string();
        if !e.label.is_empty() {
            out.push_str(&format!(" \"{}\"", e.label));
        }
        if let Some(v) = &e.value {
            out.push_str(&format!(" = \"{v}\""));
        }
        Some(out)
    }

    /// Shortcuts reachable from the CURRENT focus: the focused node's
    /// own, then each ancestor's, up to the root (the keymap-resolution
    /// order, §12a). No focus = the root's shortcuts. Feed for
    /// keymap-help overlays; unlabeled entries render as bare chords.
    pub fn keymap_of_focus_path(&self) -> Vec<(super::event::KeyChord, Option<String>)> {
        let core = self.core.borrow();
        let start = core.focus.or(core.root);
        let mut out = Vec::new();
        let mut cur = start;
        while let Some(id) = cur {
            let Some(inst) = core.insts.get(id.0) else {
                break;
            };
            if let InstPayload::Element { shortcuts, .. } = &inst.payload {
                for s in shortcuts.borrow().iter() {
                    out.push((s.chord, s.label.clone()));
                }
            }
            cur = inst.parent;
        }
        out
    }

    /// A standalone "repaint everything" handle for effects (the app's
    /// theme watcher). Captures the shared core, not `&mut self`, so it
    /// can live inside a reactive closure.
    pub fn invalidator(&self) -> impl Fn() + 'static {
        let core = Rc::downgrade(&self.core);
        move || {
            if let Some(core) = core.upgrade() {
                let mut c = core.borrow_mut();
                c.damage_all();
                c.needs_layout = true;
                drop(c);
                request_frame();
            }
        }
    }

    /// Mount `view` as the root, owned by `cx`. Disposing `cx` unmounts
    /// everything — the root subtree via the cleanup registered here,
    /// `Dyn` subtrees via their own generation cleanups. There is no
    /// separate unmount API: lifecycle is single-sourced in scopes.
    pub fn mount(&mut self, cx: Scope, view: View) -> ViewId {
        let id = mount_view(&self.core, cx, view, None);
        let core_for_cleanup = self.core.clone();
        cx.on_cleanup(move || remove_subtree(&core_for_cleanup, id));
        {
            let mut core = self.core.borrow_mut();
            core.root = Some(id);
            core.needs_layout = true;
            core.damage_all();
        }
        // Initial-focus policy: an autofocus node wins (even one mounted
        // by a nested Dyn effect — its request parked; this is the safe
        // consume point, outside every computation); apps without one
        // call focus_first() explicitly.
        self.deliver_pending_autofocus();
        request_frame();
        id
    }

    pub fn set_viewport(&mut self, size: Size) {
        let mut core = self.core.borrow_mut();
        core.viewport = size;
        core.needs_layout = true;
        core.damage_all();
        drop(core);
        request_frame();
    }

    /// Solve layout if anything changed since last solve. Structural
    /// changes (mount/viewport/theme) re-solve the whole tree; a
    /// style_signal change re-solves only its anchor SUBTREE (the
    /// nearest ancestor whose own size cannot be affected — see
    /// `mount.rs`), which is what makes a 60fps scroll drag pay for its
    /// container instead of the screen. Cheap when clean.
    ///
    /// Also the delivery point for autofocus nodes mounted inside `Dyn`
    /// Drain the layout solver's zero-collapse diagnostics (debug
    /// builds; empty in release). The driver forwards these into the
    /// startup-notices lane each frame — the solver itself never
    /// touches stderr while a session may own the terminal.
    pub(crate) fn take_collapse_notices(&mut self) -> Vec<String> {
        self.core.borrow_mut().layout.take_collapse_notices()
    }

    /// effect runs: layout is called outside every computation (frame
    /// phase L, dispatch entry, draw), so the parked focus request can
    /// run its FocusIn handlers — and any re-render those trigger folds
    /// into this very solve.
    pub fn layout(&mut self) {
        self.deliver_pending_autofocus();
        let mut core = self.core.borrow_mut();
        let full = core.needs_layout;
        let dirty: Vec<LayoutId> = std::mem::take(&mut core.dirty_subtrees);
        if !full && dirty.is_empty() {
            return;
        }
        core.layout_epoch += 1; // same-position hover memos invalidate
        core.needs_layout = false;
        let Some(root) = core.root else { return };
        let root_layout = match core.insts.get(root.0) {
            Some(inst) => inst.layout,
            None => return,
        };
        let viewport = Rect::from_size(core.viewport);
        if full {
            // A full solve covers every dirty subtree too.
            solve(&mut core.layout, root_layout, viewport);
        } else {
            for anchor in dirty {
                if core.layout.is_alive(anchor) {
                    crate::layout::resolve_subtree(&mut core.layout, anchor);
                }
            }
        }
        // Nodes the solver actually moved/resized are damage even though
        // their own content never changed (a sibling growing pushes them).
        for rect in core.layout.take_geometry_damage() {
            core.damage_rect(rect);
        }
    }

    /// Damage accumulated since last take (deduplicated coarsely by the
    /// caller/compositor; we keep raw rects here).
    pub fn take_damage(&mut self) -> Vec<Rect> {
        std::mem::take(&mut self.core.borrow_mut().damage)
    }

    /// True when a frame has work: pending damage or an unsolved layout.
    pub fn has_pending_work(&self) -> bool {
        let core = self.core.borrow();
        !core.damage.is_empty() || core.needs_layout
    }

    pub fn needs_layout(&self) -> bool {
        self.core.borrow().needs_layout
    }

    pub fn instance_count(&self) -> usize {
        self.core.borrow().insts.live()
    }

    pub fn rect_of(&self, id: ViewId) -> Rect {
        let core = self.core.borrow();
        core.insts
            .get(id.0)
            .map(|i| core.layout.rect(i.layout))
            .unwrap_or(Rect::ZERO)
    }

    pub fn focused(&self) -> Option<ViewId> {
        self.core.borrow().focus
    }

    // Hit testing, hover, pointer capture, and event dispatch live in
    // a `#[path]` sibling (file-size split): see tree_dispatch.rs —
    // same impl, different file.
}

#[path = "tree_dispatch.rs"]
mod dispatch;
