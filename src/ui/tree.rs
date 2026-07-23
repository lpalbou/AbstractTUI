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
use crate::reactive::{batch, request_frame, GenArena, Key as ArenaKey, Scope};

use super::event::{EventCtx, Key, Mods, MouseKind, Phase, UiEvent};
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
                last_hover: None,
                layout_epoch: 0,
                dirty_subtrees: Vec::new(),
                focus_memory: std::collections::HashMap::new(),
                pending_autofocus: None,
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

    /// Deepest instance whose solved rect contains `p` (later siblings
    /// win at each level — mirrors paint order). Clip-aware: a node with
    /// `clip_overflow` refuses to descend when `p` is outside its content
    /// box, so scrolled-away children are not hit at their invisible
    /// positions. Iterative: one root-to-leaf walk.
    pub fn hit_test(&self, p: Point) -> Option<ViewId> {
        let core = self.core.borrow();
        let root = core.root?;
        let rinst = core.insts.get(root.0)?;
        if !core.layout.rect(rinst.layout).contains(p) {
            return None;
        }
        let mut current = root;
        'descend: loop {
            let Some(inst) = core.insts.get(current.0) else {
                return Some(current);
            };
            if let Some(style) = core.layout.style(inst.layout) {
                if style.clips_children() {
                    let rect = core.layout.rect(inst.layout);
                    let content = Rect::new(
                        rect.x + style.padding.left,
                        rect.y + style.padding.top,
                        (rect.w - style.padding.horizontal()).max(0),
                        (rect.h - style.padding.vertical()).max(0),
                    );
                    if !content.contains(p) {
                        return Some(current); // padding gutter or clipped edge
                    }
                }
            }
            for &child in inst.children.iter().rev() {
                if let Some(cinst) = core.insts.get(child.0) {
                    if core.layout.rect(cinst.layout).contains(p) {
                        current = child;
                        continue 'descend;
                    }
                }
            }
            return Some(current);
        }
    }

    /// The PANE rect at `p` for screen-space selection (backlog 0270):
    /// the content box of the deepest clipping-or-padded ancestor on the
    /// hit path whose content box contains `p` — a `Scroll` viewport, a
    /// bordered `Block` (borders ride the padding floor), an inset panel
    /// — else the root's rect (a tree without panes is one pane). `None`
    /// when `p` misses the tree. Content boxes exclude the padding
    /// gutter, so borders never count as selectable pane content.
    /// Read-only; screen coordinates; same descent as [`Self::hit_test`].
    pub fn pane_rect_at(&self, p: Point) -> Option<Rect> {
        let core = self.core.borrow();
        let root = core.root?;
        let rinst = core.insts.get(root.0)?;
        let root_rect = core.layout.rect(rinst.layout);
        if !root_rect.contains(p) {
            return None;
        }
        let mut pane: Option<Rect> = None;
        let mut current = root;
        while let Some(inst) = core.insts.get(current.0) {
            if let Some(style) = core.layout.style(inst.layout) {
                if style.clips_children() || style.padding != crate::layout::Edges::ZERO {
                    let rect = core.layout.rect(inst.layout);
                    let content = Rect::new(
                        rect.x + style.padding.left,
                        rect.y + style.padding.top,
                        (rect.w - style.padding.horizontal()).max(0),
                        (rect.h - style.padding.vertical()).max(0),
                    );
                    if content.contains(p) {
                        pane = Some(content);
                    } else if style.clips_children() {
                        break; // gutter/clipped edge: hit_test stops here too
                    }
                }
            }
            // Descend to the child under `p` (later siblings win, like
            // hit_test); a leaf ends the walk.
            let next = inst.children.iter().rev().copied().find(|child| {
                core.insts
                    .get(child.0)
                    .is_some_and(|ci| core.layout.rect(ci.layout).contains(p))
            });
            match next {
                Some(child) => current = child,
                None => break,
            }
        }
        Some(pane.unwrap_or(root_rect))
    }

    /// True while the pointer is anywhere inside `id`'s subtree.
    pub fn is_hovered(&self, id: ViewId) -> bool {
        self.core.borrow().hovered_path.contains(&id)
    }

    /// Currently captured pointer target, if any.
    pub fn pointer_capture(&self) -> Option<ViewId> {
        self.core.borrow().capture
    }

    /// Route an event. Returns true if something consumed it
    /// (`stop_propagation`, a shortcut, or a default action).
    ///
    /// RESOLUTION ORDER (documented contract): handlers first — capture
    /// (root->target), target, bubble (target->root) — so a FOCUSED
    /// widget consumes its keys (a text input typing 'q') before any
    /// shortcut can steal them; THEN the shortcut table (root->target
    /// walk, deepest registration wins: local overrides global); THEN
    /// the built-in defaults (Tab/Shift-Tab focus traversal). Any
    /// consuming step suppresses the later ones.
    ///
    /// PINNED SEMANTICS (RT1-3, option a): the whole dispatch runs inside
    /// `reactive::batch`, so signal writes made by handlers do NOT flush
    /// effects mid-routing. Routing completes over the tree as it stood
    /// when the event arrived — every handler that fires belongs to a
    /// then-live instance — and `Dyn` disposal/remounting happens when
    /// the batch closes, after this function's routing work.
    pub fn dispatch(&mut self, event: &UiEvent) -> bool {
        batch(|| self.dispatch_inner(event))
    }

    fn dispatch_inner(&mut self, event: &UiEvent) -> bool {
        self.layout(); // hit testing needs fresh rects
        let target = match event {
            UiEvent::Mouse(m) => {
                // Capture redirects every mouse event; a stale capture
                // (node disposed) auto-releases.
                let captured = {
                    let mut core = self.core.borrow_mut();
                    match core.capture {
                        Some(c) if core.insts.contains(c.0) => Some(c),
                        Some(_) => {
                            core.capture = None;
                            None
                        }
                        None => None,
                    }
                };
                if captured.is_none() {
                    // Hover transitions ride every uncaptured mouse event
                    // (Move mostly, but a Down teleported by focus jumps
                    // must also correct hover).
                    self.update_hover(m.pos);
                }
                captured.or_else(|| self.hit_test(m.pos))
            }
            // Keys and pastes go to the focused widget (root fallback).
            UiEvent::Key(_) | UiEvent::Paste(_) => {
                self.core.borrow().focus.or(self.core.borrow().root)
            }
            // Synthesized-only events never enter from outside.
            UiEvent::FocusIn | UiEvent::FocusOut | UiEvent::MouseEnter | UiEvent::MouseLeave => {
                None
            }
        };
        let Some(target) = target else { return false };
        let path = self.path_to(target);

        let mut ctx = EventCtx {
            target: Some(target),
            target_rect: self.rect_of(target),
            ..EventCtx::default()
        };

        // --- 1. handlers: capture -> target -> bubble --------------------
        for id in path.iter() {
            let phase = if *id == target {
                Phase::Target
            } else {
                Phase::Capture
            };
            self.run_handlers(*id, phase, event, &mut ctx);
            if ctx.stopped {
                break;
            }
        }
        if !ctx.stopped {
            for id in path.iter().rev() {
                if *id == target {
                    continue; // target already ran
                }
                self.run_handlers(*id, Phase::Bubble, event, &mut ctx);
                if ctx.stopped {
                    break;
                }
            }
        }
        let mut consumed = ctx.stopped;

        // --- 2. shortcuts (key events not consumed by handlers) ----------
        if !consumed {
            if let UiEvent::Key(k) = event {
                let chord = k.chord();
                let mut winner: Option<Rc<RefCell<Vec<Shortcut>>>> = None;
                for id in &path {
                    let core = self.core.borrow();
                    if let Some(inst) = core.insts.get(id.0) {
                        if let InstPayload::Element { shortcuts, .. } = &inst.payload {
                            if shortcuts.borrow().iter().any(|s| s.chord == chord) {
                                winner = Some(shortcuts.clone());
                            }
                        }
                    }
                }
                if let Some(shortcuts) = winner {
                    let mut list = shortcuts.borrow_mut();
                    if let Some(s) = list.iter_mut().find(|s| s.chord == chord) {
                        (s.run)(&mut ctx);
                        consumed = true;
                    }
                }
            }
        }

        // --- 3. built-in defaults: Tab traversal --------------------------
        if !consumed {
            if let UiEvent::Key(k) = event {
                if k.key == Key::Tab {
                    if k.mods.contains(Mods::SHIFT) {
                        self.focus_prev();
                    } else {
                        self.focus_next();
                    }
                    consumed = true;
                }
            }
        }

        // --- pointer capture + click-to-focus lifecycle --------------------
        if let UiEvent::Mouse(m) = event {
            match m.kind {
                // Mouse down captures its target: sliders/scrollbars keep
                // receiving drags even when the pointer leaves their rect.
                MouseKind::Down(_) => {
                    self.core.borrow_mut().capture = Some(target);
                    // TARGETING RULE (documented): a click focuses the
                    // NEAREST FOCUSABLE ANCESTOR-OR-SELF of the hit target
                    // (clicking a button's label focuses the button; a
                    // list row, the list). Clicking non-focusable space
                    // changes nothing — terminal apps keep the keyboard
                    // anchored rather than blurring into the void. A
                    // handler's explicit `request_focus` (applied below)
                    // overrides this default.
                    if let Some(f) = self.focusable_ancestor_of(target) {
                        if self.core.borrow().focus != Some(f) {
                            self.set_focus(Some(f));
                        }
                    }
                }
                MouseKind::Up(_) => {
                    self.core.borrow_mut().capture = None;
                    // The pointer may sit over something else now.
                    self.update_hover(m.pos);
                }
                _ => {}
            }
        }

        // --- apply handler commands (explicit beats automatic) -------------
        if let Some(req) = ctx.capture_request.take() {
            let mut core = self.core.borrow_mut();
            core.capture = req.filter(|id| core.insts.contains(id.0));
        }
        if let Some(focus) = ctx.focus_request.take() {
            self.set_focus(Some(focus));
        }
        if ctx.damage_all {
            self.core.borrow_mut().damage_all();
            request_frame();
        }
        consumed
    }

    // Focus + hover transitions live in `ui::focus` (same type, split
    // file): focus_next/prev, set_focus, is_focused, update_hover,
    // focusable_ancestor_of and the trap machinery.

    pub(super) fn path_to(&self, target: ViewId) -> Vec<ViewId> {
        let core = self.core.borrow();
        let mut path = Vec::new();
        let mut cur = Some(target);
        while let Some(id) = cur {
            path.push(id);
            cur = core.insts.get(id.0).and_then(|i| i.parent);
        }
        path.reverse(); // root first
        path
    }

    /// Invoke handlers of one instance for one phase. Handler `Rc`s are
    /// cloned out and the core released before user code runs; liveness
    /// is re-checked because a previous handler may have remounted us.
    pub(super) fn run_handlers(
        &mut self,
        id: ViewId,
        phase: Phase,
        event: &UiEvent,
        ctx: &mut EventCtx,
    ) {
        let handlers = {
            let core = self.core.borrow();
            let Some(inst) = core.insts.get(id.0) else {
                return;
            };
            match &inst.payload {
                InstPayload::Element { handlers, .. } => handlers.clone(),
                _ => return,
            }
        };
        // The running node's identity/geometry (RT3-4: widgets do their
        // own-rect math from here, never from the possibly-deeper target).
        ctx.current = Some(id);
        ctx.current_rect = self.rect_of(id);
        let mut list = handlers.borrow_mut();
        for h in list.iter_mut() {
            let phase_match = match (h.phase, phase) {
                (Phase::Capture, Phase::Capture) => true,
                // Bubble listeners also hear the target phase — matching
                // DOM semantics where target fires both kinds. An
                // explicit Target registration fires ONLY at the target
                // (RT3-3: this arm was missing and the variant was a
                // silent no-op).
                (Phase::Bubble, Phase::Bubble) | (Phase::Bubble, Phase::Target) => true,
                (Phase::Capture, Phase::Target) => true,
                (Phase::Target, Phase::Target) => true,
                _ => false,
            };
            if phase_match {
                (h.run)(ctx, event);
                if ctx.stopped {
                    break;
                }
            }
        }
    }
}
