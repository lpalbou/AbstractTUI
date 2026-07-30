//! interaction_affordances — hover ink, live filtering, scroll bounds.
//!
//! Try:
//!   cargo run --example interaction_affordances
//!
//! What to look for:
//!   • Tab                  — focus the filter field, then type
//!   • Filter field         — the list narrows on every keystroke
//!   • "Ping" button hover  — accent ink
//!   • List row body hover  — accent + bold
//!   • Trailing ✕ hover     — error ink + bold; click REMOVES the row
//!   • Wheel at list edge   → outer scroll moves (footer shows oy > 0)
//!   • Esc                  — leave the filter field
//!   • q                    — quit (when the filter is not focused)
//!
//! Hover ink needs motion reports with no button held, which is not the
//! default posture — this app opts in with `RunConfig::hover_ink`.
//!
//! OWNER: REACT.

use abstracttui::prelude::*;
use abstracttui::widgets::TitleAlign;

/// Lines under the list, so the OUTER scroll has something to reveal.
/// A `Scroll` whose only child grows to fill it can never move: the
/// content solves to exactly the viewport, so the offset is pinned at 0.
const NOTES: &[&str] = &[
    "notes",
    "",
    "· The list owns its own viewport and scrollbar.",
    "· Wheel inside it while it still has room: the list scrolls.",
    "· Wheel once it is at the top or bottom edge: the event bubbles",
    "  here, to the outer Scroll, and this panel moves instead.",
    "",
    "· The ✕ hit target is the whole accessory column, not one cell.",
    "· Removing a row keeps both the selection and the scroll offset,",
    "  because `selection` and `offset_y` are bound to signals that",
    "  outlive the rebuild.",
    "",
    "· Filtering rebuilds the list from a SUBSET, so the index the",
    "  List reports is positional in that subset — the example maps it",
    "  back through `back[]` before touching the backing data.",
];

