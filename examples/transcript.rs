//! transcript — the app-widgets wave proof demo: a synthetic agent
//! conversation streamed live through `Feed` + `md::StreamSession` +
//! `Scroll::follow_tail` (backlogs 0100 / 0110 / 0130 composed).
//!
//! What to watch for:
//! - answers arrive token by token (`reactive::interval` drives the
//!   synthetic stream); only the OPEN markdown block re-typesets per
//!   token — closed blocks (and every earlier message) are frozen;
//! - code fences render with syntax tint mid-stream, from the moment
//!   the opening ``` arrives (never flapping to plain text);
//! - the view stays PINNED to the tail until you scroll up (wheel,
//!   PgUp, Home) — the status line flips to "scrolled"; scrolling back
//!   to the bottom (or pressing f) re-pins it;
//! - the stress toggle rebuilds the feed with 10,000 history items:
//!   scrolling stays smooth because drawing is windowed (only the
//!   visible screenful ever paints).
//!
//! The bottom COMPOSER (backlog 0120 + the 0500 passive-panel slice):
//! a `TextArea` that grows with your message (1..4 rows), submits on
//! Enter (Alt+Enter — and Shift+Enter on kitty terminals — insert a
//! newline), recalls submitted messages with Up/Down at the buffer
//! edges, and completes `/` commands + `@` mentions in an anchored
//! dropdown at the caret (Tab/Enter accept, Esc dismisses, arrows
//! navigate, click works too). Multi-line pastes insert whole.
//!
//! Keys (composer focused by default — its keys win while typing):
//!   Enter        send · /help /theme /clear /quit are commands
//!   Alt+Enter    newline (Shift+Enter too where kitty reports it)
//!   Up / Down    caret first, then history at the edges
//!   wheel        scroll the transcript · Tab moves focus for key scroll
//!   Ctrl+C       quit (or submit /quit)
//!
//! Try: `cargo run --example transcript`
//!
//! OWNER: CONTENT (composer slice: REACT, backlog 0120).

use std::cell::Cell;
use std::collections::VecDeque;
use std::rc::Rc;
use std::time::Duration;

use abstracttui::app::anchored::{Completion, CompletionCandidate};
use abstracttui::prelude::*;
use abstracttui::reactive::interval;
use abstracttui::widgets::{Feed, FeedItem, FeedState};

/// The scripted conversation, replayed round-robin with fresh keys.
const SCRIPT: [(&str, &str); 3] = [
    (
        "How does the feed stay cheap while an answer streams in?",
        "# Two freezes\n\nThe stream session seals every block that can no \
         longer change, and the feed typesets each sealed block **exactly \
         once**:\n\n- closed blocks: parsed once, typeset once, frozen rows\n\
         - the open tail: re-typeset per token, O(one block)\n\nSo a long \
         answer costs the same per token at its end as at its start.",
    ),
    (
        "Show me the follow-tail idiom in code.",
        "Bind one signal and the scroll does the bookkeeping:\n\n```rust\n\
         let follow = cx.signal(true);\n\
         Scroll::new(Feed::new(&feed).view(cx))\n\
         \x20   .follow_tail(follow)\n\
         \x20   .view(cx)\n```\n\nScrolling up releases it; reaching the \
         bottom re-arms it; `follow.set(true)` jumps to the latest. The \
         status line below renders the same signal.",
    ),
    (
        "And when the content is huge?",
        "## Windowed drawing\n\nItems keep prefix sums over their heights, \
         so painting binary-searches the first visible item and stops at \
         the last:\n\n```rust\nlet first = prefix.partition_point(|p| *p <= top);\n\
         // walk until off-screen — never the whole feed\n```\n\n> Press \
         `s` and scroll around: ten thousand items, one screenful of work.",
    ),
];

/// Tokens per tick and the pause between turns, in ticks (25 ms each).
const CHUNK_CHARS: usize = 6;
const TURN_PAUSE_TICKS: u32 = 60;

enum Phase {
    Ask,
    Stream {
        key: String,
        chunks: VecDeque<String>,
    },
    Pause {
        ticks: u32,
    },
}

/// The synthetic conversation driver: one state-machine step per tick.
struct Script {
    turn: usize,
    phase: Phase,
}

