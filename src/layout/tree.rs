//! The layout node tree: a generational arena of styled nodes with
//! optional measure callbacks (text leaves), solved into absolute `Rect`s.
//!
//! Kept separate from the ui instance tree on purpose: layout is a pure
//! geometry solver that can be tested (and re-solved incrementally)
//! without touching reactive or event state.

use crate::base::{Rect, Size};
use crate::reactive::{GenArena, Key};

use super::style::Style;

/// Content measurement for leaves: given the available box, report the
/// desired size (e.g. text width x wrapped height). Must be pure —
/// called repeatedly during solving.
pub type MeasureFn = Box<dyn Fn(Size) -> Size>;

/// Handle to a layout node. Generational: removing a subtree invalidates
/// its ids, so a stale id from a disposed ui region cannot corrupt a
/// later solve.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct LayoutId(pub(crate) Key);

pub(crate) struct LayoutNode {
    pub style: Style,
    pub parent: Option<LayoutId>,
    pub children: Vec<LayoutId>,
    pub measure: Option<MeasureFn>,
    /// Solved absolute rectangle (screen coordinates of the last solve).
    pub rect: Rect,
}

/// Arena of layout nodes. One per ui tree; the ui layer owns the mapping
/// from view instances to `LayoutId`s.
#[derive(Default)]
pub struct LayoutTree {
    pub(crate) nodes: GenArena<LayoutNode>,
    /// Old+new rects of nodes whose geometry changed during the last
    /// solve — the damage feed for the compositor (a moved sibling must
    /// repaint even though its own content never changed).
    pub(crate) geometry_damage: Vec<Rect>,
}

impl LayoutTree {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add(&mut self, style: Style) -> LayoutId {
        LayoutId(self.nodes.insert(LayoutNode {
            style,
            parent: None,
            children: Vec::new(),
            measure: None,
            rect: Rect::ZERO,
        }))
    }

    pub fn add_leaf(&mut self, style: Style, measure: MeasureFn) -> LayoutId {
        let id = self.add(style);
        self.nodes.get_mut(id.0).expect("just added").measure = Some(measure);
        id
    }

    /// Append `child` under `parent`. Panics on stale ids — a stale id
    /// here means the ui layer's bookkeeping is broken, and silently
    /// ignoring it would desync layout from the instance tree.
    pub fn add_child(&mut self, parent: LayoutId, child: LayoutId) {
        assert!(
            self.nodes.contains(parent.0),
            "layout: add_child on stale parent"
        );
        {
            let c = self
                .nodes
                .get_mut(child.0)
                .expect("layout: add_child on stale child");
            debug_assert!(c.parent.is_none(), "layout: child already attached");
            c.parent = Some(parent);
        }
        self.nodes
            .get_mut(parent.0)
            .expect("checked")
            .children
            .push(child);
    }

    /// Remove a node and its whole subtree. Detaches from the parent's
    /// child list if the parent is still alive. Stale ids are a no-op
    /// (the ui layer removes subtrees whose parents may already be gone).
    pub fn remove(&mut self, id: LayoutId) {
        let Some(node) = self.nodes.get(id.0) else {
            return;
        };
        if let Some(parent) = node.parent {
            if let Some(p) = self.nodes.get_mut(parent.0) {
                p.children.retain(|c| *c != id);
            }
        }
        // Iterative subtree teardown (no recursion: ui trees can be deep).
        let mut stack = vec![id];
        while let Some(cur) = stack.pop() {
            if let Some(node) = self.nodes.remove(cur.0) {
                stack.extend(node.children);
            }
        }
    }

    pub fn set_style(&mut self, id: LayoutId, style: Style) {
        if let Some(node) = self.nodes.get_mut(id.0) {
            node.style = style;
        }
    }

    pub fn style(&self, id: LayoutId) -> Option<&Style> {
        self.nodes.get(id.0).map(|n| &n.style)
    }

    pub fn set_measure(&mut self, id: LayoutId, measure: Option<MeasureFn>) {
        if let Some(node) = self.nodes.get_mut(id.0) {
            node.measure = measure;
        }
    }

    /// Solved rectangle from the last `solve` call (absolute coords).
    pub fn rect(&self, id: LayoutId) -> Rect {
        self.nodes.get(id.0).map(|n| n.rect).unwrap_or(Rect::ZERO)
    }

    /// Set a node's rect, recording damage when it actually moved/resized.
    pub(crate) fn assign_rect(&mut self, id: LayoutId, rect: Rect) {
        if let Some(node) = self.nodes.get_mut(id.0) {
            if node.rect != rect {
                let old = node.rect;
                node.rect = rect;
                if !old.is_empty() {
                    self.geometry_damage.push(old);
                }
                if !rect.is_empty() {
                    self.geometry_damage.push(rect);
                }
            }
        }
    }

    /// Drain the rects invalidated by geometry changes since last drain.
    pub fn take_geometry_damage(&mut self) -> Vec<Rect> {
        std::mem::take(&mut self.geometry_damage)
    }

    pub fn children(&self, id: LayoutId) -> &[LayoutId] {
        self.nodes
            .get(id.0)
            .map(|n| n.children.as_slice())
            .unwrap_or(&[])
    }

    pub fn parent(&self, id: LayoutId) -> Option<LayoutId> {
        self.nodes.get(id.0).and_then(|n| n.parent)
    }

    pub fn is_alive(&self, id: LayoutId) -> bool {
        self.nodes.contains(id.0)
    }

    pub fn len(&self) -> usize {
        self.nodes.live()
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.live() == 0
    }
}
