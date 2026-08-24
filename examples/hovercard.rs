//! hovercard — tooltips and rich hover cards, reachable by mouse AND
//! by keyboard.
//!
//! Demonstrates: `Tooltip::attach` (one line of text, no tree at all)
//! and `Tooltip::attach_content` with `TipContent::card` (a whole
//! widget subtree — the cited-message preview shape), on a feed of
//! citation chips like the ones a chat client emits.
//!
//! **Try it with the mouse**, then put the mouse down and press Tab.
//! Both triggers are the same code path: hover an anchor, or give it
//! FOCUS, and the tip opens after the same delay.
//!
//! Four things worth watching for, because each one is a defect this
//! widget had:
//!
//! - **Keyboard reach.** Tab walks the chips and the card follows
//!   focus. It only works because the tooltip's trigger sits on the
//!   node that TAKES focus: focus transitions are delivered
//!   target-only, so a tip attached around a focusable — rather than
//!   TO it — would open for the mouse and stay silent for Tab. The
//!   last row demonstrates exactly that, on purpose.
//! - **Escape** dismisses an open card, and only then — an Escape with
//!   no card up belongs to whatever is behind it.
//! - **Truncation.** The `#918` chip's card asks for sixty rows, which
//!   no terminal will lend. A browser can clip a preview honestly
//!   because the scrollbar says there is more; a terminal has no such
//!   affordance, so the card draws `… N more` rather than showing you
//!   part of it and saying nothing.
//! - **Scrolling.** Press `s` to shift the feed with the pointer
//!   parked on a chip. The card closes rather than hanging over the
//!   row that moved away — there is no mouse motion to notice that
//!   with, so the anchor's own repaint is what reports it.
//!
//! Keys: Tab/Shift+Tab move · Esc dismisses · s shifts the feed · q quits.
//!
//! Docs: docs/api.md (app::anchored — Tooltip, TipContent).
//!
//! OWNER: DESIGN.

mod common;

use abstracttui::prelude::*;
use abstracttui::ui::Role;
use abstracttui::widgets::{Block, BorderKind};
use std::time::Duration;

/// One cited message, as a chat client would have it.
struct Cited {
    chip: &'static str,
    sender: &'static str,
    title: &'static str,
    body: &'static [&'static str],
}

const FEED: [(&str, Cited); 2] = [
    (
        "agora-tui  the seam you flagged is real, see",
        Cited {
            chip: "#412",
            sender: "tui",
            title: "place_panel prefers below, and says so",
            body: &[
                "The card opens under its anchor with no gap, so",
                "the pointer never crosses anything to reach it.",
                "That was arithmetic before it was a decision.",
            ],
        },
    ),
    (
        "agora-wui  and the transit guard I mentioned in",
        Cited {
            chip: "#8",
            sender: "agora-wui",
            title: "the guard worked by pointer speed",
            body: &[
                "relatedTarget is the card only when one pointer",
                "sample spans the whole 8px trip. A fast hand.",
            ],
        },
    ),
];

/// How many rows the truncation demo asks for.
///
/// Deliberately taller than any terminal you are likely to be running,
/// because the point only lands if the card CANNOT fit: sized to the
/// window instead, it would fit on a tall screen and the marker — the
/// thing this row exists to show — would never appear. The first
/// version of this example did exactly that and looked fine.
const TALL_ROWS: i32 = 60;

/// The truncation demo: a card no viewport can lend room for.
fn tall_card() -> TipContent {
    TipContent::card(Size::new(52, TALL_ROWS), |_cx| {
        Element::new()
            .style(LayoutStyle::column().width(Dimension::Percent(1.0)))
            .draw(|canvas, rect| {
                let t = current_theme().tokens;
                canvas.fill(rect, ' ', t.text, t.surface_raised);
                canvas.print(
                    Point::new(rect.x, rect.y),
                    &format!(" delegate · a card asking for {TALL_ROWS} rows"),
                    t.accent,
                    t.surface_raised,
                );
                for i in 0..(TALL_ROWS - 2) {
                    let y = rect.y + 2 + i;
                    if y >= rect.bottom() {
                        break;
                    }
                    canvas.print(
                        Point::new(rect.x + 1, y),
                        &format!("line {:>2} of {}", i + 1, TALL_ROWS - 2),
                        t.text,
                        t.surface_raised,
                    );
                }
            })
            .build()
    })
}

/// The card a citation chip previews: sender, title, body, on its own
/// ground.
///
/// The root element FILLS before its children paint — an overlay tree
/// composites over the feed, so a card without its own ground would
/// show the message text through it.
fn card_for(c: &Cited) -> TipContent {
    let sender = c.sender;
    let title = c.title;
    let body: Vec<String> = c.body.iter().map(|s| s.to_string()).collect();
    let rows = body.len() as i32 + 3;
    TipContent::card(Size::new(52, rows), move |_cx| {
        let head = format!(" {sender} · {title}");
        let lines = body.clone();
        Element::new()
            .style(LayoutStyle::column().width(Dimension::Percent(1.0)))
            .draw(move |canvas, rect| {
                let t = current_theme().tokens;
                canvas.fill(rect, ' ', t.text, t.surface_raised);
                canvas.print(
                    Point::new(rect.x, rect.y),
                    &head,
                    t.accent,
                    t.surface_raised,
                );
                for (i, line) in lines.iter().enumerate() {
                    let y = rect.y + 2 + i as i32;
                    if y >= rect.bottom() {
                        break;
                    }
                    canvas.print(Point::new(rect.x + 1, y), line, t.text, t.surface_raised);
                }
            })
            .build()
    })
}

