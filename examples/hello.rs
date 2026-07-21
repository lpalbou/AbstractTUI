//! hello — the AbstractTUI hero snippet.
//!
//! A themed panel, a reactive line bound to a signal, quit on q or
//! Ctrl+C. The whole engine — raw terminal, damage-driven rendering,
//! fine-grained reactivity, design tokens — one prelude import, under
//! fifty lines.
//!
//! Try: `ABSTRACTTUI_THEME=rose-pine cargo run --example hello`
//!
//! OWNER: DESIGN.

use abstracttui::prelude::*;

fn main() -> abstracttui::base::Result<()> {
    if !abstracttui::term::have_tty() {
        println!("hello: needs an interactive terminal — skipping cleanly");
        return Ok(());
    }
    if let Ok(id) = std::env::var("ABSTRACTTUI_THEME") {
        set_theme_by_id(&id);
    }

    let mut app = App::new(Size::new(80, 24));
    let quitter = app.quitter();
    app.mount(move |cx| {
        let theme = use_theme(cx);
        let presses = cx.signal(0u32);

        Element::new()
            .style(LayoutStyle::column().padding(Edges::all(1)))
            .shortcut(KeyChord::plain(Key::Char('q')), move |_| quitter.quit())
            .shortcut(KeyChord::plain(Key::Char(' ')), move |_| {
                presses.update(|n| *n += 1)
            })
            .child(dyn_view(LayoutStyle::default().grow(1.0), move || {
                let t = theme.get().tokens;
                Block::new()
                    .border(BorderKind::Rounded)
                    .title("hello")
                    .fill(t.surface)
                    .layout(LayoutStyle::column().gap(1).grow(1.0))
                    .child(Logo::new().tagline(true).element(&t).build())
                    .child(dyn_view(LayoutStyle::default(), move || {
                        text(format!("space pressed {} times", presses.get()))
                    }))
                    .child(text("space · count up      q · quit"))
                    .element(&t)
                    .build()
            }))
            .build()
    })?;
    app.run()
}
