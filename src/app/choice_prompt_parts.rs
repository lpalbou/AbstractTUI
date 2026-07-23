//! Shared parts of ChoicePrompt (backlog 0515): geometry solved at
//! open, the variable-height windowing rule, and the option-row
//! renderer. `#[path]` sibling of choice_prompt.rs (file budget);
//! nothing here is public API.
//!
//! Token discipline (RT1-9b): [`RowPaint`] carries plain `Rgba`
//! resolved at open — draw closures never read the theme.
//!
//! OWNER: CHOICE (0515).

use crate::base::{Point, Rgba, Size};
use crate::layout::{Dimension, Style as LayoutStyle};
use crate::render::Style;
use crate::ui::{Element, EventCtx, MouseButton, MouseKind, Phase, Role, UiEvent, View};

use super::ChoiceQuestion;

/// Glyph column: one marker cluster + one space.
pub(crate) const GLYPH_W: i32 = 2;
/// Content width floor (buttons + hint stay legible).
pub(crate) const MIN_INNER_W: i32 = 24;

/// Geometry solved once at open. The panel size is FIXED for the
/// prompt's lifetime — when the question allows Other, the input row's
/// space is reserved up front so revealing it never resizes the modal
/// (calm geometry; no relayout jumps).
pub(crate) struct Geometry {
    /// Modal panel size (content + the Modal's 1-cell padding).
    pub panel: Size,
    /// The prompt, pre-wrapped to the content width (ellipsis-capped).
    pub prompt_lines: Vec<String>,
    /// Row budget of the (windowed) option region.
    pub region_rows: i32,
    /// Row budget of the caller's body region (first-app 0287);
    /// 0 = no body. Allocated AFTER the options claim theirs — a body
    /// may absorb the remaining height, never crush the options (the
    /// 0240 law), and keeps a 1-row floor so what the user is deciding
    /// about stays reachable (a Scroll body scrolls the rest).
    pub body_rows: i32,
}

/// Rows an option occupies (1, or 2 with a detail line).
pub(crate) fn option_height(o: &super::ChoiceOption) -> i32 {
    if o.detail.is_some() {
        2
    } else {
        1
    }
}

/// Declared shortcut letters joined for the hint row ("a/A/d").
pub(crate) fn keys_summary(q: &ChoiceQuestion) -> String {
    let keys: Vec<String> = q
        .options
        .iter()
        .filter_map(|o| o.key.map(|k| k.to_string()))
        .collect();
    keys.join("/")
}

/// Whether the buttons row exists at all: Confirm needs multiple mode,
/// the dismiss button needs dismissability — a single-choice
/// must-answer gate has no buttons (its options ARE the endings).
pub(crate) fn has_buttons(multiple: bool, dismissable: bool) -> bool {
    multiple || dismissable
}

/// The dismiss affordance's rendered label (button + the advertised
/// Esc shortcut). `None` = the built-in vocabulary ("Cancel"); a
/// caller label (first-app 0271: the approval consumer's Esc DEFERS —
/// "Cancel" beside a "Deny" option mislabels a consent surface)
/// renders verbatim.
pub(crate) fn dismiss_button_label(custom: Option<&str>) -> &str {
    custom.unwrap_or("Cancel")
}

/// The hint row's Esc segment. The built-in vocabulary keeps its
/// conjugated English ("Esc cancels" — byte-stable for every existing
/// gate); a caller label renders verbatim after the key ("Esc Defer")
/// — the engine never synthesizes grammar from a label.
pub(crate) fn esc_segment(dismiss_label: Option<&str>) -> String {
    match dismiss_label {
        Some(label) => format!("Esc {label}"),
        None => String::from("Esc cancels"),
    }
}