impl Script {
    fn new() -> Script {
        Script {
            turn: 0,
            phase: Phase::Ask,
        }
    }

    fn tick(&mut self, feed: &FeedState) {
        match &mut self.phase {
            Phase::Ask => {
                let (question, answer) = SCRIPT[self.turn % SCRIPT.len()];
                feed.push(
                    format!("q{}", self.turn),
                    FeedItem::markdown(format!("**you** — {question}")),
                );
                let key = format!("a{}", self.turn);
                feed.push_stream(&key);
                // Char-safe token chunks (never split a UTF-8 scalar).
                let chars: Vec<char> = answer.chars().collect();
                let chunks = chars
                    .chunks(CHUNK_CHARS)
                    .map(|c| c.iter().collect::<String>())
                    .collect();
                self.phase = Phase::Stream { key, chunks };
            }
            Phase::Stream { key, chunks } => match chunks.pop_front() {
                Some(token) => {
                    feed.stream_append(key, &token);
                }
                None => {
                    feed.stream_finish(key);
                    self.turn += 1;
                    self.phase = Phase::Pause {
                        ticks: TURN_PAUSE_TICKS,
                    };
                }
            },
            Phase::Pause { ticks } => {
                *ticks -= 1;
                if *ticks == 0 {
                    self.phase = Phase::Ask;
                }
            }
        }
    }
}

