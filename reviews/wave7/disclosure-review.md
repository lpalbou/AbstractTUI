# Disclosure wave — adversarial review (2026-07-24)

Reviewer pass over the disclosure wave (first-app 0260 + field-agora
0850: `Disclosure` widget, `Feed::on_item_press` + `FeedState::item_at_row`,
`Scroll::extent_signal` + `Scroll::scrollbar_auto_hide`, the
message-card recipe). Attack file: `tests/wave_disclosure_review.rs`
(15 tests, all green after fixes). Builder handoff:
`disclosure-handoff.md` (same directory).

## Verdict: SHIP

Two real defects found and fixed in-wave (both edge-of-envelope, both
now test-pinned); one harness hole in the builder's own damage test
closed by a stronger model that CONFIRMS the containment claim; the
completion reports verify against the shipped code with one wording
overstatement in 0850's consumer ledger (documented below, no code
consequence). Every gate re-run green, semver additive vs 0.2.10.

## Findings

### F1 (P2, FIXED): header paints outside a 1-cell rect

`draw_header` printed the fold glyph at `rect.x + 1` unconditionally.
Draw closures are NOT clipped to their element's rect (`ui/draw.rs`
passes the damage-clipped canvas straight through — clipping exists
only for `clip_overflow` CHILDREN), so a Disclosure mounted in a
1-cell-wide slot painted `▸` into its right neighbor's cell — a
damage-contract §5 violation (byte custody: paint only your rect).
Fix: the glyph print is gated on `rect.w >= 2` (at width 2 the glyph's
cell is the rect's last column — legal). Pinned by
`header_paints_only_inside_its_rect_at_pathological_widths`, which
also pins: CJK detail drops whole before crushing the title below 4
cells, CJK titles ellipsis-truncate by cluster width, wide details
render whole and in-rect at roomy widths, and `text::width("▸") == 1`
(the ambiguous-width chevron rides the deliberate U+2500..=U+25FF
structural exception in `text::is_risky_cluster` — the header math is
safe exactly as long as that policy stands, hence the pin).

### F2 (P3, FIXED): zero-row body stands at the full cap forever

The capped region's `style_signal` treated `measured <= 0` as "not
yet measured" and held the region open AT the cap. But `(w > 0, 0)`
is a REAL measurement (an empty `.body(..)` build — the probe runs
even for zero-height rects), so an expanded empty body rendered
`max_body_rows` rows of blank, permanently — "padded to", violating
the widget's own documented "limited to, never padded to" rule. Only
`(0, 0)` is the unmeasured sentinel — the exact reading Scroll's
offset-repair effect already uses one file over; the two now agree.
Fix: measured zero takes the existing 1-row floor. Pinned by
`empty_build_body_settles_to_one_row_not_the_cap`.

### F3 (P3→verified-sound, harness strengthened): the CUP-only damage parse

The builder self-flagged it: `tests/wave_disclosure.rs::cup_rows`
parses only absolute `ESC[..H`, but the presenter ALSO moves
relatively (`ESC[nA`/`B` on a shared column, CR, CUF/CUB — see
`Presenter::move_cursor`), and the very first motion of a captured
frame resolves against the PREVIOUS frame's bottom-left park, not
home. The shipped test would have passed even if damage had leaked
upward through relative motion. The review adds
`toggle_damage_containment_under_a_full_cursor_model`: a full cursor
model (CUP/HVP, CUU/CUD/CNL/CPL, CHA/VPA, CR/LF/BS, EL/ED touched
rows, DECSTBM+SU/SD region rewrites, OSC skip) seeded at the park row.
Result: the containment claim HOLDS — the live stream's opening
`ESC[9A` lands exactly on the card's header row (row 2), every touched
row ≥ the card's band, across an unfold AND a re-fold. The builder's
test stays (absolute re-anchors are the common path); the review test
is the one that would catch a relative-motion leak.

### F4 (P3, wording — no code change): 0850's "DELETE the workaround" bullet

The 0850 completion report's consumer ledger says «DELETE the "no
click path" workaround — wire `on_item_press`». Read against
agora-tui's actual code (`src/ui/panes.rs` + `src/ui/mod.rs`,
read-only): there IS no click-workaround code to delete — the filing
named a missing capability ("hit info the app cannot reach today"),
and the app simply lacks the gesture. What lands is an ADDITION
(~10–15 lines: `.on_item_press` gated on `row_within_item == 0`,
feeding the existing `toggle_fold` path), not a deletion. Everything
else in that ledger is accurate — see the consumer-fit section.

## Charter attacks that found the engine sound (all now pinned)

- **Warm start**: a re-expand opens at the LAST measured height on its
  first frame (the durable extent signal outlives generations); the
  cap-flash exists only on the first-ever expand — exactly as the
  handoff claims.
- **Toggle storms**: 5 toggles with zero draws between, then 20 with a
  draw each (arming stale probe publishes against disposed
  generations) — geometry lands exact, the loop quiets, stale
  `after(0)` publishes stay coherent against the durable signal.
- **Theme switch mid-settle**: a theme-tracked rebuild disposes the
  generation while its probe publish is still armed; the timer's
  `try_get_untracked` guard keeps it inert, the fresh generation
  re-measures, controlled fold state survives. Uncontrolled state
  resets by design (documented: durable state belongs app-side).
