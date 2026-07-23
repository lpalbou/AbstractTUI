//! Interaction half of ChoicePrompt (backlog 0515, wave-5 cycle 2):
//! the gate's signal bundle, the root key handler (movement, shortcut
//! letters, digits, Space, Enter, non-dismissable Esc refusal), the
//! focus-retreat machinery, and the hint row. `#[path]` sibling of
//! choice_prompt.rs — nothing here is public API.
//!
//! OWNER: CHOICE (0515).

use std::cell::Cell;
use std::rc::Rc;

use crate::base::{Point, Rgba};
use crate::layout::{Dimension, Style as LayoutStyle};
use crate::reactive::{Memo, Signal};
use crate::render::Style;
use crate::ui::{dyn_view, Element, EventCtx, Key, Mods, MouseKind, UiEvent, View, ViewId};

use super::parts::{clamp_row, hint_segments};

/// The gate's reactive state (all `Signal`s are Copy ids — the bundle
/// rides into closures by value). Lives in the MODAL scope.
#[derive(Copy, Clone)]
pub(crate) struct GateState {
    /// Highlight row (options then Other).
    pub highlight: Signal<usize>,
    /// Multiple mode: per-option checked marks.
    pub checks: Signal<Vec<bool>>,
    /// Multiple mode: the Other row's checked mark.
    pub other_on: Signal<bool>,
    /// The Other draft (persists across reveals — charter O3).
    pub other_text: Signal<String>,
    /// Hollow-Other commit refusal note (charter O4).
    pub needs_other: Signal<bool>,
    /// Non-dismissable Esc refusal note (charter G3); cleared by the
    /// next movement/toggle.
    pub esc_note: Signal<bool>,
    /// The Other editor holds focus (drives the layered-Esc hint truth
    /// — wave-5 F4).
    pub editing: Signal<bool>,
    /// The options region holds focus (selection pair vs accent — A5).
    pub region_focused: Signal<bool>,
}

/// The gate's static shape (Copy).
#[derive(Copy, Clone)]
pub(crate) struct GateDims {
    pub n: usize,
    pub rows_total: usize,
    pub other_row: usize,
    pub multiple: bool,
    pub has_other: bool,
    pub dismissable: bool,
    pub windowed: bool,
}

/// FOCUS ANCHORS: unmounting a focused node drops the tree's focus to
/// None (mount.rs) and keys then target the PANEL root — the content
/// root's handlers would fall off the routing path (the 0230
/// dead-keys class). Every path that can unmount the focused Other
/// editor re-anchors focus FIRST: preferably on the options REGION
/// (its focus affordance is visible — A5), else on the content root
/// (programmatic focus needs no focusability, focus_init clause 3).
/// Ids are recorded by Capture-phase handlers (capture runs before
/// target/bubble, so an anchor is always fresh by the time a retreat
/// needs it).
#[derive(Clone, Default)]
pub(crate) struct Anchors {
    pub region: Rc<Cell<Option<ViewId>>>,
    pub root: Rc<Cell<Option<ViewId>>>,
}

impl Anchors {
    /// Move focus back to the list (or the content root when the
    /// region never routed an event — the mouse-only corner).
    pub(crate) fn retreat(&self, ctx: &mut EventCtx) {
        if let Some(a) = self.region.get().or_else(|| self.root.get()) {
            ctx.request_focus(a);
        }
    }
}

/// Highlight movement. Clears the Esc-refusal note: the user acted.
pub(crate) fn move_row(s: GateState, d: GateDims, row: usize) {
    s.esc_note.set(false);
    s.highlight.set(clamp_row(row, d.rows_total));
}

/// Toggle a row's mark (multiple mode). Clears the Esc-refusal note.
pub(crate) fn toggle_row(s: GateState, d: GateDims, row: usize) {
    s.esc_note.set(false);
    if row < d.n {
        s.checks.update(|c| {
            if let Some(m) = c.get_mut(row) {
                *m = !*m;
            }
        });
    } else {
        s.other_on.update(|b| *b = !*b);
    }
}

