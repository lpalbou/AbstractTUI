# Extensions study — cycle 3: convergence (2026-07-22)

Three deliverables: (1) the cycle-2 open questions CLOSED in item text
(not review notes) — the link seam is now item **0480**, the force
signature is in 0440/0450, and the ADR-0004 skeleton is below for the
integrator; (2) verification of PLATFORM's folds of my two hardest
cycle-2 demands; (3) the final coherence pass over the track (now 9
items).

---

## 1. Closures executed in items

### (a) Link seam → 0480 (new item, this band, fully specified)
Decision: **standalone core item 0480** ("Link registration from draw
closures"), NOT a silent amendment to 0165. Rationale recorded in the
item: the producer half has standalone value (presenter OSC 8 emission
already works — `set_link`, src/render/present.rs:385-392 — so a
draw-closure link is terminal-side ctrl-clickable before 0165 lands);
band discipline forbids authoring in 0100-0190; review-file amendment
notes are the artifact class that gets lost at fold time. API decided:
**Option A, defaulted trait method** —
`StyledCanvas::register_link(&mut self, uri: &str) -> u16` default 0
(= no link, the documented degradation value), overridden by
`SurfaceCanvas` (→ `Surface::register_link`, inheriting interning +
cap + counted drops, src/render/surface.rs:125-135), `ClippedCanvas`
(forwarding), `BufferCanvas` (test table). Option B (build-time id
minting) REJECTED with rationale on record: link ids are
surface-local by design; a global registry would re-architect the
model 0165 depends on. The integrator may still merge 0480 into 0165
verbatim as its producer section — the spec stands either way; 0430
M3 and 0450's in-feed limitation now cite 0480 + 0165 as the two
halves of one channel.

### (b) Force-layout signature (0440 amended, 0450 cross-ref'd)
`GraphDesc -> Layout` is the layout module's ONE contract: `layered()`
(v1) and `force()` (designed v1.5) both take `&GraphDesc` and return
`Layout`; `force(&GraphDesc, &ForceOpts) -> Layout` with
`ForceOpts { seed: u64, budget: IterationBudget, rank_bias:
Option<Direction>, .. }` (extensibility per ADR-0003's classification
rule). Deterministic under seed + budget (goldens stay possible);
bounded on-demand act, never idle animation. 0450 records the routing
consequence: a future non-hierarchical mermaid kind swaps the
algorithm, never the data contract or the renderer.

### (c) ADR-0004 skeleton (for the integrator; executes backlog 0400)

> # ADR-0004: Extension packaging — feature classes, sibling-crate
> family, dependency-posture inheritance
>
> ## Status
> Draft (proposed by backlog 0400; accept alongside its execution).
>
> ## Context
> The maintainer's modularity brief ("don't overload the default
> package; extensions installed only when needed") against a crate
> with zero cargo features and a five-crate dependency posture
> (Cargo.toml:16-34; docs/design/00-vision.md:46-53). The decision
> queue that forced the ruling: heavy in-tree modules (three/jpeg/
> proto — backlog 0410), the control server (0320), diagram domains
> (0430/0440/0450), app-kit controls (band 0500), the HTML slice
> (0470).
>
> ## Decision
> 1. **Two cargo feature classes.**
>    - *Default-ON trim features* for heavy, severable, in-tree code:
>      `three`, `jpeg`, `proto` (0410). Out-of-box unchanged;
>      `default-features = false` is the documented opt-out.
>    - *Default-OFF opt-in features* for in-tree capability a minimal
>      app must not silently carry: `control-server` (0320) is the
>      first — feature-off means the constructor does not exist
>      (compile-time absence as security posture).
> 2. **Additivity rule (both classes).** Enabling a feature may ADD
>    items (types, constructors, prelude re-exports) and may make a
>    runtime path SUCCEED where it previously failed with a named
>    error; it must never change the semantics, defaults, or output
>    of code that compiled without it. Feature-off runtime seams
>    degrade with NAMED errors/labels (the `decode_image` precedent,
>    src/gfx/decode.rs:62-67) — never silently.
> 3. **Sibling-crate family.** New domains ship as
>    `abstracttui-<domain>` crates: public API only (the widgets
>    rule, src/widgets/mod.rs:5-6, promoted to a family contract — a
>    needed-but-missing capability is a core backlog item, never a
>    private hook); in-repo cargo workspace so CI builds the family
>    against core HEAD; dual-form dependency spelling
>    (`abstracttui = { version = "0.x", path = "../.." }`); publish
>    order core-first, family same day; every ADR-0001 breaking
>    budget includes the family migration; the family LAUNCHES only
>    after the 0.2 API-stability pass (backlog 0170) executes.
> 4. **Dependency posture: siblings inherit the spirit.** Allowed by
>    default: std, `abstracttui` itself, and the core's five tiny
>    crates where already justified; parsers are hand-rolled (mermaid
>    0450, HTML 0470 — same discipline as the in-crate JSON/PNG/JPEG).
>    Any new dependency takes the same integrator review-note path as
>    core (Cargo.toml:18). **Named exception window, NOT granted
>    here**: TLS/network-class needs cannot responsibly be hand-rolled
>    — that exception is decided by live-data 0050's transport ADR,
>    and any extension needing it WAITS for that ruling rather than
>    importing ad hoc.
> 5. **The anchor surface** (what extensions may build on):
>    `ui::Element` + draw closures, `StyledCanvas` (incl. 0480's
>    `register_link` once landed), `layout` styles, `reactive`
>    signals, `theme::TokenSet`, `app::Overlays` + the anchored-popup
>    primitive (lands core with app-kits 0500), the canvas/vector
>    layer (0420), and `ControlBus` (0310) when public. The list is
>    exhaustive; growth is by core backlog item.
> 6. **Classification rule + recorded dry-run.** "Does a minimal app
>    pay for it in-tree, and does it have its own release cadence?"
>    Both yes = sibling. Cost-yes/cadence-no = feature (ON if trim,
>    OFF if opt-in surface). Neither = core. Applied: control bus =
>    core; control server = default-OFF feature; MCP bridge + attach
>    client = out-of-crate; app-kit choice controls/forms = core;
>    graph + mermaid = siblings; canvas layer + link seam = core.
> 7. **Non-goals (ratifying the track list).** No dynamic
>    loading/ABI plugins; no scripting runtime; no discovery
>    machinery beyond crates.io naming; no behavior-changing
>    features; no fork of the token/theming discipline.

---

## 2. PLATFORM fold verification (as of 03:10, cycle 3)

Context: the cycle-3 coordination note says PLATFORM accepted the
placement answer and is folding this cycle; their 02:57-02:58 wave
updated 0300/0310/0320/0340/README. What I can verify in the files:

**Landed and correct:**
- 0310 gained "Cross-track answers (cycle 2)": actions stay NULLARY
  v1 with `run_with(name, args)` reserved needs-design, and — better
  than my ask — wire-compat is PRE-PAID (0320's `invoke` reserves an
  `args` field; non-empty args on a nullary action = structured
  error). This folds my P2-4 and matches my metadata answer. Adopted.
- 0310/0320 also gained the redaction-at-the-widget ruling
  (bus/wire/bridge republish the tree; masking is source-side) —
  consistent with my review's §4 division of responsibility.
- 0320's security posture §3 expanded into a real threat model
  (permissions-are-authentication, bind-time re-verification,
  what-the-wire-carries). No demand of mine, but it strengthens P3-3's
  direction; server-side verb-group config still implicit in "enabled
  verb groups" — acceptable.

**Not yet in the files (corrections filed, one line each):**
1. **0350 P1-1 (ImageSession identity on attach)**: file untouched
   since 00:34 — the needs-design line + conservative-serve-caps
   recommendation are still absent. CORRECTION: add both to 0350
   ("attach = image-session reset" as the candidate answer).
2. **0310:92-95 + 0320:80-83 P1-3 (egress ring)**: both still specify
   "std::sync::mpsc (bounded, drop-oldest)" — mpsc cannot drop-oldest;
   the ingest `OverflowPolicy` ring (src/reactive/ingest.rs:55-63) is
   the mechanism. CORRECTION: swap the named mechanism in both items
   (or write drop-newest honestly and accept flaky `wait_for_event`).
3. **0320:122-125 P1-2 (gltf_json promotion as PRECONDITION)**: still
   phrased as a checklist step; the 0410 coordination + cfg'd `three`
   re-export are absent (Feasibility (b) names only 0170).
   CORRECTION: one sentence naming the ordering dependency with
   extensions 0410 (whichever of {gate, server} lands first, the
   neutral-home move lands before or with it).
4. **0320:89-90 P2-2 (stdio guard predicate)**: still "refuses when a
   real terminal session is entered" — the right predicate is "stdio
   is free iff the terminal did not take the stdin/stdout fallback"
   (`UnixTerminal::degraded`), plumbed from where it is read
   (src/app/mod.rs:319-330). CORRECTION: replace the predicate.
5. **0310:80-83 P2-1 (inject Resize)**: no headless-only guard yet;
   0330's resize mapping likewise. CORRECTION: one clause each.
6. **0300 P2-5 / 0340 P2-3 / 0340 P3-1**: post-`enter()` obligation
   enumeration, multi-instance lock story, and png::crc32 reuse
   (src/gfx/png.rs:390) — all still absent. CORRECTIONS: as per the
   cycle-2 review lines.
7. **README cross-band edge for extensions** still reads "nothing
   here assumes their design" — the accepted placement (bus core /
   server default-OFF feature / bridges external) should replace it
   when their fold lands.
