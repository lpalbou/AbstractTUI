//! modal_backdrop — a modal you can actually see over a busy page.
//!
//! Demonstrates: `Modal`'s backdrop. The panel is solid (`surface_raised`)
//! and the page behind it is dimmed by the theme's `overlay` scrim: the
//! page's text stays in place, veiled, so the panel reads at a glance. An
//! opaque plate under the panel means nothing below shows through it, even
//! where the panel paints a translucent color. Stacked modals each dim
//! what is under them; `ChoicePrompt` rides the same backdrop.
//!
//! Keys (page): `m` workspace picker · `c` choice prompt · `s` scrim on/off
//! for the next modal · `t` next theme · `q` quit.
//! Keys (picker): `↑`/`↓` move · `Enter` choose · `n` open a second modal on
//! top · `Esc` close. A click outside a modal closes it.
//!
//! Try: `cargo run --example modal_backdrop`
//!      `ABSTRACTTUI_THEME=catppuccin-latte cargo run --example modal_backdrop`
//!
//! OWNER: DESIGN.

mod common;

use std::cell::RefCell;
use std::rc::Rc;

use abstracttui::app::Overlays;
use abstracttui::prelude::*;

const OPTIONS: [&str; 2] = [
    "Deny everything, allow listed workspaces — Agents may only work in the listed workspaces.",
    "Allow everything, refuse listed workspaces — Agents may work in any workspace except the refused ones.",
];

type Slot = Rc<RefCell<Option<Modal>>>;

fn main() -> abstracttui::base::Result<()> {
    if !abstracttui::term::have_tty() {
        println!("modal_backdrop: needs an interactive terminal — skipping cleanly");
        return Ok(());
    }
    if let Ok(id) = std::env::var("ABSTRACTTUI_THEME") {
        set_theme_by_id(&id);
    }

    let mut app = App::new(Size::new(110, 32));
    let overlays = app.overlays();
    let quit = app.quitter();
    app.mount(move |cx| {
        let vp = use_viewport(cx);
        let theme = use_theme(cx);
        let scrim = cx.signal(true);
        let status = cx.signal(String::from("press m to open the picker"));

        let picker = {
            let overlays = overlays.clone();
            move |_: &mut abstracttui::ui::EventCtx| {
                open_picker(
                    &overlays,
                    cx,
                    vp.get_untracked(),
                    scrim.get_untracked(),
                    status,
                );
            }
        };
        let choice = move |_: &mut abstracttui::ui::EventCtx| {
            ChoicePrompt::new("Run `cargo build` in ~/playground2?")
                .option("once", "Allow once")
                .option("always", "Always allow for this session")
                .option("deny", "Deny")
                .on_resolve(move |outcome| status.set(format!("choice prompt: {outcome:?}")))
                .open(cx);
        };
        let next_theme = move |_: &mut abstracttui::ui::EventCtx| {
            let all = abstracttui::theme::themes();
            let now = theme.get_untracked().id;
            let i = all
                .iter()
                .position(|t| t.id == now)
                .map_or(0, |i| (i + 1) % all.len());
            set_theme_by_id(all[i].id);
            status.set(format!("theme: {}", all[i].id));
        };

        Element::new()
            .style(
                LayoutStyle::column()
                    .width(Dimension::Percent(1.0))
                    .height(Dimension::Percent(1.0)),
            )
            .shortcut(KeyChord::plain(Key::Char('q')), move |_| quit.quit())
            .shortcut(KeyChord::plain(Key::Char('m')), picker)
            .shortcut(KeyChord::plain(Key::Char('c')), choice)
            .shortcut(KeyChord::plain(Key::Char('s')), move |_| {
                scrim.update(|on| *on = !*on)
            })
            .shortcut(KeyChord::plain(Key::Char('t')), next_theme)
            .child(dyn_view(
                LayoutStyle::default()
                    .grow(1.0)
                    .width(Dimension::Percent(1.0)),
                move || {
                    let t = theme.get().tokens;
                    Element::new()
                        .style(LayoutStyle::fill())
                        .draw(move |canvas, rect| common::paint_busy_page(canvas, rect, &t))
                        .build()
                },
            ))
            .child(dyn_view(
                LayoutStyle::line(1).width(Dimension::Percent(1.0)),
                move || {
                    let t = theme.get().tokens;
                    let scrim_label = if scrim.get() {
                        "scrim: ON"
                    } else {
                        "scrim: off"
                    };
                    let line = status.get();
                    Element::new()
                        .style(LayoutStyle::fill())
                        .draw(move |canvas, rect| {
                            canvas.fill(rect, ' ', t.text, t.surface);
                            common::key_legend(
                                canvas,
                                rect,
                                &t,
                                &[
                                    ("m", "picker"),
                                    ("c", "choice prompt"),
                                    ("s", scrim_label),
                                    ("t", "theme"),
                                    ("q", "quit"),
                                ],
                            );
                            let x = rect.right() - abstracttui::text::width(&line) - 1;
                            canvas.print(
                                Point::new(x.max(rect.x), rect.y),
                                &line,
                                t.text,
                                Rgba::TRANSPARENT,
                            );
                        })
                        .build()
                },
            ))
            .build()
    })?;
    app.run()
}