/// The ONE root key handler (Bubble on the content root): movement,
/// declared shortcut letters, digit jumps, Space toggles, Enter
/// commits, wheel movement, and — non-dismissable gates only — the
/// visible Esc refusal (dismissable gates leave Esc to the advertised
/// `shortcut_labeled` cancel; a focused Other editor's Esc was already
/// consumed deeper by the retreat handler).
#[allow(clippy::too_many_arguments)]
pub(crate) fn root_key_handler(
    s: GateState,
    d: GateDims,
    letters: Rc<Vec<(char, usize)>>,
    try_commit: Rc<dyn Fn()>,
    anchors: Anchors,
) -> impl FnMut(&mut EventCtx, &UiEvent) {
    // Single-mode movement OFF the Other row unmounts the focused
    // editor: re-anchor focus before the flush (see [`Anchors`]).
    let retreat_guard = {
        let anchors = anchors.clone();
        move |ctx: &mut EventCtx, from: usize, to: usize| {
            if !d.multiple && d.has_other && from == d.other_row && to != d.other_row {
                anchors.retreat(ctx);
            }
        }
    };
    move |ctx: &mut EventCtx, ev: &UiEvent| match ev {
        UiEvent::Key(k) => {
            // The two wire spellings of a shifted letter fold to ONE
            // canonical key before the gate (first-app 0288): kitty's
            // Char('a')+SHIFT and legacy's Char('A') both mean the
            // uppercase letter — matching runs on the normalized
            // spelling, plain keys only. Shift on anything else
            // (movement, Space, digits, symbols) still bounces here.
            let k = k.normalized();
            if k.mods != Mods::NONE {
                return;
            }
            let h = s.highlight.get_untracked();
            match k.key {
                Key::Down => {
                    retreat_guard(ctx, h, clamp_row(h + 1, d.rows_total));
                    move_row(s, d, h + 1);
                }
                Key::Up => {
                    retreat_guard(ctx, h, h.saturating_sub(1));
                    move_row(s, d, h.saturating_sub(1));
                }
                Key::Home => {
                    retreat_guard(ctx, h, 0);
                    move_row(s, d, 0);
                }
                Key::End => move_row(s, d, d.rows_total.saturating_sub(1)),
                Key::Enter => {
                    // Bookkeeping-first (0297): stop propagation BEFORE
                    // a commit that may dispose this tree.
                    ctx.stop_propagation();
                    try_commit();
                    return;
                }
                Key::Escape if !d.dismissable => {
                    // Must-choose mode: refuse VISIBLY (charter G3) —
                    // the hint row explains; nothing resolves.
                    s.esc_note.set(true);
                }
                Key::Char(c) if !c.is_control() => {
                    // Declared option letters first (case-sensitive —
                    // 'a' and 'A' are distinct keys; the approval
                    // consumer's vocabulary — the normalization above
                    // folds SPELLINGS, never case: Shift+A reads as
                    // 'A' on both wires and can never fire a declared
                    // 'a'). A letter is an EXPLICIT activation:
                    // select+commit in single mode, jump-toggle in
                    // multiple. A focused Other editor consumed
                    // printables before this handler (letters type,
                    // never activate).
                    if let Some(&(_, row)) = letters.iter().find(|(key, _)| *key == c) {
                        if d.multiple {
                            move_row(s, d, row);
                            toggle_row(s, d, row);
                        } else {
                            move_row(s, d, row);
                            ctx.stop_propagation();
                            try_commit();
                            return;
                        }
                    } else if c == ' ' && d.multiple {
                        toggle_row(s, d, h);
                    } else if let Some(digit) = c.to_digit(10).filter(|d| (1..=9).contains(d)) {
                        // Digit jump: single mode MOVES only (0250 —
                        // digits are movement; declared letters are
                        // the activation vocabulary); multiple mode
                        // jump-toggles (the mark IS the selection act).
                        let row = digit as usize - 1;
                        if row >= d.n {
                            return;
                        }
                        move_row(s, d, row);
                        if d.multiple {
                            toggle_row(s, d, row);
                        }
                    } else {
                        return;
                    }
                }
                _ => return,
            }
            ctx.stop_propagation();
        }
        UiEvent::Mouse(m) => match m.kind {
            MouseKind::ScrollUp => {
                let h = s.highlight.get_untracked();
                retreat_guard(ctx, h, h.saturating_sub(1));
                move_row(s, d, h.saturating_sub(1));
                ctx.stop_propagation();
            }
            MouseKind::ScrollDown => {
                let h = s.highlight.get_untracked();
                retreat_guard(ctx, h, clamp_row(h + 1, d.rows_total));
                move_row(s, d, h + 1);
                ctx.stop_propagation();
            }
            _ => {}
        },
        _ => {}
    }
}

