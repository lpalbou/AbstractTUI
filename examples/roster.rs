//! roster — keyboard selection over rows the engine does NOT render.
//!
//! Demonstrates `RowSelect`: a `Scroll` of arbitrary two-line
//! `Element`s that keeps everything `List` gives you — arrows, Home/End,
//! Page keys, click-to-select, Enter to activate, ensure-visible, and
//! sticky selection by key.
//!
//! **Why this exists.** `List` renders its own rows and they are one
//! line each; a member row that is a name above a mission has to be your
//! own tree. Before this widget that trade cost the keyboard, which in a
//! terminal roster is not a nicety — it is how the surface is used.
//!
//! Three things worth watching:
//!
//! - **The second line travels with the selection.** Arrow to the
//!   bottom: ensure-visible scrolls by CONTENT ROWS, so a two-row member
//!   comes into view whole rather than half.
//! - **Press `m`.** Two members arrive at the TOP of the list. The
//!   selection does not stay on index N — it follows the KEY, so the
//!   same person stays selected while their index moves under them.
//!   That is the half an index-based reimplementation loses silently.
//! - **Press `x`.** The selected member is removed. Their key is gone,
//!   so the SLOT is held, clamped — the next member down is selected,
//!   never a phantom row.
//!
//! Keys: ↑↓ PgUp/PgDn Home/End move · Enter activates · click selects ·
//! m adds two at the top · x removes the selected · r resets · q quits.
//!
//! Docs: docs/api.md (widgets::row_select — RowSelect).
//!
//! OWNER: REACT.

mod common;

use abstracttui::prelude::*;
use abstracttui::widgets::{Block, BorderKind, RowSelect, Scroll};

/// A member: the two lines a `List` cannot draw.
#[derive(Clone)]
struct Member {
    id: &'static str,
    name: &'static str,
    mission: &'static str,
}

const SEATS: [Member; 6] = [
    Member {
        id: "tui",
        name: "tui",
        mission: "owns abstracttui — rendering, layout, reactivity",
    },
    Member {
        id: "agora-tui",
        name: "agora-tui",
        mission: "the terminal client for the hub",
    },
    Member {
        id: "agora-wui",
        name: "agora-wui",
        mission: "the web console",
    },
    Member {
        id: "agora",
        name: "agora",
        mission: "the hub itself — protocol, store, ledger",
    },
    Member {
        id: "delegate",
        name: "delegate",
        mission: "carries operator requests end to end",
    },
    Member {
        id: "scribe",
        name: "scribe",
        mission: "records decisions so they survive the thread",
    },
];

const ARRIVALS: [Member; 2] = [
    Member {
        id: "newcomer-a",
        name: "newcomer-a",
        mission: "arrived at the TOP — watch the selection not move",
    },
    Member {
        id: "newcomer-b",
        name: "newcomer-b",
        mission: "arrived at the TOP — the key followed, the index did not",
    },
];

/// Rows per member. Declared here because `RowSelect` keys ensure-visible
/// and hit-testing off it, and the row tree below has to agree — one
/// number, used twice, deliberately not two.
const ROW_H: i32 = 2;

/// The rows themselves: yours to draw, which is the whole point.
fn member_rows(seats: Vec<Member>, sel: Signal<usize>) -> View {
    // Width fills, HEIGHT IS INTRINSIC. `fill()` here would squeeze the
    // whole roster into the viewport and there would be nothing to
    // scroll — the trap `Style::fill`'s own doc comment names.
    dyn_view(
        LayoutStyle::column().width(Dimension::Percent(1.0)),
        move || {
            let selected = sel.get();
            let mut col =
                Element::new().style(LayoutStyle::column().width(Dimension::Percent(1.0)));
            for (i, m) in seats.iter().enumerate() {
                let (name, mission) = (m.name.to_string(), m.mission.to_string());
                let is_sel = i == selected;
                col = col.child(
                    Element::new()
                        .style(
                            LayoutStyle::column()
                                .width(Dimension::Percent(1.0))
                                .height(Dimension::Cells(ROW_H)),
                        )
                        .draw(move |canvas, rect| {
                            let t = current_theme().tokens;
                            // The selection pair is the audited one; an
                            // unselected row keeps the panel's ground.
                            let (fg, bg) = if is_sel {
                                (t.selection_fg, t.selection_bg)
                            } else {
                                (t.text, t.surface)
                            };
                            canvas.fill(rect, ' ', fg, bg);
                            canvas.print(
                                Point::new(rect.x + 1, rect.y),
                                &format!("{} {name}", if is_sel { "▸" } else { " " }),
                                fg,
                                bg,
                            );
                            if rect.h > 1 {
                                canvas.print(
                                    Point::new(rect.x + 3, rect.y + 1),
                                    &mission,
                                    if is_sel { fg } else { t.text_faint },
                                    bg,
                                );
                            }
                        })
                        .build(),
                );
            }
            col.build()
        },
    )
}

