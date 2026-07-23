//! reasoning — the reasoning-controls demo (backlog app-kits/1250): a
//! fake chat turn whose ThinkingFold STREAMS reasoning fragments and
//! then receives the trailing COMPLETE aggregate (watch the body
//! recompose — last wins), plus a ReasoningSelect in the footer
//! cycling the three capability states.
//!
//! What to watch for:
//! - the "Thinking" card arrives FOLDED (the operator ruling) with a
//!   dot indicator that advances per fragment — data-driven, no timer:
//!   when the stream pauses, the dot freezes (park = silent);
//! - unfold it mid-stream (click / Tab+Enter): markdown typesets live —
//!   the fence tints, the mini table renders — and the capped body
//!   scrolls; at completion the fragments are REPLACED by the
//!   recomposed aggregate (the wording changes — deliberately);
//! - `m` cycles three fake models: reasoner-9b (capable — the popup
//!   offers auto/none + its three declared levels ONLY), plain-2b
//!   (does not reason — the control locks and refuses to open) and
//!   mystery-x (capability unknown — locked to none with a "set
//!   anyway" override row that unlocks the full ladder). The footer
//!   control REMOUNTS with fresh facts on every swap — the documented
//!   reset recipe: a stale override can never leak between models;
//! - the wire line: `on_change` hands the app a VALUE; the app writes
//!   the `thinking` key itself (the engine mints no wire vocabulary);
//! - the footer-right label renders `reasoning_label` /
//!   `reasoning_label_glyph` — the parity grammar every
//!   AbstractFramework console footer shares.
//!
//! Keys:  m cycle model · p replay the turn · Tab focus · Ctrl+C quit
//!
//! Try: `cargo run --example reasoning`
//!
//! Docs: docs/api.md § "app::ReasoningSelect" and § "ThinkingFold".

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use abstracttui::prelude::*;
use abstracttui::widgets::{Feed, FeedItem, FeedState};

/// The streamed thinking (fragments) — fences and a table tint live.
const THINKING_SRC: &str = "Let me check the layer math.\n\n\
    ```rust\nlet z = overlays.top_z() + 1; // above everything live\n```\n\n\
    | side | rows |\n|:-----|-----:|\n| below | 19 |\n| above | 13 |\n\n\
    Below wins, so the popup opens under the trigger. Now the width: \
    content-sized, clamped into the viewport, anchored at the SCREEN \
    cell. The claim holds; drafting the answer next.";

/// The trailing COMPLETE aggregate — deliberately reworded so the
/// last-wins replace is visible on screen.
const THINKING_AGGREGATE: &str = "**Recomposed at completion (last wins).**\n\n\
    Checked the layer math: owned popups allocate `top_z() + 1`, place \
    below-preferred with the flip rule, and anchor at SCREEN cells — \
    so a trigger inside a modal opens its menu adjacent to itself. \
    Answer follows from there.";

const ANSWER: &str = "The popup opens **below the trigger** at screen-cell \
    coordinates, one z above everything live — a select inside a modal \
    layers correctly by construction.";

/// The three fake models the footer cycles through.
fn model_facts(which: usize) -> (&'static str, ReasoningFacts) {
    match which % 3 {
        0 => (
            "reasoner-9b",
            ReasoningFacts::capable(["low", "medium", "high"]),
        ),
        1 => ("plain-2b", ReasoningFacts::non_reasoning()),
        _ => ("mystery-x", ReasoningFacts::unknown()),
    }
}

