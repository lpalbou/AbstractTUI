//! decide — the decision gate, concretely (app-kits/0515).
//!
//! Three flavors of "block the flow on a question", each behind a key:
//!
//! - `1` a destructive confirmation with alternatives — single choice,
//!   details, per-option shortcut letters (`o`/`k`/`d`), a danger-
//!   tinted option, an "Other" free-text escape, and MUST-CHOOSE mode
//!   (`dismissable(false)`: Esc refuses visibly — destructive gates
//!   should not be dismissable into limbo),
//! - `2` a feature pick (multiple choice, pre-checked current state),
//! - `3` a two-step setup sequence (`ChoiceSequence`).
//!
//! The status line renders the last outcome: the flow CONTINUES in
//! `on_resolve` — that is the whole point of a gate. Esc inside a
//! dismissable gate cancels it (an explicit outcome, never silent;
//! from inside the Other editor the first Esc only retreats to the
//! list); a click outside does nothing (a decision gate has explicit
//! endings only).
//!
//! Keys: 1/2/3 open gates · Ctrl+T theme · q quit.
//!
//! OWNER: CHOICE (0515).

use abstracttui::prelude::*;
use abstracttui::theme::themes;
use abstracttui::ui::text;

fn describe(outcome: &ChoiceOutcome) -> String {
    match outcome {
        ChoiceOutcome::Answered(a) => {
            let mut parts: Vec<String> = a.selected.clone();
            if let Some(other) = &a.other {
                parts.push(format!("other: {other}"));
            }
            if parts.is_empty() {
                String::from("answered: (none)")
            } else {
                format!("answered: {}", parts.join(", "))
            }
        }
        ChoiceOutcome::Cancelled => String::from("cancelled"),
    }
}

fn main() -> abstracttui::base::Result<()> {
    if !abstracttui::term::have_tty() {
        println!("decide: needs an interactive terminal — skipping cleanly");
        return Ok(());
    }
    let mut app = App::new(Size::new(72, 22));
    let quitter = app.quitter();
    app.mount(move |cx| {
        let status = cx.signal(String::from("no decision yet — press 1, 2 or 3"));
        let theme_ix = cx.signal(0usize);

        Element::new()
            .style(LayoutStyle::column().padding(Edges::all(1)).gap(1))
            .shortcut(KeyChord::plain(Key::Char('q')), move |_| quitter.quit())
            .shortcut(KeyChord::new(Mods::CTRL, Key::Char('t')), move |_| {
                theme_ix.update(|i| *i = (*i + 1) % themes().len());
                set_theme_by_id(themes()[theme_ix.get_untracked()].id);
            })
            // -- gate 1: destructive confirmation with alternatives ----
            // Shortcut letters (o/k/d), a danger-tinted option, and
            // must-choose mode: a destructive decision has explicit
            // endings only — its options ARE the exits.
            .shortcut(KeyChord::plain(Key::Char('1')), move |_| {
                ChoicePrompt::new("Overwrite 3 locally modified files?")
                    .option_with(
                        ChoiceOption::new("overwrite", "Overwrite them")
                            .detail("the local edits are lost")
                            .key('o')
                            .danger(true),
                    )
                    .option_with(
                        ChoiceOption::new("keep", "Keep my copies")
                            .detail("the sync is skipped")
                            .key('k'),
                    )
                    .option_key("diff", "Show me the diff first", 'd')
                    .allow_other("Something else…")
                    .initial("keep")
                    .dismissable(false)
                    .on_resolve(move |o| status.set(format!("sync · {}", describe(&o))))
                    .open(cx);
            })
            // -- gate 2: feature pick (multiple, pre-checked) -----------
            .shortcut(KeyChord::plain(Key::Char('2')), move |_| {
                ChoicePrompt::new("Which capabilities may this agent use?")
                    .option_detail("fs", "File system", "read + write inside the workspace")
                    .option_detail("net", "Network", "outbound requests")
                    .option("shell", "Shell commands")
                    .option("clipboard", "Clipboard")
                    .allow_multiple(true)
                    .checked(["fs"])
                    .allow_other("Custom grant…")
                    .on_resolve(move |o| status.set(format!("grants · {}", describe(&o))))
                    .open(cx);
            })
            // -- gate 3: a two-question sequence ------------------------
            .shortcut(KeyChord::plain(Key::Char('3')), move |_| {
                let mut q1 = ChoiceQuestion::new("Where should the project live?");
                q1.options.push(ChoiceOption::new("here", "This folder"));
                q1.options
                    .push(ChoiceOption::new("sub", "A new subfolder").detail("keeps things tidy"));
                let mut q2 = ChoiceQuestion::new("Which template?");
                q2.options.push(ChoiceOption::new("minimal", "Minimal"));
                q2.options
                    .push(ChoiceOption::new("full", "Batteries included"));
                q2.other = Some(String::from("Other template…"));
                ChoiceSequence::new(vec![q1, q2])
                    .on_resolve(move |o| {
                        status.set(match o {
                            ChoiceSequenceOutcome::Completed(answers) => format!(
                                "setup · {} → {}",
                                answers[0].selected.join("+"),
                                answers[1]
                                    .other
                                    .clone()
                                    .unwrap_or_else(|| answers[1].selected.join("+")),
                            ),
                            ChoiceSequenceOutcome::Cancelled { index, .. } => {
                                format!("setup · cancelled at step {}", index + 1)
                            }
                        });
                    })
                    .open(cx);
            })
            .child(text("== decide: gate a flow on a structured question =="))
            .child(text(
                "1 · confirm a destructive sync (single + details + Other)",
            ))
            .child(text(
                "2 · grant capabilities (multiple, pre-checked, Other)",
            ))
            .child(text("3 · two-step setup (ChoiceSequence)"))
            .child(dyn_view(LayoutStyle::line(1), move || {
                text(format!("last outcome: {}", status.get()))
            }))
            .child(
                Element::new()
                    .style(LayoutStyle::default().grow(1.0))
                    .build(),
            )
            .child(text(
                "keys: 1/2/3 gates · inside: arrows move, Enter commits,",
            ))
            .child(text(
                "Space toggles (multi), Esc cancels · Ctrl+T theme · q quit",
            ))
            .build()
    })?;
    app.run()
}
