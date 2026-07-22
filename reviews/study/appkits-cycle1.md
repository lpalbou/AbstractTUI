# App-kits roadmap study — cycle 1 (APPKITS seat)

Date: 2026-07-22 · Seat: APPKITS (band 0500–0590,
`docs/backlog/proposed/app-kits/`) · Peers in parallel: PLATFORM
(0300–0390, control-plane), EXTENSIONS (0400–0490, extensions).
Cycle 1 of 4: gap survey against real code + full band authored.

## What was read (evidence base)

Roadmap + state: `docs/backlog/planned/0001_roadmap.md`,
`docs/backlog/overview.md`, planned/app-widgets/README.md + 0120
(grammar exemplar), proposed/app-widgets/0140, proposed/first-app/
0250 + 0260 + README. Engine: `src/widgets/mod.rs` (catalog + token
lint), `table.rs`, `list.rs`, `input.rs`, `badge.rs`, `tabs.rs`,
`button.rs`, `block.rs`, `checkbox.rs`, `radio.rs`, `grid.rs`,
`scroll.rs`, `feed.rs`; `src/ui/compose.rs` (whole), `src/ui/access.rs`
(Role), `src/ui/view.rs` (focus API), `src/ui/event.rs` (MouseKind);
`src/app/popups.rs` (whole), `src/app/overlays.rs`, `src/app/
notices.rs`; `src/render/md.rs` header; `src/text/truncate.rs`;
`examples/dashboard/main.rs`; `docs/api.md`, `docs/theming.md`.
Targeted greps: select/dropdown/combo/picker, splitter/divider/tree,
mask/secret, Role::Menu consumers.

## Verified gap survey (the claims the band rests on)

| Claim | Evidence |
| --- | --- |
| No select/dropdown/combobox exists | widgets/mod.rs:19-40 module list; grep hits only theme "picker surface" doc comments (theme/registry.rs:69,99) |
| No form machinery — deliberate v1 stance | ui/compose.rs:100-134 "There is deliberately no `Form` type in v1" |
| No wizard/stepped flow; router deliberately absent | ui/compose.rs:136-155; Tabs is free-navigation + dispose-on-switch (tabs.rs:5-8,214-221) |
| Table cells are plain Strings, one style per row, no actions/activation/multi-select/key identity | table.rs:73-76 (rows), 347-358 (draw), 48-57 (struct); List has selection_key, Table does not (list.rs:99-111) |
| Selection callbacks fire on arrow movement (footgun class) | first-app 0250 (List, crash reproduced in the field); table.rs:153-174 same shape |
| Badge is static tone-only; no counts/dots/interactivity | badge.rs:32-87 |
| Tabs titles are bare Strings; no counts, no overflow | tabs.rs:26-32, span walk 186-208 |
| No masked input for API keys | input.rs:35-41 struct; grep mask/secret → one comment (input.rs:302-305) |
| No tree, no splitter/divider, no panel rail | module list + grep (zero matches); 0260's non-goals leave tree generalization explicitly unowned |
| No banner (persistent); Toast is transient by design | popups.rs:136-214; notices signal exists without chrome (app/notices.rs:40-45) |
| Popup substrate ready for anchored dropdowns | overlays.rs:158-222 (layer_tree, on_outside_press); anchor available in-handler via EventCtx::current_rect (table.rs:178, tabs.rs:130); no out-of-handler rect query (0120 records the same gap for carets) |
| Role vocabulary pre-provisioned (Menu/MenuItem unused) but lacks Tree/TreeItem | ui/access.rs:31-51; grep Role::Menu → as_str arms only |
| md parses no tables — chat "messages containing structured tables" needs a recipe | render/md.rs:14-17; owned by app-widgets band, flagged in 0530's non-goals |

## The band as authored

