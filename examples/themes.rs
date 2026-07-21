//! themes — every registered theme on one screen, applied live.
//!
//! Demonstrates: the ONE-theme-signal architecture (damage contract §5) —
//! Enter writes the signal and the whole screen restyles through normal
//! reactivity, no manual repaint anywhere in this file — plus per-theme
//! token swatches and measured contrast ratios (the audit, visible).
//!
//! Keys: arrows move · Enter applies · q quits.
//!
//! OWNER: DESIGN.

mod common;

use std::cell::Cell as StdCell;
use std::rc::Rc;

use abstracttui::prelude::*;
use abstracttui::theme::{contrast_ratio, themes};
use abstracttui::ui::Canvas;

const CARD_W: i32 = 26;
const CARD_H: i32 = 3;

fn main() -> abstracttui::base::Result<()> {
    if !abstracttui::term::have_tty() {
        println!("themes: needs an interactive terminal — skipping cleanly");
        return Ok(());
    }
    let mut app = App::new(Size::new(100, 30));
    let quitter = app.quitter();
    app.mount(move |cx| {
        let theme = use_theme(cx);
        let sel = cx.signal(0usize);
        let n = themes().len();
        // Column count depends on the drawn width; the draw closure
        // publishes it here so arrow-key math matches the visible grid.
        // Plain interior mutability (not a signal): navigation reads it,
        // nothing renders from it.
        let cols = Rc::new(StdCell::new(1usize));
        let cols_nav = cols.clone();

        let step = move |sel: Signal<usize>, delta: i32| {
            sel.update(|s| {
                let next = *s as i64 + delta as i64;
                *s = next.clamp(0, n as i64 - 1) as usize;
            });
        };
        let cols_for = move || cols_nav.get().max(1) as i32;

        Element::new()
            .shortcut(KeyChord::plain(Key::Char('q')), move |_| quitter.quit())
            .shortcut(KeyChord::plain(Key::Left), move |_| step(sel, -1))
            .shortcut(KeyChord::plain(Key::Right), move |_| step(sel, 1))
            .shortcut(KeyChord::plain(Key::Up), {
                let cols_for = cols_for.clone();
                move |_| step(sel, -cols_for())
            })
            .shortcut(KeyChord::plain(Key::Down), {
                let cols_for = cols_for.clone();
                move |_| step(sel, cols_for())
            })
            .shortcut(KeyChord::plain(Key::Enter), move |_| {
                set_theme_by_id(themes()[sel.get_untracked()].id);
            })
            .child(dyn_view(LayoutStyle::default().grow(1.0), move || {
                let active = theme.get();
                let selected = sel.get();
                let cols_out = cols.clone();
                Element::new()
                    // Draw-only element: no intrinsic content, so it must
                    // claim its space explicitly.
                    .style(LayoutStyle::default().grow(1.0))
                    .draw(move |canvas, rect| {
                        draw_gallery(canvas, rect, active, selected, &cols_out)
                    })
                    .build()
            }))
            .build()
    })?;
    app.run()
}

