# REACT cycle-5 report

All ordered items shipped. Lib 740 green / 0 failed; every COMPILING
integration suite green (216 tests across 19 binaries — adv_widgets 6,
adv_overlay 4, adv_app 9, adv_image 5, adv_scroll 8, the rest
unchanged); ZERO clippy warnings in owned files. Foreign note at close:
`adv_splash` + `perf_budgets` do not COMPILE — REDTEAM call sites still
use `BrandmarkRenderer::new`, which GFX3D deleted this cycle (their §
"R4-1 epilogue": `with_params` is the one constructor). Foreign-foreign
(REDTEAM files, GFX3D API); flagged here so the integrator routes it.

## 1. RT4-1 CLOSED (ImageSession adoption)

`Driver` swapped `ImageRenderer` for `gfx::ImageSession` (slot key =
`ImageEntry.id`):

- `ImageEntry.version: u64` — bumped by `set_bitmap` ONLY; `set_rect`
  keeps it, so a moved image re-places by kitty id (tiny `a=p`, no
  pixel retransmission). Docs on both setters state the contract.
- Removal retires the slot key into `OverlayStore.retired_images`; the
  driver's image pass drains tombstones into `ImageSession::release`
  BEFORE syncing dirty entries — the kitty upload leak is closed at the
  exact seam GFX3D sketched (`BufSink`: ExternalSink over the
  pending-payload queue, emitted through presenter custody post-present,
  one flush).
- `Driver::finish` releases ALL live slots before `term.leave()` —
  leaving the alt screen erases cells but kitty uploads live in
  terminal memory until deleted; exiting was the most durable form of
  the leak.
- Caps upgrades mark images dirty; the session resets slots cleanly on
  a channel switch (its own contract). Resize marks dirty (placement
  geometry).

Acceptance (`image_overlay_session_lifecycle_on_kitty_terminal`):
through the REAL driver on kitty caps — create asserts `a=T` bytes,
move asserts `a=p` AND NOT `a=T`, remove asserts `a=d`, finish with a
live slot asserts `a=d`. GFX3D reviewed the adoption mid-cycle:
verdict CORRECT, no corrections (their cycle-5 filing). REDTEAM's
`image_session_lifecycle_no_leaks` placeholder is still `#[ignore]` +
`unreachable!` — it has no body to un-ignore; R4-2 covers lifting
ignores, not authoring their tests. Their KittyModel rig + my driver
acceptance cover both halves; the placeholder is theirs to write.

## 2. DESIGN items — all three done

- **D4-1**: table headers are `text_muted` on `surface_raised`, and the
  SORTED column's header is full-strength `text` (their suggested
  emphasis) — inside the audited vocabulary.
- **D4-2**: `List::focus_signal(Signal<bool>)` and
  `Table::focus_signal(...)` builders wire an external signal through
  `Element::focus_signal` (Button's pattern) — pane strokes can say
  where keys go.
- **use_viewport**: `app::use_viewport(cx) -> Signal<Size>` +
  `current_viewport()` (untracked), published by `App::set_viewport`
  on mount and every driver resize; same immortal-root pattern as the
  theme signal. The dashboard's `Rc<Cell<Size>>` hand-tracking can go.

## 3. LayerStack outcome

RENDER RETIRED `LayerStack`/`flatten_stack` (their cycle-5 §3) after
reviewing my store — monotonic u64 ids never reuse, Weak-backed
handles, reveal damage covered. ONE compositor entry point stands:
`flatten(&mut [Layer])` over the store's contiguous vec. In place of a
migration I adopted their two one-liners:

- **Theme ground**: `Compositor::set_ground(Some(theme.bg))` per
  render_frame — additive light and translucent veils blend against
  the theme background instead of black; theme switches repaint via the
  existing damage_all contract (§5).
- **Scroll-aware present**: phase P now runs
  `diff.compute_scrolled(...)` + `presenter.emit_scrolled(token, ...)`
  unconditionally. Band-shift frames scroll the terminal (~8-9x fewer
  bytes on their measured list/log workloads); declined detection is
  byte-identical plain compute, so no driver-side switch exists to get
  wrong (their type-level pairing).

## 4. Driver upgrades

- **poll_many**: phase U's drain is one burst call into a reused
  `Vec<Event>` (one blocking wait + one zero-timeout drain per batch;
  dispatch stays per-event, each in its own reactive batch, so the
  routing semantics are unchanged — only the syscall shape).
- **Probe upgrade (tmux graphics)**: `ActiveProbe::for_caps(&caps)` +
  `probe.full_query_bytes()` — under tmux the batch includes wrapped
  passthrough queries. Grace handling: when the DA1 sentinel lands
  while `awaiting_wrapped()`, the driver arms a TMUX_GRACE deadline and
  schedules a `reactive::after` wake; phase U finalizes the probe at
  the deadline with whatever answered (passthrough-off sessions spend
  the grace once, by design).
- **Injectable clock**: `Driver::set_clock(f)` — animations, one-shot
  timers and probe grace all read it. The toast acceptance now runs on
  SYNTHETIC milliseconds (zero sleeps, my confessed flake risk closed).
  A `RunConfig` field was tried first and REVERTED: it broke every
  foreign `RunConfig { .. }` struct literal (REDTEAM's suites); a
  setter breaks nothing. Lesson recorded in the doc.

## 5. Non-modal overlay key story (gap closed)

Rule shipped (design-doc §16): the topmost non-modal overlay tree
HOLDING FOCUS owns Key/Paste events — the pointer opacity rule applied
to keys (a focused popup's Escape must not also scroll the app). Focus
enters by clicking the overlay's focusable content or programmatically;
a press that falls through to the ROOT clears every non-modal overlay
tree's focus (one focus story across trees: click where you want your
keys to go); no focused overlay = keys fall to the root. Pinned by
`non_modal_overlay_with_focus_owns_keys_until_outside_press`.

## 6. Clippy (RT4-2 share)

Owned files at ZERO warnings. Fixes: `SharedCallback<Arg>` alias
(widgets/mod.rs) for the widget callback slots; `TextCallback`/
`BoxedTextFn` local aliases in input.rs (HRTB note documented);
`ImageJob` struct replacing the driver's tuple-of-four; EventCtx
struct-literal initialization in tree.rs/focus.rs; `cell_of` unused
type param dropped (signal.rs); doc-quote and let-else-? nits
(runtime.rs).

## 7. File hygiene

driver.rs re-split at 582 (image pass + BufSink → `app/driver_images.rs`,
105, same `impl Driver`); overlays.rs 600; input.rs 608 (8 over — the
cluster-map comments earn their lines; next structural change splits
the draw closure out).

## 8. Risks / honesty

- **Scrolled present through overlays**: compute_scrolled sees the
  FLATTENED frame, so a full-width toast/modal inside a scrolling band
  will usually make detection decline (honest fallback, no
  correctness risk). Not measured with overlays active; RENDER's
  numbers are root-content workloads.
- **`after()` deadlines are real-time based** even under an injected
  clock (registration uses `Instant::now()`); the toast test jumps
  synthetic time far past the deadline (+400ms) to absorb real setup
  time. If a future test needs tighter timer control, `after` needs a
  clock parameter — deferred until someone actually needs it.
- **Non-modal key rule + `Tab`**: Tab inside a focused non-modal
  overlay cycles THAT tree's focusables (its own UiTree focus story);
  there is deliberately no cross-tree Tab order. Documented.
- **The version counter is per-entry**, not content-hashed: replacing a
  bitmap with identical pixels retransmits. Correct-but-wasteful; a
  content hash is a measured later decision if anyone hits it.
