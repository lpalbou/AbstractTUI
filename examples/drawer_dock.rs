//! DrawerDock: the right-edge drawer rail (app-kits 1255).
//!
//! The team-page pattern in cells: a transcript-shaped main surface
//! and drawers — Assistant, Files, Desk — behind always-visible
//! vertical tabs on the right edge. Click a tab
//! to dock its panel open (one at a time); click the active tab or the
//! panel's ✕ to collapse back to the bare rail. The Desk tab carries a
//! reactive badge dot.
//!
//! Run: `cargo run --example drawer_dock`  (q quits; 1-3 open drawers,
//! 0 collapses — the open state is a plain signal, so keys just write
//! it.) Three drawers, deliberately: rail tabs render top-down at
//! fixed height, so the set must FIT the terminal — a tab past the
//! bottom edge is clipped and unreachable by mouse.

use abstracttui::base::Size;
use abstracttui::prelude::*;
use abstracttui::ui::text;
use abstracttui::widgets::DrawerDock;

fn main() -> abstracttui::base::Result<()> {
    let mut app = App::new(Size::new(100, 30));
    let quitter = app.quitter();
    let actions = app.actions();
    app.mount(move |cx| {
        let open = cx.signal(None::<String>);
        // Durable drawer state lives OUTSIDE the builders (the PageHost
        // recipe): the Desk badge survives open/close cycles.
        let desk_waiting = cx.signal(true);

        // Keys drive the same signal the tabs write — the signal IS the
        // API. (Registered as actions so they never fight a composer.)
        for (key, id) in [
            ('1', Some("assistant")),
            ('2', Some("files")),
            ('3', Some("desk")),
            ('0', None),
        ] {
            actions.register(
                format!("drawer-{key}"),
                Some(KeyChord::new(Mods::NONE, Key::Char(key))),
                move || open.set(id.map(str::to_string)),
            );
        }
        let quit = quitter.clone();
        actions.register(
            "quit",
            Some(KeyChord::new(Mods::NONE, Key::Char('q'))),
            move || quit.quit(),
        );

        let content = Element::new()
            .style(LayoutStyle::column().grow(1.0))
            .child(text("#agora-wui-work — 7 members"))
            .child(text(""))
            .child(text("continuum  banked: readability settled"))
            .child(text("           From the continuum seat on Monday,"))
            .child(text("           I am adopting #301 as the room split."))
            .child(text(""))
            .child(text("agora      Used. That is the current room split."))
            .child(text("           No further restatements needed."))
            .child(text(""))
            .child(text("[1-3] open drawers · [0] collapse · [q] quit"))
            .build();

        DrawerDock::new(content)
            .drawer("assistant", "Assistant", |_cx| {
                Element::new()
                    .style(LayoutStyle::column())
                    .child(text("Ask anything about this channel —"))
                    .child(text("strategies, deviations, who owes what."))
                    .child(text(""))
                    .child(text("The model reads the current window"))
                    .child(text("and cites seqs. It never posts."))
                    .build()
            })
            .drawer("files", "Files", |_cx| {
                Element::new()
                    .style(LayoutStyle::column())
                    .child(text("channel/   1"))
                    .child(text("plan/      1"))
                    .child(text("plans/     1"))
                    .build()
            })
            .drawer("desk", "Desk", move |_cx| {
                // Opening the desk consumes what was waiting.
                desk_waiting.set(false);
                text("Nothing on the desk right now.")
            })
            .drawer_badge(move || desk_waiting.get())
            .open(open)
            .panel_width(40)
            .view(cx)
    })?;
    app.run()
}
