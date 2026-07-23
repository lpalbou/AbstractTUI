# Wave 11 — CODE → DOCS handoff

Everything below is a DOC-side consequence of code-side changes made in
this wave, or a doc defect found while auditing as a stranger. Code,
tests and rustdoc comments are already updated; docs/, README, examples
prose, llms files and CHANGELOG prose are yours.

## 1. CHANGELOG entries needed (facts; prose is yours)

### Fixed

- **Timer arm/fire clock coherence** (`reactive::after`/`interval`,
  `Connection` retry arms): deadlines were armed from a fresh
  `Instant::now()` but FIRED against the driver's injectable clock.
  Under an injected test clock on a loaded machine, real time races
  ahead of the injected timeline and a zero-delay timer never comes
  due — the live symptom was `wave_drawers::
  feed_page_inside_the_drawer_scrolls` flaking in full-suite runs
  (the Scroll width probe's `after(0)` never fired; the Feed stayed
  one item tall; the wheel had nothing to scroll). Arms now measure
  from the LOOP's clock: the turn's published time inside a driven
  turn (new `reactive::set_loop_clock`, published/cleared by the
  driver around each turn), the fire-pass clock inside a timer
  callback (pre-existing), real time outside any turn (bare rigs
  unchanged). Pinned by `wave11_review::after_armed_inside_a_turn_*`
  and `interval_first_deadline_*`.
- **Drawer close released the keyboard only after the slide** — a key
  pressed during the ~160 ms closing flight died in the focus-trapped
  dying panel (Esc-then-shortcut lost the shortcut; found live by the
  new pty smoke: `i`, Esc, `q` hung the drawers example). A close now
  releases the input trap the INSTANT it begins (modal routing off
  immediately; focus blurred on the next turn's phase U via a one-shot
  frame task — Esc arrives mid-dispatch of that very tree, so the blur
  cannot be synchronous); a reopen reversing the flight re-arms the
  trap and re-establishes initial focus. Pinned by `wave11_review::
  keys_reach_the_app_the_instant_a_modal_drawer_begins_closing` and
  `reopen_mid_close_restores_the_modal_trap`.
- **`Viewport3D` default layout was zero-height** — the widget shipped
  with `LayoutStyle::default()`, which solves to zero rows for a node
  with no intrinsic measure: an un-`layout()`ed viewport silently
  rendered nothing and hit-tested nothing (every in-repo call site was
  passing `.grow(1.0)` by hand). The default is now grow-into-region,
  matching the Meter/AudioScope family. Found by the first driver-level
  integration test ever written for this widget (`wave11_review::
  viewport3d_renders_and_reports_orbit_and_zoom_through_the_driver`).
- **Select type-ahead read the wall clock** — the popup's prefix-jump
  window measured `Instant::now()` inside the key handler instead of
  the ambient event time the click-chain doctrine established
  (`ui::event_time`). Now ambient-first with a real-time fallback;
  production behavior unchanged, injected-clock tests can script the
  prefix window deterministically.

### Added

- **Canonical `.view(cx)` on every element-only widget**: Sparkline,
  LineChart, BarChart, Progress, Spinner, Badge, Separator,
  RichTextView, CodeView, MarkdownView, Logo (api.md's "the canonical
  build is `.view(cx)`" sentence was false for these eleven — they had
  only `element(&tokens)`). Uniform shape: theme tokens from context,
  `element` stays the explicit-theming door. Pinned by
  `wave11_review::canonical_view_builds_render_for_every_element_only_widget`.
- **`reactive::set_loop_clock(Option<Instant>)`** — public: custom
  loop authors driving `run_due_timers` with their own clock publish
  the same value around their user-code phases (the driver does this
  automatically); `None` restores real-time arming.
- **Ten new pty smoke cases** (`tests/live_smoke.rs`, `--ignored`):
  feed, gallery, decide, drawers, shell, screenshot, caps + the
  extension examples workflow, network, mermaid — every example in the
  repo now runs under the VT referee (the shared build is now
  `--workspace --examples`).
- **Driver-level coverage fills** (`tests/wave11_review.rs`): live
  theme switching through the Driver + screenshot diff; Viewport3D
  render/orbit/zoom through real SGR input.

## 2. api.md corrections

- §widgets intro (~line 224): "the canonical build is `.view(cx)` …
  with an `element` form for explicit tokens — stateless widgets take
  just `&TokenSet`" — now true for every catalog widget. Consider
  adding the one honest exception the audit codified: **Meter and
  AudioScope are `view(cx)`-only** (signal-driven; tokens resolve
  TRACKED inside their own dyn region; a fixed-token `Element` form
  would need a wrapper node just to change the token source — the
  deliberate-absence note is in their rustdoc now).
- Drawer section: add the close-releases-input-immediately behavior
  ("keys pressed during the closing slide go to the app; a reopen
  mid-flight re-arms the trap").
- `Viewport3D` doc/snippet: the `.layout(LayoutStyle::default()
  .grow(1.0))` line in examples is now redundant (default). The
  `examples/viewer3d.rs` code still passes it explicitly — harmless,
  but if you touch that example, it can drop.
- Screenshots/timers sections: `set_loop_clock` deserves one line in
  the reactive/timers part ("one injected clock drives animations,
  timers, AND double-click" — the 0.2.16 claim — is now true at ARM
  time too; before, only firing was injected).

## 3. Regenerated artifacts warning

Running the functional sweep executed `target/debug/examples/capture`,
which rewrote `docs/captures/` (58 artifacts). Captures are
deterministic and no rendering-path change shipped this wave, so they
should be byte-identical — but verify before assuming (a diff there is
a finding, not noise).

## 4. Doc-defect observations (yours to fix or reject)

- `docs/api.md` promises the element form for explicit tokens
  generally; Meter/AudioScope violated silently until now (see §2).
- The controlled-mode naming convention is real but undocumented:
  bindings are named after the STATE they bind — `value` (Select,
  Combobox), `selection` (List, Table), `active` (PageHost), `folded`
  (Disclosure), `selected` (GraphView), `offset_x/y` (Scroll, GraphView)
  — EXCEPT `Drawer::bind(Signal<bool>)`, the one outlier (its state
  word would be "open", which reads like a verb). Worth one line in
  api.md's conventions so the next widget follows the state-name rule
  deliberately. Renaming `bind` is breaking and NOT proposed.
- `Tabs::on_change(usize)` vs `PageHost::on_change(&str)`: index-
  addressed vs id-addressed by design (Tabs = positional strip,
  PageHost = id-addressed pages). Fine — but api.md never says why;
  one sentence prevents the next "inconsistency" report.
- The 0.3 budget list (`docs/backlog/planned/0002…`) gained entry 7
  (`DrawerSize` `#[non_exhaustive]` candidate — the GraphAlgo
  precedent). If you maintain a budget summary anywhere, sync it.
- New proposed item `docs/backlog/proposed/wave11/0990_file_size_
  budget_reconciliation.md` (the >600-line inventory). If wave11 needs
  a README index like other proposed/ folders, that's yours.

## 5. Environment note for any doc claims about the pty smoke

In THIS sandbox the pty battery is timing-sensitive: parallel runs hit
pty-allocation failures (spawn errno -6) and three long-script cases
(reader, transcript, widgets) flake on fixed 150 ms key pacing under
load — the reader app itself exits 0 with generous pacing (probed).
Serial runs (`--test-threads=1`) are the honest mode here; the
operator's machine remains the reference for "N examples exit 0"
claims. Don't publish sandbox numbers.
