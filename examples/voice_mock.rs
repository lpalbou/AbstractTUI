//! voice_mock — the whole voice surface, no audio, no network
//! (backlog media-av/0650: the validation vehicle for 0700/0610/0620).
//!
//! A voice app's UI surface, provable without microphones, TTS engines,
//! or a gateway — everything synthesized from the frame clock:
//!
//! - **Fake mic**: while "talking", a 30 ms `reactive::interval` runs a
//!   deterministic sine+noise envelope through `bounded_source`
//!   (`DropOldest` — the retained window IS the scope's rolling ring)
//!   and a latest-level signal. NOTHING runs while idle: the interval
//!   is cancelled on stop, the meters decay to silence and reach their
//!   fixpoint, and the app parks in a blocking read (the zero-idle law,
//!   live).
//! - **Meters** (media-av/0620): one horizontal channel with dB
//!   ballistics + peak hold, an 8-band spectrum, and an `AudioScope`
//!   drawing the rolling waveform.
//! - **Push-to-talk** (media-av/0610 over games/0700): Space is the
//!   capture gesture — HOLD-to-talk where kitty release events are
//!   live, PRESS-to-toggle on legacy wires — and the footer prints the
//!   truthful gesture label plus the key-state fidelity
//!   (`Full`/`Degraded`). Losing terminal focus stops capture (the mic
//!   privacy rule).
//! - **Fake transcription**: while talking, words append into a `Feed`
//!   streaming item; release/toggle-off finishes the line.
//!
//! Keys: Space talk (hold or toggle per fidelity) · `c` clear the
//! transcript · `q` / Ctrl+C quit.
//!
//! Try: `cargo run --example voice_mock`
//!
//! OWNER: INPUTAV (wave 3).

use std::time::Duration;

use abstracttui::app::use_key_state;
use abstracttui::prelude::*;
use abstracttui::reactive::{bounded_source, interval, IntervalHandle, OverflowPolicy};
use abstracttui::widgets::{Feed, FeedItem, FeedState};

/// Scope window: ~4 s of levels at the 30 ms synth cadence.
const SCOPE_WINDOW: usize = 128;
/// Synth cadence — the shape real recorder threads produce (~30 ms
/// chunks), here driven by the engine's timer lane.
const CHUNK: Duration = Duration::from_millis(30);

const WORDS: &[&str] = &[
    "the", "quick", "brown", "fox", "jumps", "over", "the", "lazy", "dog", "while", "the", "meter",
    "watches", "every", "word", "land",
];

