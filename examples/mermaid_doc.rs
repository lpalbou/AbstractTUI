//! mermaid_doc — diagrams INSIDE a markdown document.
//!
//! The integration this crate exists to make trivial: a ```mermaid
//! fence renders as a diagram in place, in the document's own scroll
//! surface. One widget, one scroll offset, one search index — the app
//! does not split the document and splice widgets between the pieces.
//!
//! ```rust,ignore
//! MarkdownView::new(doc).fence_block(Rc::new(MermaidFence::new()))
//! ```
//!
//! `FenceBlock` is the core seam; `MermaidFence` is this crate's
//! claimant for it. Fences it does not claim (```rust below) render as
//! code, untouched — which is what you should see as you scroll.
//!
//! Scrolling is the `Scroll` container's, not the app's: wheel,
//! arrows, PgUp/PgDn, Home/End and a draggable scrollbar thumb all
//! work because the document lives inside one. An app that tracks its
//! own offset gets keyboard-only scrolling — the mistake this example
//! used to make.
//!
//! Keys: wheel or ↑/↓/PgUp/PgDn scroll · t theme · q quits.
//!
//! Try: `cargo run --example mermaid_doc`
//! Or:  `cargo run --example mermaid_doc -- NOTES.md`
//!
//! Docs: docs/graphs-and-diagrams.md § "Diagrams inside a markdown
//! document".

use std::rc::Rc;

use abstracttui::app::set_theme_by_id;
use abstracttui::prelude::*;
use abstracttui::theme::themes;
use abstracttui::ui::{dyn_view_scoped, text};
use abstracttui::widgets::{MarkdownView, Scroll};
use abstracttui_mermaid::MermaidFence;

const DOC: &str = r#"# Release pipeline

Every push runs the same three stages. The diagram below is a
` ``mermaid ` fence in this document's source — the reader sees the
picture, never the code.

```mermaid
flowchart LR
    Push[git push] --> CI{CI}
    CI -- green --> Stage[(staging)]
    CI -- red --> Fix[fix it]
    Fix -.-> Push
    Stage --> Prod[(production)]
```

## What each stage owns

- **CI** builds, tests, and lints. A red run stops here.
- **Staging** is the last place a change is reversible for free.
- **Production** is a tag push, and nothing else.

## The state a release moves through

Grouping is written as a `subgraph`; the group's members render and the
box does not, with a notice saying so:

```mermaid
flowchart TB
    Draft --> Review
    subgraph gate [Release gate]
        direction LR
        Review --> Approve --> Tag
    end
    Tag --> Published
```

## A fence this crate does not claim

Code stays code — the claimant declines anything that is not mermaid:

```rust
fn main() {
    println!("still highlighted as code");
}
```

## A sequence, with its control flow

Sequence diagrams draw their blocks as labeled frames, and activations
as bars on the lifeline:

```mermaid
sequenceDiagram
    participant Web
    participant Api
    Web->>+Api: GET /orders
    alt cache warm
        Api-->>Web: 200 (cached)
    else cold
        loop every page
            Api->>Api: fetch batch
        end
        Api-->>-Web: 200 OK
    end
```

## An unsupported diagram

Falls back atomically, in place, naming what it could not read — the
document keeps flowing around it:

```mermaid
gantt
    title Not in the subset
    section Section
    A task :a1, 2014-01-01, 30d
```

That is the whole contract: render it, or say why not.
"#;

fn main() -> abstracttui::base::Result<()> {
    if !abstracttui::term::have_tty() {
        println!("mermaid_doc: needs an interactive terminal — skipping cleanly");
        return Ok(());
    }
    if let Ok(id) = std::env::var("ABSTRACTTUI_THEME") {
        set_theme_by_id(&id);
    }
    let doc = match std::env::args().nth(1) {
        Some(path) => std::fs::read_to_string(&path)
            .unwrap_or_else(|e| format!("# cannot read {path}\n\n{e}\n")),
        None => DOC.to_string(),
    };

    let mut app = App::new(Size::new(96, 34));
    let quitter = app.quitter();
    app.mount(move |cx| {
        let theme_ix = cx.signal(0usize);
        let doc = doc.clone();
        // ONE claimant for the whole document: it caches per
        // (source, width, theme), so scrolling re-paints rows without
        // re-rendering a diagram.
        let fence: Rc<dyn abstracttui::widgets::FenceBlock> = Rc::new(MermaidFence::new());

        // `Scroll` owns the gesture surface — wheel, arrows, PgUp/PgDn,
        // Home/End, and a draggable scrollbar thumb — so the document
        // scrolls the way every other pane in the engine does. An app
        // that hand-rolls a scroll offset gets keys and NO wheel; this
        // is why the container exists.
        let body = dyn_view_scoped(LayoutStyle::default().grow(1.0), move |vcx| {
            let doc = doc.clone();
            let fence = Rc::clone(&fence);
            Scroll::new(
                MarkdownView::new(doc)
                    .fence_block(fence)
                    .layout(LayoutStyle::default().grow(1.0))
                    .view(vcx),
            )
            .axes(false, true)
            .view(vcx)
        });

        Element::new()
            .style(LayoutStyle::column().padding(Edges::all(1)).gap(1))
            .shortcut(KeyChord::plain(Key::Char('q')), move |_| quitter.quit())
            .shortcut(KeyChord::plain(Key::Char('t')), move |_| {
                theme_ix.update(|i| *i = (*i + 1) % themes().len());
                set_theme_by_id(themes()[theme_ix.get_untracked()].id);
            })
            .child(text(
                "mermaid_doc — wheel or ↑/↓/PgUp/PgDn scrolls · t theme · q quits",
            ))
            .child(body)
            .build()
    })?;
    app.run()
}
