# Disclosure wave handoff (first-app 0260 + field-agora 0850) — 2026-07-24

The commissioned fold/unfold card, built against the maintainer's
words ("a component/card with a clean title that can fold/unfold and
a customizable default state … limited to a few lines once unfolded
(configurable), and if the content exceeds the number of lines, we
should have a scrollbar") plus the two consumer filings. Both filings
now live in `docs/backlog/completed/` (first-app/0260,
field-agora/0850 — the field-agora completed/ directory is new this
wave) with dated completion reports; each is honest about what
shipped as WIDGET, what shipped as ENABLER + documented RECIPE, and
what stays deferred (the Feed-native card kind, behind first-app
0280's draw-only block boundary).

This file exists because overview.md is deliberately NOT touched by
this wave — the rows below are for its owner to fold at the next
refresh.

## overview.md rows to fold

REMOVE from the Proposed ledger (line 139 today):

| ID | Title | Track | Note |
| --- | --- | --- | --- |
| 0260 | Disclosure widget: per-item fold/unfold for transcripts (maintainer ask) | first-app | Fold into Feed's item model (0100 shipped — extend), or standalone on a second consumer. |

(The "standalone on a second consumer" promotion arm is exactly what
happened — 0850 was the second consumer.)

ADD to the Completed ledger:

| ID | Title | Final path |
| --- | --- | --- |
| 0260 | Disclosure widget — standalone fold/unfold card: `▸`/`▾` title row + muted detail slot, click/Enter/Space toggle, `initially_folded` + controlled `folded(Signal<bool>)`, `max_body_rows` cap (default 8) with auto-hiding scrollbar, folded = body unmounted (zero idle cost), text/markdown/any-View bodies; a11y region+button "collapsed"/"expanded" | completed/first-app/ |
| 0850 | Disclosure/Card for feed items — the standalone widget + `Feed::on_item_press(key, row_within_item)` / public `FeedState::item_at_row` (the click-hit-info the app could not reach) + the documented message-card recipe (fold map + `(rev, folded)` sync fingerprint); Feed-NATIVE card kind deferred behind 0280 | completed/field-agora/ |

0850 had no Proposed-ledger row in overview.md (the field-agora track
is listed as a directory, its items live in the track README) — its
row moves inside `proposed/field-agora/README.md`, already done this
wave. Count deltas: Proposed −1, Completed +2; reconcile against the
directories ("the directory is the truth").

## What shipped (one paragraph each)

**`widgets::Disclosure`** (src/widgets/disclosure.rs, prelude-exported).
One-row header — fold glyph `▸`/`▾` in `accent`, truncate-ellipsis
title, optional right-aligned `text_muted` detail slot (renders whole
or drops when under 4 title cells would remain; the title always
wins) — over a body region. Chrome is borderless two-tone (header
`surface_raised`, body `surface`): cards stack at transcript scale
where per-card borders read as noise; `Block` composes around one
when a frame is wanted. One tab stop; focus wears the selection pair
(§3.2), hover is accent garnish; click on the title row or
Enter/Space while focused toggles. State: uncontrolled
`initially_folded(bool)` (default FOLDED — progressive disclosure) or
controlled `folded(Signal<bool>)` (two-way; the signal's current
value is the state — the 0850 policy hook, demoed with an external
toggle button). `on_toggle(FnMut(bool))` receives the NEW folded
state after the write (0297 disposal law, test-pinned). Bodies:
`Disclosure::text` / `::markdown` (one-item Feed, typeset once, kept
across folds) or `.body(FnMut(Scope) -> View)` built per EXPANSION on
a generation scope — folded = UNMOUNTED (zero idle cost, pinned by
bytes), unfold REMOUNTS. `max_body_rows(n)` (default 8) caps the
region at `min(content, n)` rows via the Scroll's measured extent;
overflow scrolls with a visible scrollbar, `n <= 0` = uncapped.
A11y: `region`(title) wrapping `button` valued
"collapsed"/"expanded" (Role enum frozen till 0.3 — Select precedent).

**`Feed::on_item_press` + `FeedState::item_at_row`** (field-agora
0850's "unreachable hit info"). A left press over an item's rows
fires `(key, row_within_item)` — row 0 is the title-row gate for
click-to-toggle cards. Gap rows and the void past the tail are
silent (honest geometry, never rounded to a neighbor); unbound feeds
attach no handler (zero cost, byte-stable). `item_at_row` is the
public inverse of `row_of` (O(log n) on the prefix sums; entries now
carry their key for the reverse lookup). The state borrow ends
before the callback, so callbacks may mutate the feed reentrantly or
dispose its scope (both test-pinned).

**`Scroll::extent_signal` + `Scroll::scrollbar_auto_hide`** (the two
gaps between Scroll-as-shipped and what a capped card body needs).
Scroll ALREADY had an always-on scrollbar — the real gaps were (a) no
way to read the measured content extent (Disclosure sizes its capped
region from it; apps get "N more rows" chrome), and (b) the bar
renders a full-height thumb over FITTING content — a false
affordance on a card body shorter than its cap. `extent_signal`
binds the internal extent to an app-visible signal (measured mode
publishes the solver's answer; hint mode lands the hint verbatim;
a supplied signal's pre-existing value warm-starts remounting
callers). `scrollbar_auto_hide(true)` hides the bar while content
fits: the column stays RESERVED (painted bare ground — no width
re-wrap when the bar appears, no stale glyphs in damaged regions)
and the hidden strip ignores drags (an invisible target must never
steer). Default `false` keeps 0.2.10 behavior byte-stable.

**The message-card recipe** (api.md "The message-card recipe", pointed
at from live-data.md). The packaged form of what both validators
hand-rolled: fold state in `Signal<HashMap<key, bool>>`, `(rev,
folded)` in the `FeedState::sync` fingerprint (a toggle re-typesets
exactly the changed item), folded = one rich header line (+
`max_rows` preview), unfolded = header + body blocks, Enter on
`selected_key` + click via `on_item_press` gated on
`row_within_item == 0`. Documented WHY it is a recipe and not an
engine feed-item kind: feed custom blocks are draw-only cell
closures (0280) — no widget lifecycle/focus/handlers inside items —
so an engine-owned interactive card inside Feed waits on that
boundary. Deferred honestly, in the docs and in both completion
reports.

## Design decisions (for the reviewer)

- **Default FOLDED**: progressive disclosure — the header is the
  summary, opening is the user's act; 0850's newest-expanded policy
  is one signal write away in controlled mode. Documented in the
  module doc and the builder.
- **Default cap 8**: the maintainer's "limited to a few lines" reads
  as a behavior of the widget, not an opt-in; 8 matches
  `ChoicePrompt::body_rows`'s default. `max_body_rows(0)` uncaps
  (documented + tested); negative values clamp into "uncapped".
- **Borderless two-tone chrome** over a bordered Block: transcript
  stacking; `Block` composes around a card for frames. Tokens used
  per their documented tiers (`surface_raised` = raised chrome,
  `surface` = card ground).
- **Body remounts per expansion** (`FnMut(Scope) -> View`, the
  ChoicePrompt body precedent adapted): folded must cost zero, so
  the fold disposes the generation; durable state lives app-side.
  text/markdown conveniences keep their typeset across folds via a
  mount-scope FeedState.
- **`min(content, cap)` region** ("limited to", not "padded to"):
  sized from `extent_signal`; the first-ever expand opens AT the cap
  and settles down one turn later for short content (opening short
  would clip tall bodies, the common case under a cap) — the
  documented Scroll settle contract, called out in the module doc.
- **on_toggle arg = new FOLDED state** (true = just folded), matching
  Checkbox's on_change(new value) shape.

## Gates (2026-07-24, whole tree)

- `cargo test --all-targets`: green — 78 suites, 1,775 passed /
  0 failed / 62 ignored (lib 1,316 incl. the 21 new unit tests; the
  new `tests/wave_disclosure.rs` is 6 tests; alloc pins
  `alloc_budget.rs` 10/10).
- `cargo test --doc`: 47 passed / 0 failed (38 ignored).
- `cargo clippy --all-targets`: zero warnings.
- `cargo fmt --check`: clean.
- `cargo semver-checks --baseline-version 0.2.10`: 196 checks —
  196 pass, 57 skip, "no semver update required" — additive-clean
  (new public API: `widgets::Disclosure` + builders,
  `Feed::on_item_press`, `FeedState::item_at_row`,
  `Scroll::extent_signal`, `Scroll::scrollbar_auto_hide`,
  `prelude::Disclosure`).
- File discipline: `feed.rs` and `scroll_tests.rs` crossed 600 with
  the additions and were split (`feed_draw.rs` painter sibling —
  joined the widgets token-lint list; `scroll_extent_tests.rs`);
  `disclosure.rs` 504 / `disclosure_tests.rs` 532.
- `cargo doc --no-deps`: zero warnings (three
  `redundant_explicit_link` warnings introduced and fixed in-wave).
- `llms.txt`/`llms-full.txt` NOT regenerated — release-lane per the
  house release flow (the api.md deltas fold in at the next release's
  coredoc pass).

## Test names (new, 27)

Unit — `src/widgets/disclosure_tests.rs` (14):
`starts_folded_by_default_and_click_on_the_title_toggles`,
`initially_unfolded_shows_the_body_at_mount`,
`controlled_signal_drives_the_card_and_receives_toggles`,
`enter_and_space_toggle_only_while_focused`,
`on_toggle_reports_the_new_state_after_the_write`,
`on_toggle_may_dispose_the_disclosures_scope`,
`max_body_rows_caps_the_region_and_the_body_scrolls_with_a_bar`,
`short_body_takes_its_natural_height_and_hides_the_bar`,
`max_body_rows_zero_means_unbounded_natural_height`,
`title_truncates_and_the_detail_drops_when_the_row_is_tight`,
`folded_body_unmounts_and_every_unfold_remounts`,
`markdown_body_renders_the_doc_vocabulary`,
`access_reports_region_button_label_and_fold_state`,
`focused_header_shows_a_visible_affordance`.

Unit — `src/widgets/feed_press_tests.rs` (5):
`item_at_row_maps_rows_and_refuses_gaps_and_void`,
`item_at_row_is_none_before_the_first_draw_discovers_a_width`,
`on_item_press_reports_key_and_row_within_item`,
`press_callback_may_mutate_the_feed_reentrantly`,
`on_item_press_may_dispose_the_feeds_scope`.

Unit — `src/widgets/scroll_extent_tests.rs` (2):
`extent_signal_reports_the_measured_content_and_the_hint`,
`scrollbar_auto_hide_hides_on_fit_shows_on_overflow_and_inerts_drags`.

Integration — `tests/wave_disclosure.rs` (real Driver + wire bytes,
6): `click_on_the_title_unfolds_and_enter_folds_back`,
`capped_body_shows_a_scrollbar_and_the_wheel_scrolls_it`,
`parked_cards_cost_zero_idle_bytes`,
`toggle_damage_stays_inside_the_cards_band` (CUP-row containment —
no absolute re-anchor above the card; static rows byte-identical),
`disclosure_composes_inside_a_modal` (honest measure: the panel
footer sits at header+1 folded, header+1+body unfolded),
`feed_sgr_click_reports_key_and_row_within_item`.

Demo: `examples/components.rs` gained the Disclosure section — three
cards (initially-unfolded markdown, folded 14-line log with
`max_body_rows(4)` + scrollbar, external-signal card driven by a
"toggle 3rd" Button); headless exit-0 verified.

## Consumer follow-ups

- **abstractcode-tui (0260)**: per-item disclosure can replace the
  global Ctrl+D arm for reasoning/tool sections — standalone
  `Disclosure` outside the feed, the recipe inside it; the
  height-remeasure bookkeeping the filing complained about is
  engine-owned either way.
- **agora-tui (0850)**: keep the panes.rs fold map + fingerprint
  (that IS the recipe, now documented); DELETE the "click
  unreachable" workaround — wire `on_item_press` gated on row 0;
  `Disclosure` directly for non-feed card surfaces.
- **Engine future work**: the Feed-native card kind rides 0280
  (widget-hosting feed blocks); when that lands, the recipe's
  fingerprint dance can fold into the item model (0260's direction
  2, still on record in its completion report).
