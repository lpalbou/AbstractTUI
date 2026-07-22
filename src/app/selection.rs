//! Screen-text selection + clipboard copy (backlog 0270, tiers 2+3).
//!
//! Terminals in mouse-capture mode route drags to the APPLICATION, so
//! native text selection stops working the moment an app enables wheel
//! scrolling. This module ships the engine-side answers:
//!
//! - **Tier 2 — [`mouse_capture`]**: a cloneable handle that suspends and
//!   resumes mouse reporting at runtime (`set_mouse_reporting(bool)`
//!   shape). While suspended the terminal owns the pointer again — native
//!   drag-select works at native quality; keys still arrive, so apps
//!   typically resume on the next keypress.
//! - **Tier 3 — [`selection`]**: an opt-in engine selection layer. While
//!   enabled, a left-button drag paints a selection highlight (theme
//!   `selection_fg`/`selection_bg`) over the RENDERED screen; releasing
//!   the button — or pressing Enter / `c` / Ctrl+C while a selection is
//!   visible — copies the selected text to the system clipboard via
//!   OSC 52 through the presenter's byte custody. Esc or a click clears.
//!   Wheel scrolling is untouched (only left Down/Drag/Up are claimed).
//! - **[`copy_to_clipboard`]**: the app-reachable clipboard verb (backlog
//!   0150's clipboard leg) — queue any text for OSC 52 emission through
//!   the same custody path, from any component handler.
//!
//! ## Honesty: this is SCREEN-text extraction
//!
//! The copied text is what the composed frame shows — glyphs from the
//! flattened cell grid, wide glyphs never split, trailing whitespace
//! trimmed per row, rows joined with `\n`. It is NOT logical widget
//! content: soft-wrapped lines copy as separate rows, and content
//! scrolled out of view cannot be selected. The logical text↔cells
//! mapping is backlog 0160's remaining scope.
//!
//! ## Selection semantics (v1)
//!
//! LINEAR row-flow over the rendered screen, like a terminal's own
//! selection: the first row runs from the anchor to the pane's right
//! edge, middle rows span the pane, the last row runs from the pane's
//! left edge to the head cell (inclusive). Both endpoints clamp to the
//! PANE under the drag anchor — the content box of the nearest clipping
//! or padded ancestor of the deepest view hit at mouse-down (a `Scroll`
//! viewport, a bordered `Block`), else the whole tree — so unrelated
//! panes and border glyphs never leak into a copy.
//!
//! ## Rendering: a post-flatten frame patch
//!
//! The highlight honors the damage contract without a dedicated layer:
//! when the region changes, the old and new row rects are damaged on the
//! root layer BEFORE compositing (the compositor recomposes truth there,
//! full z-stack), and the selection inks are patched into the composed
//! frame AFTER — glyphs kept, colors replaced — so the diff emits only
//! cells that actually changed. Zero idle cost: with no active selection
//! the per-frame hook is one borrow and two empty checks, and the event
//! hook is one `enabled` test.

use std::cell::RefCell;
use std::rc::Rc;

use crate::base::{Point, Rect, Rgba};
use crate::input::{Event, KeyCode, KeyEventKind, Mods, MouseButton, MouseKind};
use crate::reactive::request_frame;
use crate::render::{Cell, Surface};

// ---------------------------------------------------------------------------
// Thread-local stores (house pattern: app::theme / app::viewport — the app
// runs on one thread; handles are cheap Rc clones of the same state).
// ---------------------------------------------------------------------------

thread_local! {
    static SELECTION: Rc<RefCell<SelectionState>> =
        Rc::new(RefCell::new(SelectionState::default()));
    static CAPTURE: Rc<RefCell<CaptureState>> =
        Rc::new(RefCell::new(CaptureState::default()));
}

/// The app-thread [`Selection`] handle (same state the driver renders).
pub fn selection() -> Selection {
    Selection {
        state: SELECTION.with(Rc::clone),
    }
}

/// The app-thread [`MouseCapture`] handle (tier-2 suspend verb).
pub fn mouse_capture() -> MouseCapture {
    MouseCapture {
        state: CAPTURE.with(Rc::clone),
    }
}

