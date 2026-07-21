//! Focus and hover state machinery for [`super::tree::UiTree`] — split
//! from `tree.rs` for the file-size budget; the state lives in
//! `TreeCore`, these are its transitions.
//!
//! FOCUS MODEL: one focused instance; Tab/Shift-Tab cycle focusables in
//! DFS pre-order (document order) with wrap; a FOCUS TRAP constrains the
//! cycle to its subtree while focus is inside (modal pattern) — traps
//! gate TRAVERSAL, not authority (`set_focus` crosses them). Clicking
//! focuses the nearest focusable ancestor-or-self of the hit target;
//! clicking non-focusable space changes nothing. FocusIn/FocusOut are
//! delivered target-only.
//!
//! HOVER MODEL (DOM `mouseenter` semantics): the hovered PATH is
//! root->deepest under the pointer; membership = hovered. On change,
//! nodes leaving get `MouseLeave` (deepest first) and nodes entering get
//! `MouseEnter` (outermost first), each delivered to that node only —
//! an ancestor is hovered while the pointer is anywhere in its subtree,
//! which is exactly what `Element::hover_signal` needs.

use crate::base::Point;
use crate::reactive::request_frame;

use super::event::{EventCtx, Phase, UiEvent};
use super::tree::{UiTree, ViewId};

impl UiTree {
    /// Move focus to the next focusable instance in DFS (visual) order.
    pub fn focus_next(&mut self) {
        self.cycle_focus(1);
    }

    pub fn focus_prev(&mut self) {
        self.cycle_focus(-1);
    }

    /// Programmatic focus. Crosses focus traps (they constrain Tab
    /// cycling only). `None` blurs.
    pub fn set_focus(&mut self, id: Option<ViewId>) {
        let old = self.core.borrow().focus;
        if old == id {
            return;
        }
        // FocusOut to the old node, FocusIn to the new one — target-only
        // delivery (focus transitions do not bubble; the signal helpers
        // hear them because bubble-registered handlers hear the target
        // phase).
        if let Some(old_id) = old {
            self.core.borrow_mut().focus = None;
            let mut ctx = EventCtx::default();
            self.run_handlers(old_id, Phase::Target, &UiEvent::FocusOut, &mut ctx);
            let rect = self.rect_of(old_id);
            self.core.borrow_mut().damage_rect(rect);
        }
        if let Some(new_id) = id {
            if self.core.borrow().insts.contains(new_id.0) {
                self.core.borrow_mut().focus = Some(new_id);
                self.record_focus_memory(new_id);
                let mut ctx = EventCtx::default();
                self.run_handlers(new_id, Phase::Target, &UiEvent::FocusIn, &mut ctx);
                let rect = self.rect_of(new_id);
                self.core.borrow_mut().damage_rect(rect);
            }
        }
        request_frame();
    }

    /// Whether `id` currently holds focus — the "focus ring flag" widgets
    /// consult when drawing (though widgets usually bind `focus_signal`).
    pub fn is_focused(&self, id: ViewId) -> bool {
        self.core.borrow().focus == Some(id)
    }

    /// Recompute the hovered path from a pointer position and deliver
    /// per-node MouseLeave (deepest first) / MouseEnter (outermost first)
    /// to the nodes whose membership changed.
    ///
    /// Memoized on (position, layout epoch): any-motion streams repeat
    /// positions and wheel bursts repeat them exactly — those pay one
    /// tuple compare instead of a hit-test walk. A re-layout bumps the
    /// epoch, so same-position hits re-evaluate when geometry moved.
    pub(super) fn update_hover(&mut self, pos: Point) {
        {
            let core = self.core.borrow();
            if core.last_hover == Some((pos, core.layout_epoch)) {
                return;
            }
        }
        let new_path = match self.hit_test(pos) {
            Some(t) => self.path_to(t),
            None => Vec::new(),
        };
        let (old_path, epoch) = {
            let core = self.core.borrow();
            (core.hovered_path.clone(), core.layout_epoch)
        };
        self.core.borrow_mut().last_hover = Some((pos, epoch));
        if new_path == old_path {
            return;
        }
        for id in old_path.iter().rev().filter(|id| !new_path.contains(id)) {
            let mut ctx = EventCtx {
                target: Some(*id),
                target_rect: self.rect_of(*id),
                ..EventCtx::default()
            };
            self.run_handlers(*id, Phase::Target, &UiEvent::MouseLeave, &mut ctx);
        }
        for id in new_path.iter().filter(|id| !old_path.contains(id)) {
            let mut ctx = EventCtx {
                target: Some(*id),
                target_rect: self.rect_of(*id),
                ..EventCtx::default()
            };
            self.run_handlers(*id, Phase::Target, &UiEvent::MouseEnter, &mut ctx);
        }
        self.core.borrow_mut().hovered_path = new_path;
    }

