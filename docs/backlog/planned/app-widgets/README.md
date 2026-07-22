# App-grade widgets + API honesty backlog track

## Status
Planned (not started). Numbering band: 0100–0290 (gaps deliberate; the
0010–0090 band belongs to the live-data track, referenced but never
authored here).

## Purpose
AbstractTUI 0.1.0 is a published, test-pinned terminal-UI **engine** —
rendering, damage, input, layout, reactivity, images, 3D, themes are done and
enforced by the default suite. What it is not yet is a foundation you can
build a large **application** on. Two independent evaluations
(`reviews/cycle11/completeness-and-code-port.md`,
`reviews/cycle11/robustness-and-chat-port.md`) audited the crate against two
concrete target applications — a coding-agent console and a hub chat client —
and converged on the same verdict: the async-ingestion and rendering
substrate is ready and better than most established stacks, but the
**content-widget layer is missing**. The sharpest form of the critique, kept
here so the track never loses it: the crate ships *only a single-line text
input*; a transcript has to be faked with `List`-of-strings or a whole-source
`MarkdownView` re-parse; and the API was frozen at 0.1.0 in the final
review cycle while the cycle before it was still removing public items —
with zero external users to have validated the freeze.

This track holds the widget items both target applications need (both
reviews found they are largely the *same* widgets) plus the API-stability
and platform-claim honesty work that must land before a 0.2 that external
applications can trust.

## Items (planned/)
- `0100_feed_transcript_widget.md` — virtualized append-only Feed of rich
  multi-block items; the #1 shared need (agent transcript == chat message
  list). Highest priority; start here.
- `0110_streaming_markdown_session.md` — `md::StreamSession`: append tokens,
  re-parse only the open tail block; closed blocks freeze.
- `0120_textarea_multiline_composer.md` — multiline composer with history
  recall, block paste, and a caret-anchored completion dropdown.
- `0130_scroll_follow_tail_and_size_query.md` — pin-to-bottom idiom +
  layout size-query so `Scroll::content_size` becomes optional.
- `0150_terminal_verbs_from_components.md` — reach `notify`/`bell`/
  `set_title`/`clipboard_copy` from component code (small).
- `0180_platform_claims_and_ci_gates.md` — Linux pty CI (or corrected README
  claim), scheduled perf/fuzz/soak gates, MSRV declaration.

## Related items (proposed/, same band)
- `../../proposed/app-widgets/0140_language_lexers.md` — stateful cross-line
  lexers (python/js/toml + diff) behind the `Highlighter` seam.
- `../../proposed/app-widgets/0160_content_selection_copy.md` — content
  selection + copy: command-copy recipe, region text extraction, opt-in drag
  selection (the "later item" 0100's non-goals defer to).
- `../../proposed/app-widgets/0165_link_hit_testing.md` — hyperlink/reference
  hit-testing through the event path (click/hover on rendered links reaches
  the app).
- `../../proposed/app-widgets/0170_api_stability_pass.md` — 1.0-track API
  audit: `#[non_exhaustive]` coverage, the two-`Style` collision, prelude
  curation, a written breaking-change budget for 0.2.
- `../../proposed/ports/` — the two port epics (0200 coding console, 0210
  a2a chat TUI) that consume this track and the live-data track.

## Dependency shape
0100 (Feed) is the trunk. 0110 (streaming markdown) feeds 0100's open tail
item; 0130 (follow-tail + size query) is how 0100 composes with `Scroll` and
should be designed together with it; 0120 (TextArea — completed
2026-07-22, now in `../../completed/app-widgets/`) is independent; 0140
(lexers) plugs into blocks 0100/0110 typeset; 0150 is independent and small.
0170 should rule on API shape **before** 0100/0130 ship public surfaces,
because those two items land exactly on the crate's own named churn points
(`Scroll::content_size`, `List` multi-row content). 0180 is independent of
all of the above.

The live-data track (band 0010–0090, separately authored:
`../live-data/` 0010–0030 planned, `../../proposed/live-data/` 0040–0060
proposed) owns the `WakeHandle`/`spawn_worker` feed pattern,
bounded/coalescing ingestion, and connection-lifecycle/transport work;
the port epics depend on both tracks.

## Reading order
0100 → 0110 → 0130 → 0120 (all four completed) → 0150 → 0180, then the
proposed items (0140, 0170) and the port epics (0200, 0210).

## Governing ADRs
None — this repository has no ADR system (docs/design/ holds design notes,
not decision records). Item 0170 proposes creating the first ADRs; until it
lands, the design notes and the reviews are the closest authority.

## Scope
Engine widgets, app-surface plumbing, CI and docs claims inside this crate.

## Non-goals
- The applications themselves (they are epics in `../../proposed/ports/`,
  built as separate crates).
- Live-data/ingestion items (band 0010–0090 — the other track owns them).
- New rendering/layout/input engine capability; every item here composes
  existing, test-pinned engine surfaces.
