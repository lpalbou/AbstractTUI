//! grid — the track-grid layout, reflowing live.
//!
//! Demonstrates: `Display::Grid` with every track flavor on one screen —
//! fixed `Cells`, `Percent`, content-sized `Auto`, weighted `Fr` — plus
//! column/row spans. Resize the terminal and watch fr tracks re-tile
//! exactly (largest-remainder rounding: no dropped columns); `g` cycles
//! three track recipes so the SAME children reflow under different
//! geometry — the resize story, scripted.
//!
//! Keys: g cycle grid recipe · t theme · q quit.
//!
//! OWNER: DESIGN.

mod common;

use abstracttui::prelude::*;
use abstracttui::theme::themes;

/// Three track recipes over the same 8 children.
fn recipe(ix: usize) -> (Vec<Track>, &'static str) {
    match ix % 3 {
        0 => (
            vec![Track::Fr(1.0), Track::Fr(1.0), Track::Fr(1.0)],
            "3 equal fr columns",
        ),
        1 => (
            vec![Track::Cells(24), Track::Fr(2.0), Track::Fr(1.0)],
            "24 cells · 2fr · 1fr",
        ),
        _ => (
            vec![Track::Percent(0.25), Track::Fr(1.0), Track::Percent(0.25)],
            "25% · fr · 25%",
        ),
    }
}

fn main() -> abstracttui::base::Result<()> {
    if !abstracttui::term::have_tty() {
        println!("grid: needs an interactive terminal — skipping cleanly");
        return Ok(());
    }
    let mut app = App::new(Size::new(96, 28));
    let quitter = app.quitter();
    app.mount(move |cx| {
        let theme = use_theme(cx);
        let recipe_ix = cx.signal(0usize);
        let theme_ix = cx.signal(0usize);

        Element::new()
            .style(LayoutStyle::column().padding(Edges::all(1)).gap(1))
            .shortcut(KeyChord::plain(Key::Char('q')), move |_| quitter.quit())
            .shortcut(KeyChord::plain(Key::Char('g')), move |_| {
                recipe_ix.update(|i| *i += 1)
            })
            .shortcut(KeyChord::plain(Key::Char('t')), move |_| {
                theme_ix.update(|i| *i = (*i + 1) % themes().len());
                set_theme_by_id(themes()[theme_ix.get_untracked()].id);
            })
            .child(dyn_view(LayoutStyle::default().h(1), move || {
                let (_, label) = recipe(recipe_ix.get());
                text(format!("grid — {label}    (g cycles · t theme · q quit)"))
            }))
            .child(dyn_view(LayoutStyle::default().grow(1.0), move || {
                let t = theme.get().tokens;
                let (cols, _) = recipe(recipe_ix.get());
                // Rows: a fixed header band, then implicit rows size to
                // their tallest child.
                let grid_style = LayoutStyle::default()
                    .grid(cols, vec![Track::Cells(5)])
                    .gap(1)
                    .grow(1.0);
                let mut grid = Element::new().style(grid_style);
                // A spanning hero card first (col_span 2), then cards.
                grid = grid.child(card(
                    &t,
                    "hero — col_span 2",
                    0,
                    LayoutStyle::default().col_span(2),
                ));
                for i in 1..8 {
                    let label: &'static str = CARD_LABELS[i % CARD_LABELS.len()];
                    grid = grid.child(card(&t, label, i, LayoutStyle::default()));
                }
                grid.build()
            }))
            .build()
    })?;
    app.run()
}

const CARD_LABELS: [&str; 4] = ["alpha", "beta", "gamma", "delta"];

/// One themed grid cell: a shadowed panel with a chart-slot accent bar —
/// visually distinct per index so reflow is easy to follow.
fn card(t: &TokenSet, label: &'static str, ix: usize, layout: LayoutStyle) -> View {
    let accent = t.chart(ix % 8);
    let ink = t.text_muted;
    Block::new()
        .border(BorderKind::Rounded)
        .title(label)
        .fill(t.surface)
        .shadow(t.shadow_ground)
        .layout(layout.min_h(4))
        .child(
            Element::new()
                .style(LayoutStyle::default().h(1))
                .draw(move |canvas, rect| {
                    for x in rect.x..(rect.x + (rect.w / 2).max(1)).min(rect.right()) {
                        canvas.put(Point::new(x, rect.y), '▄', accent, Rgba::TRANSPARENT);
                    }
                })
                .build(),
        )
        .child(
            Element::new()
                .style(LayoutStyle::default().h(1))
                .draw(move |canvas, rect| {
                    let msg = format!("cell {ix}");
                    canvas.print(Point::new(rect.x, rect.y), &msg, ink, Rgba::TRANSPARENT);
                })
                .build(),
        )
        .element(t)
        .build()
}
