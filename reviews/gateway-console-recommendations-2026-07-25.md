# Engine seat → gateway-console: recommendation letter (2026-07-25)

From the AbstractTUI engine seat, after a read-only survey of
`abstractgateway/console-tui` at 0.3.3 (src/, tests/, Cargo.toml/lock,
your field-gateway backlog) against engine 0.2.16 (crates.io current).
Nothing in your repo was edited. Every item cites both sides.

## Executive summary

1. You pin and lock abstracttui **0.2.12**; bumping to **0.2.16** is a two-line change (Cargo.toml:23 + `cargo update -p abstracttui`, MSRV unchanged at 1.87) and buys you your own 1030 fusion fix, Modal resize re-clamp, and `Table::on_activate` — every delta is in your favor, semver-checked additive.
2. The 0.2.15 recipes are mostly already yours: R1 chrome pins are thorough, R3 (`use_startup_notices`) renders in your footer — the one gap is R4: `line_styled` (ui/util.rs:34) clips horizontally only; one `rect.is_empty()` guard in that shared helper covers nearly every hand-rolled row in the app.
3. Double-click adoption is four one-liners (`.on_activate`) on the profiles, routes, users and entities tables — routed through the exact refusal-carrying handlers your `e`/Enter/`m` keys already call; the runs table and the modal tables honestly have no "open" verb, skip them.
4. One live defect found, owned half-and-half: your Runtimes `s` (steer) dies whenever the runs table holds focus, because Table consumes `s` with no sort handler bound — your own finding 0980, still unfixed at engine HEAD (the 0.2.16 Enter/Space work adopted the rule but did not retrofit `s`); the engine owes you that fix, and your cycle-1 "avoid `s`" workaround eroded when 0.2.0 added steer.
5. Graph family verdict: **do not adopt** — nothing this console serves is more than one hop deep, and your tables + state columns + refusal reasons already carry those relations; screenshots are the real 0.2.14 win: your headless suite is one line away from minting SVG evidence for the operator's "tests, screenshots and reports" mandate.

## Version state (verified)

- `Cargo.toml:23` requests `abstracttui = "0.2.12"`; `Cargo.lock`
  resolves 0.2.12. The caret already admits 0.2.16 — recommend writing
  `"0.2.16"` explicitly (your comment style documents the version's
  rationale; keep that) and running `cargo update -p abstracttui`.
- MSRV: engine 0.2.12 and 0.2.16 both declare `rust-version = "1.87"`
  (your Cargo.toml:5-6 comment stays true).
- Changelog errata, engine side: the 0.2.15 release notes (fusion fix,
  Modal re-clamp, the wave-10 sweep) currently sit under the `[0.2.16]`
  heading in the engine CHANGELOG — there is no `[0.2.15]` heading.
  When you cross-reference the bump, both fix sets are in that one
  section. We will correct the heading engine-side.

Behavioral deltas you inherit at 0.2.16, all verified benign for your
code:

- **Fusion class fixed (your 1030)**: zero-area rects no longer run
  draw closures (engine `src/ui/draw.rs`; guarantees table
  `reviews/wave10/size-ratio-sweep.md` G1-G11). Your pinned-chrome
  matrix test (`tests/headless_ui.rs:1885`) asserts presence of pinned
  rows — still green by construction. Unpinned lines now degrade to
  clean absence instead of overprinting siblings.
- **Modal re-clamps on resize** (engine `src/app/popups.rs`, wave10 §2):
  your `open_form_guarded` passes the at-open viewport
  (ui/mod.rs:441-453) and previously kept those bounds forever on the
  engine side; after the bump a mid-session terminal shrink re-centers
  and re-clamps every form modal. You never wrote a workaround for
  this, so there is nothing to delete — it is a free fix.
- **Enter/Space on Table** are consumed only when `on_activate` is
  bound (engine table.rs:224-240) — your screen-level Enter shortcut on
  Routes (routes.rs:39) keeps working unchanged until you bind the
  callback, after which a table-focused Enter fires the same action
  through the widget instead.

Workaround-deletion audit: **nothing to delete.** You never wrote a
manual modal re-clamp; your "fusion defenses" are the R1 chrome pins,
which are the standing recipe (wave10 R1), not workarounds — keep every
one of them (including the sandbox result-slot pins, review.rs:95-102:
post-fix the failure mode becomes clean absence of the result the
operator paid tokens for, which the pin still rightly prevents).

## P1 — adopt now (one-liners)

### P1.1 Bump the pin to 0.2.16

