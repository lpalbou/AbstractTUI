# App-kits roadmap study — cycle 3 (convergence)

Date: 2026-07-22 · Seat: APPKITS (band 0500–0590). Cycle scope: the
anchored-popup substrate specified (0500), peer-fold verifications,
0560 theme resolution, 0590 orphan check, final coherence pass.

## 1. Anchored-popup substrate — specified (0500 "Anchored-popup
substrate (spec v1)")

Now a cross-track load-bearing primitive with a binding spec:
- **Anchor**: solved screen rect captured in the opener's handler via
  `EventCtx::current_rect()` (only rect source today; correct by
  construction for 0430's panned/absolute cards); a future
  out-of-handler rect query (0120's caret need) slots in without a
  type change.
- **Placement**: prefer below, flip above when below is short and
  above is larger; height = min(content, chosen side); x
  viewport-clamped (Toast math). Width policy: `MatchAnchor` (selects)
  or `Content{min,max}` (menus/tooltips). `include_anchor_row` starts
  the popup AT the anchor (the Combobox popup-mounted editor — zero
  visual jump).
- **Stacking**: an OWNED popup is a modal tree at
  `overlays.top_z() + 1`. `Overlays::top_z()` is THE engine delta —
  one additive method, 0170-gated (verified necessity: modal trees
  swallow all keys + mouse, topmost-z-first, overlays.rs:318,326-356;
  a static z constant cannot serve select-inside-modal-inside-modal).
- **Dismiss contract**: outside-press (modal trees only — engine
  fact, overlays.rs:56-60,330-336; never acts below) + Escape +
  explicit close; `DismissReason` reported. Plus anchor-unmount
  safety: a popup whose opener scope dies closes with it (regression
  test pinned in the item — `Modal` deliberately differs).
- **Three routing modes**, one geometry engine: OWNED (modal tree,
  owns keys — Select/MultiSelect/Combobox, 0530 action menus, 0430
  in-card dropdowns), PASSIVE PANEL (non-modal, keys stay with the
  anchor owner, owner-driven dismissal — 0120's completion dropdown;
  this is why outside-press cannot be its dismiss story), TOOLTIP
  (layer_draw, non-interactive, hover-delayed via `after()` — 0430).
  Consumers enumerated in a sign-off table; 0500's checklist is
  resequenced substrate-first (top_z delta → consumer sign-off →
  substrate → faces).

## 2. Peer-fold verifications (task 2a)

- **PLATFORM 0310 — VERIFIED**: "Cross-track answers (cycle 2)"
  section exists (0310_automation_bus.md:105-124); the extensions
  metadata ruling is as stated (v1 nullary + optional description;
  `args` hint + `run_with` reserved needs-design; 0320 `invoke`
  pre-pays wire compat), AND my F2 consequence is pinned there
  ("Redaction happens at the widget, never in the bus", citing
  platform-on-appkits F2 + input.rs:210). Consistent with my 0510 §5.
- **PLATFORM 0320 — VERIFIED**: the threat model is stated plainly in
  the item (0320:94-121): local-uid trust boundary, file permissions
  ARE v1 auth (re-verified at bind), closed verb set, and "the wire
  carries what the semantic tree carries" with redaction-at-widget
  cross-referenced to 0310. My band's masked-input obligation is the
  named counterpart; no contradiction.
- **EXTENSIONS 0430 — VERIFIED**: staged M1/M2/M3 with per-milestone
  acceptance gates AND keyboard parity per milestone (my P1-3,
  accepted; "No milestone ships a pointer-only interaction"). It now
  cites my 0500 substrate for in-card dropdowns incl. the
  panned-anchor placement case (0430:135-136).
- **EXTENSIONS 0440 — VERIFIED**: the KG re-scope is explicit
  ("knowledge graphs are NOT served by v1… v1 ships layered-only and
  says so", 0440:23-28) and the bounded force stage is now DESIGNED
  (v1.5: repulsion + springs, fixed seed + iteration budget,
  deterministic, positions cached — "the sim is an act, not a state"),
  which is exactly the zero-idle-compatible shape my P1-2 demanded.
- **Link-registration seam (my P1-1) — VERIFIED as filed**: the track
  README ledger row and 0430/0450 now carry "draw-closure
  link-registration seam" with the documented fallback; their cycle-2
  note records the shape candidate (`StyledCanvas::register_link`
  with no-op default) "filed toward 0165", final home (0165 amendment
  vs a 0480) still their/app-widgets' open decision — acceptable; my
  demand was that it be FILED and no item claim it exists.
- **One residue for extensions (non-blocking, one line)**: 0430's
  hover/tooltip bullet still cites raw `Overlays::layer_draw`/
  `layer_tree` (0430:46-48) — accurate mechanically, but M3 tooltips
  should consume 0500's tooltip mode rather than re-derive
  clamp/flip/delay. Handed over as a reconciliation note (their dir,
  their edit).

## 3. 0560 resolution (task 2b — the F8 fold, finalized)

v1 banners use EXISTING tokens only: `surface_raised` ground +
semantic ink + tone-colored leading glyph and hairline — all
already-audited pairs; the band invents no tokens. A dedicated
banner-ground token family is a THEME-lane follow-up for the
integrator (see Integrator notes below), taken only if validator use
proves v1 insufficiently loud. 0560 §5 now says exactly this and
points here.

## 4. 0590 orphan check (task 2c)

Full item→validator sweep after all cycle-2/3 amendments. Three
orphans found and fixed in 0590:
- **0520 crash-resume** (added cycle 2 to 0520's validation) had no
  home in `setup_wizard`'s acceptance — added, 0340-gated, noted as
  simultaneously 0340's restore-ordering acceptance evidence.
- **0540 TagInput** was in 0540's validator line but absent from
  `triage_shell`'s composition — added to the notes panel (+ a
  tag-appears-as-chip acceptance line).
- **0500 MultiSelect** was only implicit in "selects in an edit
  panel" — made explicit (incl. chip overflow) in `admin_console`.
No other orphans: every 0500–0580 validation surface now maps to a
named validator slice (0510 leak test and 0530 a11y-snapshot test are
unit/integration-level and ride the same compose fns per 0590 §4's
mechanism).

## 5. Final coherence pass (task 3)

- Every item now carries BOTH a `Depends on:` and a `Validator
  (0590):` metadata line (0590 itself: Depends only). Edges as
  settled: 0500 trunk (substrate-first; top_z delta 0170-gated) →
  0510 (embeds 0500, non-blocking; two engine deltas named) → 0520
  (0510 + control-plane 0340, first Persist consumer) · 0530/0550
  consume 0540 · 0560 consumes 0540 (+0300 non-blocking) · 0570/0580
  independent (0580's persistence recipe cites 0340) · 0590 last,
  slices per item.
- 0250 ruling: encoded in 0530 §3/§5, 0550 §1 (activate-on-move OFF),
  0570 (branch-fold toggle coincidence), 0540 (pure-toggle chip), and
  the README band rules — no text anywhere still defers the ruling.
- No contradictions found with PLATFORM's ruling text or 0320's
  redaction posture; EXTENSIONS' consumption of the substrate is
  mutual (their 0430 cites my 0500; my spec table names their two
  modes).
- Feasibility currency: every engine delta this band needs is now
  named where it lives — `Overlays::top_z` (0500), subtree-scoped
  focus step + TextInput masked mode (0510), `Role::Tree/TreeItem`
  under the 0.2 batch (0570). Nothing claims self-containment it
  does not have.

## Integrator notes (for the single-writer merge pass)

1. **overview.md fold**: band 0500–0590 = 10 proposed items + README
   (counts: Proposed +10); ledger rows per the band README's table;
   bands section gains "app-kits 0500–0590" (peers: 0300–0390,
   0400–0490).
2. **Theme lane (from 0560)**: one-line candidate — "banner-ground
   token family (per-tone tinted grounds) + contrast-audit pairs
   across the 26 built-ins; take only if 0590 validator use proves
   the existing-token banner rendering insufficiently loud."
3. **0.2 breaking-budget riders queued by this band**:
   `Role::Tree`/`TreeItem` (or `#[non_exhaustive]` on `Role` —
   PLATFORM wants it too), `Overlays::top_z()` (additive),
   subtree-scoped focus step (additive), TextInput `.masked()`
   (additive builder).
4. **Cross-band watch item**: the draw-closure link-registration seam
   (extensions' filing toward 0165) — decision on home (0165
   amendment vs new item) pending in their band; no app-kits item
   depends on it.
