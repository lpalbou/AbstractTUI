//! screenshot — capture what the app actually shows, from a key binding
//! or from a headless test.
//!
//! Interactive (in a terminal): a themed scene with `s` bound to
//! [`abstracttui::app::request_screenshot`] — each press writes three
//! artifacts of the LAST PRESENTED frame under the system temp dir:
//!
//!   screenshot-demo.txt    plain text (`Screenshot::to_text`)
//!   screenshot-demo.ansi   SGR-styled, replay with `cat` (`to_ansi`)
//!   screenshot-demo.svg    GitHub-renderable vector still (`to_svg`)
//!
//! There is deliberately no engine-default hotkey — the binding below IS
//! the recipe (docs/api.md "Screenshots & captures").
//!
//! Headless (no tty — CI, agents): the same scene drives through
//! `Driver` + `testing::CaptureTerm`, captures from BOTH truth surfaces
//! (the composed frame and the VT-modeled bytes), writes the same
//! artifacts, and exits 0 — the test-artifact recipe in miniature.
//!
//! OWNER: DESIGN.

use abstracttui::app::{request_screenshot, App, Driver, RunConfig};
use abstracttui::prelude::*;
use abstracttui::render::Screenshot;
use abstracttui::testing::CaptureTerm;
use abstracttui::ui::text;

fn scene(cx: Scope, quitter: Quitter, note: Signal<String>) -> View {
    let theme = use_theme(cx);
    Element::new()
        .style(LayoutStyle::column().padding(Edges::all(1)))
        .shortcut(KeyChord::plain(Key::Char('q')), move |_| quitter.quit())
        .shortcut(KeyChord::plain(Key::Char('s')), move |_| {
            // The recipe: bind a key, request, export in the callback.
            // The callback receives the frame as last presented — the
            // screen exactly as it looked when `s` landed.
            request_screenshot(move |shot| {
                let stem = std::env::temp_dir().join("screenshot-demo");
                let mut wrote = Vec::new();
                for (ext, result) in [
                    ("txt", shot.write_text(stem.with_extension("txt"))),
                    ("ansi", shot.write_ansi(stem.with_extension("ansi"))),
                    ("svg", shot.write_svg(stem.with_extension("svg"))),
                ] {
                    match result {
                        Ok(()) => wrote.push(ext),
                        Err(e) => {
                            note.set(format!("write failed ({ext}): {e}"));
                            return;
                        }
                    }
                }
                note.set(format!("wrote {}.{{{}}}", stem.display(), wrote.join(",")));
            });
        })
        .child(dyn_view(LayoutStyle::default().grow(1.0), move || {
            let t = theme.get().tokens;
            Block::new()
                .border(BorderKind::Rounded)
                .title("screenshot")
                .fill(t.surface)
                .layout(LayoutStyle::column().gap(1).padding(Edges::all(1)))
                .child(text("A themed scene worth capturing: 世界 🚀"))
                .child(dyn_view(LayoutStyle::line(1), move || text(note.get())))
                .child(text("s · capture      q · quit"))
                .element(&t)
                .build()
        }))
        .build()
}

fn main() -> abstracttui::base::Result<()> {
    if !abstracttui::term::have_tty() {
        return headless_demo();
    }
    let mut app = App::new(Size::new(72, 12));
    let quitter = app.quitter();
    app.mount(move |cx| {
        let note = cx.signal(String::from("press s to capture"));
        scene(cx, quitter, note)
    })?;
    app.run()
}

/// No tty: the exact shape a headless test uses to produce artifacts —
/// drive the real pipeline against a captured terminal, snapshot both
/// truth surfaces, export.
fn headless_demo() -> abstracttui::base::Result<()> {
    let size = Size::new(72, 12);
    let mut app = App::new(size);
    let quitter = app.quitter();
    app.mount(move |cx| {
        let note = cx.signal(String::from("headless capture"));
        scene(cx, quitter, note)
    })?;
    let mut term = CaptureTerm::new(size);
    // Fixed capabilities: a test artifact must not vary with the
    // runner's TERM/COLORTERM (the capture-pipeline discipline).
    let caps = abstracttui::term::Capabilities::with(|c| {
        c.truecolor = true;
        c.colors_256 = true;
    });
    let cfg = RunConfig {
        caps: Some(caps),
        enter: None,
        probe: false,
    };
    let mut driver = Driver::new(&mut app, &mut term, cfg)?;
    for _ in 0..16 {
        if driver.turn(&mut app, &mut term)?.idle {
            break;
        }
    }

    // Two capture surfaces, one truth: the composed frame (driver) and
    // the emitted bytes replayed through the VT model (term.screen()).
    let from_frame: Screenshot = driver.screenshot();
    let from_bytes: Screenshot = term.screen().screenshot();
    assert_eq!(
        from_frame.to_text(),
        from_bytes.to_text(),
        "both capture surfaces must agree"
    );

    let stem = std::env::temp_dir().join("screenshot-demo");
    from_frame.write_text(stem.with_extension("txt"))?;
    from_frame.write_ansi(stem.with_extension("ansi"))?;
    from_frame.write_svg(stem.with_extension("svg"))?;
    println!(
        "screenshot: headless capture ok — wrote {}.{{txt,ansi,svg}} ({}x{})",
        stem.display(),
        from_frame.width(),
        from_frame.height()
    );
    Ok(())
}