pub(crate) fn measure(
    q: &ChoiceQuestion,
    viewport: Size,
    max_visible: i32,
    dismissable: bool,
    dismiss_label: Option<&str>,
    body_rows_pref: Option<i32>,
    body_width_pref: Option<i32>,
) -> Geometry {
    // Width: the widest content line wins, clamped into the viewport
    // (2-cell breathing margin each side beyond the Modal padding).
    let opt_w = q
        .options
        .iter()
        .map(|o| {
            // Label row: glyph + label (+ " (k)" when a key is declared).
            let key_w = if o.key.is_some() { 4 } else { 0 };
            let label = GLYPH_W + crate::text::width(&o.label) + key_w;
            let detail = o
                .detail
                .as_ref()
                .map(|d| GLYPH_W + crate::text::width(d))
                .unwrap_or(0);
            label.max(detail)
        })
        .max()
        .unwrap_or(0);
    let other_w = q
        .other
        .as_ref()
        .map(|l| GLYPH_W + crate::text::width(l))
        .unwrap_or(0);
    // The prompt wraps anyway; it only WIDENS the panel up to a cap so
    // short questions stay compact and long ones read as a paragraph.
    let prompt_w = crate::text::width(&q.prompt).min(52);
    // Buttons render at label width + 2 (Button's 1-cell pad each
    // side), with a 2-cell gap when both exist — computed from the
    // ACTUAL labels so a caller dismiss label measures honestly.
    let confirm_w = if q.allow_multiple {
        crate::text::width("Confirm") + 2
    } else {
        0
    };
    let dismiss_w = if dismissable {
        crate::text::width(dismiss_button_label(dismiss_label)) + 2
    } else {
        0
    };
    let buttons_w = confirm_w + dismiss_w + if confirm_w > 0 && dismiss_w > 0 { 2 } else { 0 };
    let esc = dismissable.then(|| esc_segment(dismiss_label));
    let hint_w = crate::text::width(
        &hint_segments(q.allow_multiple, esc.as_deref(), &keys_summary(q)).join(" · "),
    );
    // The BODY's declared content width (first-app 0271): options,
    // prompt, hint and buttons are all visible to `natural`, but the
    // body closure is opaque — a 72-col card body would clip inside a
    // panel sized by three short options. `body_width` is the caller's
    // honest declaration; it participates in the same max/clamp as
    // every other content line (narrow viewports still clamp, and the
    // body then clips inside its region — never the options).
    let body_w = body_width_pref.unwrap_or(0);
    let natural = opt_w
        .max(other_w)
        .max(prompt_w)
        .max(buttons_w)
        .max(hint_w)
        .max(body_w)
        .max(MIN_INNER_W);
    let inner_w = natural.min((viewport.w - 6).max(12));

    // The prompt may wrap to several rows; cap it at a third of the
    // screen with an honest ellipsis (a longer prompt is a document,
    // not a question).
    let prompt_cap = ((viewport.h / 3).max(2)) as usize;
    let mut prompt_lines = crate::text::wrap(&q.prompt, inner_w);
    if prompt_lines.len() > prompt_cap {
        prompt_lines.truncate(prompt_cap);
        if let Some(last) = prompt_lines.last_mut() {
            // `truncate_ellipsis` cuts within the width budget; the
            // appended `…` guarantees the cap itself is visible even
            // when the kept line happened to fit exactly.
            *last = crate::text::truncate_ellipsis(&format!("{last}…"), inner_w);
        }
    }

    let region_full: i32 = q.options.iter().map(option_height).sum();
    // Fixed rows: prompt + blank + [body separator blank] + [Other row
    // + reserved input row] + blank + [buttons] + hint. The option
    // region gets what remains FIRST; a body absorbs the rest.
    let fixed = prompt_lines.len() as i32
        + 1
        + i32::from(body_rows_pref.is_some())
        + if q.other.is_some() { 2 } else { 0 }
        + 1
        + i32::from(has_buttons(q.allow_multiple, dismissable))
        + 1;
    let avail = (viewport.h - 2 - fixed).max(1);
    let region_rows = region_full
        .min(max_visible)
        .min(avail)
        .max(i32::from(!q.options.is_empty()));
    // The body allocates AFTER the options (they are never crushed by
    // a tall body — the 0240 law), capped at the caller's preference,
    // floored at 1 row (a vanished body would hide what the user is
    // deciding about; a Scroll body keeps the rest reachable).
    let body_rows = match body_rows_pref {
        Some(pref) => (avail - region_rows).min(pref.max(1)).max(1),
        None => 0,
    };
    let panel = Size::new(
        (inner_w + 2).min(viewport.w),
        (fixed + region_rows + body_rows + 2).min(viewport.h),
    );
    Geometry {
        panel,
        prompt_lines,
        region_rows,
        body_rows,
    }
}

