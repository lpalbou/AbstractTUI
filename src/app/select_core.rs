//! Shared machinery of the select family (backlog 0500): the framed
//! trigger row, the popup option-rows renderer, enabled-skipping
//! highlight movement, and closed-select type-ahead. Private sibling
//! of select.rs — nothing here is public API; the three faces are.
//!
//! Token discipline (RT1-9b): tokens resolve at view-build time into
//! plain `Rgba` captured by draw closures — no color literals, no
//! color arithmetic.
//!
//! OWNER: SELECT (0500).

use std::rc::Rc;
use std::time::{Duration, Instant};

use crate::base::Point;
use crate::layout::{Dimension, Style as LayoutStyle};
use crate::reactive::{Scope, Signal};
use crate::render::Style;
use crate::theme::TokenSet;
use crate::ui::{dyn_view, Element, Phase, Role, UiEvent, View};

use super::SelectOption;

/// Type-ahead accumulation window: keys within it extend the buffer;
/// a pause resets it (the classic closed-select gesture).
pub(crate) const TYPE_AHEAD_WINDOW: Duration = Duration::from_millis(900);

/// Popup rows shown at once before the list windows around the
/// highlight (face default; `max_visible(n)` overrides).
pub(crate) const DEFAULT_MAX_VISIBLE: usize = 8;

// ------------------------------------------------------------- movement

/// Is display position `pos` selectable (in range and not disabled)?
fn enabled_at(options: &[SelectOption], display: &[usize], pos: usize) -> bool {
    display
        .get(pos)
        .and_then(|&i| options.get(i))
        .is_some_and(|o| !o.disabled)
}

/// One arrow step from `from` in `dir` (+1/-1): the next enabled
/// display position, skipping disabled options; stays put when none
/// remains in that direction (clamp, no wrap — the List precedent).
pub(crate) fn step_highlight(
    options: &[SelectOption],
    display: &[usize],
    from: usize,
    dir: i32,
) -> usize {
    let n = display.len();
    if n == 0 {
        return 0;
    }
    let mut pos = from.min(n - 1) as i32;
    loop {
        pos += dir.signum();
        if pos < 0 || pos >= n as i32 {
            return from.min(n - 1);
        }
        if enabled_at(options, display, pos as usize) {
            return pos as usize;
        }
    }
}

/// First enabled display position (Home; also the highlight seed when
/// the bound value is absent or disabled).
pub(crate) fn first_enabled(options: &[SelectOption], display: &[usize]) -> Option<usize> {
    (0..display.len()).find(|&p| enabled_at(options, display, p))
}

/// Last enabled display position (End).
pub(crate) fn last_enabled(options: &[SelectOption], display: &[usize]) -> Option<usize> {
    (0..display.len())
        .rev()
        .find(|&p| enabled_at(options, display, p))
}

/// Page jump: `page` positions in `dir`, snapped to an enabled
/// position — first continuing in `dir`, else backtracking toward the
/// start point (a fully disabled tail cannot strand the highlight).
pub(crate) fn page_highlight(
    options: &[SelectOption],
    display: &[usize],
    from: usize,
    dir: i32,
    page: usize,
) -> usize {
    let n = display.len();
    if n == 0 {
        return 0;
    }
    let target = (from as i32 + dir.signum() * page as i32).clamp(0, n as i32 - 1) as usize;
    if enabled_at(options, display, target) {
        return target;
    }
    let onward = step_highlight(options, display, target, dir);
    if onward != target && enabled_at(options, display, onward) {
        return onward;
    }
    let back = step_highlight(options, display, target, -dir);
    if enabled_at(options, display, back) {
        return back;
    }
    from.min(n - 1)
}

// ------------------------------------------------------------ type-ahead

/// Closed-select type-ahead state: printable keys accumulate within
/// [`TYPE_AHEAD_WINDOW`]; a repeated single char cycles instead.
#[derive(Default)]
pub(crate) struct TypeAhead {
    buf: String,
    at: Option<Instant>,
}

impl TypeAhead {
    /// Fold one printable key at `now`; returns the active buffer.
    pub(crate) fn push(&mut self, ch: char, now: Instant) -> &str {
        let stale = self
            .at
            .is_none_or(|t| now.duration_since(t) > TYPE_AHEAD_WINDOW);
        if stale {
            self.buf.clear();
        }
        self.at = Some(now);
        self.buf.push(ch);
        &self.buf
    }

    pub(crate) fn clear(&mut self) {
        self.buf.clear();
        self.at = None;
    }
}

