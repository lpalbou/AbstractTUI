//! Semantic hit targets for DRAWN spans (backlog app-widgets 0187).
//!
//! A draw closure tags what it paints with an [`ActionId`]
//! ([`StyledCanvas::print_action`](super::StyledCanvas::print_action),
//! [`StyledCanvas::mark_action`](super::StyledCanvas::mark_action)); the
//! engine records the tagged rects per drawing element, every time that
//! element draws, and hit-tests pointer presses and keyboard activation
//! back to the id. The nearest [`Element::on_action`](super::Element::on_action)
//! host on the hit path receives the activation. Apps never mirror their
//! draw arithmetic to know where a span landed.
//!
//! ## The record
//!
//! - Spans are recorded by the element whose draw closure painted them
//!   (the DRAWER). Each draw of that element REPLACES its previous spans,
//!   so a span that is no longer painted stops being clickable on the
//!   next frame that repaints the drawer.
//! - They are stored relative to the drawer's solved rect: a layout move
//!   that does not repaint the drawer still hit-tests at the new place.
//! - They are clipped to the drawer's rect at record time (a span printed
//!   past the element's edge is not clickable past it).
//! - They die with the drawer (unmount drops them).
//!
//! ## Activation
//!
//! - Mouse: a Left press on a span, then a Left release on the SAME span,
//!   is one activation. The press stops propagation (the span owns it).
//!   Among drawers on the pointer's hit path, the DEEPEST one with a
//!   span under the pointer wins; within one drawer, the span recorded
//!   last (painted on top) wins.
//! - Keyboard: only when the host ITSELF is focused (`on_action` does
//!   not make an element focusable — add [`Element::focusable`](super::Element::focusable)).
//!   Left/Right move a cursor across the host's spans in reading order
//!   (top-to-bottom, then left-to-right; wrapping), Enter/Space activate
//!   it. The cursor starts on the first span. The cursor span is the
//!   focus ring: `print_action` adds REVERSE to it while the host is
//!   focused; `mark_action` returns `true` for it so a custom ring can
//!   be drawn.

use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt;
use std::rc::Rc;

use crate::base::{Point, Rect};

use super::event::{EventCtx, Key, Mods, MouseButton, MouseKind, UiEvent};
use super::tree::{InstPayload, TreeCore, ViewId};

/// An app-chosen name for something a drawn span does ("pick-model",
/// "open-session"). Cheap to clone; compared by value.
///
/// ```
/// use abstracttui::ui::ActionId;
/// let id = ActionId::from("pick-model");
/// assert_eq!(id, "pick-model");
/// assert_eq!(id.as_str(), "pick-model");
/// ```
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ActionId(Rc<str>);

impl ActionId {
    pub fn new(id: impl AsRef<str>) -> ActionId {
        ActionId(Rc::from(id.as_ref()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for ActionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ActionId({:?})", &*self.0)
    }
}

impl fmt::Display for ActionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<&str> for ActionId {
    fn from(s: &str) -> ActionId {
        ActionId::new(s)
    }
}

impl From<String> for ActionId {
    fn from(s: String) -> ActionId {
        ActionId(Rc::from(s))
    }
}

impl From<&ActionId> for ActionId {
    fn from(id: &ActionId) -> ActionId {
        id.clone()
    }
}

impl PartialEq<str> for ActionId {
    fn eq(&self, other: &str) -> bool {
        &*self.0 == other
    }
}

impl PartialEq<&str> for ActionId {
    fn eq(&self, other: &&str) -> bool {
        &*self.0 == *other
    }
}

/// One drawer's record: its rect at draw time and the spans it tagged
/// (absolute, in that draw's coordinates).
pub(super) struct DrawnActions {
    pub(super) origin: Point,
    pub(super) spans: Vec<(Rect, ActionId)>,
}

impl DrawnActions {
    /// The spans translated to where the drawer sits NOW.
    fn at(&self, now: Rect) -> impl Iterator<Item = (Rect, &ActionId)> + '_ {
        let (dx, dy) = (now.x - self.origin.x, now.y - self.origin.y);
        self.spans
            .iter()
            .map(move |(r, id)| (r.translate(dx, dy), id))
    }
}

/// Per-host state shared between its handler and the paint walk.
#[derive(Default)]
pub(crate) struct ActionHost {
    /// The keyboard cursor (an id, not an index: spans come and go).
    pub(crate) cursor: RefCell<Option<ActionId>>,
}

/// The recording frame for ONE drawer's draw closure.
struct Frame {
    clip: Rect,
    ring: Option<ActionId>,
    spans: Vec<(Rect, ActionId)>,
}

thread_local! {
    static FRAMES: RefCell<Vec<Frame>> = const { RefCell::new(Vec::new()) };
}

/// Open the frame a drawer's closure records into. `ring` is the
/// focused host's cursor when this drawer sits inside it.
pub(super) fn begin_record(drawer_rect: Rect, ring: Option<ActionId>) {
    FRAMES.with(|f| {
        f.borrow_mut().push(Frame {
            clip: drawer_rect,
            ring,
            spans: Vec::new(),
        })
    });
}

/// Close the innermost frame, returning what it recorded.
pub(super) fn end_record() -> Vec<(Rect, ActionId)> {
    FRAMES.with(|f| f.borrow_mut().pop().map(|fr| fr.spans).unwrap_or_default())
}

/// Record `rect` as `action` in the active frame. Returns whether the
/// span is the focused host's keyboard cursor. Outside a tree draw (a
/// bare canvas) this records nothing and returns `false`.
pub(crate) fn record(rect: Rect, action: &ActionId) -> bool {
    FRAMES.with(|f| {
        let mut frames = f.borrow_mut();
        let Some(frame) = frames.last_mut() else {
            return false;
        };
        let clipped = rect.intersect(frame.clip);
        if clipped.is_empty() {
            return false;
        }
        frame.spans.push((clipped, action.clone()));
        frame.ring.as_ref() == Some(action)
    })
}

impl TreeCore {
    /// Store a drawer's freshly recorded spans (an empty record clears).
    pub(super) fn store_actions(
        &mut self,
        drawer: ViewId,
        rect: Rect,
        spans: Vec<(Rect, ActionId)>,
    ) {
        if spans.is_empty() {
            self.action_spans.remove(&drawer);
        } else {
            self.action_spans.insert(
                drawer,
                DrawnActions {
                    origin: Point::new(rect.x, rect.y),
                    spans,
                },
            );
        }
    }

