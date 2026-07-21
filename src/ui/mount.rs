//! Mounting blueprints into live instances — including the `Dyn`
//! reactive-region lifecycle, the load-bearing piece of the whole bet.
//!
//! ## Lifecycle model (the part REDTEAM should attack)
//!
//! Mounting a `Dyn` creates ONE effect owned by the surrounding scope.
//! Each run of that effect: (1) disposes the previous run's child scope —
//! which runs cleanups that remove the previous instances and layout
//! nodes — (2) creates a fresh child scope, (3) evaluates the build
//! closure TRACKED (this subscribes the region to exactly the signals it
//! reads), (4) mounts the produced subtree untracked, (5) damages the
//! region and requests a frame. Unmounting is therefore never a special
//! case: disposing any ancestor scope cascades through effect disposal
//! and the registered cleanups; instance/layout bookkeeping cannot leak
//! as long as cleanups run — which the reactive layer guarantees even on
//! re-run (test-pinned there).

use std::cell::RefCell;
use std::rc::Rc;

use crate::base::{Rect, Size};
use crate::reactive::{request_frame, untrack, Scope};

use super::tree::{Inst, InstPayload, TreeCore, ViewId};
use super::view::{View, ViewNode};

/// Recursively mount a blueprint under `parent`. Borrows of the core are
/// short bursts — never held across child mounts or reactive calls.
pub(super) fn mount_view(
    core: &Rc<RefCell<TreeCore>>,
    cx: Scope,
    view: View,
    parent: Option<ViewId>,
) -> ViewId {
    match view.0 {
        ViewNode::Element(mut el) => {
            let (id, layout) = {
                let mut c = core.borrow_mut();
                let mut style = el.style.clone();
                if let Some(floor) = el.padding_floor {
                    apply_padding_floor(&mut style, floor);
                }
                let layout = c.layout.add(style);
                let id = ViewId(c.insts.insert(Inst {
                    parent,
                    children: Vec::new(),
                    layout,
                    focusable: el.focusable,
                    focus_trap: el.focus_trap,
                    focus_memory: el.focus_memory,
                    access: el.access.clone(),
                    payload: InstPayload::Element {
                        draw: el.draw.map(|d| Rc::new(RefCell::new(d))),
                        handlers: Rc::new(RefCell::new(el.handlers)),
                        shortcuts: Rc::new(RefCell::new(el.shortcuts)),
                    },
                }));
                if el.autofocus {
                    // Recorded now, consumed AFTER the mount completes
                    // (focus delivery runs handlers; the core borrow
                    // must be released first). Last-mounted wins.
                    c.pending_autofocus = Some(id);
                }
                attach(&mut c, parent, id);
                (id, layout)
            };
            // Reactive layout style: re-applied on signal change WITHOUT
            // remounting (scroll offsets, animated panes). The effect is
            // owned by the mounting scope, so it dies with the subtree;
            // a stale layout id after removal is a no-op (generational).
            //
            // Invalidation is INCREMENTAL: the re-solve anchor is the
            // nearest ancestor whose own size cannot be affected by this
            // node changing (climb past Auto-sized ancestors — an
            // Auto-sized parent inherits its children's size, so the
            // change bubbles through it; a Cells/Percent/grow-sized one
            // absorbs it inside its fixed box). resolve_subtree(anchor)
            // then recomputes every affected rect, including this node's
            // own (assigned by its parent's pass) and displaced siblings.
            if let Some(mut style_fn) = el.style_fn.take() {
                let core_for_style = core.clone();
                let floor = el.padding_floor;
                cx.effect(move || {
                    let mut style = style_fn(); // tracked
                    if let Some(f) = floor {
                        // The chrome floor survives reactive styles too.
                        apply_padding_floor(&mut style, f);
                    }
                    let mut c = core_for_style.borrow_mut();
                    if !c.layout.is_alive(layout) {
                        return;
                    }
                    c.layout.set_style(layout, style);
                    let anchor = resolve_anchor(&c.layout, layout);
                    c.dirty_subtrees.push(anchor);
                    drop(c);
                    request_frame();
                });
            }
            for child in el.children {
                mount_view(core, cx, child, Some(id));
            }
            id
        }
        ViewNode::Text(t) => {
            let mut c = core.borrow_mut();
            let content = t.content;
            let measured = content.clone();
            // Measurement through the engine's ONE width authority:
            // text::measure is wrap-aware (newlines + width-constrained
            // wrapping), so a multi-line leaf reports its true block
            // size instead of one enormous line (the RT3-4 repro's inner
            // content collapsed a sibling through exactly that).
            let layout = c.layout.add_leaf(
                t.style,
                Box::new(move |avail: Size| crate::text::measure(&measured, avail)),
            );
            let id = ViewId(c.insts.insert(Inst {
                parent,
                children: Vec::new(),
                layout,
                focusable: false,
                focus_trap: false,
                focus_memory: false,
                access: Default::default(),
                payload: InstPayload::Text { content },
            }));
            attach(&mut c, parent, id);
            id
        }
        ViewNode::Dyn(d) => {
            let dyn_id = {
                let mut c = core.borrow_mut();
                let layout = c.layout.add(d.style.clone());
                let id = ViewId(c.insts.insert(Inst {
                    parent,
                    children: Vec::new(),
                    layout,
                    focusable: false,
                    focus_trap: false,
                    focus_memory: false,
                    access: Default::default(),
                    payload: InstPayload::Dyn,
                }));
                attach(&mut c, parent, id);
                id
            };
            let mut build = d.build;
            let core_for_effect = core.clone();
            // One scope per render generation, disposed before the next.
            let holder: Rc<RefCell<Option<Scope>>> = Rc::new(RefCell::new(None));
            cx.effect(move || {
                if let Some(prev) = holder.borrow_mut().take() {
                    // Runs the previous generation's cleanups: instances
                    // and layout nodes of the old subtree are removed.
                    prev.dispose();
                }
                // TRACKED: the signals read while building subscribe this
                // region — the fine-grained re-render unit. The build
                // receives the GENERATION scope (dyn_view_scoped): state
                // created on it dies at the next rebuild.
                let child_cx = cx.child();
                let view = build(child_cx);
                *holder.borrow_mut() = Some(child_cx);
                let core2 = core_for_effect.clone();
                // Mount UNTRACKED: bookkeeping must not add dependencies.
                let mounted = untrack(|| mount_view(&core2, child_cx, view, Some(dyn_id)));
                let core3 = core_for_effect.clone();
                child_cx.on_cleanup(move || remove_subtree(&core3, mounted));
                // A mounted subtree may carry an autofocus node (a
                // dialog's default field appearing via Dyn). The request
                // stays PARKED in `pending_autofocus` — focus delivery
                // runs user handlers (`focus_signal` writes), and firing
                // those inside this effect re-enters the running
                // computation through the flush: the 0220 "dependency
                // cycle" mount panic. Safe consume points, both outside
                // every computation: `UiTree::mount` right after the
                // initial mount returns, and `UiTree::layout` (frame
                // phase L) for regenerations — the `request_frame`
                // below guarantees that layout happens.
                {
                    let mut c = core_for_effect.borrow_mut();
                    // Old content's region is stale; a structure change
                    // may also move siblings, so re-solve and damage the
                    // region (whole-tree damage only on first mount when
                    // no rect is known yet).
                    let rect = c
                        .insts
                        .get(dyn_id.0)
                        .map(|inst| c.layout.rect(inst.layout))
                        .unwrap_or(Rect::ZERO);
                    if rect.is_empty() {
                        c.damage_all();
                    } else {
                        c.damage_rect(rect);
                    }
                    c.needs_layout = true;
                }
                request_frame();
            });
            dyn_id
        }
    }
}