fn main() -> abstracttui::base::Result<()> {
    if !abstracttui::term::have_tty() {
        println!("reasoning: needs an interactive terminal — skipping cleanly");
        return Ok(());
    }
    if let Ok(id) = std::env::var("ABSTRACTTUI_THEME") {
        set_theme_by_id(&id);
    }

    let mut app = App::new(Size::new(96, 30));
    let quitter = app.quitter();
    app.mount(move |cx| {
        let t = use_theme(cx).get().tokens;
        let model = cx.signal(0usize);
        let turn_gen = cx.signal(0u64);
        let wire = cx.signal(String::from(
            "wire: thinking key unset (auto — the provider decides)",
        ));

        // ---- the chat turn: one generation per replay -----------------
        // Per-turn state is the contract: a new thought is a NEW
        // ThinkingFoldState, created on the generation scope and
        // disposed with it — `p` rebuilds everything, and the script
        // interval dies with the old generation automatically.
        let turn = dyn_view_scoped(
            LayoutStyle::column()
                .width(Dimension::Percent(1.0))
                .grow(1.0),
            move |gcx| {
                let _ = turn_gen.get(); // replay key
                let question = FeedState::new(gcx);
                question.push(
                    "q",
                    FeedItem::markdown("**you** — why does the popup open where it does?"),
                );
                let thinking = ThinkingFoldState::new(gcx);
                let answer = FeedState::new(gcx);
                answer.push_stream("a");

                // The synthetic stream: thinking fragments, the
                // complete aggregate, then the answer — then the
                // interval CANCELS itself (zero idle when parked).
                let chunks: Vec<String> = THINKING_SRC
                    .chars()
                    .collect::<Vec<_>>()
                    .chunks(7)
                    .map(|c| c.iter().collect())
                    .collect();
                let answer_chunks: Vec<String> = ANSWER
                    .chars()
                    .collect::<Vec<_>>()
                    .chunks(7)
                    .map(|c| c.iter().collect())
                    .collect();
                let tick = Rc::new(Cell::new(0usize));
                let handle: Rc<RefCell<Option<IntervalHandle>>> = Default::default();
                let h = handle.clone();
                let thinking_for_script = thinking.clone();
                let answer_for_script = answer.clone();
                let installed = interval(gcx, Duration::from_millis(40), move || {
                    let i = tick.get();
                    tick.set(i + 1);
                    if i < chunks.len() {
                        // Reasoning arrives from result METADATA —
                        // the app hands fragments in; nothing is
                        // parsed out of reply prose.
                        thinking_for_script.append(&chunks[i]);
                        thinking_for_script.set_detail(format!("{} tk", (i + 1) * 3));
                    } else if i == chunks.len() {
                        thinking_for_script.complete(THINKING_AGGREGATE);
                        thinking_for_script.set_detail("642 tk");
                    } else if i - chunks.len() - 1 < answer_chunks.len() {
                        answer_for_script.stream_append("a", &answer_chunks[i - chunks.len() - 1]);
                    } else {
                        answer_for_script.stream_finish("a");
                        if let Some(h) = h.borrow_mut().take() {
                            h.cancel(); // the turn is over: park silent
                        }
                    }
                });
                *handle.borrow_mut() = Some(installed);

                Element::new()
                    .style(
                        LayoutStyle::column()
                            .width(Dimension::Percent(1.0))
                            .grow(1.0),
                    )
                    .child(Feed::new(&question).gap(0).element(gcx, &t).build())
                    .child(ThinkingFold::new(&thinking).view(gcx))
                    .child(Feed::new(&answer).gap(0).element(gcx, &t).build())
                    .build()
            },
        );

        // ---- the footer: model cycler + the three-state control -------
        // The RESET RECIPE: the control remounts with fresh facts when
        // the model signal changes — per-instance state (the unknown
        // override, the uncontrolled value) dies with the generation.
        let footer = dyn_view_scoped(
            LayoutStyle::default()
                .width(Dimension::Percent(1.0))
                .height(Dimension::Cells(1)),
            move |gcx| {
                let (name, facts) = model_facts(model.get());
                let support = facts.support;
                let value = gcx.signal(String::from(REASONING_AUTO));
                let committed = gcx.signal(false);
                let control = ReasoningSelect::new(facts)
                    .value(value)
                    .layout(
                        LayoutStyle::default()
                            .width(Dimension::Cells(44))
                            .height(Dimension::Cells(1))
                            .shrink(0.0),
                    )
                    .on_change(move |v| {
                        committed.set(true);
                        // THE APP writes the wire key (the engine
                        // mints no wire vocabulary).
                        wire.set(if v == REASONING_AUTO {
                            "wire: thinking key unset (auto — the provider decides)".into()
                        } else {
                            format!("wire: request.thinking = {v:?}")
                        });
                    })
                    .view(gcx);
                Element::new()
                    .style(
                        LayoutStyle::row()
                            .width(Dimension::Percent(1.0))
                            .height(Dimension::Cells(1)),
                    )
                    .child(text(format!(" model: {name}  ")))
                    .child(control)
                    .child(dyn_view(
                        LayoutStyle::default().height(Dimension::Cells(1)).grow(1.0),
                        move || {
                            // The parity grammar, both forms — what a
                            // code-tui/code/console footer renders.
                            let locked = match support {
                                Some(true) => false,
                                Some(false) => true,
                                None => !committed.get(),
                            };
                            let (v, state) = if locked {
                                (String::from("none"), LockState::Locked)
                            } else {
                                (value.get(), LockState::Unlocked)
                            };
                            text(format!(
                                "  grammar: {}  ·  {}",
                                reasoning_label(&v, state),
                                reasoning_label_glyph(&v, state),
                            ))
                        },
                    ))
                    .build()
            },
        );

        Element::new()
            .style(LayoutStyle::column())
            .shortcut(KeyChord::plain(Key::Char('q')), {
                let quitter = quitter.clone();
                move |_| quitter.quit()
            })
            .shortcut(KeyChord::plain(Key::Char('m')), move |_| {
                model.update(|m| *m = (*m + 1) % 3)
            })
            .shortcut(KeyChord::plain(Key::Char('p')), move |_| {
                turn_gen.update(|g| *g += 1)
            })
            .child(
                Block::new()
                    .border(BorderKind::Rounded)
                    .title("reasoning")
                    .fill(t.surface)
                    .layout(LayoutStyle::column().grow(1.0))
                    .child(turn)
                    .element(&t)
                    .build(),
            )
            .child(footer)
            .child(dyn_view(LayoutStyle::line(1), move || {
                text(format!(" {}", wire.get()))
            }))
            .child(text(
                " m cycle model · p replay turn · Tab focus · Enter open/toggle · Ctrl+C quit",
            ))
            .build()
    })?;
    app.run()
}
