//! scrollbar — the strip every scrolling widget shares, four ways.
//!
//! One seam (`widgets::scrollbar`) owns the geometry, the hit test and
//! the pointer→offset inverse, so `Scroll`, `List` and `Table` answer
//! the SAME gestures with the thumb glued to the cursor. This example
//! exists to be grabbed: four panes over the same 200 rows, with the
//! live offsets printed underneath so you can watch what a press does.
//!
//! Try:
//!   cargo run --example scrollbar
//!   ABSTRACTTUI_THEME=rose-pine cargo run --example scrollbar
//!
//! Gestures (all four panes):
//!   press the thumb    — takes hold, moves NOTHING (watch the offsets)
//!   drag it            — the thumb tracks the pointer row for row, and
//!                        keeps steering after the pointer leaves the strip
//!   press bare track   — teleports: the thumb centers on the pressed row
//!   hover the strip    — the thumb takes accent ink (this app opts into
//!                        `RunConfig::hover_ink`)
//!   press the Table's strip — scrolls, and never selects the row beside it
//!   wheel / arrows     — the other two doors onto the same offset
//!
//! Keys:
//!   w — cycle the second pane's gutter: `Scroll::scrollbar_width(1..=4)`
//!   q — quit
//!
//! Docs: docs/api.md § "The scrollbar".
//!
//! OWNER: REACT.

use abstracttui::prelude::*;
use abstracttui::widgets::{ColWidth, Column, Scroll, Table};

/// The shared content: enough rows that the thumb is at its 3-row floor
/// and every press moves the view a long way — the geometry that made
/// the old mapping jump.
const ROWS: usize = 200;

fn rows_view(cx: Scope) -> View {
    let mut col = Element::new().style(LayoutStyle::column());
    for i in 0..ROWS {
        col = col.child(text(format!("row {i:>3} — drag the bar, watch me move")));
    }
    let _ = cx;
    col.build()
}

fn main() -> abstracttui::base::Result<()> {
    if !abstracttui::term::have_tty() {
        println!("scrollbar: needs an interactive terminal — skipping cleanly");
        return Ok(());
    }
    if let Ok(id) = std::env::var("ABSTRACTTUI_THEME") {
        set_theme_by_id(&id);
    }

    let mut app = App::new(Size::new(100, 30));
    let quitter = app.quitter();
    app.mount(move |cx| {
        let t = use_theme(cx).get().tokens;
        // Bound offsets: the panes keep their position across the
        // rebuild that `w` triggers, and the footer can read them.
        let left = cx.signal(0i32);
        let right = cx.signal(0i32);
        let list_off = cx.signal(0i32);
        let gutter = cx.signal(1i32);

        let items: Vec<String> = (0..ROWS).map(|i| format!("item {i:>3}")).collect();
        let table_rows: Vec<Vec<String>> = (0..ROWS)
            .map(|i| vec![format!("file-{i:>3}.rs"), format!("{} kB", i * 3 % 97)])
            .collect();
        let selected = cx.signal(0usize);

        let pane_style = || LayoutStyle::column().grow(1.0).basis(Dimension::Cells(0));

        // Pane 1 — the terminal-native default: one reserved column.
        let one = Block::new()
            .title("Scroll · gutter 1")
            .layout(pane_style())
            .child(
                Scroll::new(rows_view(cx))
                    .offset_y(left)
                    .element(cx, &t)
                    .build(),
            )
            .element(&t)
            .build();

        // Pane 2 — the widened gutter, rebuilt when `w` cycles it. The
        // offset signal lives OUTSIDE this closure, so the position
        // survives the rebuild.
        let two = dyn_view(pane_style(), move || {
            let t = use_theme(cx).get().tokens;
            let w = gutter.get();
            Block::new()
                .title(format!("Scroll · gutter {w} (press w)"))
                .layout(pane_style())
                .child(
                    Scroll::new(rows_view(cx))
                        .offset_y(right)
                        .scrollbar_width(w)
                        .element(cx, &t)
                        .build(),
                )
                .element(&t)
                .build()
        });

        // Pane 3 — `List`: its strip used to be inert while it alone lit
        // on hover. Same seam now, same gestures.
        let list = Block::new()
            .title("List · same strip")
            .layout(pane_style())
            .child(List::of(items).offset_y(list_off).element(cx, &t).build())
            .element(&t)
            .build();

        // Pane 4 — `Table`: pressing the strip scrolls; the row beside
        // it keeps its selection (it used to be selected, and on
        // FilePicker's same math, activated).
        let table = Block::new()
            .title("Table · strip never selects")
            .layout(pane_style())
            .child(
                Table::new(vec![
                    Column::new("name", ColWidth::Flex(1.0)),
                    Column::new("size", ColWidth::Cells(7)),
                ])
                .rows(table_rows)
                .selection(selected)
                .element(cx, &t)
                .build(),
            )
            .element(&t)
            .build();

        let row = |a: View, b: View| {
            Element::new()
                .style(
                    LayoutStyle::row()
                        .gap(1)
                        .grow(1.0)
                        .basis(Dimension::Cells(0)),
                )
                .child(a)
                .child(b)
                .build()
        };

        Element::new()
            .style(LayoutStyle::column().gap(1).padding(Edges::all(1)))
            .shortcut(KeyChord::plain(Key::Char('q')), move |_| quitter.quit())
            .shortcut(KeyChord::plain(Key::Char('w')), move |_| {
                gutter.update(|w| *w = if *w >= 4 { 1 } else { *w + 1 });
            })
            .child(row(one, two))
            .child(row(list, table))
            .child(dyn_view(LayoutStyle::line(1).shrink(0.0), move || {
                text(format!(
                    "offsets — scroll {} · scroll {} · list {} · table row {}   \
                     ·  press the thumb: nothing moves  ·  w gutter  ·  q quit",
                    left.get(),
                    right.get(),
                    list_off.get(),
                    selected.get(),
                ))
            }))
            .build()
    })?;
    // Hover ink on the strip is half the point: without this opt-in the
    // terminal reports motion only while a button is down.
    app.run_with(RunConfig {
        hover_ink: true,
        ..RunConfig::default()
    })
}
