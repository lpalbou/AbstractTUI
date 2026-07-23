//! attachments — file attachments in a composer, both doors.
//!
//! Terminals have no drop protocol: dropping a file PASTES its path
//! (spelling varies per terminal — see `input::paste`'s corpus table).
//! This example wires the three engine surfaces a client needs:
//!
//! 1. `TextArea::on_paste` — the intercept that sees every paste
//!    BEFORE insertion;
//! 2. `input::paste::classify` — the cross-terminal drop classifier
//!    (pure; the fs-existence check would be YOUR side);
//! 3. `FilePicker` in a `Modal` — explicit selection for when there is
//!    nothing to drag from.
//!
//! Try it:
//!   drag a file onto this terminal   -> it becomes a chip, not text
//!   paste prose                      -> inserts exactly as always
//!   Ctrl+O                           -> picker modal (type to filter,
//!                                       Space marks, Enter picks,
//!                                       Backspace = parent, Esc closes)
//!   Enter                            -> "send" (status line), chips clear
//!   Ctrl+C                           -> quit
//!
//! Docs: docs/api.md § "File attachments — paste intercept, drop
//! classifier, FilePicker".
//!
//! OWNER: REACT.

use std::cell::RefCell;
use std::rc::Rc;

use abstracttui::input::paste::classify;
use abstracttui::prelude::*;
use abstracttui::widgets::StdFileSource;

/// Chip label: the file name tail keeps the row readable.
fn chip_label(path: &str) -> String {
    std::path::Path::new(path)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string())
}

fn main() -> abstracttui::base::Result<()> {
    if !abstracttui::term::have_tty() {
        println!("attachments: needs an interactive terminal — skipping cleanly");
        return Ok(());
    }
    if let Ok(id) = std::env::var("ABSTRACTTUI_THEME") {
        set_theme_by_id(&id);
    }

    let mut app = App::new(Size::new(80, 24));
    let overlays = app.overlays();
    app.mount(move |cx| {
        let t = use_theme(cx).get().tokens;
        let vp = use_viewport(cx);
        let chips: Signal<Vec<String>> = cx.signal(Vec::new());
        let status = cx.signal(String::from("nothing sent yet"));

        // ---- composer: pastes classified BEFORE insertion ----------
        let state = TextAreaState::new(cx);
        let submit_state = state.clone();
        let composer = TextArea::new()
            .state(&state)
            .placeholder("write a message — dropped files become chips")
            .placeholder_while_focused(true)
            .rows(1, 4)
            .on_paste(move |pasted| match classify(pasted) {
                // A drop: consume the paste, grow the chip row. A
                // real client would fs-check + offer an undo here
                // (never-silent-attach); the classifier alone stays
                // deliberately conservative — prose always inserts.
                Some(paths) => {
                    chips.update(|c| c.extend(paths));
                    PasteAction::Consume
                }
                None => PasteAction::Insert,
            })
            .on_submit(move |text| {
                let n = chips.with_untracked(Vec::len);
                let shown = if text.is_empty() { "(no text)" } else { text };
                status.set(format!("sent: {shown:?} with {n} attachment(s)"));
                chips.set(Vec::new());
                submit_state.clear();
            })
            .element(cx, &t)
            .autofocus()
            .build();

        // ---- picker modal (Ctrl+O) ---------------------------------
        let modal_slot: Rc<RefCell<Option<Modal>>> = Default::default();
        let overlays = overlays.clone();
        let open_picker = move |_: &mut abstracttui::ui::EventCtx| {
            if modal_slot.borrow().is_some() {
                return; // one picker at a time
            }
            let start = std::env::current_dir()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|_| ".".to_string());
            let slot_pick = modal_slot.clone();
            let slot_esc = modal_slot.clone();
            let modal = Modal::open(
                &overlays,
                cx,
                vp.get_untracked(),
                Size::new(60, 16),
                move |mcx| {
                    let t = current_theme().tokens;
                    Element::new()
                        .style(LayoutStyle::fill())
                        // Esc is the HOST's: the picker never
                        // consumes it (module docs).
                        .shortcut(KeyChord::plain(Key::Escape), move |_| {
                            if let Some(m) = slot_esc.borrow_mut().take() {
                                m.close();
                            }
                        })
                        .child(
                            FilePicker::new(StdFileSource::default())
                                .start_in(start.clone())
                                .multi_select(true)
                                .on_pick(move |paths| {
                                    chips.update(|c| c.extend(paths));
                                    if let Some(m) = slot_pick.borrow_mut().take() {
                                        m.close();
                                    }
                                })
                                .element(mcx, &t)
                                .build(),
                        )
                        .build()
                },
            );
            *modal_slot.borrow_mut() = Some(modal);
        };

        // ---- layout -------------------------------------------------
        Element::new()
            .style(LayoutStyle::column().padding(Edges::all(1)))
            .shortcut(KeyChord::ctrl(Key::Char('o')), open_picker)
            .child(
                Block::new()
                    .border(BorderKind::Rounded)
                    .title("attachments")
                    .fill(t.surface)
                    .layout(LayoutStyle::column().gap(1).grow(1.0))
                    .child(text(
                        "drop a file onto the terminal, paste a path, or Ctrl+O for the picker",
                    ))
                    .child(dyn_view(LayoutStyle::default(), move || {
                        text(format!("status: {}", status.get()))
                    }))
                    .element(&t)
                    .build(),
            )
            .child(dyn_view(LayoutStyle::line(1), move || {
                let c = chips.get();
                if c.is_empty() {
                    text("no attachments")
                } else {
                    let row = c
                        .iter()
                        .map(|p| format!("[{}]", chip_label(p)))
                        .collect::<Vec<_>>()
                        .join(" ");
                    text(format!("attachments ({}): {row}", c.len()))
                }
            }))
            .child(composer)
            .child(text(
                " Enter send · Alt+Enter newline · Ctrl+O pick files · Ctrl+C quit",
            ))
            .build()
    })?;
    app.run()
}