fn main() -> abstracttui::base::Result<()> {
    if !abstracttui::term::have_tty() {
        println!("roster: needs an interactive terminal — skipping cleanly");
        return Ok(());
    }
    if let Ok(id) = std::env::var("ABSTRACTTUI_THEME") {
        set_theme_by_id(&id);
    }

    let mut app = App::new(Size::new(72, 16));
    let quitter = app.quitter();
    app.mount(move |cx| {
        // Durable state, OUTSIDE the Dyn that rebuilds the roster — a
        // signal created inside would die with its generation and take
        // the selection with it.
        let seats = cx.signal(SEATS.to_vec());
        let sel = cx.signal(0usize);
        // The sticky half: the selected member's ID, not their index.
        let sel_key = cx.signal(String::from(SEATS[0].id));
        let offset = cx.signal(0i32);
        let activated = cx.signal(String::new());

        let add = move || {
            seats.update(|s| {
                let mut next: Vec<Member> = ARRIVALS.to_vec();
                next.extend(s.iter().cloned());
                *s = next;
            });
        };
        let remove = move || {
            let gone = sel_key.get_untracked();
            seats.update(|s| s.retain(|m| m.id != gone));
        };
        let reset = move || {
            seats.set(SEATS.to_vec());
            sel_key.set(String::from(SEATS[0].id));
        };

        let t = current_theme().tokens;
        Element::new()
            .style(
                LayoutStyle::column()
                    .width(Dimension::Percent(1.0))
                    .height(Dimension::Percent(1.0)),
            )
            .shortcut(KeyChord::plain(Key::Char('q')), move |_| quitter.quit())
            .shortcut(KeyChord::plain(Key::Char('m')), move |_| add())
            .shortcut(KeyChord::plain(Key::Char('x')), move |_| remove())
            .shortcut(KeyChord::plain(Key::Char('r')), move |_| reset())
            .child(
                Block::new()
                    .title("roster — two-line rows, and the keyboard that usually costs")
                    .border(BorderKind::Rounded)
                    .fill(t.surface)
                    .layout(
                        LayoutStyle::column()
                            .width(Dimension::Percent(1.0))
                            .grow(1.0),
                    )
                    .child(dyn_view_scoped(
                        LayoutStyle::column()
                            .width(Dimension::Percent(1.0))
                            .height(Dimension::Percent(1.0)),
                        move |cx| {
                            // Rebuilt when the DATA changes; the arrow
                            // keys move selection without coming through
                            // here at all.
                            let list = seats.get();
                            let rows = member_rows(list.clone(), sel);
                            let scroll = Scroll::new(rows)
                                .offset_y(offset)
                                .scrollbar_auto_hide(true)
                                .view(cx);
                            let names: Vec<String> =
                                list.iter().map(|m| m.name.to_string()).collect();
                            RowSelect::new(list.iter().map(|m| m.id))
                                .row_heights(|_| ROW_H)
                                .selection(sel)
                                .selection_key(sel_key)
                                .offset_y(offset)
                                .on_activate(move |i| {
                                    activated.set(match names.get(i) {
                                        Some(n) => format!("activated: {n}"),
                                        None => String::new(),
                                    });
                                })
                                .wrap(cx, scroll)
                                .build()
                        },
                    ))
                    .element(&t)
                    .build(),
            )
            // The status line reads four signals, so it is a `dyn_view`
            // and not a `draw` that reads them: a tracked read inside a
            // draw closure paints once and never repaints (RT1-2, and
            // the engine panics on it — this example met that guard).
            .child(dyn_view(LayoutStyle::line(1).shrink(0.0), move || {
                // The index AND the key, side by side: pressing `m`
                // moves one and not the other, which is the whole
                // demonstration.
                let status = format!(
                    " index {} · key {:?} · {} members   {}",
                    sel.get(),
                    sel_key.get(),
                    seats.get().len(),
                    activated.get()
                );
                Element::new()
                    .style(LayoutStyle::line(1).shrink(0.0))
                    .draw(move |canvas, rect| {
                        let t = current_theme().tokens;
                        canvas.fill(rect, ' ', t.text_faint, t.bg);
                        canvas.print(Point::new(rect.x, rect.y), &status, t.accent, t.bg);
                    })
                    .build()
            }))
            .child(
                Element::new()
                    .style(LayoutStyle::line(1).shrink(0.0))
                    .draw(|canvas, rect| {
                        let t = current_theme().tokens;
                        canvas.fill(rect, ' ', t.text_faint, t.bg);
                        canvas.print(
                            Point::new(rect.x + 1, rect.y),
                            "↑↓ PgUp/PgDn Home/End move · Enter activates · m adds two at the \
                             top · x removes · r resets · q quit",
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
