# 0630 — Speaking-highlight primitive: progressive text emphasis by offset

## Metadata
- Created: 2026-07-22
- Status: Proposed
- Track: media-av (band 0600–0690)
- Completed: N/A
- Depends on: the text↔cells mapping substrate shared by app-widgets
  0148 (search-highlight overlay) and 0160 (content selection) — this is
  the THIRD consumer of that one mapping; build it once, there.
- Promotion trigger: a voice-reader app (TTS reading a document aloud)
  or the assistant port's spoken-reply view.

## ADR status
- Governing ADRs: ADR-0001 (additive). ADR impact: none.

## Context
Voice readers highlight what is being spoken (karaoke-style): a word or
sentence range lights up and advances with playback. The driving datum
is trivial — a `Signal<Range<usize>>` of byte offsets into the source
text — but mapping SOURCE offsets to SCREEN cells across wrapped,
styled, markdown-rendered text is exactly the text↔cells problem 0148
(search highlight) and 0160 (logical selection) already own. Timing
sources are app-side and honest by construction: STT engines provide
word timestamps (faster-whisper exposes them —
`abstractvoice/adapters/stt_faster_whisper.py`), TTS engines in the
gateway lane do NOT (wav bytes only, no alignment — verified 2026-07-22),
so TTS reading uses estimated pacing (chars/sec against the known audio
duration) unless the engine adds timings later. The ENGINE only needs
the offset→cells→emphasis primitive; where offsets come from is app
policy.

## Current code reality
- `render::rich`/`md` produce styled lines; `widgets::RichText`/
  `Markdown` render them; NO structure maps a source-text range to the
  cells it landed on (0148 names this the shared substrate;
  docs/backlog/proposed/app-widgets/0148_search_highlight_overlay.md:24
  — "the text↔cells mapping is the shared substrate with 0160").
- The selection layer proves the emphasis mechanic: recolor cells
  post-flatten without touching glyphs (src/app/selection.rs paint —
  the 0270 tier-3 pattern: inks replaced, glyphs kept).
- `reactive::animate`/frame tasks provide the advance clock; a
  `Signal<Range<usize>>` write per word is a normal signal update and
  damages only the affected rows (cell-diff containment).

## Problem
Without the mapping, a voice reader must re-render its text through a
private pipeline to know where words landed — re-deriving wrap math the
engine owns, and breaking the moment markdown styling shifts widths.

## What we want
1. **Range→cells query** on the rich/markdown view state (the 0148
   substrate): `cells_for_range(Range<usize>) -> Vec<Rect>` in
   widget-local space, wrap- and style-aware.
2. **`SpeakingHighlight` decorator**: takes the view + a
   `Signal<Range<usize>>`, recolors the mapped cells with theme
   emphasis tokens (selection-style ink swap, glyphs untouched), clears
   on `None`. Auto-scroll: optional follow mode keeps the active range
   visible via the existing `Scroll::follow` machinery (0130 shipped
   measured content extent — cite its completion report for the API).
3. **Damage containment**: advancing one word repaints only the rows
   the old+new ranges touch (assert byte containment like the composer
   wave did).

## Scope / Non-goals
Scope: the query (in the 0148 substrate work), the decorator, follow
mode, docs + the 0650 demo wiring.
Non-goals: audio timing estimation (app-side), TTS/STT anything,
per-phoneme granularity (word/sentence ranges only — cells are coarse).

## Expected outcomes
A reader app drives one `Signal<Range<usize>>` from its playback clock
and gets correct karaoke emphasis over wrapped markdown, with the
scroll following the voice.

## Validation
- Unit: range→cells over wrapped/styled/CJK/emoji content (the cluster
  cases the composer wave already enumerated — reuse those fixtures).
- CaptureTerm: advance a range across a wrap boundary → only affected
  rows re-emit; theme tokens only; follow mode scrolls when the range
  leaves the viewport.

## Progress checklist
- [ ] text↔cells substrate lands (0148/0160 joint — cite it here)
- [ ] cells_for_range query
- [ ] SpeakingHighlight decorator + follow mode
- [ ] containment tests + docs
