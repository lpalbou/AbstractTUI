# Wave 3 — CLOSER handoff (cycle-3 close)

Date: 2026-07-23. Owner: CLOSER. Scope closed: the cycle-2 review
demands (`reviews/wave3/review-cycle2.md` C-1/C-2/C-3/C-4+C-6/R-3/I-2,
the 0300 renumber, the FIXNET lint verification, the final battery).
Code-side docs (rustdoc, backlog completion reports, CHANGELOG
Unreleased) are done; the items below are for DOCS (this cycle's
README/docs/*.md/llms/examples-README owner).

## For api.md § "Feed — syncing from a `Signal<Vec<T>>`" (~line 326)

Three sentences to fold in (the rustdoc twins are already in
`src/widgets/feed_sync.rs` — keep the wording aligned):

1. **C-3 (the review's exact ask — the rebuild-storm cost)**: "A
   source that reorders on every drain rebuilds on every drain — a
   rebuild re-renders every visible item, so for feeds ordered by
   mutable rank, sync a stable order and sort at render time, or
   accept O(visible) per change."
2. **C-2 (NaN fingerprints)**: "Float fingerprints must compare by
   bits (`f32::to_bits`) — NaN never equals itself and re-renders the
   item every drain."
3. **C-1 (the one-writer sentence changed semantics)**: api.md
   currently says "A synced feed has ONE writer (the bridge)" — still
   the contract, but violations are no longer silent/permanent: the
   bridge now DETECTS foreign writes (mutation counter) and SELF-HEALS
   at the next drain with a full rebuild (stray items evicted, order
   restored to source order). Worth one clause so readers don't
   design around the old permanent-desync behavior.

## api.md § Charts (~line 370) — no correction NEEDED, one optional

The existing sentence ("Missed slots pad with `NAN`, so a sampling
pause draws as a HOLE ... instead of compressing the x-axis") is now
MORE true: the old `missed >= capacity` restart (which collapsed to a
zero-span dot) is deleted — padding caps at `capacity - 1` per push,
so any pause ≥ the window shows a full window of hole ending in the
fresh sample. Optional one-liner if you touch the section anyway.

## Optional (I-2, new public API a doc pass may want to name)

- `Driver::suspend(app, term)` — the orchestrated job-control suspend
  (key-state drain → `Terminal::suspend` → resume re-sync). The
  api.md mouse-capture platform note (~line 863: "suspend again after
  resume if you keep it off") still holds; `Driver::suspend` is now
  the endorsed entry point over calling `Terminal::suspend` directly.
- `KeyState::suspend_cleared()` and `StopReason::Suspended` — the
  suspend twins of `focus_cleared()`/`FocusLost` (PTT stops in every
  mode before the stop signal).

## Backlog renumbers (already done — for your cross-reference sweep)

- first-app **0300 → 0299** (full-redraw verb) and the mid-close
  filing **0310 → 0291** (textarea placeholder-while-focused): both
  collided with control-plane's band. If any docs/README text you own
  references those filings by the old ids, the new ids are 0299/0291.
  All backlog-side rows/notes are updated.

## Heads-up: a parallel writer is filing into docs/backlog

The 0310 textarea filing (and a v2 rewrite of the 0299 file's
workaround section) landed at 04:46–04:47 local, mid-close — a field
agent (abstractcode-tui lane, by the content) is actively filing. If
more first-app filings appear: the decade ids 0220–0299 are now all
taken, but non-decade ids in-band are fine and plentiful (0281–0289,
0271–0279, … — the 0291/0292/0294 precedent). Ids at 0300+ belong to
control-plane; that collision class must not recur.