    fn rect_of_inst(&self, id: ViewId) -> Option<Rect> {
        self.insts.get(id.0).map(|i| self.layout.rect(i.layout))
    }

    pub(super) fn action_host(&self, id: ViewId) -> Option<Rc<ActionHost>> {
        match &self.insts.get(id.0)?.payload {
            InstPayload::Element { action_host, .. } => action_host.clone(),
            _ => None,
        }
    }

    /// The keyboard ring for a drawer: the focused host's cursor when
    /// the focused node is an action host that is this drawer or one of
    /// its ancestors.
    pub(super) fn ring_for(&self, drawer: ViewId) -> Option<ActionId> {
        let focus = self.focus?;
        let host = self.action_host(focus)?;
        let mut cur = Some(drawer);
        while let Some(id) = cur {
            if id == focus {
                return host.cursor.borrow().clone();
            }
            cur = self.insts.get(id.0).and_then(|i| i.parent);
        }
        None
    }

    /// The span under `pos` on `path` (root-first), deepest drawer first.
    pub(super) fn action_at(&self, path: &[ViewId], pos: Point) -> Option<ActionId> {
        for id in path.iter().rev() {
            let (Some(rec), Some(now)) = (self.action_spans.get(id), self.rect_of_inst(*id)) else {
                continue;
            };
            let hit = rec
                .at(now)
                .filter(|(r, _)| r.contains(pos))
                .last()
                .map(|(_, a)| a.clone());
            if hit.is_some() {
                return hit;
            }
        }
        None
    }

    /// Every live span in `root`'s subtree, reading order (y, then x),
    /// first occurrence of each id.
    pub(super) fn actions_in(&self, root: ViewId) -> Vec<(ActionId, Rect)> {
        let mut out: Vec<(ActionId, Rect)> = Vec::new();
        let mut stack = vec![root];
        while let Some(id) = stack.pop() {
            let Some(inst) = self.insts.get(id.0) else {
                continue;
            };
            if let (Some(rec), Some(now)) = (self.action_spans.get(&id), self.rect_of_inst(id)) {
                out.extend(rec.at(now).map(|(r, a)| (a.clone(), r)));
            }
            stack.extend(inst.children.iter().rev().copied());
        }
        out.sort_by_key(|(_, r)| (r.y, r.x));
        let mut seen = std::collections::HashSet::new();
        out.retain(|(a, _)| seen.insert(a.clone()));
        out
    }
}

/// The per-tree span store.
pub(super) type ActionSpans = HashMap<ViewId, DrawnActions>;

/// The handler `Element::on_action` installs (bubble phase).
pub(super) fn host_handler(
    host: Rc<ActionHost>,
    mut f: impl FnMut(&mut EventCtx, &ActionId) + 'static,
) -> impl FnMut(&mut EventCtx, &UiEvent) + 'static {
    let mut pressed: Option<ActionId> = None;
    move |ctx, ev| match ev {
        UiEvent::Mouse(m) => match m.kind {
            MouseKind::Down(MouseButton::Left) => {
                pressed = ctx.action.clone();
                if let Some(id) = &pressed {
                    *host.cursor.borrow_mut() = Some(id.clone());
                    ctx.stop_propagation();
                }
            }
            MouseKind::Up(MouseButton::Left) => {
                if let Some(id) = pressed.take() {
                    ctx.stop_propagation();
                    if ctx.action.as_ref() == Some(&id) {
                        f(ctx, &id);
                    }
                }
            }
            _ => {}
        },
        UiEvent::FocusIn => {
            let mut cursor = host.cursor.borrow_mut();
            let valid = cursor
                .as_ref()
                .is_some_and(|c| ctx.host_actions.contains(c));
            if !valid {
                *cursor = ctx.host_actions.first().cloned();
            }
        }
        UiEvent::Key(k) if ctx.target.is_some() && ctx.target == ctx.current => {
            if k.mods != Mods::NONE {
                return; // modified chords stay the app's
            }
            let list = &ctx.host_actions;
            if list.is_empty() {
                return;
            }
            let at = {
                let cursor = host.cursor.borrow();
                cursor
                    .as_ref()
                    .and_then(|c| list.iter().position(|a| a == c))
                    .unwrap_or(0)
            };
            match k.key {
                Key::Left | Key::Right if list.len() > 1 => {
                    let n = list.len();
                    let next = if k.key == Key::Right {
                        (at + 1) % n
                    } else {
                        (at + n - 1) % n
                    };
                    *host.cursor.borrow_mut() = Some(list[next].clone());
                    ctx.request_repaint();
                    ctx.stop_propagation();
                }
                Key::Enter | Key::Char(' ') => {
                    let id = list[at].clone();
                    *host.cursor.borrow_mut() = Some(id.clone());
                    ctx.stop_propagation();
                    f(ctx, &id);
                }
                _ => {}
            }
        }
        _ => {}
    }
}
