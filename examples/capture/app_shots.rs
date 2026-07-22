//! In-process app shots: the 0.2.x app-layer surfaces rendered through
//! the REAL pipeline (`Driver::turn` against `CaptureTerm`) with fixed
//! data and scripted input — no pty, no clocks, no wall-time wobble.
//! Byte-deterministic by construction, so these four sit in the visual
//! regression surface alongside the pty stills:
//!
//! - `transcript-stream` — Feed mid-stream (open code fence tinting),
//!   follow-tail pinned, TextArea composer with the `/` completion
//!   dropdown OPEN at the caret;
//! - `select-open` — a settings form with the Select popup open,
//!   highlight moved off the value (movement-vs-commit visible);
//! - `code-diff` — a unified diff through `CodeView::lang("diff")`:
//!   added/removed/hunk/meta inks on the code ground;
//! - `feed-scrolled` — follow-tail BROKEN by a wheel scroll: the tail
//!   is off-screen and the status line says so.

use std::path::Path;

use abstracttui::app::anchored::{Completion, CompletionCandidate};
use abstracttui::app::{App, Driver, RunConfig};
use abstracttui::prelude::*;
use abstracttui::term::Capabilities;
use abstracttui::testing::CaptureTerm;
use abstracttui::ui::text;
use abstracttui::widgets::{CodeView, Feed, FeedItem, FeedState};

use crate::write_shot;

/// Fixed capabilities: the host terminal must never leak into a still.
fn fixed_caps() -> Capabilities {
    Capabilities::with(|c| {
        c.truecolor = true;
        c.colors_256 = true;
    })
}

/// Mount `build`, drive to idle, feed `keys` (settling after each
/// chunk), and dump the modeled screen.
fn shot(
    out: &Path,
    name: &str,
    size: Size,
    keys: &[&[u8]],
    build: impl FnOnce(&mut App) + 'static,
) -> Vec<String> {
    let mut app = App::new(size);
    build(&mut app);
    let mut term = CaptureTerm::new(size);
    let cfg = RunConfig {
        caps: Some(fixed_caps()),
        enter: None,
        probe: false,
    };
    let mut driver = Driver::new(&mut app, &mut term, cfg).expect("driver");
    settle(&mut driver, &mut app, &mut term);
    for chunk in keys {
        term.push_input(chunk);
        settle(&mut driver, &mut app, &mut term);
    }
    write_shot(out, name, term.screen())
}

fn settle(driver: &mut Driver, app: &mut App, term: &mut CaptureTerm) {
    for _ in 0..64 {
        if driver.turn(app, term).expect("turn").idle {
            return;
        }
    }
    panic!("capture app shot failed to settle within 64 turns");
}

pub fn app_shots(out: &Path) -> Vec<String> {
    let mut produced = Vec::new();
    produced.extend(transcript_stream(out));
    produced.extend(select_open(out));
    produced.extend(code_diff(out));
    produced.extend(feed_scrolled(out));
    produced
}

// ------------------------------------------------------- transcript

/// The streamed answer is cut mid-code-fence: the fence tints from the
/// moment its opening line arrived — the "never flapping to plain
/// text" claim, visible.
fn transcript_stream(out: &Path) -> Vec<String> {
    shot(
        out,
        "transcript-stream",
        Size::new(90, 26),
        // "/th" opens the command dropdown at the caret, filtered.
        &[b"/th"],
        |app| {
            let overlays = app.overlays();
            app.mount(move |cx| {
                let t = use_theme(cx).get().tokens;
                let feed = FeedState::new(cx);
                feed.push(
                    "q0",
                    FeedItem::markdown("**you** — How does the feed stay cheap while streaming?"),
                );
                feed.push(
                    "a0",
                    FeedItem::markdown(
                        "# Two freezes\n\nClosed blocks typeset **exactly once**:\n\n\
                         - sealed blocks: parsed once, frozen rows\n\
                         - the open tail: re-typeset per token, O(one block)",
                    ),
                );
                feed.push("q1", FeedItem::markdown("**you** — Show me the follow-tail idiom."));
                // The live answer, mid-stream, inside an OPEN rust fence.
                feed.push_stream("a1");
                feed.stream_append(
                    "a1",
                    "Bind one signal and the scroll does the bookkeeping:\n\n\
                     ```rust\nlet follow = cx.signal(true);\nScroll::new(feed_view)\n    .follow_tail(fo",
                );
                let follow = cx.signal(true);
                let state = TextAreaState::new(cx);
                let composer = TextArea::new()
                    .state(&state)
                    .rows(1, 4)
                    .placeholder("Message — / commands · Enter sends")
                    .element(cx, &t)
                    .autofocus()
                    .build();
                let composer = Completion::new()
                    .trigger('/', |q| {
                        [
                            ("help", "list commands"),
                            ("theme", "switch theme"),
                            ("thanks", "be polite"),
                            ("quit", "exit"),
                        ]
                        .iter()
                        .filter(|(c, _)| c.starts_with(q))
                        .map(|(c, hint)| {
                            CompletionCandidate::new(format!("/{c}"), format!("/{c} "))
                                .detail(*hint)
                        })
                        .collect()
                    })
                    .attach(cx, &overlays, &state, composer);
                let status_feed = feed.clone();
                Element::new()
                    .style(LayoutStyle::column())
                    .child(
                        Block::new()
                            .border(BorderKind::Rounded)
                            .title("transcript")
                            .fill(t.surface)
                            .layout(LayoutStyle::column().grow(1.0))
                            .child(
                                Scroll::new(Feed::new(&feed).view(cx))
                                    .follow_tail(follow)
                                    .view(cx),
                            )
                            .element(&t)
                            .build(),
                    )
                    .child(composer)
                    .child(dyn_view(LayoutStyle::line(1), move || {
                        text(format!(
                            " {} items · {} rows · following · streaming",
                            status_feed.len(),
                            status_feed.total_rows().get()
                        ))
                    }))
                    .build()
            })
            .expect("mount transcript shot");
        },
    )
}