/// Per-side maximum: the floor holds where the user style is smaller,
/// user padding beyond it wins (RT8-7 merge semantics).
fn apply_padding_floor(style: &mut crate::layout::Style, floor: crate::layout::Edges) {
    style.padding.left = style.padding.left.max(floor.left);
    style.padding.right = style.padding.right.max(floor.right);
    style.padding.top = style.padding.top.max(floor.top);
    style.padding.bottom = style.padding.bottom.max(floor.bottom);
}

/// Re-solve anchor for a style change on `node`: the nearest ancestor
/// whose OWN size the change cannot alter. The climb starts at the
/// parent unconditionally — the parent's pass assigns `node`'s rect, and
/// judging by the node's own style would be wrong in both directions
/// (the OLD style is already gone: an Auto→fixed flip changed what the
/// node fed into an Auto parent's sizing). From there, climb while the
/// ancestor itself is content-sized (Auto on either axis): its size is
/// derived from children, so the change propagates through it into ITS
/// parent's arithmetic. The first fully-sized ancestor absorbs the
/// change inside its fixed box.
fn resolve_anchor(
    layout: &crate::layout::LayoutTree,
    node: crate::layout::LayoutId,
) -> crate::layout::LayoutId {
    use crate::layout::Dimension;
    let content_sized = |id: crate::layout::LayoutId| {
        layout
            .style(id)
            .map(|s| matches!(s.width, Dimension::Auto) || matches!(s.height, Dimension::Auto))
            .unwrap_or(true)
    };
    let Some(mut cur) = layout.parent(node) else {
        return node;
    };
    while content_sized(cur) {
        match layout.parent(cur) {
            Some(p) => cur = p,
            None => break,
        }
    }
    cur
}

fn attach(core: &mut TreeCore, parent: Option<ViewId>, child: ViewId) {
    if let Some(p) = parent {
        if let Some(pinst) = core.insts.get_mut(p.0) {
            pinst.children.push(child);
        }
        if let (Some(pl), Some(cl)) = (
            core.insts.get(p.0).map(|i| i.layout),
            core.insts.get(child.0).map(|i| i.layout),
        ) {
            core.layout.add_child(pl, cl);
        }
    }
}

/// Remove a mounted subtree: instances, layout nodes, parent link, focus.
/// Registered as the Dyn generation cleanup; also safe on ids already
/// gone (generational arena shrugs at stale keys).
pub(super) fn remove_subtree(core: &Rc<RefCell<TreeCore>>, root: ViewId) {
    let mut c = core.borrow_mut();
    let Some(root_inst) = c.insts.get(root.0) else {
        return;
    };
    let root_layout = root_inst.layout;
    let parent = root_inst.parent;
    let root_rect = c.layout.rect(root_layout);
    // Detach from parent's child list.
    if let Some(p) = parent {
        if let Some(pinst) = c.insts.get_mut(p.0) {
            pinst.children.retain(|k| *k != root);
        }
    }
    // Layout subtree removal is recursive inside LayoutTree.
    c.layout.remove(root_layout);
    // Instance subtree removal, iterative.
    let mut stack = vec![root];
    while let Some(id) = stack.pop() {
        if let Some(inst) = c.insts.remove(id.0) {
            stack.extend(inst.children);
            if c.focus == Some(id) {
                // Focused node vanished with its subtree: drop focus
                // rather than pointing at a corpse. (Focus restoration
                // policy is a widgets-layer concern.)
                c.focus = None;
            }
        }
    }
    c.damage_rect(root_rect);
    c.needs_layout = true;
}
