//! status_bar — app chrome from named slots, with clickable items.
//!
//! A header and a bottom status bar built from `StatusBar` slots
//! (left / center / right). Every item tagged with an action id is a
//! semantic hit target: click it, or Tab to the bar and use Left/Right
//! then Enter. Resize the terminal to watch items yield by priority. The
//! app never draws and never computes a cell coordinate.
//!
//! Try: `cargo run --example status_bar` — q quits.
//! Docs: docs/api.md § "Status bar and action spans".
//!
//! OWNER: DESIGN.

use abstracttui::prelude::*;

fn main() -> abstracttui::base::Result<()> {
    if !abstracttui::term::have_tty() {
        println!("status_bar: needs an interactive terminal — skipping cleanly");
        return Ok(());
    }
    let mut app = App::new(Size::new(100, 24));
    let quitter = app.quitter();
    app.mount(move |cx| {
        let theme = use_theme(cx);
        let last = cx.signal(String::from("nothing yet"));
        let online = cx.signal(true);

        let header = StatusBar::live(
            move || {
                let t = theme.get().tokens;
                StatusBar::new()
                    .left(BarItem::text("▲ Demo").ink(t.accent).priority(10))
                    .left(
                        BarItem::text("workflow: basic-agent")
                            .ink(t.text)
                            .action("workflow")
                            .fit(BarFit::TruncateEnd { min: 10 })
                            .priority(5),
                    )
                    .left(BarItem::text("ollama · qwen3").action("model").priority(4))
                    .right(
                        BarItem::new()
                            .run("127.0.0.1:8080 ", t.text_muted)
                            .run(
                                if online.get() { "●" } else { "✗" },
                                if online.get() { t.ok } else { t.error },
                            )
                            .action("gateway")
                            .priority(8),
                    )
            },
            move |_, id| {
                if id == "gateway" {
                    online.update(|o| *o = !*o);
                }
                last.set(format!("header: {id}"));
            },
        );
        let footer = StatusBar::live(
            move || {
                StatusBar::new()
                    .left(BarItem::text(format!("last action: {}", last.get())).priority(1))
                    .center(
                        BarItem::text("~/projects/abstractframework/abstracttui")
                            .action("workspace")
                            .fit(BarFit::TruncateStart { min: 12 })
                            .priority(3),
                    )
                    .center(BarItem::text("session 4f2a9c").action("session").priority(2))
                    .right(BarItem::text("theme ▾").action("theme").priority(6))
            },
            move |_, id| {
                if id == "theme" {
                    toggle_mode();
                }
                last.set(format!("footer: {id}"));
            },
        );
        Element::new()
            .style(LayoutStyle::column().grow(1.0))
            .shortcut(KeyChord::plain(Key::Char('q')), move |_| quitter.quit())
            .child(header)
            .child(dyn_view(LayoutStyle::default().grow(1.0), move || {
                text(format!(
                    "\n  click a header/footer item, or Tab to a bar and use ←/→ + Enter\n  last: {}\n  q quits",
                    last.get()
                ))
            }))
            .child(footer)
            .build()
    })?;
    app.run()
}