fn draw_gallery(
    canvas: &mut dyn Canvas,
    rect: Rect,
    active: &'static Theme,
    selected: usize,
    cols_out: &Rc<StdCell<usize>>,
) {
    let t = active.tokens;
    if common::too_small(canvas, rect, common::MIN_SIZE, &t) {
        return;
    }
    canvas.fill(rect, ' ', t.text, t.bg);

    // Header.
    let mut x = rect.x + 1;
    x += canvas.print(Point::new(x, rect.y), "Themes", t.accent, Rgba::TRANSPARENT);
    x += canvas.print(
        Point::new(x, rect.y),
        "  —  ",
        t.text_faint,
        Rgba::TRANSPARENT,
    );
    let dark = if active.is_dark() {
        " (dark)"
    } else {
        " (light)"
    };
    x += canvas.print(
        Point::new(x, rect.y),
        active.label,
        t.text,
        Rgba::TRANSPARENT,
    );
    canvas.print(Point::new(x, rect.y), dark, t.text_muted, Rgba::TRANSPARENT);

    // Preview pane (cycle 7): a miniature app mock rendered in the
    // SELECTED theme's own tokens — see it before you apply it. Takes
    // the right column when there is room.
    let preview_w = if rect.w >= 96 { 32 } else { 0 };

    // Grid geometry; publish the column count for key navigation.
    let grid = Rect::new(rect.x + 1, rect.y + 2, rect.w - 2 - preview_w, rect.h - 9);
    if preview_w > 0 {
        let pv = Rect::new(
            rect.right() - preview_w,
            rect.y + 2,
            preview_w - 1,
            rect.h - 9,
        );
        draw_preview(canvas, pv, &themes()[selected], &t);
    }
    let cols = ((grid.w + 1) / (CARD_W + 1)).max(1) as usize;
    cols_out.set(cols);
    let visible_rows = (grid.h / CARD_H).max(1) as usize;
    let total_rows = themes().len().div_ceil(cols);
    // Scroll so the selected card is always on screen.
    let sel_row = selected / cols;
    let first_row = sel_row
        .saturating_sub(visible_rows - 1)
        .min(total_rows.saturating_sub(visible_rows));

    for (i, entry) in themes().iter().enumerate() {
        let row = i / cols;
        if row < first_row || row >= first_row + visible_rows {
            continue;
        }
        let col = i % cols;
        let card = Rect::new(
            grid.x + col as i32 * (CARD_W + 1),
            grid.y + (row - first_row) as i32 * CARD_H,
            CARD_W.min(grid.w),
            CARD_H,
        );
        draw_card(
            canvas,
            card,
            entry,
            &t,
            i == selected,
            entry.id == active.id,
        );
    }
    if first_row + visible_rows < total_rows {
        common::print_centered(canvas, rect, grid.bottom(), "· · ·", t.text_faint);
    }

    // Bottom panel: selected theme's MEASURED ratios, then the active
    // theme's tiers, semantics and chart ramp.
    let bar_y = rect.bottom() - 6;
    let picked = &themes()[selected];
    let p = picked.tokens;
    let ratios = format!(
        "{}  text {:.1}:1 · muted {:.1}:1 · faint {:.1}:1 · accent {:.1}:1 · selection {:.1}:1",
        picked.label,
        contrast_ratio(p.text, p.bg),
        contrast_ratio(p.text_muted, p.bg),
        contrast_ratio(p.text_faint, p.bg),
        contrast_ratio(p.accent, p.bg),
        contrast_ratio(p.selection_fg, p.selection_bg),
    );
    canvas.print(
        Point::new(rect.x + 1, bar_y),
        &ratios,
        t.text_muted,
        Rgba::TRANSPARENT,
    );

    let demo_y = bar_y + 1;
    canvas.print(
        Point::new(rect.x + 1, demo_y),
        "text",
        t.text,
        Rgba::TRANSPARENT,
    );
    canvas.print(
        Point::new(rect.x + 6, demo_y),
        "muted",
        t.text_muted,
        Rgba::TRANSPARENT,
    );
    canvas.print(
        Point::new(rect.x + 12, demo_y),
        "faint",
        t.text_faint,
        Rgba::TRANSPARENT,
    );
    let mut x = rect.x + 19;
    for (name, color) in [
        ("ok", t.ok),
        ("warn", t.warn),
        ("error", t.error),
        ("info", t.info),
        ("accent", t.accent),
        ("alt", t.accent_alt),
    ] {
        x += canvas.print(Point::new(x, demo_y), "● ", color, Rgba::TRANSPARENT);
        x += canvas.print(Point::new(x, demo_y), name, t.text_muted, Rgba::TRANSPARENT);
        x += 1;
    }
    let mut x = rect.x + 1;
    canvas.print(
        Point::new(x, demo_y + 1),
        "chart",
        t.text_muted,
        Rgba::TRANSPARENT,
    );
    x += 6;
    for i in 0..8 {
        x += canvas.print(
            Point::new(x, demo_y + 1),
            "██",
            t.chart(i),
            Rgba::TRANSPARENT,
        );
    }
    // Selection swatch: the audited pair, shown as it will render.
    canvas.print(
        Point::new(x + 2, demo_y + 1),
        " selection ",
        t.selection_fg,
        t.selection_bg,
    );

    common::key_legend(
        canvas,
        rect,
        &t,
        &[("←↑↓→", "move"), ("enter", "apply"), ("q", "quit")],
    );
}