    /// Tab-order cycling with wrap. When the current focus sits inside a
    /// FOCUS TRAP (deepest trapping ancestor wins), the cycle is
    /// constrained to that subtree — the modal pattern.
    ///
    /// FOCUS MEMORY: when the cycle ENTERS a memory container (an
    /// element built with `Element::focus_memory`) coming from outside
    /// it, the container's last-focused descendant is restored instead
    /// of tab order's first pick — re-entering a form pane lands where
    /// the user left it.
    fn cycle_focus(&mut self, dir: i32) {
        let current = self.core.borrow().focus;
        let scope = current.and_then(|c| self.trap_root_of(c));
        let order = self.focusables_within(scope);
        if order.is_empty() {
            return;
        }
        let next = match current.and_then(|c| order.iter().position(|x| *x == c)) {
            Some(pos) => {
                let n = order.len() as i32;
                let idx = ((pos as i32 + dir) % n + n) % n; // wrap both ways
                order[idx as usize]
            }
            None => {
                if dir > 0 {
                    order[0]
                } else {
                    *order.last().expect("non-empty")
                }
            }
        };
        let next = self.restore_memory_target(current, next);
        self.set_focus(Some(next));
    }

    /// If moving `from -> entering` crosses INTO a memory container,
    /// return that container's remembered descendant (when still alive
    /// and focusable) instead of `entering`.
    fn restore_memory_target(&self, from: Option<ViewId>, entering: ViewId) -> ViewId {
        let core = self.core.borrow();
        // Outermost memory container of `entering` that does NOT contain
        // `from` (i.e. genuinely being entered, not moved within).
        let mut containers: Vec<ViewId> = Vec::new();
        let mut cur = Some(entering);
        while let Some(node) = cur {
            let Some(inst) = core.insts.get(node.0) else {
                break;
            };
            if inst.focus_memory {
                containers.push(node);
            }
            cur = inst.parent;
        }
        let contains = |container: ViewId, id: Option<ViewId>| -> bool {
            let mut cur = id;
            while let Some(node) = cur {
                if node == container {
                    return true;
                }
                cur = core.insts.get(node.0).and_then(|i| i.parent);
            }
            false
        };
        for container in containers.into_iter().rev() {
            if contains(container, from) {
                continue; // moving WITHIN it: normal tab order
            }
            if let Some(&remembered) = core.focus_memory.get(&container) {
                let alive_focusable = core
                    .insts
                    .get(remembered.0)
                    .map(|i| i.focusable)
                    .unwrap_or(false);
                if alive_focusable && contains(container, Some(remembered)) {
                    return remembered;
                }
            }
        }
        entering
    }

    /// Record `focused` as the memory of every enclosing memory
    /// container (called by set_focus on the way in).
    pub(super) fn record_focus_memory(&mut self, focused: ViewId) {
        let containers: Vec<ViewId> = {
            let core = self.core.borrow();
            let mut out = Vec::new();
            let mut cur = Some(focused);
            while let Some(node) = cur {
                let Some(inst) = core.insts.get(node.0) else {
                    break;
                };
                if inst.focus_memory {
                    out.push(node);
                }
                cur = inst.parent;
            }
            out
        };
        if !containers.is_empty() {
            let mut core = self.core.borrow_mut();
            for c in containers {
                core.focus_memory.insert(c, focused);
            }
        }
    }

    /// Focus the first focusable (document order) — the explicit
    /// initial-focus policy call. An `Element::autofocus` node mounted
    /// anywhere wins over this (mount focuses it directly); apps that
    /// want "first focusable on start" call this after mounting.
    pub fn focus_first(&mut self) {
        let order = self.focusables_within(None);
        if let Some(&first) = order.first() {
            self.set_focus(Some(first));
        }
    }

    /// Initial keyboard-ownership policy for trees that OWN input from
    /// frame one (modal overlay trees — `app::popups::Modal` and every
    /// `Overlays::layer_tree(modal = true)` caller run this at open):
    ///
    /// 1. an `autofocus` node already focused at mount wins (no-op);
    /// 2. otherwise the first focusable (document order);
    /// 3. otherwise the root's FIRST CHILD — the content element of a
    ///    panel/content composition. Key dispatch targets
    ///    `focus.or(root)` and shortcuts resolve along the root→focus
    ///    path, so an unfocused tree exposes only the root's shortcuts:
    ///    anchoring on the content keeps ITS shortcuts live from frame
    ///    one (the 0230 dead-keys bug). Programmatic focus does not
    ///    require focusability — Tab moves on from the anchor normally;
    /// 4. a childless root anchors on the root itself.
    pub fn focus_init(&mut self) {
        if self.core.borrow().focus.is_some() {
            return; // autofocus won at mount
        }
        self.focus_first();
        if self.core.borrow().focus.is_some() {
            return;
        }
        let anchor = {
            let core = self.core.borrow();
            core.root.map(|root| {
                core.insts
                    .get(root.0)
                    .and_then(|inst| inst.children.first().copied())
                    .unwrap_or(root)
            })
        };
        if let Some(anchor) = anchor {
            self.set_focus(Some(anchor));
        }
    }

