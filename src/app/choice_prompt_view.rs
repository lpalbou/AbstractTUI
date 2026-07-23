//! Private view half of ChoicePrompt (backlog 0515): the modal content
//! tree. `#[path]` sibling of choice_prompt.rs — nothing here is
//! public API; geometry + row rendering live in `parts`
//! (choice_prompt_parts.rs), the key handler + hint row in `interact`
//! (choice_prompt_interact.rs).
//!
//! Routing model: ONE Bubble key handler on the content root moves the
//! highlight / toggles / commits; deeper widgets shield it naturally —
//! a focused TextInput consumes its printables (digits and option
//! letters type text, they never jump or activate) and Enter
//! (on_submit = commit), focused Buttons consume Enter/Space — while
//! Up/Down bubble through from anywhere. Esc is LAYERED (wave-5 F4,
//! the Combobox precedent): the Other editor's wrapper consumes Esc
//! first (retreat to the list, draft kept); an unshielded Esc rides
//! the advertised `shortcut_labeled` cancel on dismissable gates, or
//! the root handler's visible refusal on must-choose gates.
//!
//! Token discipline (RT1-9b): tokens resolve at open into plain
//! `Rgba` captured by draw closures — no theme reads at draw time
//! (the select-family posture: the palette lives for the prompt).
//!
//! OWNER: CHOICE (0515).

use std::rc::Rc;

use crate::base::Point;
use crate::layout::{Dimension, Style as LayoutStyle};
use crate::reactive::Scope;
use crate::render::{Attrs, Style};
use crate::theme::TokenSet;
use crate::ui::{
    dyn_view, dyn_view_scoped, Element, EventCtx, Key, KeyChord, Mods, Phase, Role, UiEvent, View,
};
use crate::widgets::{Button, TextInput};

use super::interact::{
    hint_row, move_row, root_key_handler, toggle_row, Anchors, GateDims, GateState, HintPaint,
};
use super::parts::{
    blank_row, choice_row, has_buttons, keys_summary, option_height, window_start, Geometry,
    RowPaint, RowSpec, GLYPH_W,
};
use super::{ChoiceAnswer, ChoiceOutcome, ChoiceQuestion};

pub(crate) struct GateSpec {
    pub question: Rc<ChoiceQuestion>,
    pub geo: Geometry,
    pub initial_row: usize,
    pub initial_checked: Vec<bool>,
    pub dismissable: bool,
    pub resolve: Rc<dyn Fn(ChoiceOutcome)>,
    /// Caller body (first-app 0287), built in the modal scope; None =
    /// the classic prompt→options gate.
    pub body: Option<Box<dyn FnOnce(Scope) -> View>>,
}