/// Where a type-ahead buffer jumps the highlight (case-insensitive):
/// a repeated single char CYCLES through options starting with it
/// (wrapping past `current`); any other buffer jumps to the first
/// enabled option whose label starts with the whole prefix. `None` =
/// no match, highlight stays.
pub(crate) fn type_ahead_target(
    options: &[SelectOption],
    display: &[usize],
    buf: &str,
    current: usize,
) -> Option<usize> {
    let lower = buf.to_lowercase();
    let mut chars = lower.chars();
    let first = chars.next()?;
    let cycling = buf.chars().count() > 1 && chars.clone().all(|c| c == first);
    let matches = |pos: usize, prefix: &str| {
        enabled_at(options, display, pos)
            && options[display[pos]]
                .label
                .to_lowercase()
                .starts_with(prefix)
    };
    let n = display.len();
    if cycling {
        let prefix = first.to_string();
        // Next match strictly after `current`, wrapping around.
        return (1..=n)
            .map(|d| (current + d) % n.max(1))
            .find(|&p| matches(p, &prefix));
    }
    (0..n).find(|&p| matches(p, &lower))
}

// ------------------------------------------------------------- trigger

/// What the closed control shows: the label text, an optional SHORT
/// fallback used when the text does not fit the row (MultiSelect's
/// "N selected"), and whether it is placeholder-toned.
pub(crate) struct TriggerLabel {
    pub text: String,
    pub short: Option<String>,
    pub placeholder: bool,
}

/// The closed one-row control shared by all three faces: side strokes
/// (`border` -> `border_focus` on focus — the TextInput frame
/// vocabulary), current label (placeholder in `text_faint`), and a
/// `▾` affordance by the right stroke. Disabled renders faint and the
/// FACE leaves the element non-focusable (theming state table:
/// disabled is out of the focus order).
pub(crate) fn trigger_view(
    t: &TokenSet,
    focused: Signal<bool>,
    hovered: Signal<bool>,
    disabled: bool,
    label: Rc<dyn Fn() -> TriggerLabel>,
) -> View {
    let text_fg = t.text;
    let ground = t.surface;
    let stroke = t.border;
    let stroke_focus = t.border_focus;
    let faint = t.text_faint;
    let accent = t.accent;
    dyn_view(
        LayoutStyle::default()
            .width(Dimension::Percent(1.0))
            .height(Dimension::Cells(1)),
        move || {
            let TriggerLabel {
                text: shown,
                short,
                placeholder: is_placeholder,
            } = label();
            let focus = focused.get();
            let hover = hovered.get();
            Element::new()
                .style(
                    LayoutStyle::default()
                        .width(Dimension::Percent(1.0))
                        .height(Dimension::Cells(1)),
                )
                .draw(move |canvas, rect| {
                    if rect.is_empty() || rect.w < 4 {
                        return;
                    }
                    // Disabled and placeholder share the faint tone.
                    let ink = if disabled || is_placeholder {
                        faint
                    } else {
                        text_fg
                    };
                    let style = Style::new().fg(ink).bg(ground);
                    canvas.fill_styled(rect, ' ', &style);
                    let stroke_style = Style::new()
                        .fg(if focus && !disabled {
                            stroke_focus
                        } else {
                            stroke
                        })
                        .bg(ground);
                    canvas.print_styled(rect.origin(), "▐", &stroke_style);
                    canvas.print_styled(Point::new(rect.right() - 1, rect.y), "▌", &stroke_style);
                    // Label, clipped to the room left of the chevron;
                    // an overflowing label falls back to its SHORT
                    // form when one exists ("N selected").
                    let room = (rect.w - 4).max(0);
                    let shown = match &short {
                        Some(s) if crate::text::width(&shown) > room => s.as_str(),
                        _ => shown.as_str(),
                    };
                    let mut col = 0;
                    for seg in crate::text::segments(shown) {
                        if col + seg.width > room {
                            break;
                        }
                        canvas.print_styled(
                            Point::new(rect.x + 1 + col, rect.y),
                            seg.cluster,
                            &style,
                        );
                        col += seg.width;
                    }
                    let chevron_ink = if disabled {
                        faint
                    } else if hover || focus {
                        accent
                    } else {
                        text_fg
                    };
                    canvas.print_styled(
                        Point::new(rect.right() - 2, rect.y),
                        "▾",
                        &Style::new().fg(chevron_ink).bg(ground),
                    );
                })
                .build()
        },
    )
}

// ----------------------------------------------------------- popup rows

