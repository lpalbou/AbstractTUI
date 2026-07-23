//! drawers — the global drawer system demo (app-kits 0585): full pages
//! summoned from viewport edges over a live app.
//!
//! Right drawer `i`: a MODAL inspector (scrim, focus trap, Esc/✕
//! closes, outside press dismisses) hosting a full scrollable Feed
//! page. Left drawer `g`: a PASSIVE nav panel — glanceable, the app
//! keeps the keyboard until you click into it (then Esc closes it).
//! Both keep their page state across close/reopen because the state
//! lives in app-owned signals outside the builders (the Tabs rule).
//!
//! Keys: i inspector · g nav · n add a feed line (live while open) ·
//! Ctrl+T theme · q quit.
//!
//! OWNER: DRAWER (0585).

use std::time::Duration;

use abstracttui::prelude::*;
use abstracttui::theme::themes;
use abstracttui::widgets::{Feed, FeedItem, FeedState};

fn main() -> abstracttui::base::Result<()> {
    if !abstracttui::term::have_tty() {
        println!("drawers: needs an interactive terminal — skipping cleanly");
        return Ok(());
    }
    if let Ok(id) = std::env::var("ABSTRACTTUI_THEME") {
        abstracttui::app::set_theme_by_id(&id);
    }

    let mut app = App::new(Size::new(100, 30));
    let quitter = app.quitter();

    app.mount(move |cx| {
        let theme = use_theme(cx);
        // App-owned state: survives drawer close/reopen (the builders
        // below only CAPTURE these — the Tabs rule).
        let events = cx.signal(0u32);
        let theme_ix = cx.signal(0usize);
        let feed = FeedState::new(cx);
        feed.push(
            "intro",
            FeedItem::markdown(
                "**Inspector** — a full page in a drawer.\n\n\
                 Wheel scrolls; `n` on the main surface adds lines\n\
                 while this panel is closed, and they are here when\n\
                 you come back.",
            ),
        );
        for i in 0..18 {
            feed.push(format!("seed{i}"), FeedItem::text(format!("event {i:02}")));
        }

        // The MODAL inspector: scrim + trap + Esc; a complex page.
        let inspector = Drawer::new(DrawerEdge::Right)
            .size(DrawerSize::Percent(0.45))
            .title("Inspector")
            .motion(Duration::from_millis(160))
            .install(cx, {
                let feed = feed.clone();
                move |mount| {
                    let t = theme.get().tokens;
                    Element::new()
                        .style(LayoutStyle::column().gap(1).grow(1.0))
                        .child(dyn_view(LayoutStyle::line(1), move || {
                            text(format!("live events: {} (app store)", events.get()))
                        }))
                        .child(
                            Scroll::new(Feed::new(&feed).gap(0).view(mount))
                                .element(mount, &t)
                                .build(),
                        )
                        .build()
                }
            });

        // The PASSIVE nav: glanceable — the app keeps typing focus;
        // click into the panel to give it keys (Esc then closes it).
        let nav = Drawer::new(DrawerEdge::Left)
            .size(DrawerSize::Cells(24))
            .focus(DrawerFocus::Passive)
            .title("Navigate")
            .motion(Duration::from_millis(120))
            .install(cx, move |_| {
                Element::new()
                    .style(LayoutStyle::column().gap(1).grow(1.0))
                    .child(text(
                        "  overview\n  reader\n  settings\n\n(glanceable: keys stay\nwith the app — click in\nto focus, esc closes)",
                    ))
                    .build()
            });

        let ins = inspector.clone();
        let nv = nav.clone();
        let feed_for_n = feed.clone();
        Element::new()
            .style(
                LayoutStyle::column()
                    .width(Dimension::Percent(1.0))
                    .height(Dimension::Percent(1.0)),
            )
            .shortcut(KeyChord::plain(Key::Char('q')), move |_| quitter.quit())
            .shortcut(KeyChord::plain(Key::Char('i')), move |_| ins.toggle())
            .shortcut(KeyChord::plain(Key::Char('g')), move |_| nv.toggle())
            .shortcut(KeyChord::plain(Key::Char('n')), move |_| {
                let n = events.get_untracked() + 1;
                events.set(n);
                feed_for_n.push(
                    format!("live{n}"),
                    FeedItem::text(format!("event {:02} (added live)", 18 + n)),
                );
            })
            .shortcut(KeyChord::new(Mods::CTRL, Key::Char('t')), move |_| {
                let list = themes();
                let next = (theme_ix.get_untracked() + 1) % list.len();
                theme_ix.set(next);
                abstracttui::app::set_theme(&list[next]);
            })
            .child(
                Element::new()
                    .style(LayoutStyle::column().gap(1).grow(1.0))
                    .child(text("main surface — drawers overlay this page"))
                    .child(dyn_view(LayoutStyle::line(1), move || {
                        text(format!("events: {} (press n, then open the inspector)", events.get()))
                    }))
                    .build(),
            )
            .child(text(
                " i inspector · g nav · n event · Ctrl+T theme · q quit",
            ))
            .build()
    })?;
    app.tree().focus_first();
    app.run()
}
