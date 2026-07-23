# Wave 4 handoff — scroll/feed seat (0281 + 0283 + 0284)

Three consumer-filed items shipped, tested, and moved to
`docs/backlog/completed/first-app/` (full completion reports in-item).
This file hands the integrator the overview rows; overview.md itself
was deliberately not touched (integrator folds). None of the three ids
appear in the current proposed ledger, so the rows below are pure
additions to the completed ledger.

## Completed-ledger rows (paste-ready)

| 0281 | Scroll offset repair on content shrink: bound offsets beyond the new `max_off` clamp down when the measured extent or viewport box changes (untracked offset reads — in-range programmatic writes never touched, growth never moves a reading user, follow neither disengaged nor armed); crate-private `Element::probe_when_culled` paint-walk exemption keeps the extent probe measuring while the wrapper is fully scrolled out (the starved-probe void state — flag proven load-bearing by test) — completed 2026-07-23 (scroll/feed wave 4) | completed/first-app/ |
| 0283 | Capped preview blocks: `FeedItem::max_rows(n)` + `FeedItem::overflow_marker(f)` on the last appended Text/Rich block — post-wrap cap at the typeset width, total ≤ n rows with the marker row counted ("… (+K more lines)", `text_muted`; K recomputes on resize); streaming unaffected; hang-indent (gap 2) + rhythm knob deliberately deferred in-item — completed 2026-07-23 (scroll/feed wave 4) | completed/first-app/ |
| 0284 | TextArea/TextInput placeholder clipping: both branches in both widgets clip the hint to the interior via `truncate_ellipsis` (unfocused: `tw`; focused opt-in: `tw - 1` past the caret cell) — the widget's right stroke and neighboring cells are untouchable at any width; `tw == 1` degrades to a bare `…` — completed 2026-07-23 (scroll/feed wave 4) | completed/first-app/ |

## Follow-ups surfaced (for the integrator's triage, not folded by me)

- 0283 scoped OUT two of the item's three gaps, with reasoning in the
  completion report: hanging-indent continuations (`RichText::wrap`
  has no per-line width concept — the cheap approximation misrenders
  first lines, so it was deferred rather than shipped wrong) and
  truncate-to-width per-line ellipsis. The item's separator-rhythm
  question (tight vs separated capped bodies; the Custom→Markdown
  double-blank wart) also remains open — it is separator policy across
  all block kinds, not row capping. A future rhythm filing could carry
  all three.
- 0281's `probe_when_culled` is crate-private by design (probe-class
  need, zero public surface). If another measured widget ever starves
  the same way, reuse the flag rather than widening the cull rule.

## Gates at handoff (run on the shared tree, peer's in-flight work included)

- Whole-tree `cargo test`: green — 1236 lib passed / 0 failed (18
  ignored), all integration suites 0 failed, doctests 47 passed.
- `cargo clippy --all-targets`: zero warnings.
- `cargo fmt`: clean (verified no peer file was rewritten — mtime
  audit; their files were already fmt-clean).
- `cargo test --test alloc_budget -- --test-threads=1`: 10/10.
- `cargo semver-checks` vs 0.2.6: 196 checks pass, additive-clean
  (new public API this seat: `FeedItem::max_rows`,
  `FeedItem::overflow_marker`).

## Files touched by this seat

- `src/widgets/scroll.rs` (+ module doc §offset repair),
  `src/widgets/scroll_tests.rs`
- `src/ui/draw.rs`, `src/ui/view.rs`, `src/ui/mount.rs`,
  `src/ui/tree.rs` (the probe-when-culled plumbing, crate-private)
- `src/widgets/feed_item.rs`, `src/widgets/feed_typeset.rs`,
  `src/widgets/feed_rich_tests.rs`
- `src/widgets/textarea.rs`, `src/widgets/input.rs`,
  `src/widgets/input_tests.rs`,
  `src/widgets/textarea_placeholder_tests.rs` (new `#[path]` sibling —
  `textarea_tests.rs` is at its size budget)
- Shared, append-only edits: `CHANGELOG.md` (Unreleased: one Added
  bullet, new Fixed subsection with two bullets), `docs/api.md` (one
  new "Feed — capped preview blocks" subsection at the Feed/Charts
  boundary). `src/widgets/mod.rs` and `src/prelude.rs` untouched (no
  new exported types — the builders live on the already-exported
  `FeedItem`).
