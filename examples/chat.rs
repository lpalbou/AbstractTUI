//! A local echo chat: no framework, service, model or network is required.
use abstracttui::prelude::*;
fn main() -> abstracttui::base::Result<()> {
    if !abstracttui::term::have_tty() {
        println!("chat: run in an interactive terminal");
        return Ok(());
    }
    let mut app = App::new(Size::new(88, 30));
    let quit = app.quitter();
    app.mount(move |cx| {
        let messages = cx.signal(vec![ChatMessage::assistant(
            "Welcome. This example echoes messages locally.\n\n**Ctrl+Q** quits.",
        )]);
        let draft = cx.signal(String::new());
        Element::new()
            .style(LayoutStyle::column().grow(1.0).padding(Edges::all(1)))
            .shortcut(KeyChord::ctrl(Key::Char('q')), move |_| quit.quit())
            .child(
                ChatPanel::new(
                    draft,
                    move || messages.get(),
                    move |question| {
                        messages.update(|m| {
                            m.push(ChatMessage::user(question));
                            m.push(ChatMessage::assistant(format!("You wrote:\n\n{question}")));
                        });
                        draft.set(String::new());
                    },
                )
                .assistant_label("Local helper")
                .view(cx),
            )
            .build()
    })?;
    app.run()
}
