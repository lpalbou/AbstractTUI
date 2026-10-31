# 0160 — Content selection + copy (take text out of any view)

## Metadata
- Created: 2026-07-21
- Status: Proposed (remaining surfaces; source-aware Feed selection implemented)
- Track: app-widgets
- Completed: N/A

## 2026-10-31 — source-aware Feed slice implemented

[ADR 0007](../../../adr/0007-logical-content-selection.md) accepts producer-authored
logical source mapping alongside existing screen copying. `ContentSelection`,
`ContentHit` and `SelectedContent` provide retained Unicode word/sentence/paragraph
and drag-span selections. Feed Markdown/Text/Rich/Code and streamed Markdown carry
parser offsets through their actual wrap/table fold; tables retain header/row/column
and code retains language/line metadata. `Element::drag_hit_test` preserves source
selection on mapped regions and legacy screen-copy fallback elsewhere.

Evidence: `src/widgets/content_selection_tests.rs`, `tests/content_selection.rs`,
existing `tests/adv_selection.rs`, and AbstractCode's actual Driver/composer integration
in `tui/tests/content_selection.rs`. Context actions append to an existing draft and
require explicit normal submission. Source invalidation/cancel/teardown releases
capture; scoped follow-tail holds compose with screen/manual freezes. See
[public API](../../../api.md#feed--logical-content-selection).

Remaining item scope: a public rectangle-to-screen-text extraction API; built-in
logical selection bindings for standalone MarkdownView/other readers; and
producer-authored geometry for custom blocks such as UserCard/tool/JSON renderers.
`ContentSelection::bind` provides the generic resolver seam; it cannot infer source
from arbitrary draw callbacks. Root item 1082 records the implemented Code flow;
root proposed 1086 tracks custom-content geometry. This broad item stays open.

## 2026-07-22 — screen-level v1 landed via 0270 (scope note)

Backlog 0270 (first-app, now completed) shipped the SCREEN-LEVEL slice
of this item's layer 3, taking the "screen-level layer" side of the open
ruling for v1: an opt-in engine selection layer (`src/app/selection.rs`)
— drag paints `selection_fg`/`selection_bg` over the composed frame as a
damage-honest post-flatten patch, clamped to the pane under the drag
anchor (`UiTree::pane_rect_at`: nearest clipping-or-padded ancestor's
content box); release or Enter/`c`/Ctrl+C copies the rendered text over
OSC 52 through presenter custody; wide glyphs never split, per-row
trailing trim, `\n` row joins. Layer 1's clipboard verb also landed:
`app::selection::copy_to_clipboard` queues app-held source text through
the same custody path (0150's clipboard leg). What this item still owns:
- **Layer 2 as a public API**: the row-flow extraction shipped
  crate-internal (`app::selection::extract_text` over `Surface`); the
  public rect→text API over the snapshot walk remains to be shaped.
- **Logical widget-content selection** — the text↔cells mapping (copy
  markdown SOURCE, unwrap soft-wrapped rows, per-widget `Selectable`),
  which 0270 explicitly deferred here.
- **The shared substrate question**: search-highlight and selection
  want the same "recolor a region of the composed frame" machinery —
  0270's post-flatten patch + pre-flatten repair-damage pattern is the
  candidate to generalize when search lands.

## ADR status

Governing ADRs: [0005](../../../adr/0005-content-rendering-responsibilities.md)
(content ownership) and [0007](../../../adr/0007-logical-content-selection.md)
(logical provenance alongside screen fallback). Clipboard reads remain excluded.

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

## Original code reality (2026-07-21)
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
- [x] Design ruling: per-widget logical mapping alongside screen-layer copying (ADR0007)
- [x] Layer 3: screen drag selection + selection inks + copy wiring (0270)
- [ ] Extraction/acceptance/idle tests
