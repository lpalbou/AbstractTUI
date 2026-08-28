//! The pointer/dispatch plane of [`UiTree`]: hit testing, hover,
//! pointer capture, event routing (capture -> target -> bubble),
//! shortcuts, and the handler runner. `#[path]` sibling of tree.rs
//! (file-size split) — same `impl UiTree`, different file.
//!
//! OWNER: REACT.

use std::cell::RefCell;
use std::rc::Rc;

use crate::base::{Point, Rect};
use crate::reactive::{batch, request_frame};

use super::super::event::{
    EventCtx, Key, Mods, MouseButton, MouseEvent, MouseKind, Phase, UiEvent,
};
use super::super::view::Shortcut;
use super::{InstPayload, UiTree, ViewId};

/// What a left press at a point resolves to, for the screen-text
/// selection layer: see [`UiTree::press_probe_at`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct PressProbe {
    /// The pane that would clamp a selection dragged from here; `None`
    /// when the point misses the tree entirely.
    pub pane: Option<Rect>,
    /// The point lies in a widget's declared drag zone — a scrollbar
    /// strip, a splitter, an orbit surface. The selection layer stands
    /// down so the widget keeps its gesture.
    pub drag_owner: bool,
}

impl PressProbe {
    /// The point missed the tree: no pane, nothing owns it.
    pub const MISS: PressProbe = PressProbe {
        pane: None,
        drag_owner: false,
    };
}

