//! activate — selection vs activation, and double-click, concretely.
//!
//! The engine-wide vocabulary on its two row widgets, side by side:
//!
//! - `on_select` fires when the HIGHLIGHT moves (arrows, Home/End,
//!   any click). Never wire commitment or destruction to it.
//! - `on_activate` fires when the user COMMITS a row. On both widgets
//!   Enter (always) and Space activate. By mouse they differ, on
//!   purpose:
//!     * `List` — the timing-free picker gesture: a click on the
//!       ALREADY-selected row activates (click 1 selects, click 2
//!       lands on the selected row and opens — any pace).
//!     * `Table` — a browsing surface: only a true DOUBLE-click
//!       (second press within 400 ms / 1 cell, on the already-selected
//!       row) activates. Re-clicking a row slowly only re-selects, so
//!       focusing a pane never opens an editor; the engine synthesizes
//!       the click chain (terminals only report raw presses) and every
//!       press stays deliverable — selection is never delayed.
//!
//! Unbound is honest too: a List/Table WITHOUT `on_activate` lets
//! Enter/Space bubble to app shortcuts instead of eating them.
//!
//! Keys: Tab focus pane · arrows move (watch "selected") · Enter/Space
//! activate · mouse: click, click-on-selected (list), double-click
//! (table) · q or Ctrl+C quit.
//!
//! Docs: docs/api.md § "Double-click", § "List — selection vs
//! activation", § "Table — selection vs activation".
//!
//! OWNER: DOCS.

use abstracttui::prelude::*;
use abstracttui::widgets::{ColWidth, Column};

fn main() -> abstracttui::base::Result<()> {
    if !abstracttui::term::have_tty() {
        println!("activate: needs an interactive terminal — skipping cleanly");
        return Ok(());
    }
    if let Ok(id) = std::env::var("ABSTRACTTUI_THEME") {
        set_theme_by_id(&id);
    }

    let mut app = App::new(Size::new(80, 24));
    let quitter = app.quitter();
    app.mount(move |cx| {
        // One status line, written by all four callbacks — run the demo
        // and watch which gestures merely move it vs. commit it.
        let last = cx.signal(String::from(
            "select a row (click / arrows), then commit it (Enter / the mouse gesture)",
        ));

        let themes: Vec<&str> = vec![
            "abstract-dark",
            "nord",
            "rose-pine",
            "catppuccin-mocha",
            "gruvbox",
            "solarized-dark",
        ];
        let names = themes.clone();

        let list = List::of(themes.clone())
            .on_select(move |i| last.set(format!("list: selected row {i} — browsing")))
            // The picker gesture: click the selected row (no timer), or
            // Enter/Space. Here activation APPLIES the theme — a real
            // commitment, which is exactly why it must not fire on mere
            // movement.
            .on_activate(move |i| {
                set_theme_by_id(names[i]);
                last.set(format!("list: ACTIVATED row {i} — theme applied"));
            })
            .view(cx);

        let table = Table::new(vec![
            Column::new("session", ColWidth::Flex(1.0)),
            Column::new("state", ColWidth::Cells(10)),
        ])
        .rows(
            (1..=6)
                .map(|n| {
                    vec![
                        format!("run-{n:02}"),
                        if n % 2 == 0 { "waiting" } else { "done" }.to_string(),
                    ]
                })
                .collect(),
        )
        .on_select(move |i| last.set(format!("table: selected row {i} — browsing")))
        // Double-click (or Enter/Space) is "open this row". A slow second
        // click on the same row deliberately does NOT land here.
        .on_activate(move |i| last.set(format!("table: ACTIVATED row {i} — double-click or Enter")))
        .view(cx);

        Element::new()
            .style(LayoutStyle::column().gap(1).padding(Edges::all(1)))
            .shortcut(KeyChord::plain(Key::Char('q')), move |_| quitter.quit())
            .child(text(
                "selection follows movement — activation is the explicit commit",
            ))
            .child(
                Element::new()
                    .style(LayoutStyle::row().gap(2).grow(1.0))
                    .child(
                        Element::new()
                            .style(LayoutStyle::column().gap(1).grow(1.0))
                            .child(text("List — click the selected row to apply the theme"))
                            .child(list)
                            .build(),
                    )
                    .child(
                        Element::new()
                            .style(LayoutStyle::column().gap(1).grow(1.0))
                            .child(text("Table — double-click a row to open it"))
                            .child(table)
                            .build(),
                    )
                    .build(),
            )
            .child(dyn_view(LayoutStyle::line(1), move || {
                text(format!("» {}", last.get()))
            }))
            .child(text(
                "Tab pane · arrows select · Enter/Space activate · double-click (table) · q quit",
            ))
            .build()
    })?;
    app.run()
}