/// Highlight clamp: rows are `0..total` (options then Other).
pub(crate) fn clamp_row(row: usize, total: usize) -> usize {
    row.min(total.saturating_sub(1))
}

/// The hint row's verb segments, LEAST important first: when the row
/// runs out of width the hints degrade by WHOLE segments from the
/// front — the tail (Esc, then Enter) survives longest, never a
/// mid-word cut. `keys` = declared shortcut letters ("a/A/d", possibly
/// empty); `esc` = the ready [`esc_segment`] text, `None` on a
/// non-dismissable gate — it lists NO Esc segment (advertising a dead
/// key would lie — the refusal note explains on attempt).
pub(crate) fn hint_segments(multiple: bool, esc: Option<&str>, keys: &str) -> Vec<String> {
    let mut segs = Vec::new();
    if !keys.is_empty() {
        segs.push(format!("{keys} pick"));
    }
    if multiple {
        segs.push(String::from("Space toggles"));
    }
    segs.push(String::from("Enter confirms"));
    if let Some(esc) = esc {
        segs.push(esc.to_string());
    }
    segs
}

/// First option of the window: the smallest start index keeping the
/// anchored option fully visible within `budget` rows — the select
/// family's windowing rule, generalized to variable heights (an option
/// with a detail line costs two rows).
pub(crate) fn window_start(heights: &[i32], budget: i32, anchor: usize) -> usize {
    if heights.is_empty() {
        return 0;
    }
    let anchor = anchor.min(heights.len() - 1);
    let mut start = anchor;
    let mut used = heights[anchor];
    while start > 0 && used + heights[start - 1] <= budget {
        start -= 1;
        used += heights[start];
    }
    start
}

/// Resolved paint for option rows (Copy — captured per draw closure).
#[derive(Copy, Clone)]
pub(crate) struct RowPaint {
    pub ink: Rgba,
    pub ground: Rgba,
    pub muted: Rgba,
    pub accent: Rgba,
    pub danger: Rgba,
    pub sel_fg: Rgba,
    pub sel_bg: Rgba,
}

/// One row's render/interaction spec (per-rebuild, cheap).
pub(crate) struct RowSpec {
    pub height: i32,
    pub glyph: &'static str,
    pub label: String,
    pub detail: Option<String>,
    /// Declared shortcut letter, rendered dim as `(k)` after the label.
    pub key: Option<char>,
    pub highlighted: bool,
    /// The options region holds focus: the highlight wears the audited
    /// selection pair; unfocused it degrades to accent ink (the
    /// RadioGroup focus precedent — A5's visible affordance).
    pub focused: bool,
    pub danger: bool,
    /// Multiple mode: `Some(checked)` — the row reports
    /// `Role::Checkbox` with an on/off access value. `None` = single
    /// mode (`Role::MenuItem`; "selected" rides the highlighted row).
    pub checked: Option<bool>,
}

/// A collapsible spacer row (min 0: under height pressure the blanks
/// yield before anything meaningful does).
pub(crate) fn blank_row() -> View {
    Element::new()
        .style(LayoutStyle::line(1).min_h(0).shrink(1.0))
        .build()
}

