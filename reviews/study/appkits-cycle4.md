# App-kits roadmap study — cycle 4 (final pass)

Date: 2026-07-22 · Seat: APPKITS (band 0500–0590). Cycle scope: the
newcomer final read of all ten items + README, cross-track
contradiction check against both peers' final files, and the
integrator handoff block (bottom of this file).

## 1. Final read — fixes applied

Read all ten items + README as a newcomer (standalone-executability,
explicit dependencies, current validator lines, stale cross-refs).
Fixes this cycle:
1. **README**: status stamp "cycle 1" → "cycles 1–4 — final"; the
   0500 ledger row now names the anchored-popup substrate (it is the
   band's trunk deliverable, not a sub-detail); the Table-in-Feed
   non-goal now states the draw-only precision directly.
2. **0500**: H1 title and ADR-status realigned to the substrate
   promotion (title carries it; ADR line names the one additive
   engine delta and the cross-band consumers incl. 0430's TOOLTIP-mode
   adoption, which landed this cycle).
3. **0510 §4 — a false claim in my own item, corrected**: cycle-1 text
   said "focusing scrolls it into view already (scroll.rs:3-9)". Wrong:
   that doc line only guarantees focus SURVIVES scrolling; nothing in
   `Scroll` follows focus. Resolution needs NO engine delta — the form
   owns its `Scroll` and its rows are fixed-height, so the kit computes
   the erring row's y and writes the bound `offset_y` signal
   (scroll.rs:57-68) before focusing. Same honesty class as the
   claims I flagged in peers' items; caught on the final read.
4. **0520 §7**: "ergonomics feedback OWED to 0340" was stale — it was
   delivered and ACCEPTED (0340:154-162 now cites 0520 §7's
   one-wizard-key shape and the consume-once `take(key)` ask).
5. **0570**: body's "check 0170's non_exhaustive stance" aligned to
   the metadata's resolved handling; Scope's validator line aligned to
   the real 0590 set (triage-shell outline; file-manager triptych
   named as the fuller exercise if a fourth validator is added).
6. **0580**: promotion trigger and Scope pointed at a 0590 validator
   that does not exist (a file-manager example) — realigned to the
   triage shell (split + rail) with the triptych as the named
   possible fourth; "repositon" typo fixed.
Everything else read standalone and current: every item carries
`Depends on:` + `Validator (0590):` lines; the 0250-ruling encodings
in 0530 §3/§5, 0550 §1, 0570 §4 (and 0540's pure-toggle chip) are
mutually consistent and consistent with the ruling text.

## 2. Contradiction check against the peers' final state

- **platform-cycle4.md**: their 0340 ledger row names 0520 as the
  accepted first consumer; their cross-track edges record 0340↔0520,
  the 0250-ruling adoption ("app-kits 0500/0530/0550/0570 are born to
  it"), both engine-delta homes (`top_z` → 0500, subtree focus step →
  0510), and the shared `Role` non_exhaustive want. All match my
  items. Verified in their ITEMS too: 0310 "Cross-track answers"
  (nullary actions; redaction-at-widget pinned citing F2) and 0320's
  four-bullet threat model (wire carries what the tree carries →
  masked inputs mask `access_value`). **No contradiction.**
- **extensions-cycle4.md**: 0430 tooltips now consume 0500's TOOLTIP
  routing mode (their cycle-3 residue, folded — verified in
  0430:48-49,130); substrate recorded as "CORE, owned with 0500";
  all 13 findings from appkits-on-extensions.md recorded as folded;
  their md-tables seed for the app-widgets band names the
  solve_columns-reuse coordination with 0530. **No contradiction.**
  One vocabulary nit, noted here rather than filed (their summary
  says 0430 consumes "TOOLTIP + SELECT modes"): 0500's spec names the
  modes OWNED / PASSIVE PANEL / TOOLTIP — "SELECT mode" is their
  shorthand for OWNED. 0500's spec is the authoritative vocabulary.
- Their earlier finals also confirmed two decisions my band depends
  on: extensions' 0400 dry-run classified app-kit choice controls/
  forms as CORE (my cycle-2 P2-5, accepted), and 0440's KG re-scope +
  designed v1.5 force stage (my P1-2, accepted).

---

## INTEGRATOR HANDOFF BLOCK (app-kits band, final)

**Track row for overview.md "Topic tracks" table:**

| app-kits | `proposed/app-kits/` | Proposed | The application-kit layer over the content widgets: anchored-popup substrate + choice controls, form kit + wizard, rich data tables, chip/count vocabulary, navigation (sidebar + filter tabs), header/banners, tree view, split panes + panel rail — proven by three in-repo reference validators (admin console, setup wizard, triage shell). |

**Ledger rows for overview.md "Proposed ledger" (counts: Proposed +10):**

| ID | Title | Track | Promotion trigger |
| --- | --- | --- | --- |
| 0500 | Anchored-popup substrate (cross-track: 0120/0530/0430 consume) + Select/Combobox/MultiSelect family | app-kits | First config/settings surface in a dogfood app, or 0510 starting. Substrate lands first; `Overlays::top_z()` rides the 0.2 budget window. |
| 0510 | Form kit: field rows, form-state helpers, submit gating, masked input | app-kits | 0520 starting, or a second settings form in any dogfood app. Engine deltas: subtree-scoped focus step; `TextInput::masked`. |
| 0520 | Wizard flow container (on the form kit) | app-kits | 0510 landing, or a first-run/setup flow in a dogfood app. Crash-resume slice gated on control-plane 0340 (its accepted first consumer). |
| 0530 | Data-table upgrades: rich cells, row actions, activation, row identity, multi-select | app-kits | The 0590 admin-console validator, or any product table needing badges/actions in rows. |
| 0540 | Chip & count vocabulary: Badge count/dot forms, Chip, ChipGroup, TagInput | app-kits | First consumer among 0500 MultiSelect, 0550 counts, or smart-note tag surfaces. |
| 0550 | Navigation kit: NavList (sections, badges, sticky keys) + FilterTabs (counts, no panels) | app-kits | The 0590 admin-console or triage-shell validator, or the 0210 chat epic's room-list phase. |
| 0560 | App header + banner primitives (Banner/BannerStack, notices bridge) | app-kits | The 0590 admin-console validator, or any dogfood app needing a persistent attention line. |
| 0570 | Tree view: virtualized keyed hierarchy with lazy children | app-kits | The 0590 triage-shell notes outline, or any hierarchy browser. `Role::Tree`/`TreeItem` ride the 0.2 batch. |
| 0580 | Workspace chrome: SplitPane (resizable) + PanelRail (collapsible) | app-kits | The 0590 triage shell (thread/rail split + rail), or any master-detail surface. |
| 0590 | Reference-app validators: admin console, setup wizard, triage shell + the consumption law | app-kits | The first 0500–0580 item reaching implementation — validator slices grow with the band. |

**Cross-track edges to record (both sides already written in-item):**
1. **0520 ↔ control-plane 0340**: the wizard is 0340's accepted first
   consumer (one `wizard.<id>` key, schema-versioned, consume-once
   restore); `examples/setup_wizard`'s kill-mid-wizard journey doubles
   as 0340's restore-ordering acceptance evidence. (Mirrored in
   platform-cycle4's edges.)
2. **0.2 breaking-budget riders queued by this band** (sequence with
   the 0170 audit): `Role::Tree`/`TreeItem` — or `Role` gains
   `#[non_exhaustive]` (control-plane co-wants it for protocol
   enums); `Overlays::top_z()` (additive, home 0500);
   subtree-scoped focus step (additive `ui::focus` API, home 0510);
   `TextInput::masked` (additive builder, home 0510 — masks the draw
   AND `access_value` together; band rule).
3. **Theme-lane note (from 0560)**: banner-ground token family
   (per-tone tinted grounds) + contrast-audit pairs across the 26
   built-ins — take ONLY if 0590 validator use proves the
   existing-token banner rendering insufficiently loud.
4. **Anchored-popup substrate registry** (spec v1 = 0500): mode
   vocabulary OWNED / PASSIVE PANEL / TOOLTIP is authoritative there;
   consumers: 0500 faces (owned), 0120 completion dropdown (passive
   panel — keys stay with the composer), 0530 action menus (owned),
   extensions 0430 tooltips (tooltip) + in-card dropdowns (owned,
   panned-anchor case). Consumer sign-off is checklist item 2 of
   0500.
5. **0250 ruling**: text at reviews/study/platform-on-appkits.md
   "The 0250 ruling"; adopted by 0530/0550/0570 (+ 0540, README band
   rules). The eventual List/Table engine fixes should cite it.
6. **Watch items (no app-kits dependency)**: the draw-closure
   link-registration seam (extensions filed toward 0165; home
   decision open in their/app-widgets' lane) and the md-tables seed
   (their recommendation for the app-widgets band; coordinates with
   0530's `solve_columns` reuse — static rich-pipeline block, never
   the interactive Table widget).

**Band rules the integrator should carry into any future kit item**
(README "Band rules"): semantic visibility (every interactive
affordance in the accessibility snapshot), secrets redact at the
widget (draw + `access_value` together), and the activation
vocabulary (`on_select`/`on_activate`/`on_change`; Space toggles
where a toggle exists, Enter always activates; commit-on-move opt-in,
never default).
