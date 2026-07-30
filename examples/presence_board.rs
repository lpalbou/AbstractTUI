//! presence_board — rich list rows for a chat sidebar (agora-tui pattern).
//!
//! Each agent row has styled body text, a trailing ✕ (separate hit target),
//! and double-click on the body opens a DM. The engine owns column layout
//! (body | accessory | scrollbar) — no manual mouse X math.
//!
//! Try:
//!   cargo run --example presence_board
//!   ABSTRACTTUI_THEME=rose-pine cargo run --example presence_board
//!
//! Gestures:
//!   click body        — select row (watch the status line)
//!   double-click body — open DM (400 ms / 1 cell; watch "DM log")
//!   click ✕           — open moderation (does not change selection)
//!   hover ✕           — accent ink + bold (a badge, not a dismiss)
//!   hover row body    — the row takes accent ink; spans that set their
//!                       own color keep it
//!   wheel / arrows    — scroll the list (↑↓ when focused; wheel over list)
//!   q                 — quit
//!
//! Docs: docs/faq.md § "How does click_count() work" and
//!       § "How do I build a scrollable rich list".
//!
//! OWNER: REACT.

use abstracttui::prelude::*;
use abstracttui::render::rich::{RichLine, RichText, Span};
use abstracttui::render::Style as Ink;
use abstracttui::widgets::TitleAlign;

#[derive(Clone)]
struct Agent {
    name: &'static str,
    score: i32,
    unread: u32,
}

fn main() -> abstracttui::base::Result<()> {
    if !abstracttui::term::have_tty() {
        println!("presence_board: needs an interactive terminal — skipping cleanly");
        return Ok(());
    }
    if let Ok(id) = std::env::var("ABSTRACTTUI_THEME") {
        set_theme_by_id(&id);
    }

    let mut app = App::new(Size::new(64, 20));
    let quitter = app.quitter();
    app.mount(move |cx| {
        let agents = cx.signal(vec![
            Agent {
                name: "tui",
                score: 98,
                unread: 2,
            },
            Agent {
                name: "gateway",
                score: 87,
                unread: 0,
            },
            Agent {
                name: "runtime",
                score: 72,
                unread: 5,
            },
            Agent {
                name: "memory",
                score: 91,
                unread: 1,
            },
            Agent {
                name: "observer",
                score: 64,
                unread: 0,
            },
            Agent {
                name: "semantics",
                score: 58,
                unread: 0,
            },
            Agent {
                name: "agent",
                score: 79,
                unread: 3,
            },
            Agent {
                name: "orchestrator",
                score: 95,
                unread: 0,
            },
        ]);
        let status = cx.signal(String::from(
            "click body to select · double-click body for DM · click ✕ to moderate",
        ));
        let dm_log = cx.signal(Vec::<String>::new());
        let mod_log = cx.signal(Vec::<String>::new());
        let theme = use_theme(cx);

        Element::new()
            .style(
                LayoutStyle::column()
                    .padding(Edges::all(1))
                    .gap(1)
                    .grow(1.0),
            )
            .shortcut(KeyChord::plain(Key::Char('q')), move |_| quitter.quit())
            .child(dyn_view(LayoutStyle::line(1).shrink(0.0), move || {
                text("presence board — List rich rows + accessory column + timed double-click")
            }))
            .child(dyn_view(LayoutStyle::line(1).shrink(0.0), move || {
                text(format!("» {}", status.get()))
            }))
            .child(dyn_view(
                LayoutStyle::default()
                    .width(Dimension::Percent(1.0))
                    .grow(1.0)
                    .min_h(8),
                move || {
                    let list = agents.get();
                    let t = theme.get().tokens;
                    let rich: Vec<RichText> = list
                        .iter()
                        .map(|a| {
                            RichText::from_lines(vec![RichLine::from_spans(vec![
                                Span::plain("● "),
                                Span::new(a.name, Ink::new().fg(t.accent)),
                                Span::plain(format!("  score {}", a.score)),
                            ])])
                        })
                        .collect();
                    let names: Vec<String> = list.iter().map(|a| a.name.to_string()).collect();
                    let unread: Vec<u32> = list.iter().map(|a| a.unread).collect();

                    Block::new()
                        .title("agents")
                        .title_align(TitleAlign::Left)
                        .border(BorderKind::Rounded)
                        .fill(t.surface)
                        .layout(LayoutStyle::column().grow(1.0))
                        .child({
                            let list_dm = list.clone();
                            let list_md = list.clone();
                            List::new(names)
                                .rich_items(rich)
                                .accessory_width(4)
                                .row_accessory(move |i, _| {
                                    if unread.get(i).copied().unwrap_or(0) > 0 {
                                        Some(format!("✕{}", unread[i]))
                                    } else {
                                        Some("✕".into())
                                    }
                                })
                                .on_select(move |i| {
                                    if let Some(a) = list.get(i) {
                                        status.set(format!("selected {}", a.name));
                                    }
                                })
                                .on_row_double_click(move |i| {
                                    if let Some(a) = list_dm.get(i) {
                                        dm_log.update(|log| {
                                            log.push(format!("DM → {}", a.name));
                                            if log.len() > 6 {
                                                let drop = log.len() - 6;
                                                log.drain(0..drop);
                                            }
                                        });
                                        status.set(format!("opened DM with {}", a.name));
                                    }
                                })
                                .on_accessory_click(move |i| {
                                    if let Some(a) = list_md.get(i) {
                                        mod_log.update(|log| {
                                            log.push(format!("mod → {}", a.name));
                                            if log.len() > 6 {
                                                let drop = log.len() - 6;
                                                log.drain(0..drop);
                                            }
                                        });
                                        status.set(format!("moderation panel for {}", a.name));
                                    }
                                })
                                .layout(LayoutStyle::default().grow(1.0))
                                .element(cx, &t)
                                .build()
                        })
                        .element(&t)
                        .build()
                },
            ))
            .child(dyn_view(
                LayoutStyle::default().h(4).shrink(0.0),
                move || {
                    let t = theme.get().tokens;
                    let dm = dm_log.get();
                    let md = mod_log.get();
                    Block::new()
                        .title("action log")
                        .border(BorderKind::Plain)
                        .fill(t.surface)
                        .layout(LayoutStyle::column().gap(0))
                        .child(text(format!(
                            "DM: {}",
                            if dm.is_empty() {
                                "(none yet — double-click a row body)".into()
                            } else {
                                dm.join(" · ")
                            }
                        )))
                        .child(text(format!(
                            "mod: {}",
                            if md.is_empty() {
                                "(none yet — click ✕; hover ✕ for accent ink)".into()
                            } else {
                                md.join(" · ")
                            }
                        )))
                        .element(&t)
                        .build()
                },
            ))
            .child(dyn_view(LayoutStyle::line(1).shrink(0.0), move || {
                text("q quit · scroll with wheel when the list overflows")
            }))
            .build()
    })?;
    // Row and ✕ hover ink is the point of this example.
    app.run_with(RunConfig {
        hover_ink: true,
        ..RunConfig::default()
    })
}