fn main() -> abstracttui::base::Result<()> {
    if !abstracttui::term::have_tty() {
        println!("transcript: needs an interactive terminal — skipping cleanly");
        return Ok(());
    }
    if let Ok(id) = std::env::var("ABSTRACTTUI_THEME") {
        set_theme_by_id(&id);
    }

    let mut app = App::new(Size::new(90, 28));
    let quitter = app.quitter();
    let overlays = app.overlays();
    app.mount(move |cx| {
        let t = use_theme(cx).get().tokens;
        let feed = FeedState::new(cx);
        let follow = cx.signal(true);
        let paused = cx.signal(false);
        let stressed = cx.signal(false);

        // The synthetic stream: one scripted step every 25 ms. While
        // paused the interval still fires but does nothing — honest
        // enough for a demo knob (cancel the handle to go fully idle).
        let mut script = Script::new();
        let stream_feed = feed.clone();
        interval(cx, Duration::from_millis(25), move || {
            if !paused.get_untracked() {
                script.tick(&stream_feed);
            }
        });

        // Stress toggle: rebuild the feed with (or without) a 10k-item
        // history. `clear()` is the rebuild seam — the conversation
        // simply continues appending after it.
        let stress_feed = feed.clone();
        let toggle_stress = move || {
            let on = !stressed.get_untracked();
            stressed.set(on);
            stress_feed.clear();
            if on {
                for i in 0..10_000 {
                    stress_feed.push(
                        format!("h{i}"),
                        FeedItem::text(format!("history item {i} — windowed, never all drawn")),
                    );
                }
            }
        };

        let transcript = Scroll::new(Feed::new(&feed).view(cx))
            .follow_tail(follow)
            .view(cx);

        // ---- the composer: TextArea + '/'+'@' completion (0120) -----
        let composer_state = TextAreaState::new(cx);
        let sent = Rc::new(Cell::new(0usize));
        let submit = {
            let feed = feed.clone();
            let state = composer_state.clone();
            let quitter = quitter.clone();
            move |raw: &str| {
                let text = raw.trim();
                if text.is_empty() {
                    return;
                }
                state.push_history(text);
                state.clear();
                follow.set(true); // sending jumps back to the tail
                let n = sent.get();
                sent.set(n + 1);
                if let Some(cmd) = text.strip_prefix('/') {
                    let mut parts = cmd.split_whitespace();
                    match parts.next() {
                        Some("quit") => quitter.quit(),
                        Some("clear") => feed.clear(),
                        Some("theme") => {
                            // `/theme <id>` switches; bare `/theme` cycles.
                            let target = parts.next().map(str::to_string).unwrap_or_else(|| {
                                let themes = abstracttui::theme::list();
                                let now = current_theme().id;
                                let i = themes.iter().position(|(id, _, _)| *id == now);
                                let next = (i.map(|i| i + 1).unwrap_or(0)) % themes.len();
                                themes[next].0.to_string()
                            });
                            if !set_theme_by_id(&target) {
                                feed.push(
                                    format!("c{n}"),
                                    FeedItem::text(format!("no theme named {target:?}")),
                                );
                            }
                        }
                        Some("help") => feed.push(
                            format!("c{n}"),
                            FeedItem::markdown(
                                "**commands** — `/help` this list · `/theme [id]` switch or \
                                 cycle themes · `/clear` wipe the transcript · `/quit` exit. \
                                 `@name` mentions complete from the room list.",
                            ),
                        ),
                        other => feed.push(
                            format!("c{n}"),
                            FeedItem::text(format!(
                                "unknown command /{} — try /help",
                                other.unwrap_or_default()
                            )),
                        ),
                    }
                } else {
                    feed.push(
                        format!("u{n}"),
                        FeedItem::markdown(format!("**you** — {text}")),
                    );
                }
            }
        };
        let composer = TextArea::new()
            .state(&composer_state)
            .placeholder("Message — / commands · @ mentions · Enter sends")
            .rows(1, 4)
            .on_submit(submit)
            .element(cx, &t)
            .autofocus()
            .build();
        let composer = Completion::new()
            .trigger('/', |query| {
                [
                    ("help", "list commands"),
                    ("theme", "switch theme"),
                    ("clear", "wipe transcript"),
                    ("quit", "exit"),
                ]
                .iter()
                .filter(|(c, _)| c.starts_with(query))
                .map(|(c, hint)| {
                    CompletionCandidate::new(format!("/{c}"), format!("/{c} ")).detail(*hint)
                })
                .collect()
            })
            .trigger('@', |query| {
                ["alice", "bob", "mnemosyne"]
                    .iter()
                    .filter(|m| m.starts_with(query))
                    .map(|m| CompletionCandidate::new(format!("@{m}"), format!("@{m} ")))
                    .collect()
            })
            .attach(cx, &overlays, &composer_state, composer);

        let status_feed = feed.clone();
        Element::new()
            .style(LayoutStyle::column())
            .shortcut(KeyChord::plain(Key::Char('q')), move |_| quitter.quit())
            .shortcut(KeyChord::plain(Key::Char('f')), move |_| follow.set(true))
            .shortcut(KeyChord::plain(Key::Char(' ')), move |_| {
                paused.update(|p| *p = !*p)
            })
            .shortcut(KeyChord::plain(Key::Char('s')), move |_| toggle_stress())
            .child(
                Block::new()
                    .border(BorderKind::Rounded)
                    .title("transcript")
                    .fill(t.surface)
                    .layout(LayoutStyle::column().grow(1.0))
                    .child(transcript)
                    .element(&t)
                    .build(),
            )
            .child(composer)
            .child(dyn_view(LayoutStyle::line(1), move || {
                let rows = status_feed.total_rows().get();
                let state = if follow.get() {
                    "following"
                } else {
                    "scrolled (f to re-follow)"
                };
                let stream = if paused.get() { "paused" } else { "streaming" };
                let stress = if stressed.get() {
                    " · stress 10k ON"
                } else {
                    ""
                };
                text(format!(
                    " {} items · {rows} rows · {state} · {stream}{stress}",
                    status_feed.len()
                ))
            }))
            .child(dyn_view(LayoutStyle::line(1), {
                // Honest per-terminal key hint (backlog 0295): advertise
                // Shift+Enter ONLY where the kitty protocol is live —
                // env-claimed at enter or probe-proven a frame later
                // (0293 pushes the flags either way). Ctrl+J works on
                // every wire and anchors the fallback wording.
                let caps = use_caps(cx);
                move || {
                    let newline = if caps.get().kitty_keyboard {
                        "Shift+Enter newline"
                    } else {
                        "Alt+Enter / Ctrl+J newline"
                    };
                    text(format!(
                        " Enter send · {newline} · / commands · @ mentions · ↑↓ history · Ctrl+C quit"
                    ))
                }
            }))
            .build()
    })?;
    app.run()
}