fn main() -> abstracttui::base::Result<()> {
    if !abstracttui::term::have_tty() {
        println!("voice_mock: needs an interactive terminal — skipping cleanly");
        return Ok(());
    }
    if let Ok(id) = std::env::var("ABSTRACTTUI_THEME") {
        set_theme_by_id(&id);
    }
    let mut app = App::new(Size::new(90, 26));
    let quitter = app.quitter();

    app.mount(move |cx| {
        let keys = use_key_state(cx);
        let caps = use_caps(cx);

        // ---- fake mic lanes --------------------------------------------
        // Latest level (meter), rolling window (scope), 8-band frame
        // (spectrum) — the three data shapes of the grounding study.
        let level = cx.signal(0.0f32);
        let bands = cx.signal(vec![0.0f32; 8]);
        let (scope_tx, scope_window, _stats) =
            bounded_source::<f32>(cx, SCOPE_WINDOW, OverflowPolicy::DropOldest);

        // ---- transcript feed -------------------------------------------
        let feed = FeedState::new(cx);
        feed.push(
            "intro",
            FeedItem::markdown("**voice mock** — no audio, no network; everything below is synthesized from the frame clock."),
        );
        let utterance = cx.signal(0u32);

        // ---- push-to-talk over the key-state service --------------------
        // The synth interval exists ONLY while talking (zero idle cost);
        // interval handles are stashed so stop can cancel.
        let synth: Signal<Option<IntervalHandle>> = cx.signal(None);
        let ptt = abstracttui::app::PushToTalk::bind(cx, KeyChord::plain(Key::Char(' ')));

        let start_synth = {
            let scope_tx = scope_tx.clone();
            let feed = feed.clone();
            move || {
                let n = {
                    utterance.update(|u| *u += 1);
                    utterance.get_untracked()
                };
                let item_key = format!("say-{n}");
                feed.push_stream(&item_key);
                feed.stream_append(&item_key, "🎙 ");
                let scope_tx = scope_tx.clone();
                let feed = feed.clone();
                let mut tick = 0u64;
                let handle = interval(cx, CHUNK, move || {
                    tick += 1;
                    // Deterministic sine + hash-noise envelope (no rand,
                    // no wall entropy — the dashboard precedent).
                    let ts = tick as f32 * 0.03;
                    let envelope = 0.55 + 0.35 * (ts * 1.7).sin();
                    let noise = (hash01(tick) - 0.5) * 0.30;
                    let v = (envelope + noise).clamp(0.0, 1.0);
                    level.set(v);
                    scope_tx.send(v);
                    bands.set(
                        (0..8)
                            .map(|b| {
                                let phase = ts * (1.1 + b as f32 * 0.6) + b as f32;
                                (v * (0.45 + 0.55 * phase.sin().abs())).clamp(0.0, 1.0)
                            })
                            .collect(),
                    );
                    // A word every ~12 chunks (~360 ms) while talking.
                    if tick.is_multiple_of(12) {
                        let word = WORDS[(tick as usize / 12) % WORDS.len()];
                        feed.stream_append(&item_key, &format!("{word} "));
                    }
                });
                synth.set(Some(handle));
            }
        };
        let stop_synth = {
            let feed = feed.clone();
            move |reason: abstracttui::app::StopReason| {
                if let Some(handle) = synth.get_untracked() {
                    handle.cancel();
                }
                synth.set(None);
                // Silence the lanes: the meters DECAY to their fixpoint
                // and stop billing frames — watch the app go idle.
                level.set(0.0);
                bands.set(vec![0.0; 8]);
                let n = utterance.get_untracked();
                let key = format!("say-{n}");
                feed.stream_append(&key, &format!(" — ({reason:?})"));
                feed.stream_finish(&key);
            }
        };
        let ptt = ptt.on_start(start_synth).on_stop(stop_synth);

        // ---- widgets -----------------------------------------------------
        let mono = Meter::new(level).db_floor(-60.0).view(cx);
        let spectrum = Meter::bands(bands).bar(3, 1).view(cx);
        let scope = AudioScope::new(scope_window).range(0.0, 1.0).view(cx);

        let feed_clear = feed.clone();
        let quit = quitter.clone();
        Element::new()
            .style(LayoutStyle::column().padding(Edges::all(1)).gap(1))
            .shortcut(KeyChord::plain(Key::Char('q')), move |_| quit.quit())
            .shortcut(KeyChord::plain(Key::Char('c')), move |_| feed_clear.clear())
            // header ------------------------------------------------------
            .child({
                let ptt = ptt.clone();
                dyn_view(LayoutStyle::line(1), move || {
                    let talking = ptt.state().get().is_talking();
                    text(format!(
                        "VOICE MOCK — {}",
                        if talking { "● capturing" } else { "○ idle" }
                    ))
                })
            })
            // level + spectrum row ---------------------------------------
            .child(
                Element::new()
                    .style(LayoutStyle::row().gap(2).h(6))
                    .child(
                        Element::new()
                            .style(LayoutStyle::column().gap(1).grow(1.0))
                            .child(text("mic level (dB, peak hold)"))
                            .child(mono)
                            .child(text("waveform (rolling window)"))
                            .child(scope)
                            .build(),
                    )
                    .child(
                        Element::new()
                            .style(LayoutStyle::column().gap(1).w(34))
                            .child(text("bands"))
                            .child(spectrum)
                            .build(),
                    )
                    .build(),
            )
            // transcript ---------------------------------------------------
            .child(
                Element::new()
                    .style(LayoutStyle::column().grow(1.0))
                    .child(Scroll::new(Feed::new(&feed).view(cx)).view(cx))
                    .build(),
            )
            // footer: the TRUTHFUL gesture + fidelity line -----------------
            .child({
                let ptt = ptt.clone();
                dyn_view(LayoutStyle::line(1), move || {
                    let fidelity = keys.fidelity();
                    let kitty = caps.get().kitty_keyboard;
                    text(format!(
                        "{} to talk · key state: {:?} (kitty keyboard: {}) · c clear · q quit",
                        ptt.gesture_label(),
                        fidelity,
                        if kitty { "live" } else { "off" },
                    ))
                })
            })
            .build()
    })?;
    app.run()
}

/// Cheap deterministic hash noise in 0..1 (splitmix-style avalanche).
fn hash01(x: u64) -> f32 {
    let mut z = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^= z >> 31;
    (z & 0xFFFF) as f32 / 65535.0
}
