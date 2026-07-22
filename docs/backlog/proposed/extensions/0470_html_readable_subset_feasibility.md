# 0470 — Web/HTML rendering: feasibility verdict + the defensible slice

## Metadata
- Created: 2026-07-22
- Status: Proposed (research — this item RECORDS a verdict and its
  promotion criteria; it is not a commitment to build)
- Track: extensions
- Completed: N/A

## ADR status
- Governing ADRs: 0400's ADR (if the slice ever builds, it is a
  sibling crate under the extension posture); the core dependency
  policy (docs/design/00-vision.md:46-53) rules out every off-the-shelf
  HTML/CSS engine dependency in core, and 0400 decides whether it
  binds extensions (this item assumes it does).
  ADR impact: none until promotion.

## Verdict (the item's reason to exist)
**Full web rendering is OUT — permanently, not "not yet".** A web
renderer means CSS box-model layout, JavaScript execution, and a
networking/security surface; each alone violates the charter (engine
bloat vs. the lean-core brief; the five-crate posture; endless scope —
browsers are the largest software artifacts in existence). No
promotion criteria exist for this half: **JS and CSS layout = never**
(recorded as a track non-goal).

**The defensible slice is a readable-mode HTML SUBSET as an extension**
— headings, paragraphs, lists, blockquotes, pre/code, tables, links,
images — mapped into the SAME block vocabulary the markdown pipeline
renders. Readable-mode is a known-good product pattern (reader views
strip layout and keep structure); structurally it is "markdown-shaped
content that happens to arrive as HTML". Even this slice is deferred:
it is gated on promotion criteria below, because its real costs are a
tolerant parser and an entity/encoding layer, not the rendering.

## Current code reality
- The rendering TARGET exists end-to-end: block vocabulary + rich
  spans + typesetting (src/render/md.rs:57-97 `Block`,
  src/widgets/markdown.rs, src/render/rich.rs) — a readable-HTML
  mapper produces these blocks and rendering is done. The 0460 gap
  items (md tables, md images, anchors) enlarge exactly the vocabulary
  HTML needs; build order matters: **0460's gaps first, HTML mapper
  second** — otherwise the extension carries private typesetting.
- The parsing SOURCE does not exist and is the honest cost center:
  - No HTML tokenizer in-crate; the posture forbids `html5ever`-class
    deps. A hand-rolled tolerant subset tokenizer (tags, attributes,
    entities, comments, script/style skipping) is bounded but real —
    order 1-2k lines by analogy with the hand-rolled JSON
    (src/three/gltf_json.rs, 567 lines for a far smaller grammar) and
    PNG/JPEG decoders (the house pattern: hand-roll, fuzz hard,
    src/gfx/decode.rs:118-135).
  - Real-world HTML is soup: unclosed tags, misnesting, cp1252
    masquerading as utf-8. "Tolerant" is a correctness bar with a fuzz
    suite, not an adjective.
- Precedent for the degradation shape: mdpad renders raw HTML blocks
  "verbatim and dimmed" (mdpad src/markdown/model.rs:79-80) — the
  same honest floor applies here for anything outside the subset:
  dimmed source text, never a guess.
- What "images" can honestly mean: local/embedded sources decode
  (PNG/JPEG, src/gfx/decode.rs:58-67); REMOTE images are a transport
  question owned by the live-data track's ADR (band 0010-0090, item
  0050) — this extension must not smuggle in HTTP.

## Problem (why record this at all)
"Can it render web pages?" will be asked of any serious TUI engine —
repeatedly. Without a recorded verdict, each asking risks scope drift
toward the browser tarpit or an ad-hoc "no" that loses the defensible
slice. The verdict + criteria make the answer stable and citable.

## Promotion criteria (all required before the slice is scheduled)
1. **A named consuming app class with evidence** — a docs/help
   browser, feed/RSS reader, or gopher/gemini-adjacent client actually
   being built on AbstractTUI, whose content is HTML-at-rest that
   cannot reasonably be converted to markdown upstream. (If content
   can be converted upstream, the answer is a converter, not a
   renderer — cheaper and out of engine scope.)
2. **0460's vocabulary gaps landed** (tables, images, anchors) — the
   mapper's target must exist so the extension is parser+mapper only.
3. **0400 executed** and the extension posture (deps, naming, CI)
   ruled — this crate would be the posture's stress test (the
   tokenizer is the first extension component with a fuzz budget).
4. An owner willing to hold the tolerant-parser fuzz bar (the house
   standard: no input panics, src/gfx pattern).

Until all four hold, the item stays research; re-litigating the full
web question requires superseding this verdict in writing (ADR
discipline, docs/adr/README.md:18-19).

## Scope / Non-goals (of the slice, if promoted)
Scope: tolerant subset tokenizer, entity decoding, encoding sniff
(utf-8 + labeled fallbacks), block mapper (h1-h6/p/ul-ol-li/
blockquote/pre/table/a/img), dimmed-verbatim floor for everything
else, fuzz suite, corpus tests over saved real pages. Non-goals
(permanent): CSS (both parsing beyond a discard pass and any layout
semantics), JavaScript, forms/interactivity, frames, remote fetching
(transport belongs to live-data ADRs), cookies/auth — the extension
consumes bytes handed to it.

## Expected outcomes
The stable answer: "readable HTML as an extension when a real consumer
arrives; browser-class rendering never." Askers get criteria instead
of debate; the engine keeps its posture.

## Validation
- This file carries the verdict + criteria (index it from the track
  README) — validated by USE: the next web-rendering ask is answered
  by citation.
- If promoted: fuzz (no panics on arbitrary bytes), corpus goldens
  (saved pages → pinned blocks), degradation pins (out-of-subset →
  dimmed verbatim, labeled), encoding fallback labels.

## Progress checklist
- [ ] Verdict ratified alongside 0400's ADR (one packaging story)
- [ ] Criteria tracked (consumer app class, 0460 gaps, posture, owner)
- [ ] On promotion: parser/mapper design note first, then the crate
