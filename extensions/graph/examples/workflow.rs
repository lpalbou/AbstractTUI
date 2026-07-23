//! workflow — a pipeline DAG through the layered pass + GraphView.
//!
//! A gateway-flow-ish graph: fetch -> parse -> validate branching into
//! render/index -> publish, with node statuses as kind tints (ok /
//! warn / error), live badge counts, one DOTTED async edge, and a
//! deliberate publish -> fetch retry CYCLE so the broken-edge honesty
//! marker is on screen (dotted, error ink, routed against flow).
//!
//! Keys: Tab focuses the graph · arrows pan · Enter selects the first
//! node, then arrows move the selection, Enter presses it · Escape
//! deselects · hover a card for its tooltip · q quits.
//!
//! Try: `cargo run -p abstracttui-graph --example workflow`

use abstracttui::prelude::*;
use abstracttui::ui::{dyn_view_scoped, text};
use abstracttui_graph::{EdgeDesc, GraphDesc, GraphStyle, GraphView, NodeDesc};

fn pipeline() -> GraphDesc {
    GraphDesc::new()
        .with_node(NodeDesc::new("fetch", 13, 3).label("Fetch").kind("ok"))
        .with_node(NodeDesc::new("parse", 13, 3).label("Parse").kind("ok"))
        .with_node(
            NodeDesc::new("validate", 14, 3)
                .label("Validate")
                .kind("warn"),
        )
        .with_node(NodeDesc::new("render", 12, 3).label("Render").kind("ok"))
        .with_node(NodeDesc::new("index", 11, 3).label("Index").kind("error"))
        .with_node(NodeDesc::new("publish", 13, 3).label("Publish").kind("ok"))
        .edge("fetch", "parse")
        .edge("parse", "validate")
        .edge("validate", "render")
        .with_edge(
            EdgeDesc::new("validate", "index")
                .label("async")
                .style("dotted"),
        )
        .edge("render", "publish")
        .edge("index", "publish")
        // The retry loop: cycle-broken by the layout, rendered dotted
        // in the honesty ink — never silently reordered.
        .with_edge(EdgeDesc::new("publish", "fetch").label("retry"))
}

fn main() -> abstracttui::base::Result<()> {
    if !abstracttui::term::have_tty() {
        println!("workflow: needs an interactive terminal — skipping cleanly");
        return Ok(());
    }
    if let Ok(id) = std::env::var("ABSTRACTTUI_THEME") {
        set_theme_by_id(&id);
    }

    let mut app = App::new(Size::new(100, 30));
    let quitter = app.quitter();
    app.mount(move |cx| {
        let theme = use_theme(cx);
        let pressed = cx.signal(String::from("press a node (Enter or click)"));
        let queue = cx.signal(7u32);

        Element::new()
            .style(LayoutStyle::column().padding(Edges::all(1)).gap(1))
            .shortcut(KeyChord::plain(Key::Char('q')), move |_| quitter.quit())
            .child(text("workflow — layered pipeline · q quits"))
            // Per-generation scope: the graph's internal signals live
            // and die with each theme-driven rebuild (the engine's
            // dyn_view_scoped recipe — no leaks onto the mount scope).
            .child(dyn_view_scoped(
                LayoutStyle::default().grow(1.0),
                move |gcx| {
                    let t = theme.get().tokens;
                    let style = GraphStyle::from_tokens(&t)
                        .kind_accent("ok", t.ok)
                        .kind_accent("warn", t.warn)
                        .kind_accent("error", t.error);
                    let on_press = pressed;
                    GraphView::new(pipeline())
                        .style(style)
                        .badges(move |id| (id == "fetch").then(|| queue.get().to_string()))
                        .tooltips(std::time::Duration::from_millis(300))
                        .on_node_press(move |id| {
                            on_press.set(format!("pressed: {id}"));
                        })
                        .view(gcx)
                },
            ))
            .child(dyn_view(
                LayoutStyle::default().height(Dimension::Cells(1)),
                move || text(pressed.get()),
            ))
            .build()
    })?;
    app.run()
}