/// Queue `text` for an OSC 52 clipboard write through the presenter's
/// byte custody — reachable from any component handler (backlog 0150's
/// clipboard leg). The driver emits it with the next frame. Empty or
/// all-whitespace text is refused (an empty OSC 52 payload CLEARS the
/// clipboard — a surprise, never a copy). Capability honesty: terminals
/// that do not advertise OSC 52 get the bytes anyway (harmless — they
/// ignore the frame) plus a one-time labeled startup notice.
pub fn copy_to_clipboard(text: impl Into<String>) {
    let text = text.into();
    if text.trim().is_empty() {
        return;
    }
    SELECTION.with(|s| s.borrow_mut().pending_copies.push(text));
    request_frame();
}

// ---------------------------------------------------------------------------
// Selection (tier 3)
// ---------------------------------------------------------------------------

/// One selection region: anchor + head (screen cells, both inclusive),
/// row-flowed within `clamp` (the pane rect resolved at drag start).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) struct Region {
    anchor: Point,
    head: Point,
    clamp: Rect,
}

impl Region {
    /// Endpoints in reading order (by row, then column).
    fn ordered(&self) -> (Point, Point) {
        if (self.anchor.y, self.anchor.x) <= (self.head.y, self.head.x) {
            (self.anchor, self.head)
        } else {
            (self.head, self.anchor)
        }
    }

    /// Per-row screen rects of the linear flow: first row start→pane
    /// right, middle rows the full pane span, last row pane left→end
    /// (end cell inclusive). A single-row region spans min..=max.
    pub(crate) fn row_spans(&self, out: &mut Vec<Rect>) {
        let (s, e) = self.ordered();
        for y in s.y..=e.y {
            let x0 = if y == s.y { s.x } else { self.clamp.x };
            let x1 = if y == e.y {
                e.x + 1
            } else {
                self.clamp.right()
            };
            let rect = Rect::new(x0, y, x1 - x0, 1).intersect(self.clamp);
            if !rect.is_empty() {
                out.push(rect);
            }
        }
    }
}

/// What the driver should do with an intercepted event.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) enum SelectionAct {
    /// Not ours: route normally.
    Pass,
    /// Consumed by the selection layer (state may have changed).
    Consumed,
    /// Consumed, and the active region should be copied.
    Copy,
}

#[derive(Default)]
struct SelectionState {
    enabled: bool,
    /// Left button is down: (anchor cell, pane clamp). No region until a
    /// drag arrives — a plain click never paints.
    drag: Option<(Point, Rect)>,
    region: Option<Region>,
    /// Row rects patched into the last composed frame (the repair-damage
    /// source when the region changes or clears).
    painted: Vec<Rect>,
    /// Region changed since the last paint: pre-flatten damage owed.
    dirty: bool,
    /// App-queued clipboard texts ([`copy_to_clipboard`]), drained by the
    /// driver into presenter-custody OSC 52 emission.
    pending_copies: Vec<String>,
}

/// Cloneable handle to the engine selection layer. Enabling is the
/// opt-in: apps bind a key to [`Selection::toggle`] for a "select mode",
/// or call [`Selection::set_enabled`] once for always-on drag select
/// (sensible when left-click has no other meaning in the app — while
/// enabled, the selection layer owns left Down/Drag/Up).
#[derive(Clone)]
pub struct Selection {
    state: Rc<RefCell<SelectionState>>,
}

impl Selection {
    /// Turn select mode on or off. Disabling clears any visible
    /// selection (its cells repaint from truth on the next frame).
    pub fn set_enabled(&self, on: bool) {
        let mut st = self.state.borrow_mut();
        if st.enabled == on {
            return;
        }
        st.enabled = on;
        if !on {
            Self::clear_locked(&mut st);
        }
    }

    /// Whether select mode is on.
    pub fn enabled(&self) -> bool {
        self.state.borrow().enabled
    }

    /// Flip select mode; returns the new state.
    pub fn toggle(&self) -> bool {
        let on = !self.enabled();
        self.set_enabled(on);
        on
    }

    /// Whether a selection region is currently visible.
    pub fn is_active(&self) -> bool {
        self.state.borrow().region.is_some()
    }

    /// Clear the visible selection (keeps select mode on).
    pub fn clear(&self) {
        Self::clear_locked(&mut self.state.borrow_mut());
    }

