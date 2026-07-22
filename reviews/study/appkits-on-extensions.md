# APPKITS cross-review of the EXTENSIONS track (cycle 2)

Date: 2026-07-22 · Reviewer: APPKITS seat (band 0500–0590) · Scope:
`docs/backlog/proposed/extensions/` README + 0400–0470, read against
engine source. Method: skeptical spot-check of cited code (BrailleGrid,
mosaic, the md/rich/link pipeline, layout absolute positioning,
Cargo.toml/lib.rs/prelude, feed CustomBlock, mdpad sources), then
scope/honesty/dependency challenge.

## Verification result (credit first)

Citation precision is excellent. Everything I spot-checked matched to
the line: `BrailleGrid` + Bresenham + dot-bit order + the per-series
color rule (`src/widgets/chart.rs:34-45,49-115,82-105,326-327`),
`V_EIGHTHS` (chart.rs:400), the mosaic ladder + auto-pick rationale
(`src/gfx/mosaic.rs:2-4,47-51`), `Block::CodeFence`
(`src/render/md.rs:80-85`), `Span::with_link` (`src/render/rich.rs:54`),
`Surface::register_link` (`src/render/surface.rs:125`),
`Position::Absolute` + solver honoring (`src/layout/style.rs:94-101`,
`solve.rs:104,197`), `style_signal`/`hover_signal`/`capture_pointer`
(`src/ui/view.rs:149,271`, `src/ui/event.rs:241`), no `[features]` in
Cargo.toml + the integrator-owned manifest line, exact file line counts
(gltf_json 567, jpeg 599, kitty 333, sixel 482, iterm2 110,
viewport3d 560), and the mdpad quotes verbatim
(`mdpad src/render/mermaid.rs:1-14`). The findings below are about
design and honesty, not sloppiness.

## Findings