/// The workspace picker from the AbstractCode screenshot: a title row, two
/// long options, a selection band.
fn open_picker(overlays: &Overlays, cx: Scope, vp: Size, scrim: bool, status: Signal<String>) {
    let slot: Slot = Default::default();
    let (close_enter, close_esc) = (slot.clone(), slot.clone());
    let nested_overlays = overlays.clone();
    let width = (vp.w - 8).clamp(40, 112);
    let modal = Modal::open(overlays, cx, vp, Size::new(width, 9), move |mcx| {
        let sel = mcx.signal(0usize);
        Element::new()
            .style(LayoutStyle::fill())
            .shortcut(KeyChord::plain(Key::Up), move |_| sel.update(|s| *s = s.saturating_sub(1)))
            .shortcut(KeyChord::plain(Key::Down), move |_| {
                sel.update(|s| *s = (*s + 1).min(OPTIONS.len() - 1))
            })
            .shortcut(KeyChord::plain(Key::Enter), move |_| {
                status.set(format!("chose: {}", OPTIONS[sel.get_untracked()]));
                if let Some(m) = close_enter.borrow_mut().take() {
                    m.close();
                }
            })
            .shortcut(KeyChord::plain(Key::Escape), move |_| {
                if let Some(m) = close_esc.borrow_mut().take() {
                    m.close();
                }
            })
            .shortcut(KeyChord::plain(Key::Char('n')), move |_| {
                open_info(&nested_overlays, mcx, vp, scrim);
            })
            .child(dyn_view(LayoutStyle::fill(), move || {
                let s = sel.get();
                let t = current_theme().tokens;
                Element::new()
                    .style(LayoutStyle::fill())
                    .draw(move |canvas, rect| {
                        canvas.print(
                            Point::new(rect.x, rect.y),
                            "Workspaces agents may use — ↑↓ · Enter chooses · n stacks a modal · Esc back",
                            t.error,
                            Rgba::TRANSPARENT,
                        );
                        for (i, option) in OPTIONS.iter().enumerate() {
                            let y = rect.y + 2 + i as i32;
                            if y >= rect.bottom() {
                                break;
                            }
                            let row = Rect::new(rect.x, y, rect.w, 1);
                            let label = abstracttui::text::truncate_ellipsis(option, rect.w);
                            if i == s {
                                canvas.fill(row, ' ', t.selection_fg, t.selection_bg);
                                canvas.print(Point::new(rect.x, y), &label, t.selection_fg, t.selection_bg);
                            } else {
                                canvas.print(Point::new(rect.x, y), &label, t.text, Rgba::TRANSPARENT);
                            }
                        }
                        let hint = "the page behind is dimmed and veiled; nothing shows through this panel";
                        canvas.print(
                            Point::new(rect.x, rect.bottom() - 1),
                            &abstracttui::text::truncate_ellipsis(hint, rect.w),
                            t.text_muted,
                            Rgba::TRANSPARENT,
                        );
                    })
                    .build()
            }))
            .build()
    });
    modal.set_scrim(scrim);
    *slot.borrow_mut() = Some(modal.share());
}

/// A second modal stacked on the picker: the picker dims under it.
fn open_info(overlays: &Overlays, cx: Scope, vp: Size, scrim: bool) {
    let slot: Slot = Default::default();
    let close = slot.clone();
    let modal = Modal::open(overlays, cx, vp, Size::new(52, 7), move |_| {
        let t = current_theme().tokens;
        Element::new()
            .style(LayoutStyle::fill())
            .shortcut(KeyChord::plain(Key::Escape), move |_| {
                if let Some(m) = close.borrow_mut().take() {
                    m.close();
                }
            })
            .draw(move |canvas, rect| {
                canvas.print(
                    Point::new(rect.x, rect.y),
                    "A second modal",
                    t.accent,
                    Rgba::TRANSPARENT,
                );
                canvas.print(
                    Point::new(rect.x, rect.y + 2),
                    "The picker is now dimmed under this one.",
                    t.text,
                    Rgba::TRANSPARENT,
                );
                canvas.print(
                    Point::new(rect.x, rect.y + 4),
                    "Esc closes it.",
                    t.text_muted,
                    Rgba::TRANSPARENT,
                );
            })
            .build()
    });
    modal.set_scrim(scrim);
    *slot.borrow_mut() = Some(modal.share());
}