    /// Deliver an autofocus request parked by a mount that ran inside a
    /// computation (`Dyn` regenerations — see `TreeCore::
    /// pending_autofocus`). Called from `UiTree::layout`, which only
    /// runs outside computations; a target disposed between the mount
    /// and this frame is dropped WITHOUT blurring whatever is focused.
    pub(super) fn deliver_pending_autofocus(&mut self) {
        let target = {
            let mut core = self.core.borrow_mut();
            match core.pending_autofocus.take() {
                Some(id) if core.insts.contains(id.0) => Some(id),
                _ => None,
            }
        };
        if let Some(target) = target {
            self.set_focus(Some(target));
        }
    }

    /// Spatial focus movement: the nearest focusable in `dir` from the
    /// currently focused rect (dashboard arrow-navigation between
    /// panes). Geometry-based nearest-in-direction: candidates whose
    /// center lies strictly in the direction's half-plane, scored by
    /// primary distance + 2x orthogonal misalignment (the simple metric
    /// every TV UI uses); nothing focused = focus_first. Respects the
    /// current focus trap. Returns whether focus moved.
    pub fn focus_next_in(&mut self, dir: super::event::Key) -> bool {
        use super::event::Key;
        let current = { self.core.borrow().focus };
        let current = match current {
            Some(c) => c,
            None => {
                self.focus_first();
                return self.core.borrow().focus.is_some();
            }
        };
        let scope = self.trap_root_of(current);
        let order = self.focusables_within(scope);
        let from = self.rect_of(current);
        let (fcx, fcy) = (from.x + from.w / 2, from.y + from.h / 2);
        let mut best: Option<(i64, ViewId)> = None;
        for cand in order {
            if cand == current {
                continue;
            }
            let r = self.rect_of(cand);
            if r.is_empty() {
                continue;
            }
            let (cx, cy) = (r.x + r.w / 2, r.y + r.h / 2);
            let (primary, ortho) = match dir {
                Key::Up => (fcy - cy, (cx - fcx).abs()),
                Key::Down => (cy - fcy, (cx - fcx).abs()),
                Key::Left => (fcx - cx, (cy - fcy).abs()),
                Key::Right => (cx - fcx, (cy - fcy).abs()),
                _ => return false,
            };
            if primary <= 0 {
                continue; // not in that direction
            }
            let score = primary as i64 + 2 * ortho as i64;
            if best.map(|(s, _)| score < s).unwrap_or(true) {
                best = Some((score, cand));
            }
        }
        match best {
            Some((_, target)) => {
                self.set_focus(Some(target));
                true
            }
            None => false,
        }
    }

    /// Nearest focusable ancestor-or-self (click-to-focus targeting).
    pub(super) fn focusable_ancestor_of(&self, id: ViewId) -> Option<ViewId> {
        let core = self.core.borrow();
        let mut cur = Some(id);
        while let Some(node) = cur {
            let inst = core.insts.get(node.0)?;
            if inst.focusable {
                return Some(node);
            }
            cur = inst.parent;
        }
        None
    }

    /// Deepest ancestor-or-self of `id` marked as a focus trap.
    fn trap_root_of(&self, id: ViewId) -> Option<ViewId> {
        let core = self.core.borrow();
        let mut cur = Some(id);
        while let Some(node) = cur {
            let inst = core.insts.get(node.0)?;
            if inst.focus_trap {
                return Some(node);
            }
            cur = inst.parent;
        }
        None
    }

    /// Focusable instances in DFS pre-order (= document/tab order),
    /// restricted to `scope`'s subtree when given.
    fn focusables_within(&self, scope: Option<ViewId>) -> Vec<ViewId> {
        let core = self.core.borrow();
        let start = match scope.or(core.root) {
            Some(s) => s,
            None => return Vec::new(),
        };
        let mut out = Vec::new();
        let mut stack = vec![start];
        while let Some(id) = stack.pop() {
            let Some(inst) = core.insts.get(id.0) else {
                continue;
            };
            if inst.focusable {
                out.push(id);
            }
            for &child in inst.children.iter().rev() {
                stack.push(child);
            }
        }
        out
    }
}