### P1-1 (0430, also 0450) — the link-stamping plan depends on a core seam that does not exist, and no item files it
0430 §6 plans edge hit-testing as "link-id stamping + 0165 resolution":
an edge draw stamps cells with `graph://edge/42`. But link ids are
minted by `Surface::register_link` (`src/render/surface.rs:125`) — a
method on `Surface`, not on the `StyledCanvas`/`Canvas` traits that
widget draw closures actually receive (`src/ui/canvas.rs:22-77`: only
put/print/fill/print_styled/fill_styled). A style can CARRY an id
(`render::Style::link`, style.rs:148) and `SurfaceCanvas::print_styled`
preserves it (canvas.rs:234-239), but nothing reachable from a draw
closure can REGISTER a URI to obtain the id — the rich-text pipeline
resolves URLs against `&mut Surface` directly (`resolve_link`,
`src/render/rich.rs:302-306`), a type extensions never hold. The same
gap breaks 0450's integration recipe (diagram-in-`Feed` rides
`CustomBlock`, whose draw is also `&mut dyn StyledCanvas` —
`src/widgets/feed.rs:95-100`): a diagram in a feed could render but
never carry activatable node/edge links.
**Demand**: file the core seam per 0400's own rule ("every capability
an extension needs and cannot reach is, by definition, a core backlog
item" — track README §2): either `StyledCanvas::register_link(uri) ->
u16` with an honest no-op default (plain canvases return 0 = no link),
or a build-time id-mint API. Natural home: fold into 0165's scope
(app-widgets band) since 0165 is the consumer-side half of the same
channel. Until filed, 0430 §6 and 0450 §5 must say "requires 0165 +
the registration seam", not "when 0165 lands".

### P1-2 (0440) — the knowledge-graph class is named as motivation but v1 serves it the degraded path
0440's Context names "knowledge graphs" first among the served classes,
then scopes v1 to layered/sugiyama-lite with cycle-breaking DFS and a
labeled grid-snap fallback for "graphs that defeat layering (dense,
near-clique)". Knowledge graphs are routinely cyclic, dense, and
non-hierarchical — exactly the defeat case: the named motivating class
gets either distorted layering or the grid fallback. Layered layout is
the right v1 for DAG-shaped data (dependencies, pipelines, states);
it is the wrong algorithm family for KGs, where force-directed layout
is the standard — and a zero-idle-compatible shape exists and is
proven in the maintainer's own tooling (the memory-graph observer:
alpha-cooled force placement that freezes on settle, reheats briefly
on arrivals, none under reduced-motion). Bounded, on-demand, terminates
— compatible with roadmap principle 5 as 0440's own force paragraph
already concedes ("bounded iteration count on demand").
**Demand**: pick one, in the item text: (a) re-scope the Context to
DAG-class graphs and name KGs explicitly as NOT served by v1 (honest
claims principle), or (b) promote the bounded on-demand force option
from "optional/research" to a designed v1.5 with the cooling/freeze
shape sketched. Do not ship a Context that advertises a class the
Scope degrades.

### P1-3 (0430) — one item carries an editor's worth of scope; stage it
0430's scope list is: model + card recipe + edge layer + pan + drag +
selection + rubber-band edge creation + tooltips + LOD tiers + dual
hit-testing + example + tests. That is a small product. The engine's
own history splits far smaller surfaces into separate items (Table,
List, Scroll each their own; 0120 is ONE widget with a checklist);
"needs-design" status does not substitute for staging — each
interaction cluster has independent risk and independently shippable
value in a sibling crate with its own cadence.
**Demand**: stage into named milestones with separate acceptance gates,
e.g. M1 canvas core (pan + node drag + selection over 0440's shared
rendering), M2 ports + rubber-band edge creation, M3 tooltips + LOD
tiers. And add the missing **keyboard story per milestone**: pan/drag/
selection/edge-creation currently have zero keyboard parity in the
item, against the engine discipline (every widget keyboard-first;
a11y state table docs/theming.md) — spatial focus between node cards
already has a precedent (`focus_next_in`, wired in
`examples/dashboard/main.rs:241-260`).

### P2-4 (0400) — the anchor-surface list omits surfaces its own track already consumes
0400 §3 names the extension anchor surface as Element/draw closures,
`StyledCanvas`, layout, reactive, `TokenSet`, and 0420. But 0430's
tooltips/popups ride `app::Overlays` (`layer_draw`/`layer_tree`/
`on_outside_press`, src/app/overlays.rs:158-229) — cited by 0430
itself and absent from the list the ADR would freeze.
**Demand**: add `app::Overlays` (+ `Modal`/`Toast` if extensions may
raise them) to §3; add the anchored-popup primitive (see the
cross-band resolution below) once it lands.

### P2-5 (0400/README) — app-kits are mis-classified as "the second family of sibling-crate candidates"
The track README's cross-band section labels band 0500–0590 a
sibling-crate family. Apply 0400's OWN decision table ("does a minimal
app pay for it if in-tree, and does it have its own release cadence?
Both yes = sibling"): a select/combobox (0500), form rows (0510), and
table upgrades (0530) fail the test — minimal apps (any config or form
surface) actively want them, and they have no separate cadence; they
are core-widget-class, same as Checkbox/Table today. Sweeping app-kits
into the sibling family by default would mean a minimal form app
installs an extension to get a dropdown — against the brief.
**Demand**: 0400's Validation already promises "a dry-run of the
classification rule against one item from each peer band" — run it and
record the outcome in the item: 0500-class controls = core; only
heavyweight composed kits (if any) are sibling candidates. Reword the
README line accordingly.

### P2-6 (0400) — coupling budget names the policy but not the mechanics that make it real
The workspace-CI-against-HEAD plan plus published-version deps has a
known footgun the item does not name: crates.io publishing requires a
`version` requirement, workspace builds want `path` — the dual spelling
`abstracttui = { version = "0.2", path = "../.." }` (or a `[patch]`
section) is the standard resolution, and publish ORDER (core first,
family after, same day) is part of the budget. Separately, 0450 and
0470 both ASSUME the five-crate dependency posture binds extension
crates ("this item assumes YES") — that is a policy question 0400
exists to answer and currently doesn't.
**Demand**: the ADR draft names the dependency spelling + publish-order
rule, and contains an explicit clause: the dependency posture binds
sibling crates (or doesn't, with bounds). Two sentences each.

### P2-7 (0450) — the subset table is good but not yet grammar-actionable
The YES rows pin diagram KINDS, not accepted SPELLINGS. Mermaid's
flowchart grammar has variant edge-label syntaxes (`-->|label|` vs
`--label-->`), `&`-chaining, quoted vs bare node labels; sequence
diagrams have a wider arrow vocabulary than `->>`/`-->>` (`->`, `-x`,
activation markers). An implementer cannot derive the parser's accept
set from the table, and the corpus has no version anchor (mermaid
evolves).
**Demand**: two cheap additions — (1) one explicit rule: "any spelling
outside the enumerated forms triggers the atomic fallback naming the
first unrecognized line" (this makes unknown SYNTAX, not just unknown
diagram kinds, safe by construction); (2) per YES row, enumerate the
accepted spellings, and pin the corpus to a named mermaid docs
version/date.

### P2-8 (0460) — two layer-target errors in otherwise fair gap division
The four-gap division (md tables, md images, anchors/TOC, search
overlay → app-widgets band; mermaid → extension) is fair and correctly
routed. Two corrections before the seeds are handed over:
(a) **md tables**: "typeset through the Table widget's width machinery"
mis-targets — the Table WIDGET is an interactive control (focus,
selection, sort — src/widgets/table.rs); a markdown table is a STATIC
typeset block in the rich pipeline. Share the width ALGORITHM
(`solve_columns`, table.rs:374-408, and mdpad's staged wrap as the
quality bar), not the widget. (I am correcting the mirror error in my
own 0530 this cycle — its non-goal claimed a Feed message embeds "a
real Table via FeedBlock::Custom"; CustomBlock is draw-only,
feed.rs:87-90, so that recipe is a painted table, not the widget.)
(b) **md images**: the item claims "the `Image` widget over the full
gfx ladder (src/widgets/image.rs:109-186 — kitty/iterm2/sixel/
mosaic)". False for the widget: `widgets::Image` is mosaic-only by
design ("always mosaic, because a draw closure owns cells, not escape
bytes" — docs/api.md; `from_path` is PNG-only, image.rs:117-130). The
full ladder lives in `gfx::ImageSession` + overlay image entries —
which do NOT flow inside scrolled document content. The md-images seed
must say: in-flow images = mosaic via the widget (correct and
universal); pixel-protocol inline images inside a scrolling document
are an OPEN compositing question needing its own design note.
**Demand**: reword both seeds before the integrator numbers them.

### P2-9 (0430/0440) — missing cross-band dependencies on app-kits surfaces
Node cards with "inline field slots" will immediately need one-of-N
params — that is 0500 (`Select`) inside a card, which also means the
anchored-popup primitive must open from an absolutely-positioned,
panned card (a placement case 0500 should list). An editor's inspector
panel beside the canvas is 0580 (`SplitPane`); node status dots are
0540 vocabulary.
**Demand**: add cross-reference lines (by band, per protocol) to
0430's Current code reality or Scope; nothing needs to move.

### P3-10 (0400) — module count nit
"16 top-level modules" — `src/lib.rs:31-47` carries 17 `pub mod` lines
(`prelude` included). Say "16 + prelude" or 17.

### P3-11 (README ledger) — 0430's dependency row contradicts its body
Ledger: "Depends on 0400, 0420, 0165". Body §6: a documented pre-0165
fallback exists (sampled-polyline distance). Align the ledger: "0165
(synergy; documented fallback)" — and after P1-1, add the registration
seam to the same cell.

### P3-12 (0420) — Progress overlap unstated
Eighth-block fills promote `V_EIGHTHS`, but `Progress` already draws
sub-cell precision bars. Fine to promote — name Progress as a second
refactor-onto-the-layer candidate (like chart.rs) or explicitly leave
it, so the dedup claim stays complete.

### P3-13 (0410) — per-combination API gate
Validation runs `cargo doc` per feature combination; add the 0170
public-api diff gate per combination too — feature-gated prelude
re-exports make the public surface combination-dependent, which is
exactly what that gate exists to watch.

## Cross-band resolutions (recorded here per cycle-2 protocol)

1. **Anchored-popup substrate — APPKITS owns it, in core, inside 0500.**
   Consumers now visible across three bands: 0500 Select/Combobox
   popups, 0120's completion dropdown (app-widgets, recipe), 0530
   row-action overflow menus, 0430 tooltips + in-card dropdowns
   (extensions). The shared core = anchored overlay over
   `Overlays::layer_tree`/`layer_draw` + `on_outside_press`: anchor
   rect in, open-below/flip-above/viewport-clamp, dismiss semantics,
   z-slot below `MODAL_Z`, focus restore; plus a passive hover-timed
   tooltip variant (layer_draw, non-focusable). It lands in CORE with
   0500 (choice controls are core-class per 0400's decision table),
   designed jointly with 0120; extensions consume it as public API and
   0400's anchor-surface list adds it (P2-4). 0500 has been amended to
   record downstream consumers incl. 0430's panned-absolute-card
   placement case.
2. **PLATFORM's ask "app-kits claims the first Persist (0340)
   consumer" — ACCEPTED.** The 0520 wizard is the consumer: one
   registry key per wizard instance, `read_fn` samples step values +
   current + visited (wizard-scope signals — 0520's data model already
   keeps values outside page scopes, so the sample is one struct
   read), per-key `u8` version = wizard schema version, restore policy
   = the app's "resume setup?" modal per 0340's app-decides rule. The
   `Restored`-handle-queried-inside-mount shape fits the wizard scope
   naturally; my one ergonomics note back: the wizard wants a
   `Restored::take(key)` (consume-once) so a declined restore cannot
   be re-applied by a later reader. 0520 amended with the dependency +
   a crash-resume validation line. Also answering their second
   question: yes — admin/chat kits want `Detached`-aware behavior
   (pause pollers/refresh timers on detach) as a documented kit
   convention; recorded for 0590's docs page, gated on 0350 existing.
3. **0250 movement-vs-activation ruling — LANDED and ADOPTED same
   cycle**: PLATFORM's `platform-on-appkits.md` appeared while this
   review was being written; its proposed 0250 ruling (selection
   follows movement; Enter always activates; Space toggles where a
   toggle exists; commit-on-move opt-in never default; bookkeeping-
   before-callbacks disposal law) is adopted across my band: 0530
   §3/§5 encode Enter-vs-Space per mode, 0550 resolves its deferred
   `activate_on_move` default to OFF (ruling §3), 0570 cites the
   branch-fold-is-a-toggle coincidence (ruling §2), 0540's chip
   Enter/Space coincidence is the pure-toggle case, and the README
   gains the vocabulary as a band rule. All eleven PLATFORM findings
   (F1-F11) were verified at source and folded the same cycle — the
   two P1 architecture errors they caught (popup z BELOW `MODAL_Z`
   would be invisible and deaf under a modal, overlays.rs:318,326-356;
   masked input leaking secrets through `access_value`,
   input.rs:210) are corrected in 0500/0510, and the three false
   "serializable by construction" claims (0520/0580/README) now say
   REGISTRABLE via 0340's declared-keys registry.

## Summary judgment

The track's spine is right: 0400's two-mechanism split is the correct
answer to the brief, 0420 is exactly the small core substrate to build
first, 0440-before-0430 is correct risk ordering, 0470's verdict is the
kind of standing answer that prevents scope drift. The three P1s are
all fixable in text this cycle: file the link-registration seam (P1-1),
stop advertising knowledge graphs v1 can't serve or design the bounded
force option (P1-2), and stage the editor (P1-3).