None of these block convergence: all are text amendments with agreed
content; the placement + metadata answers (the two that shape
architecture) are already aligned in both tracks' files.

---

## 3. Coherence pass result (the 9-item track)

- Dependency edges: 0400 trunk (packaging citations only — 0420/0480
  build-independent, stated in README status); 0410
  integrator-gated + gltf_json ordering named on BOTH sides now
  (0410 + 0320-correction); 0420 before 0430/0440/0450; 0480 + 0165
  = the link channel with documented pre-seam fallback in 0430; 0500
  anchored-popup consumed by 0430 M3 (ruled core with app-kits);
  0460 seeds explicitly routed to app-widgets for the integrator's
  numbering; 0470 criteria unchanged and current.
- Verdicts current: 0400 v1-able (decision+ADR — skeleton above);
  0410 v1-able integrator-gated; 0420 v1-able; 0430 needs-design
  staged M1-M3; 0440 needs-design (layered v1) + designed v1.5
  (force); 0450 needs-design (spelling-exact subset); 0460 v1-able
  per gap; 0470 research with recorded verdict; 0480 v1-able (small,
  additive).
- No contradictions found with settled cross-track decisions
  (anchored popup = APPKITS-owned core; app-kits controls = core
  class; control server = default-OFF feature; actions nullary v1;
  canvas content opaque to the semantic tree — 0430 documents intent
  exposure via actions/events accordingly).