/// The four things worth watching, on screen rather than only in the
/// module doc — a reader running the example is not reading the source.
const NOTES: [&str; 8] = [
    "KEYBOARD    Tab walks the chips and the card follows focus.",
    "            The last row shows the limit: attached AROUND a",
    "            focusable, Tab reaches it and no card opens.",
    "ESCAPE      dismisses an open card, and only then.",
    "TRUNCATION  #918 asks for 60 rows. No terminal lends that, and",
    "            a terminal has no scrollbar to admit it — so the",
    "            card prints how much it hid.",
    "SCROLLING   press s with the pointer parked on a chip.",
];

/// Width of the prose column, so every chip starts on the same screen
/// column. Ragged chips were the old shape: each row was
/// `text(prose) + chip` with a one-cell gap, so the anchors marched
/// across the screen and a reader hunting for the next one had to look
/// for it.
const PROSE_W: i32 = 52;

/// One feed row: prose in a fixed column, the tip anchor after it.
fn feed_row(prose: &'static str, anchor: View) -> View {
    Element::new()
        .style(LayoutStyle::row().height(Dimension::Cells(1)).gap(2))
        .child(
            Element::new()
                .style(
                    LayoutStyle::default()
                        .width(Dimension::Cells(PROSE_W))
                        .height(Dimension::Cells(1))
                        .shrink(0.0),
                )
                .child(text(prose))
                .build(),
        )
        .child(anchor)
        .build()
}

/// A focusable citation chip. The tooltip attaches to THIS element, so
/// the same node that Tab lands on is the one carrying the trigger.
fn chip(cx: Scope, label: &'static str) -> Element {
    let hovered = cx.signal(false);
    let focused = cx.signal(false);
    let width = abstracttui::text::width(label) + 2;
    Element::new()
        .style(
            LayoutStyle::default()
                .width(Dimension::Cells(width))
                .height(Dimension::Cells(1))
                .shrink(0.0),
        )
        .role(Role::Button)
        .access_label(label)
        .focusable()
        .hover_signal(hovered)
        .focus_signal(focused)
        .child(dyn_view(
            LayoutStyle::default().width(Dimension::Percent(1.0)),
            move || {
                let lit = focused.get() || hovered.get();
                Element::new()
                    .style(LayoutStyle::default().width(Dimension::Percent(1.0)))
                    .draw(move |canvas, rect| {
                        let t = current_theme().tokens;
                        let (fg, bg) = if lit {
                            (t.bg, t.accent)
                        } else {
                            (t.accent, t.surface_raised)
                        };
                        canvas.fill(rect, ' ', fg, bg);
                        canvas.print(Point::new(rect.x + 1, rect.y), label, fg, bg);
                    })
                    .build()
            },
        ))
}