fn main() -> abstracttui::base::Result<()> {
    if !abstracttui::term::have_tty() {
        println!("interaction_affordances: needs an interactive terminal");
        return Ok(());
    }
    if let Ok(id) = std::env::var("ABSTRACTTUI_THEME") {
        set_theme_by_id(&id);
    }

    let mut app = App::new(Size::new(64, 22));
    let quitter = app.quitter();
    let _ = app.mount(move |cx| {
        let theme = use_theme(cx);
        let status = cx.signal(String::from("Tab to the filter, then type"));
        let outer_oy = cx.signal(0i32);
        let filter = cx.signal(String::new());
        let names: Vec<String> = (0..40).map(|i| format!("channel-{i:02}")).collect();
        let channels = cx.signal(names);
        // Selection and viewport live on the MOUNT scope: the list
        // region below rebuilds on every keystroke and every removal,
        // and neither the highlight nor the scroll position may reset
        // when it does.
        let selection = cx.signal(0usize);
        let list_oy = cx.signal(0i32);

        Element::new()
            .style(
                LayoutStyle::column()
                    .padding(Edges::all(1))
                    .gap(1)
                    .grow(1.0),
            )
            .shortcut(KeyChord::plain(Key::Char('q')), move |_| quitter.quit())
            .child(dyn_view(LayoutStyle::line(1).shrink(0.0), move || {
                text("interaction affordances — hover ink + live filter")
            }))
            .child(dyn_view(LayoutStyle::line(1).shrink(0.0), move || {
                text(format!("» {}", status.get()))
            }))
            .child(dyn_view(
                LayoutStyle::default().grow(1.0).min_h(10),
                move || {
                    let t = theme.get().tokens;
                    Block::new()
                        .title("channels")
                        .title_align(TitleAlign::Left)
                        .border(BorderKind::Rounded)
                        .fill(t.surface)
                        .layout(LayoutStyle::column().grow(1.0).gap(1))
                        // The filter row lives OUTSIDE the region that
                        // reads `filter`. Rebuilding a TextInput on its
                        // own keystroke would re-mint its caret.
                        .child(
                            Element::new()
                                .style(LayoutStyle::row().gap(2).shrink(0.0))
                                .child(
                                    TextInput::new()
                                        .value(filter)
                                        .placeholder("filter…")
                                        .layout(LayoutStyle::default().grow(1.0))
                                        .element(cx, &t)
                                        .build(),
                                )
                                .child(
                                    Button::new("Ping")
                                        .on_click(move || status.set("Ping clicked".into()))
                                        .element(cx, &t)
                                        .build(),
                                )
                                .build(),
                        )
                        .child(
                            Scroll::new(
                                Element::new()
                                    .style(LayoutStyle::column().gap(1))
                                    .child(dyn_view_scoped(
                                        LayoutStyle::default().h(8).shrink(0.0),
                                        move |gcx| {
                                            let t = theme.get().tokens;
                                            let all = channels.get();
                                            let q = filter.get();
                                            // The List reports an index
                                            // into the rows IT was given
                                            // — this filtered view, not
                                            // `channels`. Carry the
                                            // backing index or a dismiss
                                            // deletes the wrong row.
                                            let shown: Vec<(usize, String)> = all
                                                .iter()
                                                .cloned()
                                                .enumerate()
                                                .filter(|(_, c)| {
                                                    q.is_empty() || c.contains(q.as_str())
                                                })
                                                .collect();
                                            let back: Vec<usize> =
                                                shown.iter().map(|(i, _)| *i).collect();
                                            List::of(shown.iter().map(|(_, c)| c.as_str()))
                                                .selection(selection)
                                                .offset_y(list_oy)
                                                // Removable rows in one
                                                // call: the ✕ is drawn
                                                // and routed for you.
                                                .on_remove(move |row| {
                                                    let Some(&real) = back.get(row) else {
                                                        return;
                                                    };
                                                    channels.update(|v| {
                                                        let gone = v.remove(real);
                                                        status.set(format!("removed {gone}"));
                                                    });
                                                })
                                                .on_select(move |i| {
                                                    status.set(format!("selected row {i}"))
                                                })
                                                .layout(LayoutStyle::default().grow(1.0))
                                                .element(gcx, &t)
                                                .build()
                                        },
                                    ))
                                    .child(dyn_view(
                                        LayoutStyle::default().h(NOTES.len() as i32).shrink(0.0),
                                        move || {
                                            let t = theme.get().tokens;
                                            Element::new()
                                                .style(
                                                    LayoutStyle::default()
                                                        .h(NOTES.len() as i32)
                                                        .shrink(0.0),
                                                )
                                                .draw(move |canvas, rect| {
                                                    for (i, line) in NOTES.iter().enumerate() {
                                                        let y = rect.y + i as i32;
                                                        if y >= rect.bottom() {
                                                            break;
                                                        }
                                                        let _ = canvas.print(
                                                            Point::new(rect.x, y),
                                                            line,
                                                            t.text_muted,
                                                            Rgba::TRANSPARENT,
                                                        );
                                                    }
                                                })
                                                .build()
                                        },
                                    ))
                                    .build(),
                            )
                            .offset_y(outer_oy)
                            .scrollbar_auto_hide(true)
                            .layout(LayoutStyle::default().grow(1.0))
                            .element(cx, &t)
                            .build(),
                        )
                        .element(&t)
                        .build()
                },
            ))
            .child(dyn_view(LayoutStyle::line(1).shrink(0.0), move || {
                let oy = outer_oy.get();
                let shown = {
                    let q = filter.get();
                    let all = channels.get();
                    all.iter()
                        .filter(|c| q.is_empty() || c.contains(q.as_str()))
                        .count()
                };
                text(if oy > 0 {
                    format!("{shown} shown · outer scroll oy={oy} — wheel bubbled from the list")
                } else {
                    format!("{shown} shown · scroll the list to an edge, then keep wheeling")
                })
            }))
            .build()
    });
    // Hover ink is the reason this app pays for motion reporting.
    app.run_with(RunConfig {
        hover_ink: true,
        ..RunConfig::default()
    })
}
