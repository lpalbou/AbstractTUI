# Prompt: upgrade abstractcode-tui to AbstractTUI 0.2.6

Copy everything below the line into the abstractcode-tui session.

---

Your project `~/tmp/abstractframework/abstractcode-tui` is built against
`abstracttui = "0.2.1"`. **AbstractTUI 0.2.6 is released** (crates.io +
https://github.com/lpalbou/AbstractTUI, ~1,660 tests green; five releases
landed since your 0.2.1). Your filings moved: 0290/0293/0295/0296 landed
in 0.2.2, 0297 in 0.2.3, and 0299 + 0291 (your full-redraw and
focused-placeholder asks, renumbered from 0300/0310 — band collision on
our side) shipped in 0.2.6; your Card system and
sync mirror motivated new engine surface (0102/0104), and your reconnect
hand-roll is the named example in the new `reactive::connection` module
doc. Upgrade and delete app-side machinery where the engine now
owns the job — with the honest caveats below where it does NOT.

**Dependency first**: set `abstracttui = "0.2.6"` in `Cargo.toml` (path
dep `{ path = "../abstracttui" }` if the index lags). No breaking API
changes 0.2.1 → 0.2.6 (`cargo semver-checks` clean at every hop); MSRV stays
1.87, so your `rust-version` comment only needs the version string
updated. ONE behavior change can move your screens: Feed markdown items
now typeset the full doc vocabulary (see item 2) — any test fixture whose
transcript text LOOKS like a GFM table will start rendering as one.

Work through these in order, running your suite after each:

## 1. Delete the three workarounds the engine fixed (~65 lines, all drop-in)

- **Ctrl+J newline machinery** — 0.2.2 folded Ctrl+J into `TextArea`
  under every submit policy (your 0295 ask). Your
  `insert_newline_at_caret` (`src/ui/chrome.rs:581-587`), the
  `.shortcut(KeyChord::new(Mods::CTRL, Key::Char('j')), …)` registration
  (`:650-661`), and the helper's unit test are dead code now — the
  engine's edit model consumes the chord before your shortcut can see
  it. Your behavior test (`tests/headless_ui.rs`, "Ctrl+J newline")
  should keep passing through the engine path; if it does, delete with
  confidence.
- **Selection-clear `on_change` hack** (`src/ui/chrome.rs:627-640`) —
  your comment names the blocker ("engine backlog 0290: the selection
  layer consumes `c`/Enter BEFORE tree dispatch; only the engine can fix
  that half"). 0290 is fixed in 0.2.2: EVERY copy (release copy and
  mid-drag key copies) now ends the gesture and clears the region.
  Delete the hook body; typing after a copy routes normally without help.
- **Modal retire one-tick deferral** (`src/ui/mod.rs:76-101`) — your
  comment says "Delete only when EVERY widget callback that can close a
  modal is disposal-safe." That day is 0.2.3: the disposal-safety law is
  engine-wide (your 0297 filing — Button's post-callback `pressed` write
  fixed, TextArea's caret republish fixed, per-site pins on
  Checkbox/RadioGroup/Tabs/TextInput/Table/Select). `retire()` collapses
  to `m.close()` — `Modal::close` removes the layer AND disposes the
  scope synchronously, so the equal-z invisible-key-eater window your
  split existed for has no surface left. KEEP `open_modal`'s
  atomic-replace ordering (slot filled before the epoch bump) — that
  contract is about reactive observers mid-flush, not disposal.

## 2. Take the free win: real tables in agent answers

