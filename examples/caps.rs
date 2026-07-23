//! caps — the terminal capability report.
//!
//! Answers "what did the engine detect on THIS terminal?" — the first
//! question when images, Shift+Enter, or clipboard copy behave
//! unexpectedly. The screen renders the LIVE capability set through
//! `use_caps`, so probe upgrades (kitty graphics proof, cell pixel
//! size, tmux passthrough verification) appear moments after launch:
//! watch the "images via" line settle.
//!
//! Reading the image rows:
//! - `kitty graphics` / `iterm2 images` / `sixel` — the first protocol
//!   the ladder proves wins; all `no` = unicode mosaic (works
//!   everywhere, including Terminal.app).
//! - `cell pixel size` — needed for pixel-accurate placement; `-`
//!   means protocols fall back to conservative sizing.
//! - under tmux, graphics stay off unless the passthrough probe proves
//!   `allow-passthrough` is enabled (`graphics wrap: tmux`).
//!
//! Headless (no tty): prints the environment-detected set to stdout
//! and exits 0 — the active probe needs a real terminal to answer.
//!
//! Keys: q quits.
//!
//! OWNER: MEDIA.

use abstracttui::prelude::*;
use abstracttui::term::Capabilities;

/// The report rows: label + value from the live set. One list, used by
/// both the screen and the headless print.
fn rows(c: &Capabilities) -> Vec<(&'static str, String)> {
    let yn = |b: bool| String::from(if b { "yes" } else { "no" });
    let mut out = vec![
        ("truecolor", yn(c.truecolor)),
        ("256 colors", yn(c.colors_256)),
        ("unicode", yn(c.unicode_ok)),
        ("kitty keyboard", yn(c.kitty_keyboard)),
        ("kitty graphics", yn(c.kitty_graphics)),
        ("iterm2 images", yn(c.iterm2_images)),
        ("sixel", yn(c.sixel)),
        (
            "cell pixel size",
            c.cell_pixel_size
                .map(|p| format!("{}x{}", p.w, p.h))
                .unwrap_or_else(|| String::from("-")),
        ),
        (
            "graphics wrap",
            c.graphics_wrap
                .map(|w| format!("{w:?}").to_lowercase())
                .unwrap_or_else(|| String::from("-")),
        ),
        ("sgr mouse", yn(c.sgr_mouse)),
        ("bracketed paste", yn(c.bracketed_paste)),
        ("focus events", yn(c.focus_events)),
        ("hyperlinks", yn(c.hyperlinks)),
        ("osc52 clipboard", yn(c.osc52_copy)),
        ("in tmux", yn(c.in_tmux)),
        (
            "terminal",
            c.term_version.clone().unwrap_or_else(|| String::from("-")),
        ),
    ];
    // The derived line users came for: which channel the image ladder
    // picks on this terminal (preference order = the ladder's).
    let channel = if c.kitty_graphics {
        "kitty graphics protocol"
    } else if c.iterm2_images {
        "iterm2 inline images"
    } else if c.sixel {
        "sixel"
    } else {
        "unicode mosaic (universal fallback)"
    };
    out.push(("images via", String::from(channel)));
    out
}

fn main() -> abstracttui::base::Result<()> {
    if !abstracttui::term::have_tty() {
        // Headless honesty: environment detection only — say so.
        let caps = Capabilities::detect_env();
        println!("caps (environment detection only — no tty, probe skipped):");
        for (label, value) in rows(&caps) {
            println!("  {label:<16} {value}");
        }
        return Ok(());
    }
    if let Ok(id) = std::env::var("ABSTRACTTUI_THEME") {
        set_theme_by_id(&id);
    }

    let mut app = App::new(Size::new(80, 24));
    let quitter = app.quitter();
    app.mount(move |cx| {
        let theme = use_theme(cx);
        let caps = use_caps(cx);
        Element::new()
            .style(LayoutStyle::column().padding(Edges::all(1)))
            .shortcut(KeyChord::plain(Key::Char('q')), move |_| quitter.quit())
            .child(dyn_view(LayoutStyle::default().grow(1.0), move || {
                let t = theme.get().tokens;
                let c = caps.get();
                let mut col = Element::new().style(LayoutStyle::column().padding(Edges::hv(2, 1)));
                col = col.child(text(
                    "terminal capabilities (live — probe upgrades appear here)",
                ));
                col = col.child(text(""));
                for (label, value) in rows(&c) {
                    col = col.child(text(format!("{label:<16} {value}")));
                }
                col = col.child(text(""));
                col = col.child(text("q quits · docs/graphics-and-3d.md explains each row"));
                Block::new()
                    .border(BorderKind::Rounded)
                    .title("caps")
                    .fill(t.surface)
                    .layout(LayoutStyle::default().grow(1.0))
                    .child(col.build())
                    .element(&t)
                    .build()
            }))
            .build()
    })?;
    app.run()
}