- Yours: `Cargo.toml:23`, `Cargo.lock`.
- Engine: CHANGELOG `[0.2.16]` (double-click + fusion/re-clamp
  sections); semver gate recorded additive-clean
  (reviews/wave10/size-ratio-sweep.md §6).
- Gate it with your own discipline: the 36-cell chrome matrix
  (`title_bar_and_separator_survive_content_pressure`,
  tests/headless_ui.rs:1885), the full headless suite, and the pty
  smoke (`scripts/pty_smoke.py`).

### P1.2 `Table::on_activate` on the four browse tables

Engine surface: `Table::on_activate(usize)` — Enter (always), Space
(single-select alias), and double-click on the already-selected row;
consumed only when bound (docs/api.md § "Table — selection vs
activation"; CHANGELOG 0.2.16; engine table.rs:224-240). The adoption
note you already received covered the profiles table; this extends it
to every open-a-row surface, each routed through the SAME handler your
key already calls so the refusal-with-reason logic rides along:

| Table | Yours | Bind to | Today's key |
|---|---|---|---|
| Endpoint profiles | providers.rs:245-251 | the `e` handler body (synthetic-profile refusal included) | providers.rs:68-83 |
| Routes | routes.rs:147-153 | `edit_selected` (non-editable refusals included) | routes.rs:36-41 → 163-189 |
| Users | users.rs:462-468 | `open_user_form(edit)` | users.rs:115-123 |
| Entities | users.rs:509-514 | `open_manage_menu` (the row's action hub; `i` stays the glance verb) | users.rs:73-81 |

Notes:
- Selection signals already exist on all four (`.selection(sel)`), so
  the callback body is `move |_| <same fn as the key>` — the row index
  you need is the clamped selection signal you already maintain
  (util.rs:106 `clamp_selection`).
- Footer hints (ui/mod.rs:1005-1037) can stay as-is; if you touch them,
  "Enter/e" on Routes (mod.rs:1015) becomes true for every bound table.
- Deliberate non-adoptions, agreed from the engine side: the **runs
  table** (runtimes.rs:268-278) has no unambiguous "open" (`c` cancel /
  `s` steer are decisions, not openings); the modal tables
  (reservations users.rs:272-284, data homes runtimes.rs:444-456,
  candidates entity_manage.rs:1600-1609) act through explicit buttons
  with danger confirms — wiring activation to a destructive verb would
  violate your own danger-confirm law. Skip all four.

### P1.3 R4: guard `line_styled` (and the `field_w` label closure)

- Yours: ui/util.rs:34-54 — the draw closure clips to `rect.w`
  (`x >= right`, `fit_width`) but never checks height; same shape in
  the label closure at util.rs:92-95. This is the exact closure class
  your 1030 named ("clips on one axis only").
- Engine: wave10 recipe **R4** (reviews/wave10/size-ratio-sweep.md §3):
  the engine now makes the zero-area case unreachable (G1), but a
  PARTIALLY crushed rect still reaches the closure with less than it
  asked for, and closures own their rect's interior.
- One-liner, once, covering every `line()`/`line_styled()`/`field()`
  row in the app: `if rect.is_empty() { return; }` at the top of both
  closures. On your current 0.2.12 this also closes the remaining
  fusion exposure for every UNPINNED line row until the bump lands.

### P1.4 SVG evidence from the tests you already have

- Engine: `render::Screenshot` (0.2.14) — in your harness it is
  `self.term.screen().screenshot()` (the VtScreen side: what the
  emitted bytes actually produced) with `to_svg()`/`write_svg(path)`;
  deterministic, GitHub-renderable (docs/api.md § "Screenshots &
  captures"; the headless-test recipe is verbatim there).
- Yours: the harness already exposes the surface
  (tests/headless_ui.rs:94-99, `term.screen().to_text()`), so each
  capture is one added line. The scenes worth minting, matched to the
  operator's "tests, screenshots and reports" mandate:
  - `title_bar_and_separator_survive_content_pressure`
    (headless_ui.rs:1885) — the 12-cell chrome-pinning proof; this is
    the reproducible version of the operator's original screenshots.
  - `connection_states_render_distinctly` (:404) — the honest-states
    gallery.
  - `routes_table_renders_states_distinctly` (:766) and the route
    editor mid-flow (:835).
  - `users_and_entities_render_with_admin_gate` (:1010).
  - `runtime_knobs_render_with_provenance` (:1796).
  - `narrow_terminal_keeps_payload_columns` (:1626) — the width story.
- Suggested shape: an env-gated helper (`CONSOLE_SHOTS=dir` → write
  `.svg` per scene, else no-op) so CI stays quiet by default; engine
  precedent is the `capture` example emitting `.svg` beside every
  still in `docs/captures/`.

### P1.5 The dead `s` on Runtimes (both-sides item)

- Yours: runtimes.rs:42-50 binds `s` = steer, footer-advertised
  (mod.rs:1031). The runs table is focusable (engine 0.2.12
  table.rs:269), and a focused Table consumes `s` with `ncols > 0`
  even with NO `on_sort_requested` bound (0.2.12 table.rs:194-206 —
  `stop_propagation` sits outside the handler check). Repro: Tab into
  the runs table to pick a run, press `s` → the key vanishes; the
  steer form never opens. This is your own finding 0980, and the
  cycle-1 workaround it recorded ("the console avoids `s`") eroded
  when 0.2.0 added steer.
- Engine, honestly: **0980 is still unfixed at HEAD** — current
  table.rs:246-258 has the same unconditional `stop_propagation`; the
  0.2.16 work applied the "never claim a key without a consumer" rule
  to the NEW Enter/Space arm but did not retrofit `s`. The engine seat
  owes you this fix (move the claim inside the bound-handler arm) and
  will treat 0980 as accepted; it does not require a semver bump.
- Until it ships: either re-key steer off `s`, or accept the
  dead-window and rely on the runtimes-table-focused path — but given
  your F2/F3 "a refused action always says why" law, a silently eaten
  footer-advertised key is exactly the class you hunt; we recommend
  waiting on 0.2.17 rather than re-keying, and we will flag you when
  it lands.

## P2 — worth planning

### P2.1 Bound the runtime-knobs block (Disclosure or Scroll)

- Yours: runtimes.rs:112-137 pins the knobs Block `shrink(0.0)` while
  `knobs_view` (runtimes.rs:143-176) renders one fixed row PER KNOB —
  unbounded server-fed data inside incompressible chrome. A knob-rich
  gateway starves the two tables above it (engine G5 clips pinned
  chrome honestly in document order, but the growing tables die
  first). Your own 1020 lesson applies: the trigger is data volume,
  and light fixtures never see it.
- Engine: `Disclosure` shipped in 0.2.11 — already inside your pinned
  0.2.12, no bump needed (docs/api.md § "Disclosure — the fold/unfold
  card"): title row + `max_body_rows(8)` caps the body behind an
  auto-hiding scrollbar; folded by default costs zero rows beyond the
  header. Alternatively wrap `knobs_view` in a `Scroll` with a max
  height (recipe R2). Disclosure reads better here: knobs are
  reference data, not working data.

### P2.2 Locked-tab interim: PageHost badges (until 1010 ships)

- Yours: finding 1010 (wizard gated steps lost the greyed-future-step
  look; you accepted the gap — 1010 "Workaround shipped meanwhile:
  none").
- Engine, honestly: `page_state(id, Locked)` is NOT shipped and not
  yet scheduled; the ask is recorded and considered sound (small
  surface, presentation-only — see "what the engine owes you" below).
- Interim available today in 0.2.12: `.badge(id, getter)` is reactive
  (docs/api.md § widgets::PageHost — "badges are reactive getters —
  a count change repaints the bar only"). A getter returning
  `Some("locked".into())` for gated steps in wizard mode restores a
  visible affordance at the bar (info-tinted, not the text_faint you
  asked for — stated so you can judge whether the interim is worth
  it, or whether digit-refusal-with-reason remains the better teacher
  until the real per-tab state lands).

### P2.3 Discovered-providers table: activation as models drill-in

- Yours: the read-only discovered-providers table
  (providers.rs:279-290) has NO row verb today; your models drill-in
  (`open_models_modal`, providers.rs:319) is reachable only through a
  selected PROFILE (`m`, providers.rs:100-108).
- One-liner after the bump: `.on_activate(move |i| open_models_modal(
  cx, &ctx, items[i].name.clone()))` — Enter/double-click on a
  discovered provider answers "what models does this serve", the most
  common question a read-only inventory row gets. New verb, so add a
  footer hint if you take it.

### P2.4 Live screenshot verb for operator evidence

- Engine: `app::request_screenshot` (0.2.14) — component-reachable,
  served the same turn, no default hotkey by design; the binding IS
  the recipe (docs/api.md § "Screenshots & captures").
- Yours: one more root shortcut beside Ctrl+L (ui/mod.rs:547), e.g.
  writing `/tmp/gateway-console-<ts>.svg` and announcing the path
  through your notice lane (store.notice → toast + footer, the same
  acknowledgment discipline your probes use). Live evidence for
  operator reports without reaching for the terminal emulator's
  screenshot.

## P3 — optional

### P3.1 Remove the PHASE_SLOTS / LAYER_SLOTS caps

- Yours: entity_manage.rs:1107-1110 (`PHASE_SLOTS: usize = 6`) and
  :1374-1376 (`LAYER_SLOTS: usize = 4`), with honest ">N not editable
  here — use the web console" refusals (:1216-1224, :1470-1478). The
  stated rationale — "signals must live in the MODAL scope (region
  scopes die on re-render), but the phase list arrives async" — is
  true, but the fixed budget is not engine-mandated: `Scope` handles
  are `Copy` and `Scope::signal`/`TextAreaState::new` may be called
  at any time while the scope lives, including inside your one-shot
  fill effect (engine src/reactive/scope.rs:45-57 — creation checks
  thread affinity only, and state parents to the scope regardless of
  when it is created). Build the `Vec<Signal<...>>` in the fill
  effect from the captured `mcx`, store it in the `Rc<RefCell<...>>`
  you already use, and the cap disappears.
- Optional because live gateways currently report few phases/layers;
  it becomes real the day one exceeds the budget.

### P3.2 `KeymapHelp` (`?`)

- Engine: a ready-made help modal listing shortcuts reachable from
  the current focus plus registered global actions (docs/api.md §
  Hooks/KeymapHelp). Your footer hints truncate on narrow terminals
  (you fixed the ordering in 0.3.3 so quit survives); a `?` overlay
  would carry the full table without fighting for footer columns.
  Optional — your refusal notices already re-teach lost verbs.

### P3.3 Non-adoptions we endorse (no action)

- **`reactive::connection`**: your 0950 analysis stands — the console
  is probe-shaped (operator fixes the URL/token; auto-retry against a
  wrong token would hammer 401s), and your five-variant `ConnPhase`
  is the right size. The engine keeps 0950 as adoption-fit evidence;
  a probe-class helper waits for a second consumer.
- **TimeSeries / Meter**: the console fetches no time-series today
  (the busy strip is op-elapsed text; `last_probe.took_ms` is a
  scalar). Nothing to chart honestly. If you ever add a polled stats
  surface (run counts, probe latency history), the engine owns the
  ring + relative time axis (`TimeSeriesState`, `time_axis` — 0.2.3)
  and the level ballistics (`Meter`); do not hand-roll those then.

## Graph family fit (0.2.13) — verdict: do not adopt

We looked for an honest `GraphView`/mermaid surface and did not find
one:

- **Routes** carry real relations (`derived ← key`, `covered by key` —
  routes.rs:113-124) but they are strictly one hop, already rendered
  in the state column, and your edit refusals name them in prose
  ("derives from X — edit that route instead", routes.rs:177-187). A
  routes-topology graph would re-state a legible table; fan-in
  ("provider ← N routes") is a count, not a topology.
- **Providers → models** is a list drill-in (open_models_modal);
  **runtimes/owners** and the **entity roster** are flat inventories —
  this console deliberately excludes the surfaces where entity
  relationships live (its own scope law: "creation/summon/visits stay
  outside", users.rs:172).

The engine's own bar applies: a table that works beats a graph that
decorates. The one future exception: if the console ever grows a
fleet/topology surface (entities ↔ shared projects ↔ runtimes — the
direction the gateway's topology plan points), `abstracttui-graph`'s
layered/force passes and `GraphView` are exactly that tool
(docs/graphs-and-diagrams.md; `abstracttui-graph` 0.1.0 on the public
API, no core bump needed). `abstracttui-mermaid` is for
markdown-carried diagrams; this console renders no markdown. Until
such a surface exists, adopting the family here would be decoration,
and we would rather you not.

## What the engine still owes YOU (honest status)

| Finding | Status at engine HEAD (0.2.16) | Note |
|---|---|---|
| 1030 zero-area rects paint (fusion) | **FIXED** in 0.2.15 | Your one real engine ask from the title-bar incident; guarantees pinned G1-G4 (wave10 §1). |
| 1020 fixed rows shrink to zero at root | Recipes shipped as docs (R1/R3 in api.md § "Small terminals & content pressure"); your withdrawn default-flip ask honored | Diagnostic notice + your footer lane close the loop. |
| 0980 Table consumes `s` without a sort handler | **STILL OPEN — the engine's miss.** The 0.2.16 Enter/Space arm adopted your rule; `s` was not retrofitted (table.rs:246-258). | Accepted; fix planned (claim `s` only when `on_sort_requested` is bound). See P1.5 — you have a live dead-key until then. |
| 1000 modal dead-keys window (async focusables) | **NOT shipped** (neither the implicit layer-root focus fallback nor the loud no-focus-owner debug note). | Your `.focusable().autofocus()` content-root workaround is the documented pattern, and this survey verified you apply it consistently: the three async-only-focusable modals carry the content-root pattern (entity_manage.rs:1163-1166, :1425-1428, :1573-1576), and every other modal in the app was audited to have an at-mount focusable (autofocused widget or always-present button — e.g. providers.rs:586/:603, routes.rs:664, users.rs:663, runtimes.rs:347, review.rs:320, entity_manage.rs:278/:407/:534/:681/:819/:1032, plus the button rows of the models/token/reservations/data-homes modals). The debug-note ask is the cheaper half and the likelier first ship. |
| 1010 PageHost locked-tab presentation | **NOT shipped**, not yet scheduled; ask recorded as sound (presentation-only, badge machinery shows the shape). | Badge interim available today (P2.2). |
| 0900 fixed columns starve flex | Open | Your width-aware column drops (providers.rs:182-244, routes.rs:104-146) remain the recipe. |
| 0905 same-value re-commit unobservable | Open | Your explicit Retry buttons (routes.rs:812-825, review.rs:390-403) remain correct. |
| 0910 shortcuts off the focus path | Open (documented behavior) | Your screen-root shortcut placement is the pattern. |
| 0920 input-immune wizard keys | Partially answered: PageHost container chords (Capture phase) are that lane in browse; wizard Ctrl+N/P as root shortcuts remains yours. | No further change planned. |
| 0930 reactive disabled | Open | Your dyn_view-region button rebuilds remain the recipe. |
| 0940 modal content close handle | Open | Your `CloserFn` slot (ui/mod.rs:244-247) remains the pattern. |
| 0945 ChoicePrompt same-z stacking | Open | Your `open_prompt` count + token-queue gating (ui/mod.rs:262-275, :784-811) remains load-bearing — do not delete it on bump. |
| 0950 connection is stream-shaped | Answered as evidence: correct non-adoption; probe-class helper deferred to a second consumer. | No action either side. |
| 0960 draw closures paint past rect | Documented (damage contract); R4 is the recipe | See P1.3. |
| 0970 selection not clamped on shrink | Open | Keep `clamp_selection` (util.rs:106-122). |

Also owed engine-side (found by this survey, not by you): the missing
`[0.2.15]` CHANGELOG heading (its entries sit under `[0.2.16]`).

## Verified good (so it stays that way)

- R1 pins are complete where they matter: header (mod.rs:884),
  separator (:677), footer rows (:943-985), message_slot (:410),
  sandbox/route-test result slots (review.rs:95/:430, routes.rs:974),
  button rows across all forms. The Users-screen hint line
  (users.rs:196-199) is unpinned — after the bump that degrades to
  clean absence under pressure, which for a hint is an acceptable
  loss; noted, not asked.
- R3: the footer renders the LAST engine startup notice whenever your
  own notice lane is idle (mod.rs:938, :949-968) — exactly the shape
  the recipe asks for.
- Autofocus inside regenerating regions (your region-hosted tables at
  providers.rs:250, routes.rs:152, users.rs:283/:467, runtimes.rs:241/
  :455, entity_manage.rs:1608) is safe: the 0220 panic was fixed in
  0.2.0 (engine CHANGELOG, "autofocus inside a regenerated dyn_view
  subtree no longer panics") — your usage is the endorsed pattern, not
  a latent hazard.
- The worker/WakeHandle threading contract, the write→verify→journal
  law, the fabricated-selection law in the route editor, and the
  Esc-guard/message-slot consolidation all read as the engine's
  documented idioms applied correctly; nothing stale found.

## Suggested bump gate (your own discipline, restated)

1. Pin 0.2.16, `cargo update -p abstracttui`, build.
2. `cargo test` (headless suite incl. the 36-cell chrome matrix) —
   expect green with zero edits.
3. Bind the four `on_activate`s (P1.2) + the R4 guard (P1.3); re-run.
4. pty smoke against a live gateway; double-click the profiles and
   routes tables by hand once — the second press must open, a slow
   re-click must not.
5. Mint the first SVG set (P1.4) and attach it to the bump report —
   the operator's mandate, now reproducible from fixtures.

— the AbstractTUI engine seat, 2026-07-25