/// Configuration of the shared popup option-rows region.
pub(crate) struct OptionRows {
    pub options: Rc<Vec<SelectOption>>,
    /// Display order: indices into `options` (filtered for Combobox,
    /// the full range for Select/MultiSelect).
    pub display: Signal<Vec<usize>>,
    /// Highlight POSITION within `display` (never the bound value —
    /// the 0250 movement-vs-activation split).
    pub highlight: Signal<usize>,
    /// MultiSelect: keys currently toggled in the working set — rows
    /// render `[x]`/`[ ]` marks. `None` for the single-choice faces.
    pub checks: Option<Signal<Vec<String>>>,
    pub max_visible: usize,
    /// Mouse activation of a display position (commit for the single
    /// faces, toggle for MultiSelect).
    pub on_activate: Rc<dyn Fn(usize)>,
}

/// The option-rows region: a reactive window of one-row items around
/// the highlight — `Role::MenuItem` rows on a `surface_raised` ground,
/// selection pair on the highlight, muted right-aligned hints, faint
/// disabled rows. Grows down its container; the FACE owns the popup
/// root (ground fill + `Role::Menu` + key handling).
pub(crate) fn option_rows_view(t: &TokenSet, rows: OptionRows) -> View {
    let ink = t.text;
    let ground = t.surface_raised;
    let muted = t.text_muted;
    let faint = t.text_faint;
    let sel_fg = t.selection_fg;
    let sel_bg = t.selection_bg;
    let OptionRows {
        options,
        display,
        highlight,
        checks,
        max_visible,
        on_activate,
    } = rows;
    dyn_view(
        LayoutStyle::column()
            .width(Dimension::Percent(1.0))
            .grow(1.0)
            .shrink(1.0),
        move || {
            let disp = display.get();
            let n = disp.len();
            let h = highlight.get().min(n.saturating_sub(1));
            let checked: Option<Vec<String>> = checks.map(|c| c.get());
            let vis = max_visible.min(n.max(1));
            let start = (if h < vis { 0 } else { h + 1 - vis }).min(n.saturating_sub(vis));
            let mut col = Element::new().style(
                LayoutStyle::column()
                    .width(Dimension::Percent(1.0))
                    .shrink(0.0),
            );
            for (pos, &opt_ix) in disp.iter().enumerate().skip(start).take(vis) {
                let Some(opt) = options.get(opt_ix) else {
                    continue;
                };
                let is_highlight = pos == h;
                let disabled = opt.disabled;
                let label = opt.label.clone();
                let hint = opt.hint.clone();
                let mark = checked.as_ref().map(|set| set.contains(&opt.key));
                let on_activate = on_activate.clone();
                col = col.child(
                    Element::new()
                        .style(LayoutStyle::line(1).shrink(0.0))
                        .role(Role::MenuItem)
                        .access_label(label.clone())
                        .on(Phase::Bubble, move |ctx, ev| {
                            if let UiEvent::Mouse(m) = ev {
                                if matches!(m.kind, crate::ui::MouseKind::Down(_)) {
                                    if !disabled {
                                        on_activate(pos);
                                    }
                                    ctx.stop_propagation();
                                }
                            }
                        })
                        .draw(move |canvas, rect| {
                            let (fg, bg) = if disabled {
                                (faint, ground)
                            } else if is_highlight {
                                (sel_fg, sel_bg)
                            } else {
                                (ink, ground)
                            };
                            let style = Style::new().fg(fg).bg(bg);
                            canvas.fill_styled(rect, ' ', &style);
                            let mut x = rect.x + 1;
                            if let Some(on) = mark {
                                canvas.print_styled(
                                    Point::new(x, rect.y),
                                    if on { "[x]" } else { "[ ]" },
                                    &style,
                                );
                                x += 4;
                            }
                            canvas.print_styled(Point::new(x, rect.y), &label, &style);
                            if let Some(hint) = &hint {
                                let hw = crate::text::width(hint);
                                let hx = rect.right() - 1 - hw;
                                // Only when it fits beside the label.
                                if hx > x + crate::text::width(&label) + 1 {
                                    let hint_ink = if disabled {
                                        faint
                                    } else if is_highlight {
                                        sel_fg
                                    } else {
                                        muted
                                    };
                                    canvas.print_styled(
                                        Point::new(hx, rect.y),
                                        hint,
                                        &Style::new().fg(hint_ink).bg(bg),
                                    );
                                }
                            }
                        })
                        .build(),
                );
            }
            col.build()
        },
    )
}

/// Resolve the overlay store a face opens its popup on: the builder's
/// explicit override first, else the app-provided reactive context
/// (`App::mount` provides it). `None` — reachable only outside an
/// `App` without an override — makes open a loud-in-debug no-op.
pub(crate) fn resolve_overlays(
    cx: Scope,
    explicit: Option<super::super::overlays::Overlays>,
) -> Option<super::super::overlays::Overlays> {
    explicit.or_else(|| cx.use_context::<super::super::overlays::Overlays>())
}