0500 select/combobox/multiselect (trunk control; shared anchored-popup
core with 0120's completion dropdown) → 0510 form kit (field rows +
signals-convention state + masked input; packages the compose.rs RT8-5
pattern instead of replacing it) → 0520 wizard (policy over
signal+Dyn paging; stays inside the router ruling) → 0530 table rich
cells/actions/activation/identity (additive `rich_rows`; needs the
0250 activation ruling) → 0540 chips/counts/tag input (the one
vocabulary for unread badges, state chips, tags) → 0550 NavList +
FilterTabs (subset-selection surfaces; FilterTabs deliberately does
NOT reuse Tabs' panel mounting) → 0560 AppHeader + Banner/BannerStack
(persistent honesty chrome; notices bridge) → 0570 Tree (flatten over
List's prefix-sum discipline; lazy children; three-event
browse/fold/open) → 0580 SplitPane + PanelRail (resize + collapse
policies encoded once) → 0590 in-repo validators (admin console,
setup wizard, triage shell) + docs/app-kits.md + the consumption law
(no band item completes unvalidated).

Reference-UI coverage: A admin console = 0500/0510/0530/0540/0550/
0560(+0590#1); B wizard = 0510/0520/0500(+0590#2); C chat/coordination
= 0540/0550/0560/0580 + shipped Feed + 0120 composer(+0590#3); D file
manager = 0570/0580/0530; D smart-note = 0570/0540/0550/0510; D
monitoring = shipped dashboard + 0530/0550/0560.

## Cross-band touchpoints (flagged, not encroached)

- **PLATFORM 0300s (control-plane/serialization/attach)**: all kit
  state is plain signals by construction (divider positions, expanded
  sets, form values, selection keys) — named as the serialization seam
  in 0580/0520; nothing here invents persistence.
- **EXTENSIONS 0400s (modularity, graph/diagram/mermaid)**: 0580's
  PanelRail and SplitPane are the hosting chrome their panels slot
  into; 0570 Tree is data-hierarchy UI, not their graph rendering — no
  overlap authored.
- **app-widgets 0100–0190**: md-table gap (0530 non-goals), 0120
  shared popup recipe (0500), 0250 activation ruling (0530/0570/0550),
  0260 disclosure gesture (0550 sections, 0580 rail headers), 0170
  breaking-budget discipline governs any public-shape change (Role
  enum growth in 0570, Table API in 0530).

## Open questions (for cycles 2–4 / the maintainer)

1. **0250 ruling first**: the movement-vs-activation split should be
   ruled once (engine-wide) before 0530/0550/0570 encode it thrice —
   candidate for folding into the remaining 0170 audit.
2. **Anchored-popup rect query**: v1 gets by with in-handler
   `current_rect`; does the engine want a general "solved rect of this
   element" query? (0120's caret-cell need is the same question one
   level down.)
3. **NavList activate-on-move default**: sidebars conventionally
   navigate while arrowing, but page dyn disposal makes that
   expensive — 0550 carries both events and defers the default.
4. **Banner tinted grounds**: derive per-theme at theme build vs.
   reuse `surface_raised` — needs measured contrast across the 26
   built-ins before deciding (0560 records the method).
5. **overview.md fold-in**: three parallel seats cannot all edit the
   shared ledger; counts/ledgers update deferred to the single-writer
   merge pass (noted in the band README).

## Next cycles (this seat)

- Cycle 2: deepen 0500/0510/0530 (the P0 trio) with API sketches and
  keyboard tables (the 0120 decision-table style); reconcile against
  peers' cycle-1 READMEs by band; check for collisions with any 0170
  movement.
- Cycle 3: 0550–0580 interaction contracts in the same depth; validate
  the 0590 example scopes against whatever the ports epics have
  scheduled (adopt, don't duplicate).
- Cycle 4: promotion ordering + a proposed 0.x band mapping for the
  kit items (which ride the 0.2 breaking budget, which are additive),
  and the final overview.md fold-in text for the merge pass.