/// Build the modal content: prompt (Heading), windowed option rows,
/// the Other row + reserved input row, Confirm/Cancel, hint. All state
/// lives in the MODAL scope (dies on close).
pub(crate) fn gate_content(mcx: Scope, t: TokenSet, spec: GateSpec) -> View {
    let GateSpec {
        question,
        geo,
        initial_row,
        initial_checked,
        dismissable,
        resolve,
        body,
    } = spec;
    let n = question.options.len();
    let has_other = question.other.is_some();
    let heights: Rc<Vec<i32>> = Rc::new(question.options.iter().map(option_height).collect());
    let dims = GateDims {
        n,
        rows_total: n + usize::from(has_other),
        other_row: n,
        multiple: question.allow_multiple,
        has_other,
        dismissable,
        windowed: heights.iter().sum::<i32>() > geo.region_rows,
    };
    let paint = RowPaint {
        ink: t.text,
        ground: t.overlay,
        muted: t.text_muted,
        accent: t.accent,
        danger: t.error,
        sel_fg: t.selection_fg,
        sel_bg: t.selection_bg,
    };

    // ---- state (modal scope: dies on close) ---------------------------
    let state = GateState {
        highlight: mcx.signal(initial_row.min(dims.rows_total.saturating_sub(1))),
        checks: mcx.signal(initial_checked),
        other_on: mcx.signal(false),
        other_text: mcx.signal(String::new()),
        needs_other: mcx.signal(false),
        esc_note: mcx.signal(false),
        editing: mcx.signal(false),
        region_focused: mcx.signal(false),
    };

    // Other engagement: single mode = the highlight rests on the Other
    // row; multiple mode = the Other row is checked. A MEMO so the
    // reveal region rebuilds on the flip only, never per arrow move.
    let engaged = {
        let s = state;
        let d = dims;
        mcx.memo(move || {
            if !d.has_other {
                false
            } else if d.multiple {
                s.other_on.get()
            } else {
                s.highlight.get() == d.other_row
            }
        })
    };

    // Declared shortcut letters (first declaration wins; a duplicate is
    // a caller bug — loud in debug, first-wins in release).
    let letters: Rc<Vec<(char, usize)>> = Rc::new({
        let mut out: Vec<(char, usize)> = Vec::new();
        for (i, o) in question.options.iter().enumerate() {
            if let Some(k) = o.key {
                if out.iter().any(|(c, _)| *c == k) {
                    debug_assert!(false, "ChoicePrompt: duplicate option key {k:?}");
                } else {
                    out.push((k, i));
                }
            }
        }
        out
    });

    let anchors = Anchors::default();

    // ---- the one commit path (Enter, letters, Confirm, click, submit).
    // Refuses a hollow Other: engaged with empty trimmed text flips the
    // visible note and keeps the gate waiting.
    let try_commit: Rc<dyn Fn()> = Rc::new({
        let question = question.clone();
        let resolve = resolve.clone();
        let s = state;
        let d = dims;
        move || {
            let engaged_now = if !d.has_other {
                false
            } else if d.multiple {
                s.other_on.get_untracked()
            } else {
                s.highlight.get_untracked() == d.other_row
            };
            let other_value = if engaged_now {
                let text = s.other_text.with_untracked(|v| v.trim().to_string());
                if text.is_empty() {
                    s.needs_other.set(true);
                    return;
                }
                Some(text)
            } else {
                None
            };
            let selected: Vec<String> = if d.multiple {
                // Canonicalized to option order by construction.
                let marks = s.checks.get_untracked();
                question
                    .options
                    .iter()
                    .enumerate()
                    .filter(|(i, _)| marks.get(*i).copied().unwrap_or(false))
                    .map(|(_, o)| o.id.clone())
                    .collect()
            } else {
                match question.options.get(s.highlight.get_untracked()) {
                    Some(o) => vec![o.id.clone()],
                    None => Vec::new(), // Other row: the text carries it
                }
            };
            resolve(ChoiceOutcome::Answered(ChoiceAnswer {
                selected,
                other: other_value,
            }));
        }
    });
    let cancel: Rc<dyn Fn()> = Rc::new({
        let resolve = resolve.clone();
        move || resolve(ChoiceOutcome::Cancelled)
    });

    // ---- prompt: a Heading (the question is IN the a11y tree — A1;
    // pixels wrap/ellipsize, the label carries the full text) ----------
    let prompt_lines = Rc::new(geo.prompt_lines);
    let prompt_h = prompt_lines.len() as i32;
    let prompt_ink = paint.ink;
    let prompt_bg = paint.ground;
    let prompt_el = Element::new()
        .style(
            LayoutStyle::default()
                .width(Dimension::Percent(1.0))
                .height(Dimension::Cells(prompt_h)),
        )
        .role(Role::Heading)
        .access_label(question.prompt.clone())
        .draw(move |canvas, rect| {
            let style = Style::new().fg(prompt_ink).bg(prompt_bg).attrs(Attrs::BOLD);
            for (i, line) in prompt_lines.iter().enumerate() {
                if i as i32 >= rect.h {
                    break;
                }
                canvas.print_styled(Point::new(rect.x, rect.y + i as i32), line, &style);
            }
        })
        .build();

    // ---- windowed option region -----------------------------------------
    let region_rows = geo.region_rows;
    let region_dyn = {
        let question = question.clone();
        let heights = heights.clone();
        let try_commit = try_commit.clone();
        let s = state;
        let d = dims;
        dyn_view(
            LayoutStyle::column()
                .width(Dimension::Percent(1.0))
                .height(Dimension::Percent(1.0)),
            move || {
                let h_row = s.highlight.get().min(d.rows_total.saturating_sub(1));
                let focused = s.region_focused.get();
                let marks = if d.multiple {
                    s.checks.get()
                } else {
                    Vec::new()
                };
                let anchor_opt = h_row.min(n.saturating_sub(1));
                let start = window_start(&heights, region_rows, anchor_opt);
                let mut col = Element::new().style(
                    LayoutStyle::column()
                        .width(Dimension::Percent(1.0))
                        .shrink(0.0),
                );
                let mut used = 0;
                for (i, opt) in question.options.iter().enumerate().skip(start) {
                    let hh = heights[i];
                    // Always give the first windowed option SOMETHING
                    // (a 2-row option under a 1-row budget clips its
                    // detail rather than vanishing).
                    let give = hh.min(region_rows - used);
                    if give <= 0 {
                        break;
                    }
                    used += give;
                    let is_highlight = i == h_row;
                    let checked = d.multiple.then(|| marks.get(i).copied().unwrap_or(false));
                    let glyph = match checked {
                        Some(true) => "☑",
                        Some(false) => "☐",
                        None if is_highlight => "●",
                        None => "○",
                    };
                    let on_press = {
                        let try_commit = try_commit.clone();
                        // No focus repair needed here: rows live inside
                        // the focusable region, so the click's own
                        // click-to-focus lands on it before any unmount.
                        move |_ctx: &mut EventCtx| {
                            if d.multiple {
                                move_row(s, d, i);
                                toggle_row(s, d, i);
                            } else if s.highlight.get_untracked() == i {
                                // Click-on-selected commits (the 0250
                                // mouse ruling); first click selects.
                                try_commit();
                            } else {
                                move_row(s, d, i);
                            }
                        }
                    };
                    col = col.child(choice_row(
                        paint,
                        RowSpec {
                            height: give,
                            glyph,
                            label: opt.label.clone(),
                            detail: opt.detail.clone(),
                            key: opt.key,
                            highlighted: is_highlight,
                            focused,
                            danger: opt.danger,
                            checked,
                        },
                        on_press,
                    ));
                }
                col.build()
            },
        )
    };
    let region_access = {
        let question = question.clone();
        let other_label = question.other.clone().unwrap_or_default();
        let s = state;
        move || {
            let h = s.highlight.get_untracked();
            question
                .options
                .get(h)
                .map(|o| o.label.clone())
                .unwrap_or_else(|| other_label.clone())
        }
    };
    let region_host = Element::new()
        .style(
            LayoutStyle::column()
                .width(Dimension::Percent(1.0))
                .height(Dimension::Cells(region_rows))
                // Explicit minimum: under pathological viewports the
                // REGION compresses (windowing keeps the highlight
                // reachable) while the floored fixed rows — buttons,
                // hint — stay visible (0240).
                .min_h(1)
                .shrink(1.0),
        )
        // One tab stop for the whole list (the RadioGroup rule);
        // clicking a row focuses here (nearest focusable ancestor).
        // AUTOFOCUS anchors the keyboard on the options from frame one
        // (focus_init clause 1): without it a focusable BODY child (a
        // Scroll wrapper is focusable by nature, and renders above)
        // wins the first-focusable pick and arrows scroll the body
        // instead of moving the highlight (0287's routing contract).
        // Body-less gates: the same node focus_first already picked.
        .focusable()
        .autofocus()
        .focus_signal(state.region_focused)
        .role(Role::Menu)
        .access_label("options")
        .access_value(region_access)
        // Record the region's id: the PREFERRED retreat anchor (its
        // focus affordance is visible — A5). Fresh whenever any event
        // routes through the region.
        .on(Phase::Capture, {
            let region_anchor = anchors.region.clone();
            move |ctx, _ev| {
                if let Some(id) = ctx.current() {
                    region_anchor.set(Some(id));
                }
            }
        })
        .child(region_dyn)
        .build();

    // ---- Other row + reserved input row ----------------------------------
    let other_label = question.other.clone().unwrap_or_default();
    let other_row_view = {
        let label = other_label.clone();
        let try_commit = try_commit.clone();
        let anchors_press = anchors.clone();
        let s = state;
        let d = dims;
        dyn_view(
            LayoutStyle::default()
                .width(Dimension::Percent(1.0))
                .height(Dimension::Cells(1)),
            move || {
                let is_highlight = s.highlight.get() == d.other_row;
                let on = if d.multiple {
                    s.other_on.get()
                } else {
                    is_highlight
                };
                let checked = d.multiple.then_some(on);
                let glyph = match checked {
                    Some(true) => "☑",
                    Some(false) => "☐",
                    None if is_highlight => "●",
                    None => "○",
                };
                // The Other row is "list-focused" when the region holds
                // focus OR its editor does (the editor IS this choice).
                let focused = s.region_focused.get() || s.editing.get();
                let on_press = {
                    let try_commit = try_commit.clone();
                    let anchors = anchors_press.clone();
                    move |ctx: &mut EventCtx| {
                        if d.multiple {
                            // Unchecking unmounts a possibly-focused
                            // editor; the row has no focusable ancestor
                            // (it sits outside the region), so re-anchor
                            // explicitly (see [`Anchors`]).
                            if s.other_on.get_untracked() {
                                anchors.retreat(ctx);
                            }
                            move_row(s, d, d.other_row);
                            toggle_row(s, d, d.other_row);
                        } else if s.highlight.get_untracked() == d.other_row {
                            try_commit();
                        } else {
                            move_row(s, d, d.other_row);
                        }
                    }
                };
                Element::new()
                    .style(
                        LayoutStyle::default()
                            .width(Dimension::Percent(1.0))
                            .height(Dimension::Cells(1)),
                    )
                    .child(choice_row(
                        paint,
                        RowSpec {
                            height: 1,
                            glyph,
                            label: label.clone(),
                            detail: None,
                            key: None,
                            highlighted: is_highlight,
                            focused,
                            danger: false,
                            checked,
                        },
                        on_press,
                    ))
                    .build()
            },
        )
    };
    // The input row is RESERVED (blank until engaged) so the panel
    // never resizes. The editor mounts in its own child scope (dies on
    // disengage) and AUTOFOCUSES — delivery is layout-time, so the
    // reveal from inside a key dispatch is safe (the 0220 class). Its
    // wrapper consumes Esc BEFORE the root shortcut/refusal: layered
    // Esc (wave-5 F4) — first Esc retreats to the list, draft kept.
    let input_row = {
        let try_commit = try_commit.clone();
        let anchors_esc = anchors.clone();
        let s = state;
        dyn_view_scoped(
            LayoutStyle::default()
                .width(Dimension::Percent(1.0))
                .height(Dimension::Cells(1)),
            move |scope| {
                if !engaged.get() {
                    return Element::new().style(LayoutStyle::line(1)).build();
                }
                let commit = try_commit.clone();
                let anchors = anchors_esc.clone();
                Element::new()
                    .style(
                        LayoutStyle::row()
                            .width(Dimension::Percent(1.0))
                            .height(Dimension::Cells(1)),
                    )
                    .on(Phase::Bubble, move |ctx, ev| {
                        // Reached only while focus is INSIDE this row
                        // (keys route focused-first): Esc = retreat.
                        if let UiEvent::Key(k) = ev {
                            if k.key == Key::Escape && k.mods == Mods::NONE {
                                ctx.stop_propagation();
                                anchors.retreat(ctx);
                            }
                        }
                    })
                    .child(
                        Element::new()
                            .style(LayoutStyle::default().w(GLYPH_W).h(1).shrink(0.0))
                            .build(),
                    )
                    .child(
                        TextInput::new()
                            .value(s.other_text)
                            .placeholder("type your answer…")
                            .placeholder_while_focused(true)
                            .on_submit(move |_| commit())
                            .element(scope, &t)
                            // Second FocusIn/FocusOut listener beside the
                            // widget's own (handlers append; focus events
                            // are target-only): the hint row's editing
                            // truth (wave-5 F4/F6).
                            .focus_signal(s.editing)
                            .autofocus()
                            .build(),
                    )
                    .build()
            },
        )
    };

    // ---- buttons (Confirm needs multiple; Cancel needs dismissability) --
    let buttons_view = has_buttons(dims.multiple, dismissable).then(|| {
        let mut row = Element::new()
            .style(
                LayoutStyle::row()
                    .width(Dimension::Percent(1.0))
                    .height(Dimension::Cells(1))
                    .gap(2),
            )
            .child(
                Element::new()
                    .style(LayoutStyle::default().grow(1.0))
                    .build(),
            );
        if dims.multiple {
            let commit = try_commit.clone();
            row = row.child(
                Button::new("Confirm")
                    .on_click(move || commit())
                    .element(mcx, &t)
                    .build(),
            );
        }
        if dismissable {
            let cancel_click = cancel.clone();
            row = row.child(
                Button::new("Cancel")
                    .on_click(move || cancel_click())
                    .element(mcx, &t)
                    .build(),
            );
        }
        row.build()
    });

    // ---- assembly ----------------------------------------------------------
    let hint = hint_row(
        state,
        dims,
        HintPaint {
            muted: paint.muted,
            accent: paint.accent,
            ground: paint.ground,
        },
        engaged,
        other_label,
        keys_summary(&question),
    );
    let mut root = Element::new().style(
        LayoutStyle::column()
            .width(Dimension::Percent(1.0))
            .height(Dimension::Percent(1.0)),
    );
    if dismissable {
        // Advertised cancel (KeymapHelp lists it). Must-choose gates
        // register NO Esc shortcut — advertising a dead key would lie;
        // the root handler's visible refusal answers the attempt.
        let cancel_esc = cancel.clone();
        root = root.shortcut_labeled(KeyChord::plain(Key::Escape), "Cancel", move |_| {
            cancel_esc()
        });
    }
    // ---- caller body (first-app 0287): a clipped, non-focusable
    // display region. Row budget solved at open (options first —
    // 0240); `.clip()` keeps an overflowing static body off the rows
    // below; shrink 2.0 yields before the region's 1.0 to its floor.
    let body_view = body.map(|f| {
        Element::new()
            .style(
                LayoutStyle::column()
                    .width(Dimension::Percent(1.0))
                    .height(Dimension::Cells(geo.body_rows))
                    .min_h(1)
                    .shrink(2.0)
                    .clip(),
            )
            .child(f(mcx))
            .build()
    });

    let root_anchor = anchors.root.clone();
    let mut root = root
        // Capture runs root→target before target/bubble: the fallback
        // anchor id is fresh before any retreat can need it.
        .on(Phase::Capture, move |ctx, _ev| {
            if let Some(id) = ctx.current() {
                root_anchor.set(Some(id));
            }
        })
        .on(
            Phase::Bubble,
            root_key_handler(state, dims, letters, try_commit.clone(), anchors),
        )
        .child(prompt_el)
        .child(blank_row());
    if let Some(body) = body_view {
        root = root.child(body).child(blank_row());
    }
    if n > 0 {
        root = root.child(region_host);
    }
    if has_other {
        root = root.child(other_row_view).child(input_row);
    }
    let mut root = root.child(blank_row());
    if let Some(buttons) = buttons_view {
        root = root.child(buttons);
    }
    root.child(hint).build()
}
