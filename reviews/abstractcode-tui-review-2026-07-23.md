# abstractcode-tui vs AbstractTUI 0.2.3 — consumer review, 2026-07-23

Reviewer posture: skeptical staff engineer. Every claim below was verified
at BOTH sources — the consumer's working tree (their current state: one
commit `34b9447` plus a large uncommitted 0.4.0 wave, ~13.7k insertions)
and the engine's `v0.2.3` tag. File:line references are to those trees.

**Consumer state**: `abstractcode-tui 0.4.0`, `abstracttui = "0.2.1"`
(Cargo.toml:26), ~24.7k lines of `src/`. Since the last review the app
grew a whole entity lane (`convo.rs` 1,378, `entities.rs` 1,226,
`gateway/entities.rs` 1,100, `ui/entity_modals.rs`, `ui/entity_actions.rs`),
a tool-policy module (1,253), a queue manager, and the 0.4.0
"conclusion + presence" wave (offloaded answers, cockpit header, Ctrl+L
heal, `/gpu`). They adopted the v2-prompt items: Feed + follow-tail,
TextArea + Completion, `reactive::interval`, selection/copy,
`List::on_activate`, startup-notices surfacing. They did NOT adopt the
0.2.1 Select family (reasoned — see finding 9). MSRV: engine 0.2.3 still
declares 1.87, same as their floor — no bump.

Backlog ground truth (directory, not the README table — the README's open
table is stale, still listing 0250/0290/0297/0298 which are in
`completed/`): open = 0260, 0280, 0291, 0292, 0294, 0299. Completed since
their last sync: 0270, 0290, 0293, 0295, 0296, 0297, 0298. The 029x id
decade is exhausted; next free first-app ids are 0281–0289 (and 0261+).

---

## Findings

### 1. Ctrl+J composer machinery — DROP-IN deletion (0.2.2)

- Theirs: `src/ui/chrome.rs:581-587` (`insert_newline_at_caret`) +
  `:650-661` (the `.shortcut(Ctrl+J)` registration and its routing
  comment) + the unit test on the helper.
- Engine: 0.2.2 folded Ctrl+J into `TextArea` itself — "Ctrl+J now
  inserts a newline under every submit policy" (CHANGELOG [0.2.2],
  textarea.rs module doc: "Ctrl+J always inserts (0x0a IS Ctrl+J on the
  legacy wire — the universal…)"). Their own 0295 filing asked for this
  ("built-in-fallback ask") and it shipped.
- Fit: **drop-in**. The engine's edit model now CONSUMES the chord, so
  their shortcut becomes unreachable dead code (no double-insert risk —
  their comment at :654-656 documents that the shortcut only fired
  because the old edit model ignored ctrl-chars). Their behavior test
  (`tests/headless_ui.rs:3499+`) keeps passing through the engine path.
- Deletion: ~25 lines.

### 2. Selection-clear `on_change` hack — DROP-IN deletion (0.2.2, their 0290)

- Theirs: `src/ui/chrome.rs:627-640` — clears a lingering drag-selection
  on typing, with an honest-limit comment naming "engine backlog 0290:
  the selection layer consumes `c`/Enter BEFORE tree dispatch… only the
  engine can fix that half".
- Engine: 0290 completed in 0.2.2 — "EVERY copy now ends the gesture"
  (release copy AND mid-drag key copies clear the region;
  regression-pinned `release_copy_frees_enter_and_c_for_the_app`).
- Fit: **drop-in** deletion; the half they could not fix app-side is the
  half the engine fixed.
- Deletion: ~14 lines.

### 3. Modal one-tick retire deferral — DROP-IN deletion (0.2.3, their 0297)

- Theirs: `src/ui/mod.rs:76-101` — `retire()` removes the layer NOW and
  defers `m.close()` one tick, with a comment naming the exact blocker:
  "`Button`'s mouse path still writes its own `pressed` signal AFTER
  `on_click` returns… Delete only when EVERY widget callback that can
  close a modal is disposal-safe."
- Engine: 0.2.3 made disposal safety the LAW engine-wide (their 0297
  filing): Button's mouse-Up arm fixed (verified at button.rs:197-208 —
  `pressed.set(false)` strictly before `fire()`), TextArea's caret
  republish fixed, per-site disposal pins across Button/Checkbox/
  RadioGroup/Tabs/TextInput/TextArea/Table + the Select commit path.
- Fit: **drop-in**: `retire()` collapses to `m.close()` (`Modal::close`
  removes the layer and disposes the scope in one call, popups.rs:108+;
  with nothing deferred, the equal-z invisible-key-eater window that
  motivated the layer-now/scope-later split no longer exists). Keep the
  `open_modal` atomic-replace ordering (slot before epoch) — that
  contract is about reactive observers, not disposal.
