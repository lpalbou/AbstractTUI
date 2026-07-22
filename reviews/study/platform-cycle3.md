# Platform track — cycle 3 (convergence)

Scope: fold of the extensions review of this track
(`reviews/study/extensions-on-platform.md`, 11 findings), closure of
the cycle-2 open questions, and the final coherence pass over the
control-plane band (0300–0390). House discipline held: every folded
finding was re-verified at source THIS session before landing (the
verification notes are per-fold below), not adopted on the reviewer's
word.

## Fold ledger (per finding, with source verification)

| Finding | Verified at | Folded into |
| --- | --- | --- |
| P1-1 ImageSession identity on attach | `src/gfx/session.rs:122-139` (sync dedupes against believed terminal state; "same version + new rect = kitty re-places WITHOUT retransmit"), session.rs:138-139 (channel-change reset exists — the gap is a same-channel terminal swap), driver.rs:434-450 (`finish` exists because uploads outlive cells) | 0350: new "ImageSession identity across attach" hard-parts entry (likely shape: session reset + damage-all on attach); conservative-default serve caps promoted from a mention to an adopted RULE with the upgrade-direction rationale; feasibility regraded (graphics-enabled serve explicitly blocked on the reset design); the misleading "image re-dirty is the same lever" line corrected to name its honest limit |
| P1-2 JSON promotion + placement | their 0410:61-69 (the precondition recorded from their side), 0400:118 (`control-server` feature ruling), `src/three/mod.rs:22` (parser location) | 0320: JSON promotion elevated from checklist step to NAMED PRECONDITION (before-or-with whichever of {0410 gate, 0320} ships first; `three` re-export cfg'd under the `three` feature); Placement block added to 0320 metadata (default-OFF `control-server` feature = compile-time absence; extraction revisited post-ADR + 0.2 pass); README items table, sequencing, non-goals and extensions edge updated |
| P1-3 mpsc cannot drop-oldest | `src/reactive/ingest.rs:49-75` (`OverflowPolicy::{DropOldest,DropNewest,Coalesce}` over a mutex-owned buffer — the crate's own statement that ring semantics need buffer ownership) | 0310 subscribe + 0320 per-client egress: mechanism specified as a mutex-owned DropOldest ring + wakeup, with the wire-visible starvation rationale (drop-newest loses exactly the awaited event); 0310 feasibility corrected (names the replaced mpsc sketch) |
| P2-1 inject(Resize) diverges on real ttys | driver.rs:508-533 (apply_resize reshapes the model only), `src/term/mod.rs:172-174` (`is_tty` default) | 0310: Resize injection accepted only when the session terminal is not a real tty, otherwise structured refusal; 0330: resize tool marked headless/serve-only, bridge surfaces the bus error verbatim |
| P2-2 stdio guard predicate wrong | `src/term/unix.rs:599-601` (degraded = stdin/stdout fallback), `src/app/mod.rs:323-331` (run_prepared reads the concrete terminal's degradation — the plumbing point) | 0320: predicate rewritten — stdio legal iff the pipes are FREE (tty resolved via /dev/tty, or headless); the `abstractcode serve` interactive+stdio shape now legal; plumbing named (App records the resolved-tty fact where the bus exposes it) |
| P2-3 multi-instance clobber | (design reasoning; no counter-evidence at source) | 0340: `running` marker upgraded to a pid-bearing exclusive lock — live-pid at load = refuse with labeled variant, dead-pid = crash; one-path-one-instance enforced, N instances = N paths documented |
| P2-4 arg-less invoke invisible | `src/app/actions.rs:31` (`run: Box<dyn FnMut()>`), actions.rs:12-13 (dotted names documented) | 0310: invoke's v1 limit stated inline + in Non-goals; `invoke_with(name, payload) -> Result<String>` adopted as the ONE reserved v2-seam name across both tracks' documents; dotted-namespace convention recorded as the discovery mechanism |
| P2-5 suspend obligations double enter() | `src/term/unix.rs:610-634` (suspend re-enters with the same options itself), 636-667 (latched verbs OUTSIDE EnterOptions: cursor style, title, pixel mouse) | 0300 item 4: the residual-obligation list is now CLOSED and enumerated (damage-all+poison, size re-query, latched-verb re-apply) with the do-not-double-apply warning |
| P3-1 second crc32 | `src/gfx/png.rs:388-390` (`pub fn crc32`, compile-time table) | 0340: container checksum reuses/promotes `gfx::png::crc32`; notes png stays ungated core under 0410 so the symbol survives trims |
| P3-2 payload opacity | (protocol design) | 0320: event payloads opaque strings end-to-end; server never parses/validates payload content |
| P3-3 read-only as server capability | (0320's own hello design) | 0320: verb-group mask in the serve config, advertised in hello, disabled verbs answer structured errors; 0330: bridge derives its tool list from hello's enabled groups — enforcement at the trust boundary, not politeness past it |
| Metadata answer §4 canvas opacity | `src/ui/access.rs:13-16` (only annotated nodes + text leaves appear) | 0310: canvas-class content opaque by design; intent exposed as actions/events; doc line mandated |

Also folded: 0310's SemanticTree query now REQUIRES composing the root
tree + visible overlay trees in z-order (`src/app/overlays.rs:61-66` —
overlay worlds are separate UiTrees; a root-only snapshot would
describe covered content and omit the modal the user sees). Surfaced
by the z-allocator resolution below; previously unstated.

## Cycle-2 question resolutions

### (a) The dynamic-z allocator's home — RESOLVED
Written as the cycle-3 addendum in
`reviews/study/platform-on-appkits.md`: the anchored popup core lives
in app-kits 0500 (core widget code); the engine delta is ONE query on
`app::Overlays` — top z among visible tree overlays (the same
iteration `dispatch` already performs, overlays.rs:303-318) — and 0500
allocates `top_z + 1`, modal, per open. No allocator object, no new
band constants. Control-plane needs NO public z API: the bus drains
inside the driver, which holds the overlay store — the composed
SemanticTree is built internally (0310 updated), and the wire just
serializes it.

### (b) The a11y-completeness rule — ADR candidate text (for the 0170 pass)
Drafted for the integrator to lift verbatim when `docs/adr/` stands
up:

> **ADR candidate — the accessibility snapshot is the machine-readable
> UI contract.** Every user-actionable affordance a widget renders
> (buttons, per-row action hit-zones, removable chips, fold glyphs,
> dividers) MUST be discoverable through
> `UiTree::accessibility_tree()`: as its own annotated element where
> it is focusable, or through the owning widget's `access_value`
> where it is a geometric hit-zone. Secret-bearing state MUST be
> redacted at the widget itself — a masked input masks its
> `access_value` together with its draw; no downstream consumer
> re-filters. Rationale: the snapshot is the single machine-readable
> UI state, consumed by tests, by the control-plane export surfaces
> (automation bus, wire protocol, protocol bridges), and by any
> future platform a11y bridge — an affordance absent from it is
> invisible to all of them, and a secret present in it leaks through
> all of them. This makes binding what `src/ui/access.rs:9-11`
> already states as intent ("if a widget's state is not in this tree,
> a screen reader could never say it"). Enforcement: acceptance tests
> assert affordances appear before they are exercised; a
> widget-lint-style membership test pins the rule for in-tree
> widgets.

Origin trail: platform-on-appkits.md F2 (masked-value leak,
`src/widgets/input.rs:210`) + F6 (table action hit-zones invisible);
both accepted by the app-kits track in cycle 3.

### (c) Extensions' question — does the graph/canvas lane need a control-plane query beyond SemanticTree/ScreenText?
**No new bus query.** Structured model export is the extension/app's
job, and v1 already composes it from existing verbs: a nullary
`invoke("graph.export")` triggers the extension to emit a custom app
event (0300) whose OPAQUE string payload carries the model — the
controller receives it via subscribe, over the bus or the wire.
Honest limits, stated: (1) `invoke` returns bool, so v1 export is
fire-then-listen, not request/reply — workable for tooling, clumsy
for interactive agents; (2) event payloads ride the bounded DropOldest
egress ring, so LARGE models must not travel as event payloads — the
honest big-export pattern is the app writing an artifact and the
event carrying its path. When the graph editor (0430) exists and this
composition proves too clumsy, `invoke_with(name, payload) ->
Result<String>` upgrades export into a direct request/reply — which is
precisely the evidence the reserved v2 seam waits for. A dedicated
model-export query is refused on principle: it would bake one
extension family's data shape into the neutral bus surface — the
inversion the bus-before-wire rule exists to prevent.

## Final track sequencing (coherence pass outcome)

1. **0300** lifecycle events — the foundation; consumed by all four
   successors (metadata cross-links verified in every item).
2. **0310** automation bus — core, unconditional; DropOldest ring
   egress; Resize guard; composed-tree query.
3. **0320** wire + serve — default-OFF `control-server` feature;
   PRECONDITION: JSON promotion coordinated with extensions 0410;
   protocol ADR at close.
4. **0330** (out-of-crate bridge) and **0340** (independent after
   0300; first consumer = app-kits 0520) proceed in parallel once
   their gates open.
5. **0350** design → **0360** proof (conservative caps, no graphics) →
   experience report feeds 0350's needs-design list and the 0320 ADR
   before any freeze.

No item contradicts a settled cross-track decision as of this pass
(checked against: extensions 0400 placement ruling + 0410
precondition; app-kits' accepted 0250 ruling + F1-F11 acceptances +
0520-claims-0340; the grep for stale mechanism words — mpsc,
headless-only stdio — returns only the corrected texts).

## For the integrator (overview fold, single-writer pass)

- Band 0300–0390 enters the proposed ledger as 7 items; the
  cross-track edges to record in `overview.md` sequencing: 0320↔0410
  (JSON promotion precondition, either-direction ordering), 0340↔0520
  (first Persist consumer + acceptance evidence), 0250's ruling
  (accepted by app-kits; the List/Table engine fixes should cite the
  ruling text in platform-on-appkits.md), 0300 before
  0310/0320/0340/0350.
- Two engine deltas surfaced by the kits/control convergence that
  belong to no existing item and should be assigned homes in the fold:
  the `app::Overlays` top-z query (smallest home: app-kits 0500's
  checklist, engine-delta-flagged for 0170) and the subtree-scoped
  focus step (app-kits 0510, same flagging — their F7 acceptance).
- The ADR candidate clause above is queued content for the 0170 pass
  (first ADRs); it should ride whichever ADR wave stands `docs/adr/`
  up, not wait for a control-plane item.
- `Role` non-exhaustiveness (platform-on-appkits F9) affects BOTH
  app-kits (Tree/TreeItem variants) and my 0320 (protocol enums must
  not freeze the engine's role vocabulary) — one 0170 line settles
  both; worth a named row in the fold.
