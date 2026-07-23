//! network — a knowledge-graph-ish network through the FORCE pass.
//!
//! Cyclic, non-hierarchical data (the layering defeat case the force
//! pass exists for): a dozen concept nodes, seeded placement (same
//! seed, same picture — re-run and it holds), hover tooltips with the
//! node kind, and pan across the oversized canvas.
//!
//! Keys: Tab focuses the graph · arrows/wheel pan · Enter selects,
//! arrows then walk nodes spatially, Escape returns to pan · hover a
//! card for its tooltip · q quits.
//!
//! Try: `cargo run -p abstracttui-graph --example network`

use abstracttui::prelude::*;
use abstracttui::ui::{dyn_view_scoped, text};
use abstracttui_graph::{ForceOpts, GraphAlgo, GraphDesc, GraphStyle, GraphView, NodeDesc};

fn concepts() -> GraphDesc {
    let node = |id: &str, label: &str, kind: &str| {
        NodeDesc::new(id, (label.chars().count() as i32 + 6).max(9), 3)
            .label(label)
            .kind(kind)
    };
    GraphDesc::new()
        .with_node(node("signals", "Signals", "core"))
        .with_node(node("damage", "Damage", "core"))
        .with_node(node("layers", "Layers", "core"))
        .with_node(node("themes", "Themes", "design"))
        .with_node(node("tokens", "Tokens", "design"))
        .with_node(node("widgets", "Widgets", "design"))
        .with_node(node("canvas", "Canvas", "gfx"))
        .with_node(node("strokes", "Strokes", "gfx"))
        .with_node(node("mosaic", "Mosaic", "gfx"))
        .with_node(node("graphs", "Graphs", "ext"))
        .with_node(node("mermaid", "Mermaid", "ext"))
        .edge("signals", "damage")
        .edge("damage", "layers")
        .edge("layers", "signals") // cyclic: no problem for force
        .edge("themes", "tokens")
        .edge("tokens", "widgets")
        .edge("widgets", "signals")
        .edge("canvas", "strokes")
        .edge("strokes", "graphs")
        .edge("mosaic", "canvas")
        .edge("graphs", "mermaid")
        .edge("mermaid", "graphs") // parallel opposite pair: bows apart
        .edge("graphs", "widgets")
        .edge("tokens", "canvas")
}

fn main() -> abstracttui::base::Result<()> {
    if !abstracttui::term::have_tty() {
        println!("network: needs an interactive terminal — skipping cleanly");
        return Ok(());
    }
    if let Ok(id) = std::env::var("ABSTRACTTUI_THEME") {
        set_theme_by_id(&id);
    }

    let mut app = App::new(Size::new(100, 30));
    let quitter = app.quitter();
    app.mount(move |cx| {
        let theme = use_theme(cx);
        let status = cx.signal(String::from("force layout · seeded · Enter to walk nodes"));

        Element::new()
            .style(LayoutStyle::column().padding(Edges::all(1)).gap(1))
            .shortcut(KeyChord::plain(Key::Char('q')), move |_| quitter.quit())
            .child(text("network — force-placed concepts · q quits"))
            .child(dyn_view_scoped(
                LayoutStyle::default().grow(1.0),
                move |gcx| {
                    let t = theme.get().tokens;
                    let style = GraphStyle::from_tokens(&t)
                        .kind_accent("core", t.chart(0))
                        .kind_accent("design", t.chart(1))
                        .kind_accent("gfx", t.chart(2))
                        .kind_accent("ext", t.chart(3));
                    let on_press = status;
                    GraphView::new(concepts())
                        .style(style)
                        .algo(GraphAlgo::Force(ForceOpts {
                            seed: 7,
                            ..Default::default()
                        }))
                        .tooltips(std::time::Duration::from_millis(300))
                        .on_node_press(move |id| {
                            on_press.set(format!("pressed: {id}"));
                        })
                        .view(gcx)
                },
            ))
            .child(dyn_view(
                LayoutStyle::default().height(Dimension::Cells(1)),
                move || text(status.get()),
            ))
            .build()
    })?;
    app.run()
}
