//! DrawerDock proof screenshots (app-kits 1255): the example scene
//! driven through the REAL pipeline — `Driver` frames, SGR mouse bytes
//! in — captured with the engine's own screenshot verb at each state.
//!
//! Always asserts the states; writes SVGs only when
//! `DRAWER_DOCK_SHOTS=<dir>` is set (the proof-artifact mode), so CI
//! runs the assertions with zero filesystem side effects.

use abstracttui::app::{App, Driver, RunConfig};
use abstracttui::base::Size;
use abstracttui::prelude::*;
use abstracttui::term::Capabilities;
use abstracttui::testing::CaptureTerm;
use abstracttui::ui::text;
use abstracttui::widgets::DrawerDock;

const W: i32 = 100;
const H: i32 = 30;

fn config() -> RunConfig {
    RunConfig {
        caps: Some(Capabilities::with(|c| {
            c.truecolor = true;
            c.colors_256 = true;
        })),
        probe: false,
        platform_clipboard: false,
        ..RunConfig::default()
    }
}

fn settle(driver: &mut Driver, app: &mut App, term: &mut CaptureTerm) {
    for _ in 0..32 {
        if driver.turn(app, term).expect("turn").idle {
            return;
        }
    }
    panic!("failed to settle");
}

/// SGR full click at cell (x, y) — press + release (coords 1-based on
/// the wire).
fn wire_click(term: &mut CaptureTerm, x: i32, y: i32) {
    term.push_input(format!("\x1b[<0;{};{}M", x + 1, y + 1).as_bytes());
    term.push_input(format!("\x1b[<0;{};{}m", x + 1, y + 1).as_bytes());
}

fn scene(app: &mut App) {
    app.mount(|cx| {
        let desk_waiting = cx.signal(true);
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
                desk_waiting.set(false);
                text("Nothing on the desk right now.")
            })
            .drawer_badge(move || desk_waiting.get())
            .panel_width(40)
            .view(cx)
    })
    .expect("mount");
}

fn shot(driver: &Driver, name: &str) {
    if let Ok(dir) = std::env::var("DRAWER_DOCK_SHOTS") {
        let path = std::path::Path::new(&dir).join(format!("{name}.svg"));
        driver.screenshot().write_svg(&path).expect("write svg");
        eprintln!("wrote {}", path.display());
    }
}

fn screen(term: &CaptureTerm) -> String {
    term.screen().to_text()
}

#[test]
fn dock_states_through_the_wire_with_proof_shots() {
    let size = Size::new(W, H);
    let mut app = App::new(size);
    scene(&mut app);
    let mut term = CaptureTerm::new(size);
    let mut driver = Driver::new(&mut app, &mut term, config()).expect("driver");
    settle(&mut driver, &mut app, &mut term);

    // 1 — collapsed: content + bare rail, badge dot on Desk, no panel.
    let s = screen(&term);
    assert!(s.contains("#agora-wui-work"));
    assert!(!s.contains("channel/"), "no panel while collapsed");
    assert!(s.contains('●'), "desk badge dot visible on the rail");
    shot(&driver, "1-collapsed");

    // 2 — click the Files tab (second block: Assistant 11 rows, Files
    // starts at row 11; label rows 12..16).
    wire_click(&mut term, W - 2, 13);
    settle(&mut driver, &mut app, &mut term);
    let s = screen(&term);
    assert!(s.contains("Files"), "panel header");
    assert!(s.contains("channel/"), "files body docked open");
    shot(&driver, "2-files-open");

    // 3 — switch to Assistant by tab (first block, label rows 1..10).
    wire_click(&mut term, W - 2, 3);
    settle(&mut driver, &mut app, &mut term);
    let s = screen(&term);
    assert!(s.contains("cites seqs"), "assistant body");
    assert!(!s.contains("channel/"), "one panel at a time");
    shot(&driver, "3-assistant-open");

    // 4 — close via the header ✕ corner: panel spans x 57..97 with the
    // rail at 97..100; ✕ at x 95.
    wire_click(&mut term, W - 5, 0);
    settle(&mut driver, &mut app, &mut term);
    let s = screen(&term);
    assert!(!s.contains("cites seqs"), "✕ collapses the panel");
    assert!(s.contains("#agora-wui-work"), "content back to full width");
    shot(&driver, "4-collapsed-again");

    // 5 — open Desk (third block): its badge dot clears on open (the
    // durable-state recipe: the signal lives outside the builder).
    wire_click(&mut term, W - 2, 18);
    settle(&mut driver, &mut app, &mut term);
    let s = screen(&term);
    assert!(s.contains("Nothing on the desk"), "desk body");
    assert!(!s.contains('●'), "opening the desk consumed its badge");
    shot(&driver, "5-desk-badge-consumed");
    driver.finish(&mut term).expect("leave");
    assert_eq!(term.screen().unknown_seq_count(), 0, "all bytes modeled");
}
