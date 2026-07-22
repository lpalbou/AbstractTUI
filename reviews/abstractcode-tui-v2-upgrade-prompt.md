# Prompt: upgrade abstractcode-tui to the new AbstractTUI engine surface

Copy everything below the line into the abstractcode-tui session.

---

Your project `~/tmp/abstractframework/abstractcode-tui` was built against
`abstracttui = "0.1.0"`. **AbstractTUI 0.2.0 is released** (crates.io +
https://github.com/lpalbou/AbstractTUI, 1,385 tests green): every bug you
filed and worked around is fixed, and the machinery you hand-rolled now
exists as engine widgets with measured performance. Upgrade the app to the
new surface, deleting app-side machinery wherever the engine now owns the
job.

**Dependency first**: set `abstracttui = "0.2.0"` in `Cargo.toml` (if the
crates.io index hasn't caught up yet, use a path dependency
`abstracttui = { path = "../abstracttui" }` temporarily). One breaking change
ships in 0.2.0: `term::Capabilities` and `GraphicsCaps` are
`#[non_exhaustive]` — only literal construction breaks (reading fields is
untouched); construct via `Default` + mutation (or `Capabilities::with`) if
you do that anywhere. Full release notes: CHANGELOG.md in the engine repo.

Then work through these, verifying each against your own tests as you go:

## 1. Delete the three bug workarounds (the engine fixed them)

- **0220 fixed** — `.autofocus()` inside a `dyn_view` regeneration no longer
  panics. Delete the avoidance note at `src/ui/chrome.rs:272` and the
  workaround comment block at `src/ui/modals.rs:654`; use plain `.autofocus()`
  wherever you deferred or avoided it.
- **0230 fixed** — modal content shortcuts work from the frame the modal
  opens (keyboard ownership moves immediately). Delete the
  `after(Duration::ZERO, ...)` focus/close deferral hacks at
  `src/ui/mod.rs:79` and `:110` if they exist only to dodge the dead-frame.
- **0240 fixed** — overflowing modal content no longer crushes fixed rows:
  one-row controls (button/input/progress/badge/spinner/separator/tabs) now
  default `shrink(0.0)`, `Scroll` defaults to `grow(1).basis(0)`, and debug
  builds record a zero-collapse diagnostic naming the offending node. Delete
  the manual `shrink(0.0)` armor in `src/ui/modals.rs` (e.g. :29, :45, :214,
  :224) where it exists purely as defense; keep it only where it documents a
  deliberate layout choice. The "Scroll-in-Modal" recipe is now documented in
  the engine's `docs/api.md`.
  Two gotchas worth knowing: (a) the shrink-resistant defaults apply to the
  widgets' DEFAULT layouts — any explicit `.layout(...)`/`.style(...)` you
  pass replaces them wholesale, so your own fixed-height rows (e.g.
  transcript chrome built from raw `Element`s with `h(1)`) should declare
  `shrink(0.0)` themselves if they must never yield to overflow; (b) the
  zero-collapse diagnostic surfaces through the startup-notices lane
  (`use_startup_notices`) in debug builds and flushes to stderr only after
  the terminal is restored — render the notices somewhere (a status line or
  toast) and you get free layout diagnostics during development.

## 2. Replace the hand-rolled transcript with `widgets::Feed`

Your `src/ui/transcript_view.rs` rebuilds a whole column of `MarkdownView`s
inside a `dyn_view_scoped` and does manual `MarkdownView::rows()` height math
(lines 253, 355, 409-440). The engine now owns this class:

- `FeedState::new(cx)` — keyed items: `push(key, FeedItem::markdown(..))`,
  `FeedItem::text/code/custom`, `update(key, item)` for in-place replacement
  (your tool-call cards), `clear()`.
- **Streaming answers**: `push_stream(key)` → `stream_append(key, token)` →
  `stream_finish(key)` — only the open markdown block re-typesets per token
  (measured ~104 bytes/frame steady streaming; 100k appends in 632 ms;
  windowed draw: a 10k-item feed draws one screenful, 42 µs full repaint).
  This replaces whatever `src/transcript.rs` does to re-render partial
  markdown.
- **Follow-tail**: wrap the feed in
  `Scroll::new(..).follow_tail(signal)` — pins to bottom through appends and
  resizes, disengages on user scroll, re-pins at the bottom edge,
  `signal.set(true)` = jump-to-latest. Content extent is now MEASURED
  (no `content_size` hint needed). Delete your scroll bookkeeping.

Expect `src/ui/transcript_view.rs` and part of `src/transcript.rs` to shrink
substantially; appends stop being O(items).

## 3. Replace timer recursion with `reactive::interval`

`src/ui/mod.rs:608/631/651` use self-rescheduling `after(..)` chains. Use
`interval(cx, period, callback)` — cancellation rides scope disposal, and a
suspended terminal coalesces missed ticks instead of replaying them. Keep
`after` for genuine one-shots.

## 4. Consider the live-data lane for your runner plumbing

`src/runner.rs` hand-rolls mpsc + wake + stale-stream protection. The engine
now ships `channel_source` / `latest_source` / `bounded_source(cap, policy)`
(drop-oldest/drop-newest/coalesce with an honest `IngestStats` signal — drops
and fold-panics are counted, never silent) with waker dedup (one wake per
burst). Your stale-run filtering stays app logic, but the transport + wake +
back-pressure can all be deleted. See the engine's `docs/live-data.md` and
`examples/feed.rs`.

## 5. Upgrade the composer: `TextArea` + completion

Your composer is a single-line `TextInput` (`src/ui/chrome.rs:267`). The
engine now has:

- `widgets::TextArea` — multiline, grapheme-correct, soft wrap, min/max rows
  with internal scroll, Enter-submits / Shift+Enter-newline (configurable
  `SubmitPolicy`), bracketed-paste-safe (multi-line paste inserts, never
  submits), input history (up/down at boundary rows), placeholder, disabled
  state.
- `app::anchored::Completion` — trigger-character completion: register
  providers (`'/'` → your `src/commands.rs` command list; `'@'` if you have
  mentions), candidates render in an anchored dropdown at the caret,
  Down/Up navigate, Enter/Tab accept, Esc dismisses, typing refilters.

This directly replaces any slash-command menu you were planning or faking.

## 6. Adopt selection + clipboard copy (the feature you filed as 0270)

All three tiers shipped:

- `selection().set_enabled(true)` / `.toggle()` — mouse drag paints a
  selection highlight over rendered text (pane-clamped, wide-glyph safe);
  Enter/`c`/Ctrl+C copies via OSC 52 through the presenter; Esc/click clears.
  Wheel scroll keeps working.
- `copy_to_clipboard(text)` — direct programmatic copy (e.g. a "copy answer"
  button on a message card).
- `mouse_capture().suspend()/.resume()` — native-selection mode if you prefer
  the terminal's own selection for a moment.
- The Shift/Option-drag native bypass matrix is now documented in the
  engine's `docs/troubleshooting.md` — point your `/help` at it instead of
  maintaining your own note.

Update your help text: "select text" now works in-app.

## 7. Still open engine-side (don't delete these guards)

- **0250**: `List::on_select` still fires on arrow movement (no activation
  event yet — the ruling is recorded, the fix is queued). Keep any guard you
  have around destructive on_select actions.
- Logical-content selection (copy the markdown SOURCE rather than screen
  text) is future work (0160); screen-text copy is what ships today.

## Process

Work incrementally: one section above per commit-sized change, running your
suite after each. If you hit engine defects or missing pieces, file them as
new items in `~/tmp/abstractframework/abstracttui/docs/backlog/proposed/first-app/`
(next free ids: 0280+) following the existing item format — your previous
reports (0220/0230/0240/0270) all got fixed, so they land.