// ----------------------------------------------------------- select

/// Enter opens the popup on the focused trigger; Down moves the
/// HIGHLIGHT to "beta" while the bound value (and the trigger label)
/// stays "stable" — the 0250 split in one still.
fn select_open(out: &Path) -> Vec<String> {
    shot(
        out,
        "select-open",
        Size::new(72, 20),
        &[b"\t", b"\r", b"\x1b[B"],
        |app| {
            app.mount(|cx| {
                let t = use_theme(cx).get().tokens;
                let channel = cx.signal(0usize);
                let notify = cx.signal(true);
                Element::new()
                    .style(LayoutStyle::column().padding(Edges::all(1)).gap(1))
                    .child(text("settings"))
                    .child(
                        Element::new()
                            .style(LayoutStyle::row().gap(1).h(1))
                            .child(text("channel "))
                            .child(
                                Select::new(vec![
                                    SelectOption::new("stable").hint("lts"),
                                    SelectOption::new("beta"),
                                    SelectOption::new("nightly").hint("daily"),
                                    SelectOption::new("archive").disabled(true),
                                ])
                                .value(channel)
                                .layout(LayoutStyle::default().w(28).h(1).shrink(0.0))
                                .view(cx),
                            )
                            .build(),
                    )
                    .child(
                        Checkbox::new("page the on-call")
                            .checked(notify)
                            .element(cx, &t)
                            .build(),
                    )
                    .child(
                        Element::new()
                            .style(LayoutStyle::default().grow(1.0))
                            .build(),
                    )
                    .child(text(
                        " enter commits · esc abandons · click outside dismisses",
                    ))
                    .build()
            })
            .expect("mount select shot");
        },
    )
}

// ------------------------------------------------------------- diff

/// The diff mapping on the code ground: added `ok`, removed `error`,
/// hunk `info`, meta `text_muted` — every theme already audits these.
fn code_diff(out: &Path) -> Vec<String> {
    const PATCH: &str = "\
diff --git a/src/render/present.rs b/src/render/present.rs
--- a/src/render/present.rs
+++ b/src/render/present.rs
@@ -41,7 +41,7 @@ fn emit_runs(out: &mut Vec<u8>) {
     for run in runs {
         // Idle frames must stay free.
-        if run.cells.is_empty() {
+        if run.is_empty() {
             continue;
         }
         emit(run, out);
@@ -88,4 +88,6 @@ fn flush(out: &mut Vec<u8>) {
     term.write(out)?;
+    // One flush per frame — the damage contract's §6.
+    term.flush()?;
 }";
    shot(out, "code-diff", Size::new(84, 20), &[], |app| {
        app.mount(|cx| {
            let t = use_theme(cx).get().tokens;
            Element::new()
                .style(LayoutStyle::column().padding(Edges::all(1)))
                .child(
                    Block::new()
                        .border(BorderKind::Rounded)
                        .title("review: present.rs")
                        .fill(t.surface)
                        .layout(LayoutStyle::column().grow(1.0))
                        .child(
                            CodeView::new(PATCH)
                                .lang("diff")
                                .layout(LayoutStyle::default().grow(1.0))
                                .element(&t)
                                .build(),
                        )
                        .element(&t)
                        .build(),
                )
                .build()
        })
        .expect("mount diff shot");
    })
}

// ---------------------------------------------------- feed-scrolled

/// A wheel-up over the feed releases follow-tail: the newest rows sit
/// below the viewport and the status line flips to "scrolled".
fn feed_scrolled(out: &Path) -> Vec<String> {
    shot(
        out,
        "feed-scrolled",
        Size::new(84, 22),
        // Three wheel-ups over the feed body.
        &[b"\x1b[<64;10;8M", b"\x1b[<64;10;8M", b"\x1b[<64;10;8M"],
        |app| {
            app.mount(|cx| {
                let t = use_theme(cx).get().tokens;
                let feed = FeedState::new(cx);
                for i in 0..36 {
                    feed.push(
                        format!("log{i}"),
                        FeedItem::text(format!(
                            "12:{:02}:{:02} worker-{} event {} — payload accepted",
                            i / 2,
                            (i * 7) % 60,
                            i % 3,
                            i
                        )),
                    );
                }
                let follow = cx.signal(true);
                let status_follow = follow;
                Element::new()
                    .style(LayoutStyle::column())
                    .child(
                        Block::new()
                            .border(BorderKind::Rounded)
                            .title("events")
                            .fill(t.surface)
                            .layout(LayoutStyle::column().grow(1.0))
                            .child(
                                Scroll::new(Feed::new(&feed).gap(0).view(cx))
                                    .follow_tail(follow)
                                    .view(cx),
                            )
                            .element(&t)
                            .build(),
                    )
                    .child(dyn_view(LayoutStyle::line(1), move || {
                        text(if status_follow.get() {
                            " following the tail".to_string()
                        } else {
                            " scrolled (f to re-follow)".to_string()
                        })
                    }))
                    .build()
            })
            .expect("mount feed shot");
        },
    )
}