- **Dispose with a pending publish**: same guard, whole-card dispose.
- **External fold mid-scrollbar-drag**: the capture target dies
  mid-gesture; orphaned Drag/Up events neither panic nor steer;
  re-expand gets a fresh per-expansion offset and a working wheel.
- **The folded-zero-cost claim under a timer**: an `interval` created
  on the body's GENERATION scope dies with the fold —
  `next_timer_deadline()` is None after folding (zero idle wakeups),
  and the tick counter freezes. (An interval created on an OUTER scope
  keeps ticking — that is the documented app-side-durability contract,
  not a defect.)
- **Maintainer's literal spec boundary**: content EXACTLY at the cap
  fits — natural height, no scrollbar pixels (auto-hide boundary is
  `content <= viewport`); overflow shows the thumb (pixel-asserted in
  unit + wire tests).
- **A11y truth**: external signal writes (no gesture) flip the
  reported "collapsed"/"expanded" value; a body-less card still
  toggles its glyph honestly.
- **Disposal law, keyboard path, controlled mode**: Enter →
  write-then-callback → dispose inside `on_toggle`; the app-owned
  signal already holds the new state.
- **Press math**: through a scrolled viewport (offset + WRAPPED rows),
  in a fixed-box feed (head mapping, in-box void silent), across
  `clear()` + rebuild (old geometry forgotten, new mapping exact,
  cleared feed silent), and at `gap(0)` (no dead rows). Last row maps;
  gaps and void never round to a neighbor.

## Completion-report verification

Checked every claim in both filings' reports + the handoff against
the tree:

- API surface: `Disclosure` (+ prelude export), `Feed::on_item_press`,
  `FeedState::item_at_row`, `Scroll::extent_signal`,
  `Scroll::scrollbar_auto_hide` — all present, semver-additive
  (196/196 checks pass vs published 0.2.10).
- Docs: api.md Disclosure section + Feed item-press section + "The
  message-card recipe"; live-data.md points at them. CHANGELOG carries
  the wave under Unreleased (now also the two review fixes).
- Backlog: both filings live in `completed/` with dated reports; the
  field-agora README carries 0850 in a Completed section;
  overview.md deliberately untouched (handoff provides the rows).
- "Typeset once, kept across folds" for text/markdown bodies: verified
  by code-read — the one-item FeedState lives on the MOUNT scope;
  re-expansion rebinds equal tokens and equal gap, so
  `retypeset_all` never re-runs (typeset happens once per
  width/theme, not per expansion).
- Deferral honesty: the Feed-NATIVE card kind is consistently
  declared future work behind 0280's draw-only block boundary, in
  both reports and api.md.
- File discipline: feed.rs 562 (painter split to feed_draw.rs 115),
  scroll.rs 522 + scroll_extent_tests.rs 123, disclosure.rs 516
  post-fix / disclosure_tests.rs 532. The review file (1,022 lines)
  follows the review-file precedent (wave_choice_review.rs, 1,231).
- Builder gate numbers reproduced exactly: their 1,775 = today's
  1,790 minus this review's 15.

## Consumer fit (the two-filings test)

**abstractcode-tui (0260)**: fits as claimed. Transcript sections as
standalone `Disclosure::text/markdown` cards replace the global
Ctrl+D arm per-item; the height-remeasure bookkeeping the filing
complained about (measure/build split updated in two places per item
type) is engine-owned in the capped region. No blocker found.

**agora-tui (0850)**: honest fit, near-zero deletion. The recipe
RATIFIES their existing ~60 lines (fold map `Signal<HashMap<key,bool>>`
+ tail policy, `(rev, expanded)` sync fingerprint, folded/expanded
`render_item`, Enter-on-selection toggle) rather than deleting them —
the reports say so plainly. The one genuinely new capability is
click-to-toggle: `on_item_press` gated on row 0 wired into their
existing `toggle_fold`, an ADDITION of ~10–15 lines (F4: nothing to
delete — the filing's gap was an absent gesture, not workaround
code). The standalone `Disclosure` widget has no consumer surface in
the watcher today (ui = header/sidebar/panes, all feed- or
rich-line-based); it becomes relevant with their next detail-drawer /
settings surface. The filing's header promise ("delete when this
lands") is NOT achieved for the in-feed lane — deliberately, behind
0280 — and both completion reports state that deferral honestly.
Estimate: **0 lines deleted, ~10–15 added, ~60 ratified as the
documented pattern.**

## Gates (2026-07-24, whole tree, post-fix)

- `cargo test --all-targets`: 79 suites — **1,790 passed / 0 failed /
  62 ignored** (includes the 15 review tests).
- `cargo test --doc`: **47 passed / 0 failed / 38 ignored**.
- `cargo clippy --all-targets`: **0 warnings**.
- `cargo fmt --check`: **clean**.
- Alloc pins serial (`--test alloc_budget -- --test-threads=1`):
  **10/10**.
- `cargo semver-checks --baseline-version 0.2.10`: **196 checks —
  196 pass, 57 skip, "no semver update required"** (additive).

## Files touched by this review

- `tests/wave_disclosure_review.rs` — NEW, 15 tests.
- `src/widgets/disclosure.rs` — F1 glyph gate, F2 sentinel fix.
- `CHANGELOG.md` — Fixed subsection under Unreleased.
- `reviews/wave7/disclosure-review.md` — this file.