impl UiTree {
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
        // `want_zones: false` keeps this accessor what its name and its
        // doc promise: a pure geometry read. Delegating to the full
        // probe would run every drag-zone closure on the hit path, and
        // those are consumer code that may do real work.
        self.probe_at(p, false).pane
    }

    /// Everything the selection layer needs to know about a left press at
    /// `p`, resolved in ONE descent: the pane that would clamp a drag
    /// from there ([`Self::pane_rect_at`]), and whether the cell already
    /// belongs to a widget that owns drags
    /// ([`Element::drag_zone`](super::super::Element::drag_zone)).
    ///
    /// Reads SOLVED geometry — it never lays out. A caller reaching for
    /// it after mutating the tree (an event dispatched with no frame in
    /// between) must call [`UiTree::layout`] first, exactly as
    /// [`Self::dispatch`] does before its own hit test; otherwise a
    /// freshly remounted region answers from a zero rect and its drag
    /// zone goes unseen. `app::selection` has the driver do this on
    /// every left Down (`selection::layout_for_anchor`).
    ///
    /// Read-only; screen coordinates; same descent as [`Self::hit_test`].
    pub fn press_probe_at(&self, p: Point) -> PressProbe {
        self.probe_at(p, true)
    }

    /// The shared descent behind [`Self::pane_rect_at`] and
    /// [`Self::press_probe_at`]. `want_zones` decides whether the walk
    /// collects drag-zone closures — the pane-only caller must not pay
    /// for consumer code it does not consult.
    fn probe_at(&self, p: Point, want_zones: bool) -> PressProbe {
        // Drag-zone closures are user code: collect the handles during
        // the walk and call them AFTER the core borrow is released (the
        // held-borrow contract — a zone that touched the tree would
        // otherwise panic).
        let mut zones: Vec<(super::super::view::DragZoneFn, Rect)> = Vec::new();
        let pane = {
            let core = self.core.borrow();
            let Some(root) = core.root else {
                return PressProbe::MISS;
            };
            let Some(rinst) = core.insts.get(root.0) else {
                return PressProbe::MISS;
            };
            let root_rect = core.layout.rect(rinst.layout);
            if !root_rect.contains(p) {
                return PressProbe::MISS;
            }
            let mut pane: Option<Rect> = None;
            let mut current = root;
            while let Some(inst) = core.insts.get(current.0) {
                if want_zones {
                    if let Some(zone) = &inst.drag_zone {
                        zones.push((zone.clone(), core.layout.rect(inst.layout)));
                    }
                }
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
            pane.unwrap_or(root_rect)
        };
        let drag_owner = zones
            .into_iter()
            .any(|(zone, rect)| zone(rect).is_some_and(|z| z.contains(p)));
        PressProbe {
            pane: Some(pane),
            drag_owner,
        }
    }

    /// True while the pointer is anywhere inside `id`'s subtree.
    pub fn is_hovered(&self, id: ViewId) -> bool {
        self.core.borrow().hovered_path.contains(&id)
    }

    /// Currently captured pointer target, if any.
    pub fn pointer_capture(&self) -> Option<ViewId> {
        self.core.borrow().capture
    }

    /// Cancel an in-progress pointer press WITHOUT a click
    /// (crate-internal; the selection layer's gesture claim, backlog
    /// 0285). When a passed-through left Down becomes a selection DRAG,
    /// the tree that saw the Down still holds a pointer capture and the
    /// pressed widget still waits for its release — leaving both would
    /// wedge the tree (the next click anywhere routes to the stale
    /// captured target). Deliver a left-Up at an impossible position —
    /// outside every rect, so release-inside-decides widgets (Button)
    /// un-press without firing — through the normal capture routing,
    /// which also drops the capture and re-derives hover. No live
    /// capture = no-op. Returns whether a press was cancelled.
    pub(crate) fn cancel_pointer_press(&mut self) -> bool {
        self.layout(); // the heal below hit-tests at the press cell
                       // The press that armed this capture was re-interpreted (it
                       // became a selection drag): it must not seed a double-click —
                       // same rule as the chain's own drag reset, applied at the one
                       // place drags are consumed BEFORE this tree can see them.
        self.core.borrow_mut().click_chain.reset();
        if self.validated_capture().is_none() {
            return false;
        }
        self.dispatch(&UiEvent::Mouse(MouseEvent {
            pos: Point::new(-1, -1),
            kind: MouseKind::Up(MouseButton::Left),
            mods: Mods::NONE,
        }));
        true
    }

    /// The pointer-capture target, validated and HEALED: a live capture
    /// answers as-is; a capture whose instance was disposed re-points at
    /// the press cell's current occupant. The staleness is routine, not
    /// exotic — Button's `pressed` write on Down regenerates its own
    /// `dyn_view` hit leaf inside that same dispatch, so every button
    /// press used to strand its capture immediately (the documented
    /// "capture keeps the release routed here" contract only held until
    /// the first pressed re-render; a release outside the widget then
    /// never reached it and its pressed state wedged visibly). Healing
    /// by press cell re-finds the replacement instance in the
    /// re-render case; when the subtree genuinely died (a modal closed
    /// mid-press), it points at whatever is beneath — which never armed
    /// a press, so the gesture tail lands harmlessly (release-inside-
    /// decides widgets ignore an un-pressed Up). No occupant = the
    /// capture is honestly gone.
    fn validated_capture(&mut self) -> Option<ViewId> {
        let (capture, pos) = {
            let core = self.core.borrow();
            (core.capture, core.capture_pos)
        };
        let c = capture?;
        if self.core.borrow().insts.contains(c.0) {
            return Some(c);
        }
        let healed = pos.and_then(|p| self.hit_test(p));
        self.core.borrow_mut().capture = healed;
        healed
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
        // Publish the tree's layer origin AROUND the batch: handlers
        // read it via `EventCtx::layer_origin`, and effects flushed at
        // the batch close (a completion controller reacting to a caret
        // write) still see the dispatching tree's truth ambiently.
        let _origin = super::publish_layer_origin(self.core.borrow().layer_origin);
        batch(|| self.dispatch_inner(event))
    }

    fn dispatch_inner(&mut self, event: &UiEvent) -> bool {
        self.layout(); // hit testing needs fresh rects
                       // Multi-click synthesis: fold every mouse event into the tree's
                       // chain BEFORE routing, so the count is readable by every
                       // handler of this very press. Time comes from the ambient
                       // event clock (driver-published each turn); without one,
                       // presses stay isolated — deterministically count 1, never an
                       // implicit wall-clock read (see ui::click's no-wall-clock rule).
        let click_count = match event {
            UiEvent::Mouse(m) => match super::super::click::event_time() {
                Some(now) => self.core.borrow_mut().click_chain.observe(now, m),
                None => {
                    self.core.borrow_mut().click_chain.reset();
                    u8::from(matches!(m.kind, MouseKind::Down(_)))
                }
            },
            _ => 0,
        };
        let target = match event {
            UiEvent::Mouse(m) => {
                // Capture redirects every mouse event; a capture whose
                // instance was disposed HEALS by re-pointing at the
                // press cell's current occupant (`validated_capture` —
                // Button's pressed re-render kills the raw id on the
                // very Down that captured it).
                let captured = self.validated_capture();
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
            click_count,
            layer_origin: self.core.borrow().layer_origin,
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
        // Chords compare NORMALIZED (`KeyChord::normalized`): a shifted
        // letter has two wire spellings (legacy Char('A'); kitty
        // Char('a')+SHIFT) and a registration must fire on both
        // (first-app 0286).
        if !consumed {
            if let UiEvent::Key(k) = event {
                let chord = k.chord().normalized();
                let mut winner: Option<Rc<RefCell<Vec<Shortcut>>>> = None;
                for id in &path {
                    let core = self.core.borrow();
                    if let Some(inst) = core.insts.get(id.0) {
                        if let InstPayload::Element { shortcuts, .. } = &inst.payload {
                            if shortcuts
                                .borrow()
                                .iter()
                                .any(|s| s.chord.normalized() == chord)
                            {
                                winner = Some(shortcuts.clone());
                            }
                        }
                    }
                }
                if let Some(shortcuts) = winner {
                    let mut list = shortcuts.borrow_mut();
                    if let Some(s) = list.iter_mut().find(|s| s.chord.normalized() == chord) {
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
                    {
                        let mut core = self.core.borrow_mut();
                        core.capture = Some(target);
                        // The press cell anchors the capture heal.
                        core.capture_pos = Some(m.pos);
                    }
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
                    {
                        let mut core = self.core.borrow_mut();
                        core.capture = None;
                        core.capture_pos = None;
                    }
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
            // The heal anchor follows the explicit request: a grant made
            // from a mouse event anchors at that event's cell; a grant
            // from a non-mouse event (or an explicit release) has no
            // press cell to heal at.
            core.capture_pos = match (core.capture, event) {
                (Some(_), UiEvent::Mouse(m)) => Some(m.pos),
                _ => None,
            };
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

    pub(in crate::ui) fn path_to(&self, target: ViewId) -> Vec<ViewId> {
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
    pub(in crate::ui) fn run_handlers(
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