- Deletion: ~25 lines incl. the comment block.

### 4. Card custom-block system → `FeedItem::rich`/`rich_lines` — PARTIAL (needs-adaptation)

- Theirs: `src/ui/transcript_view.rs:42-212` — the `Card` struct +
  `wrap_capped` + `tool_glyph` (~170 lines): glyph/label/detail headers
  and capped bodies, each part with its own ink, drawn via
  `FeedBlock::Custom` with a shared height/draw honesty contract.
- Engine: 0102 shipped `FeedItem::rich/rich_lines/rich_block` — span-model
  lines typeset through the same span-preserving wrap as every block,
  patch-style styles (`fg: None` inherits the item ink). The 0102 item
  cites this Card system as "the motivating evidence" and claims its
  "only reason to exist" is multi-ink chrome.
- Honest fit: **the span half is covered; three Card features are NOT**:
  1. `wrap_capped`'s WIDTH-AWARE post-wrap row cap with the
     "… (+K more lines — full text in the run ledger)" marker. Rich
     blocks wrap at draw width with no row clamp; render closures run
     width-independent, so the cap cannot be precomputed. Nearly every
     Card body uses a cap (user 200 / steer 40 / thinking 10 / tool 6 /
     error 12 / info 6 / probe 14).
  2. Hang-indent continuations (the `· ` info prefix, body_indent=2) —
     `RichText::wrap` has no hanging-indent concept.
  3. The header detail's ellipsis-to-remaining-width — rich lines WRAP
     on overflow instead of truncating (mitigated: `args_preview` is
     already char-capped upstream via `value_preview(…, ARGS_PREVIEW_MAX)`,
     transcript.rs:1219).
- Adaptation paths: (a) accept source-line-granularity caps (cap before
  wrap; a single very long line then wraps uncapped — a real semantics
  change they built `wrap_capped` to avoid), delete the whole Card
  (~140 net lines); or (b) move headers + uncapped bodies to
  `rich_lines` and keep a slimmer body-only custom block for capped
  bodies (~60-80 lines deleted); or (c) file the gap (a `max_rows` +
  overflow-marker knob on text/rich feed blocks) and convert wholesale
  when it ships. Recommended: (b) now + file (c) — see New tensions.

### 5. `wire_feed` sync mirror → `FeedState::sync` — NOT ACTUALLY COVERED (as shipped)

- Theirs: `src/ui/transcript_view.rs:404-657` — FNV fingerprint (~100
  lines), the `wire_feed` sync effect with positional `i{index}` keys,
  focus-dimension bookkeeping, and the `is_visible` mirror predicate —
  plus the mirror-drift pin test (:939-997). ~250 lines total.
- Engine: 0104 shipped `FeedState::sync(cx, items, SyncSpec)` with
  exactly this job (key/fingerprint/render/visible closures, engine-owned
  rebuild policy, one-writer self-heal). The 0104 completion report's
  acceptance line claims "wire_feed + fingerprint + mirror test (~180
  lines) become one `sync` call".
- Honest fit: **the acceptance claim does not survive contact with their
  actual store shape.** Two blockers, verified:
  1. `sync` demands `items: Signal<Vec<T>>` (feed_sync.rs:129-134).
     Their sources are `Signal<Fold>` (items one field among stats/
     waits/flags, mutated together under documented ordering contracts —
     store.rs:205) and `Signal<Vec<EntityConvo>>` with a focus signal
     selecting ONE convo's nested `items` (store.rs:314,
     transcript_view.rs:622-640). Neither is `Signal<Vec<Item>>`, and
     `Memo` does not coerce to `Signal`, so feeding sync means either a
     store restructure (splitting items out of Fold — surgery through
     their most contract-laden type) or a clone-mirror effect that
     copies the item vec on every fold write (including stats/activity-
     only writes, since Fold is one signal) — against the app's measured
     zero-copy grain.
  2. `SyncSpec` keys derive from `&T` alone; their `Item` variants carry
     no identity (only `Item::Tool` has a `key`). Adoption needs a
     minted per-item `seq` (a wrapper struct or field), a mechanical but
     wide refactor across fold/convo construction and matching.
- Verdict: **keep `wire_feed` today; file the source-shape gap** (a
  borrow-based source variant, e.g. `sync_with(cx, read_fn, spec)` that
  reads via a `with`-style closure — zero-copy over nested state). With
  that landed plus item identities, the ~250-line deletion is real. What
  they'd gain beyond deletion: the one-writer self-heal, the
  one-truth visibility closure (their mirror-drift test dissolves), and
  engine-owned rebuild policy.