fn main() -> abstracttui::base::Result<()> {
    if !abstracttui::term::have_tty() {
        println!("hovercard: needs an interactive terminal — skipping cleanly");
        return Ok(());
    }
    if let Ok(id) = std::env::var("ABSTRACTTUI_THEME") {
        set_theme_by_id(&id);
    }

    let mut app = App::new(Size::new(96, 30));
    let overlays = app.overlays();
    let quitter = app.quitter();
    app.mount(move |cx| {
        // Scroll under a stationary pointer: the case a mouse report
        // can never tell you about.
        let shift = cx.signal(0i32);
        let delay = Duration::from_millis(250);

        // The cited-message rows, in their own panel so the feed reads
        // as a feed and not as loose lines on the background.
        let mut feed =
            Element::new().style(LayoutStyle::column().width(Dimension::Percent(1.0)).gap(1));
        for (prose, cited) in FEED.iter() {
            feed = feed.child(feed_row(
                prose,
                Tooltip::attach_content(
                    cx,
                    &overlays,
                    card_for(cited),
                    delay,
                    chip(cx, cited.chip).build(),
                ),
            ));
        }
        // The card no viewport can fit: it must SAY how much it hid.
        feed = feed.child(feed_row(
            "delegate   taller than any terminal, so it is cut:",
            Tooltip::attach_content(cx, &overlays, tall_card(), delay, chip(cx, "#918").build()),
        ));

        // The zero-tree path: one line of plain text, no widget subtree
        // mounted at all.
        let plain = feed_row(
            "tui        a plain one-line tip, no tree:",
            Tooltip::attach(
                cx,
                &overlays,
                "TipContent::Label — a layer_draw, no tree, no scope",
                delay,
                chip(cx, "label").build(),
            ),
        );

        // The documented LIMIT, shown rather than described: the tip is
        // attached around a container, and the focusable chip is INSIDE
        // it. Hover works. Tab reaches the chip and no card opens,
        // because focus is delivered target-only and the listener is on
        // the wrapper.
        let wrapped = feed_row(
            "tui        attached AROUND the chip, not to it:",
            Tooltip::attach(
                cx,
                &overlays,
                "mouse only — the trigger is on the wrapper, not the focusable",
                delay,
                Element::new()
                    .style(LayoutStyle::row().height(Dimension::Cells(1)))
                    .child(chip(cx, "wrapped").build())
                    .build(),
            ),
        );

        let t = current_theme().tokens;
        Element::new()
            // COLUMN, explicitly. `LayoutStyle::fill()` sizes both axes
            // and leaves the direction at its default ROW, so this
            // example used to lay its header, feed and footer out SIDE
            // BY SIDE — with narrow content most of them collapsed to
            // zero width and the screen came up almost empty.
            .style(
                LayoutStyle::column()
                    .width(Dimension::Percent(1.0))
                    .height(Dimension::Percent(1.0)),
            )
            .shortcut(KeyChord::plain(Key::Char('q')), move |_| quitter.quit())
            .shortcut(KeyChord::plain(Key::Char('s')), move |_| {
                shift.update(|s| *s = if *s == 0 { 4 } else { 0 })
            })
            .child(
                Element::new()
                    .style(LayoutStyle::line(1).shrink(0.0))
                    .draw(|canvas, rect| {
                        let t = current_theme().tokens;
                        canvas.fill(rect, ' ', t.text, t.surface_raised);
                        canvas.print(
                            Point::new(rect.x + 2, rect.y),
                            "hovercard — point at a chip and wait, or press Tab",
                            t.accent,
                            t.surface_raised,
                        );
                    })
                    .build(),
            )
            // Grows and shrinks above the feed: pressing `s` moves every
            // anchor without producing one byte of mouse input.
            .child(
                Element::new()
                    .style_signal(move || {
                        LayoutStyle::default()
                            .width(Dimension::Cells(1))
                            .height(Dimension::Cells(shift.get()))
                            .shrink(0.0)
                    })
                    .build(),
            )
            .child(
                Block::new()
                    .title("cited messages — each chip previews the message it cites")
                    .border(BorderKind::Rounded)
                    .fill(t.surface)
                    .layout(
                        LayoutStyle::column()
                            .width(Dimension::Percent(1.0))
                            .h(7)
                            .shrink(0.0),
                    )
                    .child(feed.build())
                    .element(&t)
                    .build(),
            )
            .child(
                Block::new()
                    .title("the other two tip shapes")
                    .border(BorderKind::Rounded)
                    .fill(t.surface)
                    .layout(
                        LayoutStyle::column()
                            .width(Dimension::Percent(1.0))
                            .h(5)
                            .gap(1)
                            .shrink(0.0),
                    )
                    .child(plain)
                    .child(wrapped)
                    .element(&t)
                    .build(),
            )
            // Takes the slack — and earns it. A card opens BELOW its
            // anchor, so this panel is also the ground those cards
            // composite over: an overlay drawn on empty background
            // proves nothing about whether it has its own.
            .child(
                Block::new()
                    .title("what to watch, because each one was a defect this widget had")
                    .border(BorderKind::Rounded)
                    .fill(t.surface)
                    .layout(
                        LayoutStyle::column()
                            .width(Dimension::Percent(1.0))
                            .grow(1.0),
                    )
                    .child(
                        Element::new()
                            .style(
                                LayoutStyle::column()
                                    .width(Dimension::Percent(1.0))
                                    .height(Dimension::Percent(1.0)),
                            )
                            .draw(|canvas, rect| {
                                let t = current_theme().tokens;
                                canvas.fill(rect, ' ', t.text, t.surface);
                                for (i, line) in NOTES.iter().enumerate() {
                                    let y = rect.y + i as i32;
                                    if y >= rect.bottom() {
                                        break;
                                    }
                                    let ink = if line.starts_with(' ') {
                                        t.text_faint
                                    } else {
                                        t.text
                                    };
                                    canvas.print(Point::new(rect.x, y), line, ink, t.surface);
                                }
                            })
                            .build(),
                    )
                    .element(&t)
                    .build(),
            )
            .child(
                Element::new()
                    .style(LayoutStyle::line(1).shrink(0.0))
                    .draw(|canvas, rect| {
                        let t = current_theme().tokens;
                        canvas.fill(rect, ' ', t.text_faint, t.bg);
                        canvas.print(
                            Point::new(rect.x + 2, rect.y),
                            "Tab/Shift+Tab move · Esc dismisses · s shifts the feed · q quit",
                            t.text_faint,
                            t.bg,
                        );
                    })
                    .build(),
            )
            .build()
    })?;
    app.run()
}
