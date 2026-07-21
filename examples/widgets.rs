//! widgets — the widget gallery: every shipped widget, live states.
//!
//! Demonstrates: the §3 style guide in motion — Tab moves real keyboard
//! focus (buttons/input/list show the selection-pair focus state), hover
//! works with the mouse, a disabled button sits in text_faint outside the
//! focus order, and the whole gallery restyles on theme cycle.
//!
//! Keys: Tab/Shift+Tab focus · Enter/Space activate · arrows in list/tabs
//! · Ctrl+T cycle theme · F2 spin · q quit (Ctrl+C always).
//!
//! OWNER: DESIGN (gallery); interactive widgets are REACT's, visual ones
//! DESIGN's — this file exercises both through the public API only.

mod common;

use abstracttui::prelude::*;
use abstracttui::theme::themes;
use abstracttui::widgets::{SpinnerKind, TitleAlign, Tone};

const BORDER_KINDS: [(BorderKind, &str); 4] = [
    (BorderKind::Plain, "plain"),
    (BorderKind::Rounded, "rounded"),
    (BorderKind::Double, "double"),
    (BorderKind::Heavy, "heavy"),
];

fn main() -> abstracttui::base::Result<()> {
    if !abstracttui::term::have_tty() {
        println!("widgets: needs an interactive terminal — skipping cleanly");
        return Ok(());
    }
    let mut app = App::new(Size::new(100, 32));
    let quitter = app.quitter();
    app.mount(move |cx| {
        let theme = use_theme(cx);
        // Durable state: created ONCE on the mount scope, bound into the
        // widgets by signal — so list selection, typed text and counters
        // survive the theme-rebuild of the widget tree below.
        let clicks = cx.signal(0u32);
        let name = cx.signal(String::new());
        let pick = cx.signal(0usize);
        let tab = cx.signal(0usize);
        let spin = cx.signal(0u64);
        let theme_ix = cx.signal(0usize);

        Element::new()
            .style(LayoutStyle::column().padding(Edges::all(1)).gap(1))
            .shortcut(KeyChord::plain(Key::Char('q')), move |_| quitter.quit())
            .shortcut(KeyChord::new(Mods::CTRL, Key::Char('t')), move |_| {
                theme_ix.update(|i| *i = (*i + 1) % themes().len());
                set_theme_by_id(themes()[theme_ix.get_untracked()].id);
            })
            .shortcut(KeyChord::plain(Key::F(2)), move |_| {
                spin.update(|t| *t += 1)
            })
            // ONE outer Dyn reads the theme: a switch rebuilds the widget
            // tree with freshly resolved tokens (interactive widgets
            // resolve tokens at element() time — damage contract §5).
            .child(dyn_view(LayoutStyle::default().grow(1.0), move || {
                let t = theme.get().tokens;
                let label = theme.get().label;
                Element::new()
                    .style(LayoutStyle::column().gap(1))
                    // Header: logo + active-theme badge.
                    .child(
                        Element::new()
                            .style(LayoutStyle::row().gap(2).h(2))
                            .child(Logo::new().tagline(true).element(&t).build())
                            .child(Badge::new(label).tone(Tone::Accent).element(&t).build())
                            .build(),
                    )
                    .child(
                        Tabs::new()
                            .tab("interactive", {
                                move || {
                                    interactive_panel(cx, &theme.get().tokens, clicks, name, pick)
                                }
                            })
                            .tab("visual", {
                                move || visual_panel(cx, &theme.get().tokens, spin.get())
                            })
                            .active(tab)
                            .layout(LayoutStyle::column().grow(1.0))
                            .element(cx, &t)
                            .build(),
                    )
                    .child(text(
                        "tab focus · enter/space activate · ctrl+t theme · f2 spin · q quit",
                    ))
                    .build()
            }))
            // Small-terminal guard: absolute overlay, painted LAST; a
            // no-op above the minimum size (draw-time, follows resizes).
            .child(dyn_view(guard_layout(), move || {
                let t = theme.get().tokens;
                Element::new()
                    .style(guard_layout())
                    .draw(move |canvas, rect| {
                        common::too_small(canvas, rect, common::MIN_SIZE, &t);
                    })
                    .build()
            }))
            .build()
    })?;
    app.run()
}

/// Full-viewport absolute layout for the too-small overlay.
fn guard_layout() -> LayoutStyle {
    LayoutStyle::default().absolute(abstracttui::layout::Inset {
        left: Some(0),
        right: Some(0),
        top: Some(0),
        bottom: Some(0),
    })
}