/// Miniature app mock in the SELECTED theme — every major token doing
/// its real job on the theme's own grounds, inside a frame drawn with the
/// ACTIVE theme (the picker chrome stays the picker's).
fn draw_preview(canvas: &mut dyn Canvas, rect: Rect, picked: &'static Theme, active: &TokenSet) {
    if rect.w < 20 || rect.h < 12 {
        return;
    }
    let p = picked.tokens;
    canvas.print(
        Point::new(rect.x, rect.y - 1),
        "preview",
        active.text_faint,
        Rgba::TRANSPARENT,
    );
    // The mock lives on the PICKED theme's ground, edge to edge.
    canvas.fill(rect, ' ', p.text, p.bg);
    // Title bar: surface strip, accent mark, muted clock.
    canvas.fill(Rect::new(rect.x, rect.y, rect.w, 1), ' ', p.text, p.surface);
    canvas.print(Point::new(rect.x + 1, rect.y), "▲ app", p.accent, p.surface);
    canvas.print(
        Point::new(rect.right() - 6, rect.y),
        "12:04",
        p.text_muted,
        p.surface,
    );
    // Body copy tiers.
    canvas.print(
        Point::new(rect.x + 1, rect.y + 2),
        "primary text",
        p.text,
        p.bg,
    );
    canvas.print(
        Point::new(rect.x + 1, rect.y + 3),
        "secondary label",
        p.text_muted,
        p.bg,
    );
    canvas.print(
        Point::new(rect.x + 1, rect.y + 4),
        "placeholder",
        p.text_faint,
        p.bg,
    );
    // Semantic row.
    let mut x = rect.x + 1;
    for (dot, ink) in [
        ("●ok", p.ok),
        (" ●warn", p.warn),
        (" ●err", p.error),
        (" ●info", p.info),
    ] {
        x += canvas.print(Point::new(x, rect.y + 5), dot, ink, p.bg);
    }
    // Selected row + button chip.
    canvas.print(
        Point::new(rect.x + 1, rect.y + 7),
        " selected row              ",
        p.selection_fg,
        p.selection_bg,
    );
    canvas.print(
        Point::new(rect.x + 1, rect.y + 8),
        " action ",
        p.text,
        p.surface_raised,
    );
    canvas.print(Point::new(rect.x + 11, rect.y + 8), "link", p.link, p.bg);
    // Progress on the raised track.
    let bar = Rect::new(rect.x + 1, rect.y + 10, rect.w - 2, 1);
    canvas.fill(bar, ' ', p.text, p.surface_raised);
    for x in bar.x..bar.x + (bar.w * 2 / 3) {
        canvas.put(Point::new(x, bar.y), '█', p.accent, p.surface_raised);
    }
    // Border + focus stroke samples along the bottom.
    if rect.h >= 14 {
        let y = rect.y + 12;
        let mut x = rect.x + 1;
        x += canvas.print(Point::new(x, y), "──────", p.border, p.bg);
        x += canvas.print(Point::new(x, y), "  ", p.border, p.bg);
        canvas.print(Point::new(x, y), "──────", p.border_focus, p.bg);
    }
}

fn draw_card(
    canvas: &mut dyn Canvas,
    card: Rect,
    entry: &'static Theme,
    active_tokens: &TokenSet,
    selected: bool,
    is_active: bool,
) {
    let at = active_tokens;
    let (name_fg, card_bg) = if selected {
        (at.selection_fg, at.selection_bg)
    } else {
        (at.text, Rgba::TRANSPARENT)
    };
    if selected {
        canvas.fill(Rect::new(card.x, card.y, card.w, 2), ' ', name_fg, card_bg);
    }
    // Name row: an ok-colored dot marks the ACTIVE theme.
    let mut x = card.x;
    if is_active {
        x += canvas.print(Point::new(x, card.y), "● ", at.ok, card_bg);
    } else {
        x += canvas.print(Point::new(x, card.y), "  ", name_fg, card_bg);
    }
    let name: String = entry
        .label
        .chars()
        .take((card.w - 3).max(0) as usize)
        .collect();
    canvas.print(Point::new(x, card.y), &name, name_fg, card_bg);

    // Swatch row: the TARGET theme's own colors, previewed on the active
    // ground (grounds first, then accents and semantics).
    let e = entry.tokens;
    let mut x = card.x + 2;
    for color in [
        e.bg,
        e.surface,
        e.surface_raised,
        e.accent,
        e.accent_alt,
        e.ok,
        e.warn,
        e.error,
        e.info,
    ] {
        x += canvas.print(Point::new(x, card.y + 1), "██", color, card_bg);
    }
}
