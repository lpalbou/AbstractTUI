//! prompt_line — a one-line prompt with a host-drawn `❯` leader.
//!
//! Demonstrates: a `TextArea` beside a host-drawn leader. By default a
//! `TextArea` paints no frame, so the caret sits right after the leader.
//! `strokes(true)` adds side strokes (`▐ … ▌`), and the text starts one cell
//! after the left stroke. The top prompt is the default, the bottom one
//! opts into the strokes, so you can compare both.
//!
//! Keys: type in the focused prompt · `Enter` sends · `Tab` / `Shift+Tab`
//! switches prompt · `Ctrl+C` quits.
//!
//! Try: `cargo run --example prompt_line`
//!      `ABSTRACTTUI_THEME=catppuccin-latte cargo run --example prompt_line`
//!
//! OWNER: DESIGN.

mod common;

use abstracttui::prelude::*;

fn main() -> abstracttui::base::Result<()> {
    if !abstracttui::term::have_tty() {
        println!("prompt_line: needs an interactive terminal — skipping cleanly");
        return Ok(());
    }
    if let Ok(id) = std::env::var("ABSTRACTTUI_THEME") {
        set_theme_by_id(&id);
    }

    let mut app = App::new(Size::new(100, 24));
    app.mount(move |cx| {
        let theme = use_theme(cx);
        let t = theme.get_untracked().tokens;
        Element::new()
            .style(
                LayoutStyle::column()
                    .width(Dimension::Percent(1.0))
                    .height(Dimension::Percent(1.0)),
            )
            .child(spacer())
            .child(prompt_block(
                cx,
                &t,
                false,
                "default: no frame, the caret sits right after ❯",
                true,
            ))
            .child(line(|_, _| {}))
            .child(prompt_block(
                cx,
                &t,
                true,
                "strokes(true): ▐ … ▌ frame, text one cell after the left stroke",
                false,
            ))
            .child(spacer())
            .child(line(move |canvas, rect| {
                canvas.fill(rect, ' ', t.text, t.surface);
                common::key_legend(
                    canvas,
                    rect,
                    &t,
                    &[
                        ("type", "edit"),
                        ("Enter", "send"),
                        ("Tab", "switch prompt"),
                        ("Ctrl+C", "quit"),
                    ],
                );
            }))
            .build()
    })?;
    app.run()
}

/// A caption, the "last run" line, the `❯` prompt row and an
/// "Automations" status line, the shape of a coding agent's prompt.
fn prompt_block(
    cx: Scope,
    t: &TokenSet,
    strokes: bool,
    caption: &'static str,
    focus: bool,
) -> View {
    let sent = cx.signal(String::new());
    let t = *t;
    let composer = TextArea::new()
        .strokes(strokes)
        .placeholder("describe a task — Enter sends")
        .placeholder_while_focused(true)
        .rows(1, 4)
        .on_submit(move |s| sent.set(s.to_string()))
        .element(cx, &t);
    let composer = if focus {
        composer.autofocus()
    } else {
        composer
    };
    Element::new()
        .style(LayoutStyle::column().width(Dimension::Percent(1.0)))
        .child(line(move |canvas, rect| {
            canvas.print(
                Point::new(rect.x + 2, rect.y),
                caption,
                t.accent,
                Rgba::TRANSPARENT,
            );
        }))
        .child(line(move |canvas, rect| {
            canvas.print(
                Point::new(rect.x + 2, rect.y),
                "last run: done · 20s",
                t.text_muted,
                Rgba::TRANSPARENT,
            );
        }))
        .child(
            Element::new()
                .style(LayoutStyle::row().width(Dimension::Percent(1.0)))
                .child(
                    Element::new()
                        .style(
                            LayoutStyle::default()
                                .width(Dimension::Cells(2))
                                .height(Dimension::Cells(1)),
                        )
                        .draw(move |canvas, rect| {
                            canvas.print(
                                Point::new(rect.x, rect.y),
                                "❯",
                                t.error,
                                Rgba::TRANSPARENT,
                            );
                        })
                        .build(),
                )
                // The TextArea sizes itself (full width of its parent, one
                // row per wrapped line); this wrapper gives it the row's
                // remaining width beside the leader.
                .child(
                    Element::new()
                        .style(LayoutStyle::column().grow(1.0).basis(Dimension::Cells(0)))
                        .child(composer.build())
                        .build(),
                )
                .build(),
        )
        .child(dyn_view(
            LayoutStyle::line(1).width(Dimension::Percent(1.0)),
            move || {
                let last = sent.get();
                Element::new()
                    .style(LayoutStyle::fill())
                    .draw(move |canvas, rect| {
                        let text = if last.is_empty() {
                            "Automations · 3 new".to_string()
                        } else {
                            format!("sent: {last}")
                        };
                        canvas.print(
                            Point::new(rect.x + 2, rect.y),
                            &text,
                            t.error,
                            Rgba::TRANSPARENT,
                        );
                    })
                    .build()
            },
        ))
        .build()
}

fn line(draw: impl Fn(&mut dyn abstracttui::ui::StyledCanvas, Rect) + 'static) -> View {
    Element::new()
        .style(LayoutStyle::line(1).width(Dimension::Percent(1.0)))
        .draw(draw)
        .build()
}

fn spacer() -> View {
    Element::new()
        .style(LayoutStyle::default().grow(1.0))
        .build()
}