### 6. Stream retry backoff → `reactive::Backoff` — DROP-IN; `connection()` optional

- Theirs: `src/runner.rs:1288-1318` — per-run ledger stream threads retry
  with `(500 * consecutive_errors).min(5000)` ms sleeps: linear,
  capped, NO jitter. Also the 30s fixed-cadence idle probe
  (`ui/mod.rs:1626-1634`) + `Conn::Ok/Down` edge self-heal (:1649-1668).
- Engine: `reactive::Backoff` is a pure jittered-exponential struct
  (full jitter, base 500ms/×2/cap 30s, `reset()` on success) — and the
  connection module's doc names THEIR hand-roll as the motivating
  failure: "the first consumer's hand-roll (linear × consecutive_errors,
  capped, NO jitter) has exactly that failure mode" (connection.rs:44-50).
- Fit: **`Backoff` is drop-in** inside their existing stream loop — it is
  pure math with no thread affinity (their streams run off-UI-thread,
  where `connection(cx, …)` cannot be constructed). The full
  `reactive::connection` lifecycle is **needs-adaptation**: their
  streams are short-lived, per-run, spawned from the runner thread, and
  their poll-fallback is a Degraded mode the dial-fn model can express
  but only after moving stream ownership onto the UI thread. The
  app-level gateway orb (store.conn) is the natural `ConnState` consumer
  ("retry #2 in 1.4s" instead of a bare ✗) — worthwhile, not urgent.
- Deletion: ~10 lines swapped for ~5; decorrelation is the real win
  (N stream threads + the probe all retrying a restarting gateway in
  lockstep is their fleet-of-one thundering herd).

### 7. Feed markdown doc vocabulary — FREE on upgrade (0.2.3)

- Theirs: assistant bodies render as `FeedBlock::Markdown`
  (transcript_view.rs:336). On 0.2.1, GFM tables in agent answers render
  as raw pipe text (the "QLabel showing raw pipe-tables reads as broken"
  class, known from the assistant app).
- Engine: 0.2.3 Feed markdown items (static AND streaming) typeset
  through `md::parse_doc` — tables, task lists, `~~strikethrough~~`,
  in-flow images. `cargo semver-checks` clean vs 0.2.2.
- Fit: **zero code**. Caveat for their suite: fixtures containing
  pipe-shaped text now typeset as tables — screen asserts on such
  fixtures will shift.

### 8. Capability honesty — `use_caps`/`current_caps` (0.2.2, their 0295)

- Theirs, three spots verified:
  - `src/ui/transcript_view.rs:232-234` — the mosaic image block
    FABRICATES `Capabilities` (`unicode_ok = true; truecolor = true`)
    because no live accessor existed when it was written.
  - `src/commands.rs:329-330` — help text claims "Shift+Enter needs a
    kitty-protocol terminal (kitty/Ghostty)" — STALE: 0.2.2's 0293 fix
    pushes kitty enter-flags when the probe proves the protocol on
    iTerm2 ≥ 3.5, VS Code/Cursor, and Warp, mid-session.
  - `src/ui/mod.rs:374,380` — composer hints teach "Ctrl+J newline"
    unconditionally; on kitty-class terminals "Shift+Enter" is now true
    and friendlier.
- Engine: `app::use_caps(cx)`/`app::current_caps()` (prelude) — the
  live, probe-upgraded view; the engine's own transcript example derives
  its newline hint from it (examples/transcript.rs:347-356).
- Fit: **drop-in** for the mosaic caps; **small dyn_view** for the hints.
  Net code roughly even; honesty is the win.

### 9. Select family + `SelectHandle::open` (0.2.1/0.2.2, their 0296) — OPTIONAL, not a deletion

- Theirs: `open_picker` List-in-Modal machinery (`src/ui/modals.rs:539-570`
  + per-picker configs). Verified reasons it survives Select:
  theme picker live-previews on selection MOVEMENT with Esc-revert
  (:594-614), the model picker is two-stage provider→model (:679+), rows
  are wide and descriptive (84 cols), and the pickers are command-
  summoned with NO mounted trigger face — `SelectHandle::open()` anchors
  at the trigger's last-painted rect, so a face must exist somewhere
  first.
- Fit: **not-actually-covered as a replacement**; available as a UX
  option (e.g. a status-bar route chip as a Combobox face). Their 0296
  ask shipped and is usable the day they want faces; no obligation.

### 10. New capabilities that fit (not asked for, verified available)

- `Feed::selected_key(Signal<Option<String>>)` + `FeedState::row_of(key)`
  (0.2.3): transcript keyboard navigation — jump between user prompts /
  answers, "copy this message", and the selection layer their open 0260
  disclosure ask will need anyway. Feed windowing does the rest.