/// Hint-row paint (Copy).
#[derive(Copy, Clone)]
pub(crate) struct HintPaint {
    pub muted: Rgba,
    pub accent: Rgba,
    pub ground: Rgba,
}

/// The hint row: refusal notes first (hollow Other, then the
/// non-dismissable Esc refusal), the layered-Esc truth while the Other
/// editor is focused, else the verb segments (+ the windowed `i/N`
/// position, right-aligned so it survives narrow panels). `esc` = the
/// ready Esc segment ("Esc cancels" / "Esc <dismiss label>" — 0271),
/// `None` on must-choose gates.
#[allow(clippy::too_many_arguments)]
pub(crate) fn hint_row(
    s: GateState,
    d: GateDims,
    paint: HintPaint,
    engaged: Memo<bool>,
    other_label: String,
    keys: String,
    esc: Option<String>,
) -> View {
    dyn_view(
        LayoutStyle::default()
            .width(Dimension::Percent(1.0))
            .height(Dimension::Cells(1)),
        move || {
            let warn =
                s.needs_other.get() && engaged.get() && s.other_text.with(|t| t.trim().is_empty());
            let refused = !warn && s.esc_note.get();
            let editing = s.editing.get();
            let note = if warn {
                Some(format!("{other_label} needs text — type your answer"))
            } else if refused {
                Some(String::from("an answer is required"))
            } else {
                None
            };
            // Windowed lists show the highlight position RIGHT-ALIGNED
            // (the select family's "n of N"): it must survive even when
            // the verb hints degrade on a narrow panel.
            let h = s.highlight.get();
            let pos =
                (d.windowed && note.is_none() && h < d.n).then(|| format!("{}/{}", h + 1, d.n));
            let ink = if note.is_some() {
                paint.accent
            } else {
                paint.muted
            };
            let segs: Vec<String> = if editing {
                // Layered Esc (wave-5 F4): while the editor is focused
                // the truth is retreat, not cancel.
                vec![
                    String::from("Enter confirms"),
                    String::from("Esc back to the list"),
                ]
            } else {
                hint_segments(d.multiple, esc.as_deref(), &keys)
            };
            let bg = paint.ground;
            Element::new()
                .style(
                    LayoutStyle::default()
                        .width(Dimension::Percent(1.0))
                        .height(Dimension::Cells(1)),
                )
                .draw(move |canvas, rect| {
                    if rect.is_empty() {
                        return;
                    }
                    let style = Style::new().fg(ink).bg(bg);
                    let mut room = rect.w;
                    if let Some(pos) = &pos {
                        let pw = crate::text::width(pos);
                        if pw <= rect.w {
                            canvas.print_styled(Point::new(rect.right() - pw, rect.y), pos, &style);
                            room = rect.w - pw - 1;
                        }
                    }
                    let text = match &note {
                        Some(w) => crate::text::truncate_ellipsis(w, room.max(0)),
                        None => {
                            // Degrade by whole segments from the front;
                            // the last segment alone still ellipsizes
                            // on truly tiny panels.
                            let mut segs = segs.clone();
                            loop {
                                let joined = segs.join(" · ");
                                if crate::text::width(&joined) <= room || segs.len() == 1 {
                                    break crate::text::truncate_ellipsis(&joined, room.max(0));
                                }
                                segs.remove(0);
                            }
                        }
                    };
                    canvas.print_styled(rect.origin(), &text, &style);
                })
                .build()
        },
    )
}