    fn clear_locked(st: &mut SelectionState) {
        st.drag = None;
        if st.region.take().is_some() || !st.painted.is_empty() {
            st.dirty = true;
            request_frame();
        }
    }

    // ---- driver plumbing (crate-internal) --------------------------------

    /// Route one kernel event through the selection layer. `clamp_at`
    /// resolves the pane rect under a fresh drag anchor (called at most
    /// once, on left-down). See the module docs for the claim rules —
    /// wheel, motion, and non-left buttons always pass.
    pub(crate) fn on_input(
        &self,
        ev: &Event,
        clamp_at: &mut dyn FnMut(Point) -> Rect,
    ) -> SelectionAct {
        let mut st = self.state.borrow_mut();
        match ev {
            Event::Mouse(m) => {
                if !st.enabled {
                    return SelectionAct::Pass;
                }
                match (m.kind, m.button) {
                    (MouseKind::Down, MouseButton::Left) => {
                        // A click clears (spec: Esc/click clears); the new
                        // anchor arms a fresh drag against its own pane.
                        if st.region.take().is_some() {
                            st.dirty = true;
                            request_frame();
                        }
                        let clamp = clamp_at(m.pos);
                        st.drag = Some((clamp_point(m.pos, clamp), clamp));
                        SelectionAct::Consumed
                    }
                    (MouseKind::Drag, MouseButton::Left) => {
                        let Some((anchor, clamp)) = st.drag else {
                            // Drag whose Down predates select mode: not ours.
                            return SelectionAct::Pass;
                        };
                        let next = Region {
                            anchor,
                            head: clamp_point(m.pos, clamp),
                            clamp,
                        };
                        if st.region != Some(next) {
                            st.region = Some(next);
                            st.dirty = true;
                            request_frame();
                        }
                        SelectionAct::Consumed
                    }
                    (MouseKind::Up, MouseButton::Left) => {
                        if st.drag.take().is_none() {
                            return SelectionAct::Pass; // orphan release
                        }
                        if st.region.is_some() {
                            SelectionAct::Copy // release copies; region stays visible
                        } else {
                            SelectionAct::Consumed // the click's paired release
                        }
                    }
                    _ => SelectionAct::Pass, // wheel / motion / other buttons
                }
            }
            Event::Key(k) => {
                // Copy/clear keys exist only while a selection is VISIBLE
                // (Ctrl+C stays the default quit otherwise).
                if st.region.is_none() || k.kind == KeyEventKind::Release {
                    return SelectionAct::Pass;
                }
                let mods = k.mods.without_locks();
                match k.code {
                    KeyCode::Esc if mods == Mods::NONE => {
                        Self::clear_locked(&mut st);
                        SelectionAct::Consumed
                    }
                    KeyCode::Enter if mods == Mods::NONE => SelectionAct::Copy,
                    KeyCode::Char('c') if mods == Mods::NONE || mods == Mods::CTRL => {
                        SelectionAct::Copy
                    }
                    _ => SelectionAct::Pass,
                }
            }
            _ => SelectionAct::Pass,
        }
    }

    /// The active region, for extraction at copy time.
    pub(crate) fn active_region(&self) -> Option<Region> {
        self.state.borrow().region
    }

    /// Pre-flatten hook: when the region changed since the last paint,
    /// damage the OLD painted rects (their cells must recompose from
    /// truth) and the NEW row spans (fresh truth to patch over) on the
    /// root layer surface. Root layer origin is `Point::ZERO`, so screen
    /// and layer-local coordinates coincide.
    pub(crate) fn add_flatten_damage(&self, root: &mut Surface) {
        let mut st = self.state.borrow_mut();
        if !st.dirty {
            return;
        }
        st.dirty = false;
        for &r in &st.painted {
            root.add_damage(r);
        }
        let mut spans = Vec::new();
        if let Some(region) = st.region {
            region.row_spans(&mut spans);
        }
        for r in spans {
            root.add_damage(r);
        }
    }

