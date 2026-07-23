//! Overlay input routing: topmost-z-first event dispatch, the
//! outside-press contract, and pointer-press cancellation across
//! layer trees. `#[path]` sibling of overlays.rs (file-size split) —
//! same `impl Overlays`, different file.
//!
//! OWNER: OVERLAY.

use crate::base::{Point, Rect};
use crate::ui::UiTree;

use super::{OverlayContent, Overlays};

impl Overlays {
    /// Route an input event through overlay trees, topmost-z first.
    /// Returns Some(consumed) when an overlay owned the event (a MODAL
    /// overlay owns EVERYTHING while visible); None = fall through to
    /// the root tree.
    pub(crate) fn dispatch(&self, event: &crate::ui::UiEvent) -> Option<bool> {
        use crate::ui::UiEvent;
        // Snapshot (tree handle, modal, bounds, z) without holding the
        // borrow while user handlers run.
        let mut targets: Vec<(UiTree, bool, Rect, i32, u64)> = {
            let store = self.store.borrow();
            store
                .meta
                .iter()
                .zip(&store.layers)
                .filter(|(_, l)| l.visible())
                .filter_map(|(m, l)| match &m.content {
                    OverlayContent::Tree { tree, modal, .. } => {
                        Some((tree.handle(), *modal, l.bounds(), l.z(), m.id))
                    }
                    _ => None,
                })
                .collect()
        };
        targets.sort_by_key(|(_, _, _, z, _)| std::cmp::Reverse(*z));
        let mut fell_through_press = false;
        for (tree, modal, bounds, _, id) in targets.iter() {
            let mut tree = tree.handle();
            let modal = *modal;
            let bounds = *bounds;
            match event {
                UiEvent::Mouse(m) => {
                    if modal || bounds.contains(m.pos) {
                        // A press OUTSIDE a modal's bounds is swallowed
                        // AND reported to its dismiss hook (menus close;
                        // the press never acts below — deliberate).
                        if modal
                            && !bounds.contains(m.pos)
                            && matches!(m.kind, crate::ui::MouseKind::Down(_))
                        {
                            self.fire_outside_press(*id);
                            return Some(true);
                        }
                        // Overlay trees live in layer-local coordinates.
                        let mut local = *m;
                        local.pos = Point::new(m.pos.x - bounds.x, m.pos.y - bounds.y);
                        let consumed = tree.dispatch(&UiEvent::Mouse(local));
                        if modal {
                            return Some(true); // modals swallow even misses
                        }
                        // The panel is OPAQUE: pointer events over it are
                        // its own even when no handler consumed them —
                        // click-through to covered content would act on
                        // things the user cannot see.
                        return Some(consumed);
                    }
                    if matches!(m.kind, crate::ui::MouseKind::Down(_)) {
                        fell_through_press = true;
                    }
                }
                UiEvent::Key(_) | UiEvent::Paste(_) => {
                    if modal {
                        return Some(tree.dispatch(event));
                    }
                    // NON-MODAL KEY RULE (cycle 5): the topmost overlay
                    // tree HOLDING FOCUS owns keys — same opacity logic
                    // as the pointer rule (a focused popup's Escape must
                    // not also scroll the app). No focused overlay =
                    // keys fall to the root.
                    if tree.focused().is_some() {
                        return Some(tree.dispatch(event));
                    }
                }
                _ => {}
            }
        }
        // A press that landed on the ROOT (outside every overlay) steals
        // key focus back from non-modal overlays: one focus story across
        // trees — click where you want your keys to go.
        if fell_through_press {
            for (tree, modal, _, _, _) in targets.iter() {
                if !modal {
                    tree.handle().set_focus(None);
                }
            }
        }
        None
    }

    /// Take-out/run/put-back a modal's outside-press callback — user
    /// code never runs under the store borrow, and the callback may
    /// remove the layer it belongs to (a menu closing itself).
    fn fire_outside_press(&self, id: u64) {
        let taken = {
            let mut store = self.store.borrow_mut();
            store
                .index_of(id)
                .and_then(|i| match &mut store.meta[i].content {
                    OverlayContent::Tree { on_outside, .. } => on_outside.take(),
                    _ => None,
                })
        };
        let Some(mut f) = taken else { return };
        f();
        let mut store = self.store.borrow_mut();
        if let Some(i) = store.index_of(id) {
            if let OverlayContent::Tree { on_outside, .. } = &mut store.meta[i].content {
                *on_outside = Some(f);
            }
        }
    }

    /// Cancel in-progress pointer presses in every overlay tree (0285:
    /// the selection layer claimed a passed-through gesture — see
    /// `UiTree::cancel_pointer_press`). Snapshot handles first — widget
    /// un-press handlers must never run under the store borrow. Covers
    /// hidden layers too: a stale capture is worth dropping wherever it
    /// lives.
    pub(crate) fn cancel_pointer_press(&self) {
        let trees: Vec<UiTree> = {
            let store = self.store.borrow();
            store
                .meta
                .iter()
                .filter_map(|m| match &m.content {
                    OverlayContent::Tree { tree, .. } => Some(tree.handle()),
                    _ => None,
                })
                .collect()
        };
        for mut tree in trees {
            tree.cancel_pointer_press();
        }
    }
}