/// One option/Other row: glyph + label (+ dim `(k)` shortcut, +
/// optional muted detail line). Selection affordance: the audited
/// selection pair while the region is FOCUSED, accent ink otherwise
/// (A5's focus-visible guarantee — focus changes the pixels). Danger
/// rows ink glyph+label with the `Error` token except under the
/// selection pair (that pair is the audited combination). Press
/// activates; the handler stops propagation FIRST (bookkeeping before
/// a callback that may dispose this row — the 0297 law).
pub(crate) fn choice_row(
    paint: RowPaint,
    spec: RowSpec,
    on_press: impl Fn(&mut EventCtx) + 'static,
) -> View {
    let RowSpec {
        height,
        glyph,
        label,
        detail,
        key,
        highlighted,
        focused,
        danger,
        checked,
    } = spec;
    let mut el = Element::new()
        .style(
            LayoutStyle::default()
                .width(Dimension::Percent(1.0))
                .height(Dimension::Cells(height))
                .shrink(0.0),
        )
        // Multiple mode reports honest checkbox semantics; single mode
        // stays a menu item with the selection riding its value (the
        // frozen-Role vocabulary — A3).
        .role(if checked.is_some() {
            Role::Checkbox
        } else {
            Role::MenuItem
        })
        .access_label(label.clone())
        .on(Phase::Bubble, move |ctx, ev| {
            if let UiEvent::Mouse(m) = ev {
                if matches!(m.kind, MouseKind::Down(MouseButton::Left)) {
                    ctx.stop_propagation();
                    on_press(ctx);
                }
            }
        });
    if let Some(on) = checked {
        el = el.access_value(move || if on { "on" } else { "off" }.into());
    } else if highlighted {
        el = el.access_value(|| "selected".into());
    }
    el.draw(move |canvas, rect| {
        if rect.is_empty() {
            return;
        }
        let selected_pair = highlighted && focused;
        let base_ink = if danger { paint.danger } else { paint.ink };
        let (glyph_ink, label_ink, bg) = if selected_pair {
            (paint.sel_fg, paint.sel_fg, paint.sel_bg)
        } else if highlighted {
            // Unfocused highlight: accent affordance; a danger label
            // keeps its Error ink (the glyph carries the accent).
            (
                paint.accent,
                if danger { paint.danger } else { paint.accent },
                paint.ground,
            )
        } else {
            (base_ink, base_ink, paint.ground)
        };
        let row_style = Style::new().fg(label_ink).bg(bg);
        canvas.fill_styled(rect, ' ', &row_style);
        canvas.print_styled(rect.origin(), glyph, &Style::new().fg(glyph_ink).bg(bg));
        let mut room = (rect.w - GLYPH_W).max(0);
        // Reserve the shortcut chip's cells before truncating the label.
        let key_text = key.map(|k| format!("({k})"));
        if let Some(kt) = &key_text {
            let kw = crate::text::width(kt) + 1;
            if room > kw {
                room -= kw;
            }
        }
        let shown = crate::text::truncate_ellipsis(&label, room);
        canvas.print_styled(Point::new(rect.x + GLYPH_W, rect.y), &shown, &row_style);
        if let Some(kt) = &key_text {
            let kx = rect.x + GLYPH_W + crate::text::width(&shown) + 1;
            if kx + crate::text::width(kt) <= rect.right() {
                let key_ink = if selected_pair {
                    paint.sel_fg
                } else {
                    paint.muted
                };
                canvas.print_styled(Point::new(kx, rect.y), kt, &Style::new().fg(key_ink).bg(bg));
            }
        }
        if rect.h > 1 {
            if let Some(detail) = &detail {
                let ink = if selected_pair {
                    paint.sel_fg
                } else {
                    paint.muted
                };
                canvas.print_styled(
                    Point::new(rect.x + GLYPH_W, rect.y + 1),
                    &crate::text::truncate_ellipsis(detail, (rect.w - GLYPH_W).max(0)),
                    &Style::new().fg(ink).bg(bg),
                );
            }
        }
    })
    .build()
}