/// REACT's widgets under the §3 state table: focus = selection pair,
/// hover = accent ink, disabled = text_faint outside the focus order.
fn interactive_panel(
    cx: Scope,
    t: &TokenSet,
    clicks: Signal<u32>,
    name: Signal<String>,
    pick: Signal<usize>,
) -> View {
    let items: Vec<String> = [
        "abstract-dark",
        "observer-night",
        "catppuccin-mocha",
        "rose-pine",
        "tokyo-night",
        "nord",
        "one-dark",
        "dracula",
        "monokai",
        "gruvbox",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();

    Element::new()
        .style(LayoutStyle::column().gap(1).padding(Edges::all(1)))
        .child(
            Element::new()
                .style(LayoutStyle::row().gap(2).h(1))
                .child(
                    Button::new("click me")
                        .on_click(move || clicks.update(|c| *c += 1))
                        .element(cx, t)
                        .build(),
                )
                .child(
                    Button::new("disabled")
                        .disabled(true)
                        .element(cx, t)
                        .build(),
                )
                .child(dyn_view(LayoutStyle::default().h(1), move || {
                    text(format!("clicks: {}", clicks.get()))
                }))
                .build(),
        )
        .child(
            TextInput::new()
                .value(name)
                .placeholder("type your name, Enter submits…")
                .layout(LayoutStyle::default().w(44).h(1))
                .element(cx, t)
                .build(),
        )
        .child(dyn_view(LayoutStyle::default().h(1), move || {
            text(format!("hello, {}!", {
                let n = name.get();
                if n.is_empty() {
                    "stranger".to_string()
                } else {
                    n
                }
            }))
        }))
        .child(
            Block::new()
                .title("list — arrows move, wheel scrolls")
                .fill(t.surface)
                .layout(LayoutStyle::column().grow(1.0).min_h(6))
                .child(
                    List::new(items)
                        .selection(pick)
                        .layout(LayoutStyle::default().grow(1.0))
                        .element(cx, t)
                        .build(),
                )
                .element(t)
                .build(),
        )
        .build()
}

/// DESIGN's visual widgets, inside a Scroll so overflow is explorable.
fn visual_panel(cx: Scope, t: &TokenSet, spin_frame: u64) -> View {
    let mut blocks = Element::new().style(LayoutStyle::row().gap(1).h(5));
    for (i, (kind, name)) in BORDER_KINDS.iter().enumerate() {
        blocks = blocks.child(
            Block::new()
                .border(*kind)
                .title(*name)
                .title_align(TitleAlign::Left)
                .focused(i == 1) // the rounded one wears the focus ring
                .fill(t.surface)
                .layout(LayoutStyle::column().grow(1.0))
                .child(text(if i == 1 { "focused" } else { "" }))
                .element(t)
                .build(),
        );
    }

    let mut badges = Element::new().style(LayoutStyle::row().gap(1).h(1));
    for (tone, name) in [
        (Tone::Accent, "accent"),
        (Tone::Ok, "ok"),
        (Tone::Warn, "warn"),
        (Tone::Error, "error"),
        (Tone::Info, "info"),
        (Tone::Muted, "muted"),
    ] {
        badges = badges.child(Badge::new(name).tone(tone).element(t).build());
    }

    let mut spinners = Element::new().style(LayoutStyle::row().gap(3).h(1));
    for (kind, name) in [
        (SpinnerKind::Dots, "dots"),
        (SpinnerKind::Braille, "braille"),
        (SpinnerKind::Line, "line"),
    ] {
        spinners = spinners.child(
            Spinner::new()
                .kind(kind)
                .frame(spin_frame)
                .label(name)
                .element(t)
                .build(),
        );
    }

    let content = Element::new()
        .style(LayoutStyle::column().gap(1))
        .child(blocks.build())
        .child(badges.build())
        .child(Separator::horizontal().label("progress").element(t).build())
        .child(Progress::new(0.35).element(t).build())
        .child(Progress::new(0.72).ramp(true).element(t).build())
        .child(Progress::new(0.93).ramp(true).element(t).build())
        .child(
            Separator::horizontal()
                .label("spinners — f2 advances")
                .element(t)
                .build(),
        )
        .child(spinners.build())
        .build();

    Scroll::new(content)
        .content_size(96, 18)
        .axes(false, true)
        .layout(LayoutStyle::column().grow(1.0))
        .element(cx, t)
        .build()
}