- A reader surface for long answers: `MarkdownView` now renders the doc
  vocabulary with `outline_rows` (TOC), `resolve_anchor`, and
  `find`/`highlights` (0.2.3). **Feed itself has NO search** — verified;
  so transcript-wide search is not available, but an "open answer in
  reader" modal (TOC + `/` search + `n`/`N`) is buildable from engine
  parts today (examples/reader.rs is the recipe).
- `ConnState` rendering for the orb (finding 6's optional half).

### 11. What does NOT map (the honesty list)

- **`Driver::suspend` is unreachable for them.** It takes
  `&mut Driver` (driver_suspend.rs:60); `App::run()` owns the Driver
  internally (app/mod.rs:323-371). The suspend doc itself defers the
  public verb for App::run consumers to a future request-flag drain
  ("control-plane 0300's lifecycle lane"). Do not recommend; Ctrl+Z
  remains whatever the platform does today.
- **`TimeSeries`/time-axis does not fit their sparkline.** Their
  `output_series` is per-LLM-CALL indexed, already a bounded 64-slot
  drop-oldest ring (transcript.rs:1153-1161). TimeSeries is
  time-cadence machinery (slot quantization, NAN gap padding) — forcing
  a call-indexed series into it buys nothing. Only relevant if they ever
  want tokens-over-TIME or GPU-utilization-over-time panels (their /gpu
  currently shows instantaneous numbers only).
- **Key press/release state, PushToTalk, Meter, AudioScope**: no held-key
  gestures, no audio in this app. Skip.
- **Markdown in-flow images don't replace their mosaic block.** Engine
  image blocks decode lazily from PATHS; their images are HTTP-fetched,
  decoded, downscaled in-memory bitmaps (runner.rs:938-989). Their
  custom mosaic block stays; their 0280 filing (protocol images/widgets
  in feed blocks) remains the real ask. One improvement: feed it
  `current_caps()` instead of fabricated caps (finding 8).
- **`FeedState::sync` as shipped** (finding 5): source-shape mismatch.
- Their veil/heal machinery (`force_redraw`/`heal_chrome_rows`/
  `veil_and_vacate`, ui/mod.rs:1315-1420) and the focused-placeholder
  overlay (chrome.rs:758-800) stay: 0299 and 0291 are still open
  engine-side. Their completion whole-draft guard (chrome.rs:692-701)
  stays: 0292 open. Anchored-panel flip (0294): open.

### 12. New tensions the engine hasn't seen (candidates for their next filings)

Verified at both sources, none already filed:

1. **Scroll never re-clamps a bound offset on content shrink** — their
   hand-rolled shrink-clamp effect (`ui/mod.rs:232-256`: a details
   toggle folds cards, content shrinks below the scrolled offset, pane
   renders NOTHING until wheel/Esc; "Scroll never clamps a bound
   external offset signal"). Engine confirmed: scroll.rs clamps offsets
   only inside its own gesture handlers (:272/:275). The engine owns
   extent measurement; it could own offset repair on shrink (or an
   opt-in `clamp_offset(true)`).
2. **`FeedState::sync` source shape too narrow** (finding 5): a
   borrow-based source (`sync_with(cx, || …with(|items| …))` / lens)
   so fold-shaped stores (items nested inside a larger state signal,
   possibly focus-selected among several sources) can adopt without a
   restructure or clone-mirror. The 0104 acceptance line implicitly
   assumed `Signal<Vec<T>>`; the first consumer never had one.
3. **Capped preview blocks** (finding 4): width-aware `max_rows` + an
   honest overflow marker on Text/Rich feed blocks — the one feature
   keeping their 170-line Card system alive after 0102 shipped the
   span model. Also the hang-indent wrap option if cheap.
4. (Engine-side doc nit, not a filing: `docs/backlog/proposed/first-app/README.md`'s
   open table still lists 0250/0290/0297/0298, all completed — the
   directory is the truth.)

---

## Top-5 recommendations (by leverage)

1. Bump to 0.2.3 and take finding 7 (tables in answers) for free —
   verify fixtures.
2. Delete the three workaround classes: Ctrl+J machinery, selection-clear
   hack, modal retire deferral (findings 1-3, ~65 lines, all drop-in).
3. Swap the stream/probe retry math to `reactive::Backoff` (finding 6).
4. Caps honesty: `current_caps()` in the mosaic block; caps-derived
   newline hints; fix the stale Shift+Enter help claim (finding 8).
5. File the three new tensions (finding 12) — 0281-0289 are free — and
   take Card partial adoption (finding 4b) if the cap gap gets a filing
   rather than a wait.