Your assistant bodies are `FeedBlock::Markdown`
(`src/ui/transcript_view.rs:336`). On 0.2.3 they typeset GFM tables,
task lists, `~~strikethrough~~`, and in-flow `![alt](path)` images —
agent answers that carry pipe tables (they often do) stop rendering as
raw `| a | b |` text. Zero code. Two checks: (a) screen-asserting tests
whose fixtures contain table-shaped text will shift — update fixtures,
don't fight the typesetting; (b) in-flow images decode from PATHS, so
your artifact images are untouched (see item 6's honesty note).

## 3. Jittered backoff for your stream retries (the engine names you)

`reactive::connection`'s module doc cites "the first consumer's
hand-roll (linear × consecutive_errors, capped, NO jitter)" as the
thundering-herd failure mode — that is `src/runner.rs:1288-1318`. When
a gateway restarts, N per-run stream threads plus your 30s probe all
retry in lockstep. `reactive::Backoff` is pure math (no scope, no
thread affinity — fine on your stream threads):

```rust
use abstracttui::reactive::Backoff;

let mut backoff = Backoff::default();   // full jitter, 500ms base, ×2, 30s cap
// on stream error:
std::thread::sleep(backoff.next_delay());
// on a successful read:
backoff.reset();
```

Honest scope: the full `connection()` lifecycle does NOT drop into your
per-run streams — it must be constructed on the UI thread (`cx`), your
streams are short-lived and spawned from the runner thread, and your
REST poll-fallback is app logic either way. Where `connection()` DOES
fit is the app-level gateway orb (`store.conn` + the 30s probe,
`src/ui/mod.rs:1626-1668`): `ConnState::Reconnecting { attempt, next_in }`
renders "retry #2 in 1.4s" instead of a bare ✗. Optional, second pass.

## 4. Capability-honest hints (`use_caps` — your 0295, shipped)

- `src/ui/transcript_view.rs:232-234` fabricates `Capabilities`
  (`unicode_ok = true; truecolor = true`) for the mosaic image block.
  Replace with the live view: `abstracttui::app::current_caps()` —
  honest glyph/color selection on degraded terminals.
- `src/commands.rs:329-330` claims Shift+Enter "needs a kitty-protocol
  terminal (kitty/Ghostty)". STALE since 0.2.2: kitty enter-flags now
  follow the probe, so Shift+Enter works on iTerm2 ≥ 3.5, VS Code/
  Cursor, and Warp too — mid-session, no restart. Fix the text, or
  better, derive it.
- Composer hints (`src/ui/mod.rs:374,380`) can teach the BEST chord per
  terminal, the engine's own transcript example is the pattern
  (examples/transcript.rs:347-356). Related diagnostics you can point
  /help at: `cargo run --example caps` (0.2.4) prints the live
  capability report — which image channel the ladder picks, whether
  kitty keyboard is live.

```rust
let caps = abstracttui::app::use_caps(cx);
let newline_hint = if caps.get().kitty_keyboard {
    "Shift+Enter newline"
} else {
    "Ctrl+J newline"   // universal: 0x0a IS Ctrl+J on the legacy wire
};
```

## 5. Card headers → `FeedItem::rich_lines` — partial, be deliberate

0.2.3 ships the span model your Card system motivated (0102):

```rust
use abstracttui::render::rich::{RichLine, Span};
use abstracttui::render::Style;
use abstracttui::widgets::FeedItem;

FeedItem::rich_lines(vec![RichLine::from_spans(vec![
    Span::new("✓ ", Style::new().fg(t.ok)),
    Span::new("execute_command", Style::new().fg(t.text)),
    Span::new("  cargo test", Style::new().fg(t.text_muted)),
])])
```

Rich blocks typeset through the same span-preserving wrap as every
block; `fg: None` spans inherit the item ink. **What maps**: every
multi-ink HEADER row (your `Card::header` + detail), and uncapped
per-ink body lines — the custom draw closure and its height contract
die for those. **What does NOT map — do not force it**:

- `wrap_capped`'s width-aware post-wrap row cap with the
  "… (+K more lines)" marker. Rich lines wrap uncapped; render closures
  can't know draw width. Nearly all your bodies are capped.
- Hang-indent continuations (the `· ` info prefix).
- Detail ellipsis-to-width (rich WRAPS overflow instead; tolerable for
  your headers since `args_preview` is already char-capped upstream).

Recommended shape: headers + short uncapped bodies to `rich_lines`,
keep a slimmed body-only custom block for capped bodies (~60-80 lines
deleted), and FILE the gap — "width-aware max_rows + honest overflow
marker on Text/Rich feed blocks" is the one feature keeping your Card
alive, and it is exactly the kind of item that lands (see the closer).

## 6. What NOT to adopt (verified against your code — save the time)

- **`FeedState::sync`** — built from your wire_feed evidence (0104), but
  as shipped it demands `items: Signal<Vec<T>>`. Your sources are
  `Signal<Fold>` (items one field among stats/waits under one atomic
  update) and `Signal<Vec<EntityConvo>>` with the focus signal selecting
  one convo's nested items — and your `Item`s carry no per-item identity
  for its key closure. Adopting today means a store restructure or a
  clone-mirror that copies the item vec on every fold write (including
  stats-only writes). Keep `wire_feed`; file the source-shape ask
  instead: a borrow-based source variant
  (`sync_with(cx, read_fn, spec)`) plus your own minted item `seq` would
  make the ~250-line deletion real later. (When you do adopt, note the
  semantics you get free: one-writer self-heal, the visibility closure
  as the ONE truth — your mirror-drift pin test dissolves.)
- **`Driver::suspend`** — exists (0.2.3) but takes `&mut Driver`;
  `App::run()` owns the driver internally, so you cannot reach it. The
  engine doc defers the App-level verb to a future request-flag drain.
  Nothing to do.
- **`TimeSeries` + time axes** — your `output_series` is per-CALL
  indexed and already a bounded 64-slot ring (transcript.rs:1153-1161);
  TimeSeries is time-cadence machinery (slot quantization, NAN gap
  padding). Only relevant if you ever add tokens-over-TIME or GPU-%-over-
  time panels.
- **Key state / PushToTalk / Meter / AudioScope** — no held-key gestures
  or audio in this app.
- **Markdown in-flow images** — they decode from paths, lazily; your
  images are HTTP-fetched in-memory bitmaps. Your mosaic block stays
  (your 0280 filing remains the real ask) — just feed it `current_caps()`
  (item 4). Note for your image panes: 0.2.5 fixed image widgets
  collapsing in unsized rows (`Element::measure` — Image now answers its
  native cell footprint), so if you ever saw skinny empty strips, that
  class is dead.
- **UPDATE (2026-07-23): 0299 and 0291 shipped in 0.2.6 — delete two of
  the three workarounds this bullet used to protect.** Your veil/heal
  machinery (Ctrl+L + `/redraw` + the 5s chrome-band heartbeat, the
  translucent-veil trick and all its measured limits) dies: bind
  Ctrl+L → `abstracttui::app::request_full_redraw()` (prelude
  re-export) — real poison-prev + presenter-invalidate semantics, so
  the first heal frame re-anchors instead of ghost-walking, images
  re-place, and the transcript pane heals too (no chrome-band
  confinement needed). Add `set_redraw_on_focus_gained(true)` at boot
  and the heartbeat's job disappears entirely — the screen heals at the
  next focus round-trip. Your focused-placeholder overlay (~40 lines)
  dies too: `.placeholder_while_focused(true)` on the composer's
  `TextArea` paints the hint beside the caret engine-side (default off,
  so nothing changes until you opt in — delete the overlay in the same
  commit). The completion whole-draft guard STAYS: 0292 is still open
  engine-side (0294 too).

## 7. New capabilities you haven't asked for but fit your app

- **Transcript navigation**: `Feed::selected_key(Signal<Option<String>>)`
  highlights an item's row band (item inks kept);
  `FeedState::row_of(key)` gives the scroll target. That is jump-between-
  messages, "copy this answer", and the selection layer your open 0260
  disclosure ask needs anyway — worth prototyping before 0260 gets
  designed, so the filing can build on it.
- **A reader modal for long answers**: `MarkdownView` now has
  `outline_rows` (TOC), `resolve_anchor`, and `find` + `.highlights`
  (search with a distinct current match). Feed itself has NO search —
  transcript-wide search is not available — but "open this answer in a
  reader" (TOC + `/` search + `n`/`N`) is buildable from engine parts
  today; `examples/reader.rs` is the recipe.
- **`SelectHandle::open()`** — your 0296 ask, shipped in 0.2.2. Honest
  note: your List-in-Modal pickers have semantics Select lacks (theme
  live-preview on movement with Esc-revert, the two-stage model flow,
  84-col descriptive rows) and command-summoned Selects need a mounted
  trigger FACE to anchor at. Adopt only where a face makes sense (e.g.
  a status-bar route chip as a Combobox); your Picker machinery is
  small and correct — no obligation.
- **Connection state in the status bar** — item 3's optional half.

## 8. Keep filing (the closer)

The first-app record stands at 17 items born from your build: 13 fixed,
4 open and queued (0260 disclosure, 0280 rich custom blocks, 0292
completion position policy, 0294 panel flip) — nothing filed there has
been dropped. Three new
tensions verified in your current code deserve items — the 029x decade
is exhausted, next free ids are **0281–0289** (then 0261+), format per
`~/tmp/abstractframework/abstracttui/docs/backlog/proposed/first-app/`:

1. **Scroll bound-offset shrink clamp** — your hand-rolled clamp effect
   (`src/ui/mod.rs:232-256`: details toggle shrinks content below the
   scrolled offset, pane renders NOTHING until wheel/Esc). Engine
   confirmed: Scroll clamps offsets only inside its own gesture
   handlers. The engine measures the extent; it should repair (or
   opt-in clamp) a bound offset the content shrank out from under.
2. **`FeedState::sync` source shape** — the borrow-based source variant
   from item 6, with your `Signal<Fold>`/focus-selected-convo layout as
   the evidence.
3. **Capped preview blocks** — width-aware `max_rows` + honest overflow
   marker on Text/Rich feed blocks (+ hang-indent if cheap), the
   feature that keeps your Card system alive after 0102 (item 5).

Work incrementally — one section per commit-sized change, suite after
each. Everything above was verified against your working tree and the
engine's v0.2.6 tag on 2026-07-23; the full findings review (file:line
on both sides, fit verdicts, deletion estimates) is at
`~/tmp/abstractframework/abstracttui/reviews/abstractcode-tui-review-2026-07-23.md`.