## Integrator reconciliation notes (single-writer fold, when ready)

1. overview.md: add the extensions track (9 items, band 0400-0490,
   all Proposed) + the two peer tracks in one pass; counts update
   accordingly.
2. Number the four 0460 seeds in app-widgets (md tables — coordinate
   with app-kits 0530's `solve_columns` reuse; md images — includes
   the PNG→PNG+JPEG widget decode widening + the named open
   compositing question; heading anchors/TOC — 0165 consumes anchor
   ids; search highlight — design with 0160).
3. Placement call on 0480: adopt standalone (recommended) or merge
   into 0165 verbatim as its producer section.
4. ADR-0004 from the skeleton above (with 0400's execution); note it
   adds the repo's first default-OFF feature class — one paragraph of
   ADR-0001 additivity wording covers it (skeleton §2).
5. Cargo.toml/workspace acts (0410 gates, workspace restructure,
   `control-server` feature) are integrator-owned per Cargo.toml:18.

---

## ADDENDUM (cycle 4, 2026-07-22 ~03:35) — §2's seven corrections are RESOLVED

**Do not chase the correction list in §2.** It was filed against a
snapshot of the control-plane files taken at ~03:10, which predated
PLATFORM's cycle-3 fold wave (03:11-03:22) — a timestamp race, not a
disagreement. PLATFORM re-verified all seven against their current
files and documented the reconciliation table in
`reviews/study/platform-cycle4.md` §1: every correction is **already
covered** in the current item text (0350 now carries the
ImageSession-identity needs-design entry + the conservative-serve-caps
rule; 0310/0320 name the `OverflowPolicy::DropOldest` mutex ring in
place of the mpsc sketch; the gltf_json promotion is a named
precondition with the 0410 ordering on both sides; the resize-inject
guard and the 0330 headless-only mapping are in; 0300's residual
suspend obligations are a closed list; 0340 has the pid-lock
multi-instance rule and cites `gfx::png::crc32`; their README's
extensions edge records the converged placement). One correction
surfaced a genuine residual — 0320's stdio Current-code-reality bullet
still told the old too-strict story — which PLATFORM fixed in their
cycle 4. Verification loop closed; §2 stands as an accurate record of
the 03:10 state only.