    /// Post-flatten hook: patch the selection inks over the composed
    /// frame — glyphs kept, `fg`/`bg` replaced — and record what was
    /// painted. Runs every rendered frame while a selection is visible;
    /// cells the compositor did not touch receive byte-identical
    /// rewrites, so the diff emits nothing for them.
    pub(crate) fn paint_into(&self, frame: &mut Surface, fg: Rgba, bg: Rgba) {
        let mut st = self.state.borrow_mut();
        let Some(region) = st.region else {
            st.painted.clear();
            return;
        };
        st.painted.clear();
        let mut spans = Vec::new();
        region.row_spans(&mut spans);
        for span in spans {
            let Some((x0, x1)) = expand_to_glyphs(frame, span) else {
                continue;
            };
            for x in x0..x1 {
                let Some(&cell) = frame.get(x, span.y) else {
                    continue;
                };
                if cell.is_continuation() {
                    continue; // the leader's inks mirror over (pair repair)
                }
                frame.put_composed(x, span.y, Cell { fg, bg, ..cell });
            }
            frame.repair_wide_pairs(span.y, x0, x1);
            st.painted.push(Rect::new(x0, span.y, x1 - x0, 1));
        }
    }

    /// App-queued clipboard texts (drained once per turn by the driver).
    pub(crate) fn take_pending_copies(&self) -> Vec<String> {
        std::mem::take(&mut self.state.borrow_mut().pending_copies)
    }

    /// Fresh-driver reset: a new session starts with no visible
    /// selection and no paint bookkeeping (the new frame is blank), but
    /// the app's mode choice (`enabled`) survives.
    pub(crate) fn reset_session(&self) {
        let mut st = self.state.borrow_mut();
        st.drag = None;
        st.region = None;
        st.painted.clear();
        st.dirty = false;
    }

    /// Resize invalidates screen-space geometry wholesale: clear the
    /// selection (the driver's prev-poison repaints everything anyway).
    pub(crate) fn on_resize(&self) {
        let mut st = self.state.borrow_mut();
        st.drag = None;
        st.region = None;
        st.painted.clear();
        st.dirty = false;
    }
}

/// Clamp `p` into `rect` (inclusive cell coordinates). An empty rect
/// clamps to its origin — callers never produce one (pane rects come
/// from solved layout, the viewport fallback is never empty).
fn clamp_point(p: Point, rect: Rect) -> Point {
    Point::new(
        p.x.clamp(rect.x, (rect.right() - 1).max(rect.x)),
        p.y.clamp(rect.y, (rect.bottom() - 1).max(rect.y)),
    )
}

/// Snap a row span outward so it never splits a wide glyph: a span
/// starting ON a continuation pulls in its leader; a span whose first
/// excluded cell is a continuation pulls it in (its leader is inside).
/// Returns the clamped `x0..x1` walk range, `None` when off-surface.
fn expand_to_glyphs(frame: &Surface, span: Rect) -> Option<(i32, i32)> {
    if span.y < 0 || span.y >= frame.height() {
        return None;
    }
    let mut x0 = span.x.max(0);
    let mut x1 = span.right().min(frame.width());
    if x1 <= x0 {
        return None;
    }
    if frame.get(x0, span.y).is_some_and(Cell::is_continuation) {
        x0 = (x0 - 1).max(0);
    }
    if x1 < frame.width() && frame.get(x1, span.y).is_some_and(Cell::is_continuation) {
        x1 += 1;
    }
    Some((x0, x1))
}

/// What-you-see text of `region` over the composed frame: glyphs resolve
/// through the frame's pool, wide glyphs render once (leader), never
/// split; blank cells read as spaces; trailing whitespace trims per row;
/// rows join with `\n`. Colors are irrelevant — the selection patch only
/// recolors, so extraction after painting reads the same glyphs.
pub(crate) fn extract_text(frame: &Surface, region: &Region) -> String {
    let mut spans = Vec::new();
    region.row_spans(&mut spans);
    let mut out = String::new();
    for (i, span) in spans.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        let row_start = out.len();
        let Some((x0, x1)) = expand_to_glyphs(frame, *span) else {
            continue;
        };
        for x in x0..x1 {
            let Some(cell) = frame.get(x, span.y) else {
                continue;
            };
            if cell.is_continuation() {
                continue; // the leader already rendered both columns
            }
            let g = frame.glyph_str(cell);
            out.push_str(if g.is_empty() { " " } else { g });
        }
        let kept = out[row_start..].trim_end().len();
        out.truncate(row_start + kept);
    }
    out
}

