# Agora UI Plan From `abstracttui`

Verified on Monday, August 10, 2026.

## Scope

This file is the `abstracttui` package-local contribution to the Agora UI
discussion. `abstracttui` is the Rust terminal UI engine under `agora-tui`.
It is not the home for browser code, web packaging, or the standalone Agora web
product shell.

## Boundary Assumptions

- The standalone web product boundary should be a dedicated `agora-wui`
  package.
- `agora-tui` should remain the Rust TUI package.
- `abstracttui` should remain the terminal engine below the TUI, not a shared
  web/TUI umbrella package.
- Cross-surface sharing should happen at the protocol and interaction-contract
  level, not by forcing browser runtime code into this crate.
- The first web move should prove honest standalone install/runtime packaging
  before a larger source migration or package rename.

## What `abstracttui` Already Contributes

- Transcript primitives: `Feed`, `FeedState`, keyed sync, bounded live-data
  sources, and `Scroll::follow_tail`.
- Folded-card behavior for message-style UIs: the documented `Feed` +
  `Disclosure` recipe in `docs/live-data.md`.
- App chrome that Agora-style clients already rely on: `PageHost`, `Drawer`,
  `Scroll`, `TextArea`, completions, badges, and rich text.
- Headless capture-driven tests that can pin real interaction behavior without a
  live terminal.

## Package Stance

`abstracttui` should help the Agora UI family by keeping the terminal-side
interaction model stable and by deleting field-proven TUI defects. It should
not absorb web-shell ownership or become a mixed browser/terminal package.

In practice that means:

1. `agora-wui` owns browser packaging, auth/bootstrap, and the standalone web
   shell.
2. `agora-tui` owns terminal product behavior.
3. `abstracttui` owns the terminal engine primitives and the TUI-side
   interaction guarantees that `agora-tui` needs.

## Workstreams For This Package

### 1. Keep the transcript contract stable

The TUI family already has a real message-card contract:

- folded one-row card headers;
- unfolded header + body blocks;
- app-owned fold state;
- keyed item sync;
- bottom follow-tail until the user reads scrollback.

This is the documented shape in `docs/live-data.md`, and it should remain the
terminal reference surface for Agora-style transcript UX.

### 2. Close the `field-agora` gaps that still distort real Agora behavior

These findings came from the live `agora-tui` build and are the highest-value
engine work for the Agora UI family on the terminal side:

- `0895_bound_scroll_offset_ignored_inside_drawer_pages.md`
  Bound `Scroll::offset_y` must work inside `Drawer` pages. Today keyboard-first
  reader panes need an app-side workaround and lose the native scrollbar.
- `0890_disclosure_capped_body_undermeasures_rich_feed_items.md`
  Rich disclosure bodies clip under `max_body_rows(n)`, which breaks honest
  in-card scrolling for long hub reports.
- `0900_completion_panel_needs_a_reserved_row_or_offset.md`
  Completion panels occlude the row above a bottom-docked composer, hiding the
  destination/status line at the exact moment the user needs it.
- `0905_drawer_vertical_insets_for_docked_chrome.md`
  Drawers need top/bottom insets so compose chrome and status feedback stay
  visible while reference panels are open.
- `0910_scroll_of_widgets_needs_ensure_visible.md`
  Widget-scroll columns need an ensure-visible/readback seam so apps stop
  re-deriving card heights by hand.

These are the concrete `abstracttui` asks that matter most for `agora-tui`.
They also produce interaction fixtures that the web shell can compare against,
even though the browser implementation will be separate.

### 3. Produce parity fixtures, not browser code

This package should contribute reusable acceptance scenarios, not a shared web
runtime:

- long message/report cards that fold and expand cleanly;
- keyboard-scrolled reader drawers;
- command completion above a bottom composer;
- ensure-visible behavior for selection in mixed-height card columns;
- follow-tail versus user-held scrollback.

The deliverable is headless test evidence and documented behavior, so other
surfaces can intentionally match or intentionally differ.

### 4. Keep package boundaries clean

This crate should not take on:

- browser auth/session code;
- same-origin hub proxy code;
- Vite/React packaging;
- umbrella renaming such as `agora-tui` -> `agora-ui`.

Those decisions belong above this engine.

## Recommended Sequence

1. The web seats prove the standalone `agora-wui` shell and packaging/runtime
   honesty first.
2. In parallel, `abstracttui` keeps deleting the concrete `field-agora`
   defects above so the TUI reference experience stays strong.
3. Once the web shell is real, compare it against the terminal interaction
   fixtures and decide where deliberate divergence is acceptable.
4. Keep package names separate unless a later repo-level move has a concrete
   operational payoff.

## Outputs Expected From `abstracttui`

- Keep `plan/agora-ui.md` current as the package-local position.
- Use the `field-agora` track as the live ledger for TUI-side Agora findings.
- Prefer targeted engine fixes plus tests over broad branding or packaging
  changes in this crate.
