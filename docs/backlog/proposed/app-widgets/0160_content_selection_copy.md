# 0160 — Content selection + copy (take text out of any view)

## Metadata
- Created: 2026-07-21
- Status: Proposed (needs a design ruling on the selection model's home —
  per-widget vs a screen-level layer — before it is planned)
- Track: app-widgets
- Completed: N/A

## ADR status
- Governing ADRs: None — this repo has no ADR system yet (see 0170).
  ADR impact: touches one recorded stance that must NOT change — OSC 52
  clipboard is write-only by design (the read form is an exfiltration
  vector, src/term/verbs.rs:82-86); this item is egress-only and any ADR
  from 0170 capturing that stance governs it.

## Context
Terminal users expect to take text out of anything they can see: a chat
message, a log line, an error, a command from a transcript, a cell from a
table. Viewers, consoles, chat clients, and monitors all share the need —
"content you can see but cannot extract" breaks the terminal-native
contract every one of those app classes lives by. The completeness review
files it as the console class's P1-6
(`reviews/cycle11/completeness-and-code-port.md` §2b: "command-copy
first, mouse P2"), the robustness review's gap table needs it for
copy-message (`reviews/cycle11/robustness-and-chat-port.md`, Part 2), and
0100's non-goals explicitly defer to "a later item" — this is that item.
The port epics (0200 `/copy`, 0210 copy-message) are the first
validators, not the design targets.

## Current code reality
- Egress exists and is capability-honest: `Terminal::clipboard_copy`
  (src/term/mod.rs:211) emits OSC 52 (src/term/verbs.rs:87,
  `clipboard_copy_bytes`), gated by `Capabilities::osc52_copy`
  (src/term/caps.rs:72); 0150 makes it reachable from component code.
  Write-only is deliberate and stays.
- No selection model exists over rendered content: nothing in `src/ui/`
  or `src/widgets/` models "a region of the screen/an item is selected"
  (grepped; `TextInput`'s anchor+cursor selection, src/widgets/input.rs:48,
  is internal to the editor). `List` has sticky selection by key —
  item-level identity, not text.
- Extraction seam already exists: the surface knows its cells
  (`Surface::get`, src/render/surface.rs:209) and
  `render::snapshot`/`snapshot_styles` (src/render/snapshot.rs:36)
  already walk the grid into a string, resolving pooled glyphs and
  rendering wide glyphs once at their leader — the review names this
  module as the extraction seam.
- Selection inks exist in the theme: `selection_bg`/`selection_fg`
  (consumed by TextInput, src/widgets/input.rs:156-157) — a visible
  selection needs no new token vocabulary.
- Mouse machinery (click/hover/drag with capture, tests/adv_pointer.rs)
  resolves *elements*; nothing maps a drag to a cell range.

## Problem
The engine renders text it cannot give back. Apps must either fork
widgets to add copy affordances or tell users to fall back on the
terminal's own selection — which breaks the moment the app uses the
alternate screen, mouse reporting, or panes (all three are the normal
case here). Meanwhile the safe two ingredients (item identity in 0100,
clipboard egress in 0150) exist separately with nothing joining them.

## What we want (proposed shape)
Three layers, cheapest first; each is independently shippable:
1. **Command-copy recipe (docs + example)**: selected feed item (0100's
   selection-by-key) or focused widget → app-held source text →
   0150's `clipboard_copy`. Zero new engine surface; the recipe must
   state that copying SOURCE text (markdown, not typeset rows) is the
   app's choice to make.
2. **Screen-region text extraction API**: `Surface`(or snapshot-module)
   function yielding the text of a cell rect — what-you-see semantics
   (wide glyphs once, pooled glyphs resolved, trailing pad trimmed,
   rows joined with `\n`), built on the snapshot walk. This is the
   engine half any selection UI needs, and it is useful headlessly
   (tests, tools) on its own.
3. **Opt-in drag selection**: a widget/app-shell affordance mapping
   mouse drag to a cell range, rendered with the selection inks, with
   copy wired through 0150. The design ruling this item waits on: does
   selection live per-widget (Feed items know their text) or as a
   screen-level layer over the composed frame (works across panes but
   crosses widget boundaries)? The reviews deliberately rank this last.

## Scope / Non-goals
Scope: the recipe/docs, the extraction API, the ruling + v1 of drag
selection, tests. Non-goals: clipboard READ (never — the write-only
stance is security posture); semantic multi-item copy with app-defined
formatting (apps own source text; layer 1 covers it); terminal-native
selection passthrough modes; selection persistence across frames beyond
what the owning widget/layer holds.

## Expected outcomes
Any app can offer copy-message/copy-region without forking widgets; a
headless test can assert extracted text instead of screen-diffing; the
console's `/copy` and the chat client's copy-message are one-liners over
layers 1–2, and drag selection arrives once, engine-side, for every app
at once.

## Validation
- Extraction unit tests: wide-glyph (CJK/emoji) rects, pooled clusters,
  trailing-space trim, multi-row join — asserted against
  `snapshot`-rendered ground truth.
- CaptureTerm acceptance (layer 3): drag paints selection inks; copy
  emits exactly one OSC 52 sequence with the extracted bytes; caps-off
  degrades with a labeled notice, never silently.
- Idle pin: holding a selection costs nothing while nothing changes
  (extend the existing zero-idle tests).

## Progress checklist
- [ ] Layer 1: command-copy recipe in docs + one example
- [ ] Layer 2: region text-extraction API over the snapshot walk
- [ ] Design ruling: per-widget vs screen-layer selection
- [ ] Layer 3: drag selection + selection inks + copy wiring
- [ ] Extraction/acceptance/idle tests