/// Resolve the selection PANE under a fresh drag anchor: the topmost
/// visible overlay TREE layer containing the point answers with its own
/// pane (layer-local walk, mapped back to screen) — mirrors input
/// routing, so dragging over a modal clamps to the modal — else the root
/// tree answers, else the viewport. Always clipped to the viewport;
/// degenerate (empty) panes fall back to the viewport rather than
/// producing an unselectable region.
pub(super) fn selection_pane(
    app: &mut super::App,
    overlays: &super::overlays::Overlays,
    viewport: crate::base::Size,
    p: Point,
) -> Rect {
    use super::overlays::{OverlayContent, ROOT_LAYER_ID};
    let vp = Rect::from_size(viewport);
    let mut candidates: Vec<(crate::ui::UiTree, Rect, i32)> = {
        let store = overlays.store().borrow();
        store
            .meta
            .iter()
            .zip(&store.layers)
            .filter(|(m, l)| m.id != ROOT_LAYER_ID && l.visible())
            .filter_map(|(m, l)| match &m.content {
                OverlayContent::Tree { tree, .. } => Some((tree.handle(), l.bounds(), l.z())),
                _ => None,
            })
            .collect()
    };
    candidates.sort_by_key(|(_, _, z)| std::cmp::Reverse(*z));
    for (tree, bounds, _) in candidates {
        if bounds.contains(p) {
            let local = Point::new(p.x - bounds.x, p.y - bounds.y);
            let pane = tree
                .pane_rect_at(local)
                .map(|r| r.translate(bounds.x, bounds.y))
                .unwrap_or(bounds)
                .intersect(vp);
            return if pane.is_empty() { vp } else { pane };
        }
    }
    let pane = app.tree().pane_rect_at(p).unwrap_or(vp).intersect(vp);
    if pane.is_empty() {
        vp
    } else {
        pane
    }
}

// ---------------------------------------------------------------------------
// MouseCapture (tier 2)
// ---------------------------------------------------------------------------

#[derive(Default)]
struct CaptureState {
    /// Pending request (latest wins), drained by the driver's next turn.
    requested: Option<bool>,
    /// The last requested posture (for `is_suspended`).
    suspended: bool,
}

/// Cloneable handle suspending/resuming mouse reporting at runtime — the
/// tier-2 "native selection mode" verb. While suspended, the terminal
/// owns the pointer: native drag-select (and its clipboard) works at
/// native quality, and NO mouse events reach the app; keys still arrive,
/// so the conventional shape is suspend-on-keybinding, resume on the
/// next keypress. Requests apply at the driver's next turn (embedders
/// driving their own turns can call `Driver::set_mouse_reporting`
/// directly). Platform note: a job-control `Terminal::suspend` re-enters
/// with the original options, re-arming reporting — suspend again after
/// resume if you keep it off.
#[derive(Clone)]
pub struct MouseCapture {
    state: Rc<RefCell<CaptureState>>,
}

impl MouseCapture {
    /// Request mouse reporting on (`true`, the entered posture) or off
    /// (`false`, terminal-native selection). Latest request wins.
    pub fn set_reporting(&self, on: bool) {
        let mut st = self.state.borrow_mut();
        st.requested = Some(on);
        st.suspended = !on;
        drop(st);
        // A request from a posted job must wake an idle loop; from a key
        // handler this is a no-op frame the turn was running anyway.
        request_frame();
    }

    /// `set_reporting(false)`: hand the pointer back to the terminal.
    pub fn suspend(&self) {
        self.set_reporting(false);
    }

    /// `set_reporting(true)`: re-arm the entered mouse mode.
    pub fn resume(&self) {
        self.set_reporting(true);
    }

    /// The requested posture (applied by the driver's next turn).
    pub fn is_suspended(&self) -> bool {
        self.state.borrow().suspended
    }

    /// Driver drain: the pending request, if any (latest wins).
    pub(crate) fn take_request(&self) -> Option<bool> {
        self.state.borrow_mut().requested.take()
    }
}

#[cfg(test)]
#[path = "selection_tests.rs"]
mod tests;
