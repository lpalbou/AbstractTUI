# Extensions study — cycle 2 log (2026-07-22)

Two deliverables this cycle: the control-plane cross-review
(`extensions-on-platform.md` — 3 P1, 5 P2, 3 P3, all with concrete
demands) and the revision pass over my own track folding
`appkits-on-extensions.md` (all 13 findings accepted; none rejected —
every one verified at source before folding).

## Answers given (binding for my band)

1. **Control-server placement** (the question routed to this band):
   0310 bus = core unconditional (it IS the anchor surface; needs the
   driver drain seam); 0320 server = in-tree **default-OFF
   `control-server` cargo feature** — not a sibling crate (co-design
   with VirtualTerm/attach is highest-churn now; protocol freezes with
   an engine ADR, so no independent cadence yet; feature-off =
   constructor absent = compile-time security posture); 0330 bridge +
   productized attach client = out-of-crate. 0400 §1 now carries the
   two feature classes (default-ON trim / default-OFF opt-in surface)
   with this as the applied example.
2. **Action metadata** (PLATFORM's cycle-1 ask): name + description
   (+ existing chord) suffices for v1 — plus a dotted-namespace naming
   convention (zero new fields). No schemas in core (bridge-side
   concern). The real recorded seam is arg-less invoke
   (`Actions::run(name) -> bool`, `FnMut()` callbacks): v1 drives
   parameterized interaction via inject + semantic tree;
   `invoke_with(name, payload)` is the named v2 seam, designed when a
   real consumer (the graph editor) exists. Canvas-drawn content is
   deliberately opaque to the semantic tree — extensions expose intent
   as actions/events; 0310 docs should state that division.

## Track revisions (from appkits-on-extensions, all verified then folded)

- **0400**: two feature classes + control-server placement; coupling
  mechanics (dual-form dep spelling, publish order); the dependency
  posture BINDS sibling crates (explicit clause — 0450/0470's
  assumption now grounded); anchor surface += `app::Overlays` + the
  0500 anchored-popup primitive; peer-band dry-run EXECUTED and
  recorded (app-kits choice controls = CORE, not sibling family —
  README reworded); module count 16→17.
- **0410**: gltf_json promotion named as a cross-track precondition
  with 0320 (ordering + cfg'd re-export); public-api diff gate per
  feature combination added to validation.
- **0420**: consumers named precisely (chart.rs now; Progress as
  explicit second candidate or explicit leave; 0430/0440/0450 via the
  public path; app-side traces); the byte-identical-goldens migration
  gate hardened (intentional pixel changes never ride the refactor).
- **0430**: the link-stamping plan now names the REAL core gap the
  peer found (draw closures cannot register link URIs —
  `resolve_link` needs `&mut Surface`, rich.rs:302-306; seam filed
  toward 0165: `StyledCanvas::register_link` with no-op default);
  staged M1 canvas core / M2 ports+edges / M3 polish; keyboard parity
  per milestone (spatial focus precedent `focus_next_in`); cross-band
  consumption recorded (0500 select-in-card + panned-anchor placement
  case, 0580 split pane, 0540 status dots).
- **0440**: honest class boundary — v1 serves DAG-class only,
  knowledge graphs explicitly NOT served by v1; bounded on-demand
  force layout PROMOTED from research to designed v1.5 (alpha-cooled,
  freeze-on-settle, act-not-state so zero-idle holds; deterministic
  under seed+budget).
- **0450**: subset table made grammar-actionable (accepted spellings
  enumerated per YES row; unknown SPELLING → atomic fallback naming
  the first unrecognized line; corpus pinned to a named mermaid docs
  version); in-feed diagrams' link limitation named (CustomBlock is
  draw-only — no link minting until the seam lands).
- **0460**: two layer-target corrections — md tables share the width
  ALGORITHM (`solve_columns`, table.rs:374), never embed the
  interactive Table widget; md images split honestly (in-flow =
  mosaic via Image widget + decode widening PNG→PNG+JPEG;
  pixel-protocol in scrolled content = named open compositing
  question). The four gap seeds now carry explicit band-routing lines
  for the integrator (tables/images/anchors/search → app-widgets,
  with 0530/0165/0160 coordination named).

## Open questions carried to cycle 3

1. The draw-closure link-registration seam's exact shape (trait
   default vs id-mint API) and whether it folds into 0165 or stands
   alone — needs the app-widgets owner's read (integrator fold).
2. Whether 0440 v1.5's force stage should share the graph crate's
   `layout` module signature (pure data in/out like `layered()`) so
   0450 could someday route non-hierarchical diagrams to it — decide
   against the first KG-class consumer.
3. 0400's ADR draft: does the default-OFF feature class need its own
   additivity wording in ADR-0001 terms (a feature that ADDS a
   constructor is additive; one that would ever CHANGE runtime
   behavior when enabled is forbidden) — one paragraph to settle at
   ADR time.
4. From my platform review, the two demands most likely to need
   cycle-3 follow-up: ImageSession identity across attach (0350
   P1-1) and the egress ring mechanism (0310/0320 P1-3) — verify the
   platform seat's fold.
