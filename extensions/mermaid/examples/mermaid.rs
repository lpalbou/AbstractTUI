//! mermaid — the subset renderer over embedded samples or a file.
//!
//! Four embedded samples show the honest range: a TD flowchart, an LR
//! flowchart with labels + shapes, a sequence diagram, and a gantt
//! chart falling back atomically (code fence + named notice +
//! mermaid.live link).
//!
//! Keys: Left/Right (or h/l) switch samples · Tab focuses the diagram
//! (arrows pan; Enter selects flowchart nodes) · q quits.
//!
//! Try: `cargo run -p abstracttui-mermaid --example mermaid`
//! Or:  `cargo run -p abstracttui-mermaid --example mermaid -- file.mmd`

use abstracttui::prelude::*;
use abstracttui::ui::{dyn_view_scoped, text};
use abstracttui_mermaid::MermaidView;

const SAMPLES: [(&str, &str); 4] = [
    (
        "flowchart TD",
        "graph TD;\n    A[Start] --> B{Ship it?};\n    B -->|yes| C(Release);\n    C --> D([Done]);\n    B -->|no| E[Fix];\n    E -.-> B;",
    ),
    (
        "flowchart LR",
        "flowchart LR\n    A[Christmas] -->|Get money| B(Go shopping)\n    B --> C{Let me think}\n    C -->|One| D[Laptop]\n    C -->|Two| E[iPhone]\n    C -->|Three| F[Car]",
    ),
    (
        "sequence",
        "sequenceDiagram\n    participant a as Alice\n    participant j as John\n    a->>j: Hello John, how are you?\n    j-->>a: Great!\n    j->>j: schedule reply\n    Note over a,j: A typical greeting",
    ),
    (
        "unsupported (gantt)",
        "gantt\n    title A Gantt Diagram\n    section Section\n    A task :a1, 2014-01-01, 30d",
    ),
];

fn main() -> abstracttui::base::Result<()> {
    if !abstracttui::term::have_tty() {
        println!("mermaid: needs an interactive terminal — skipping cleanly");
        return Ok(());
    }
    if let Ok(id) = std::env::var("ABSTRACTTUI_THEME") {
        set_theme_by_id(&id);
    }
    let file_source = std::env::args().nth(1).map(|path| {
        std::fs::read_to_string(&path).unwrap_or_else(|e| format!("%% cannot read {path}: {e}"))
    });

    let mut app = App::new(Size::new(100, 32));
    let quitter = app.quitter();
    app.mount(move |cx| {
        let sample = cx.signal(0usize);
        let is_file = file_source.is_some();
        let title = move || {
            if is_file {
                "mermaid — file · q quits".to_string()
            } else {
                format!(
                    "mermaid — sample {}/{}: {} · ←/→ switch · q quits",
                    sample.get() + 1,
                    SAMPLES.len(),
                    SAMPLES[sample.get()].0
                )
            }
        };
        let file_for_view = file_source.clone();

        Element::new()
            .style(LayoutStyle::column().padding(Edges::all(1)).gap(1))
            .shortcut(KeyChord::plain(Key::Char('q')), move |_| quitter.quit())
            .shortcut(KeyChord::plain(Key::Left), move |_| {
                sample.set(
                    sample
                        .get_untracked()
                        .checked_sub(1)
                        .unwrap_or(SAMPLES.len() - 1),
                );
            })
            .shortcut(KeyChord::plain(Key::Right), move |_| {
                sample.set((sample.get_untracked() + 1) % SAMPLES.len());
            })
            .shortcut(KeyChord::plain(Key::Char('h')), move |_| {
                sample.set(
                    sample
                        .get_untracked()
                        .checked_sub(1)
                        .unwrap_or(SAMPLES.len() - 1),
                );
            })
            .shortcut(KeyChord::plain(Key::Char('l')), move |_| {
                sample.set((sample.get_untracked() + 1) % SAMPLES.len());
            })
            .child(dyn_view(
                LayoutStyle::default().height(Dimension::Cells(1)),
                move || text(title()),
            ))
            .child(dyn_view_scoped(
                LayoutStyle::default().grow(1.0),
                move |gcx| {
                    let source = match &file_for_view {
                        Some(src) => src.clone(),
                        None => SAMPLES[sample.get()].1.to_string(),
                    };
                    MermaidView::new(source).view(gcx)
                },
            ))
            .build()
    })?;
    app.run()
}
