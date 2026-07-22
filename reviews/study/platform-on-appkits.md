# Platform review of the app-kits track (cycle 2 cross-review)

Reviewer: platform seat (control-plane band 0300–0390). Scope: all ten
app-kits items + README, read fully; "Current code reality" claims
spot-checked against source this session (verified among others:
`src/widgets/mod.rs:19-40` module list, `src/widgets/table.rs:48-57,
73-76,153-174,266-372` struct/select/draw shapes,
`src/widgets/list.rs:1-13,54-64,99-120,139-141`,
`src/widgets/tabs.rs:3-8,26-32,155-166,183-208`,
`src/app/overlays.rs:45-66,194-229,299-381` layer_tree/on_outside/
dispatch, `src/app/popups.rs:29-32,44-70`, `src/ui/access.rs:27-51`,
`src/ui/event.rs:95-105,223-265`, `src/ui/compose.rs:56-155`,
`src/widgets/input.rs:35-41,206-213`, `src/ui/focus.rs:28,297`,
`src/render/md.rs:14-17`, `examples/dashboard/main.rs:105-121,241-260,
342-362`). Verdict up front: this is a well-grounded track — the
citations are accurate to a degree unusual for planning docs, scopes
are mostly single-problem, and the 0250 lessons are taken seriously.
The findings below are where the design contradicts the engine as
built, where a queued cross-track obligation was missed, and where
secrets would leak into my band's export surfaces.

## Findings

### F1 (P1, 0500) — The popup z/routing claim is architecturally wrong
0500 "What we want" §1: "the popup's z sits below `MODAL_Z` so a select
inside a modal still layers correctly." Verified against the engine,
this cannot work, on two independent axes:
- **Compositing**: `Compositor::flatten` z-sorts layers; a popup layer
  with z < 1000 paints UNDER the modal panel (`MODAL_Z = 1000`,
  `src/app/popups.rs:31`) — a select opened from inside a modal would
  be invisible where they overlap.
- **Routing**: `Overlays::dispatch` walks targets topmost-z-first
  (`src/app/overlays.rs:318` `sort_by_key(Reverse(z))`), and a MODAL
  tree swallows every key unconditionally (overlays.rs:354-356) and
  every mouse event (overlays.rs:326-349, incl. outside presses at
  330-336). A popup below the modal's z receives NO input while any
  modal is open.
Additionally, `on_outside_press` fires only for MODAL trees
(overlays.rs:56-60 doc, 330-336 code), so the popup must itself be
`modal: true` to get its dismiss gesture at all.
**Demand**: rewrite §1's stacking rule — the popup is a modal tree
overlay allocated a z ABOVE the topmost currently-live modal (dynamic
allocation, e.g. an `Overlays` top-z query or allocator; a single
static named slot cannot serve nested cases: select inside modal
inside…). Keep the existing acceptance bullet ("popup inside a Modal
layers and focus-restores correctly") — it becomes the proof — and add
the stacked case (two modals + popup). State the focus story across
trees explicitly (the modal tree retains its focus state while the
popup tree exists; closing the popup returns key ownership by the
cycle-5 focused-overlay rule, overlays.rs:358-365).

### F2 (P1, 0510) — Masked input must mask the accessibility value, or it leaks secrets
0510 §5 scopes masked mode as "only the draw substitutes `•` per
cluster". Verified engine reality: `TextInput` exposes its RAW value
through the semantic tree — `.access_value(move || value.get_untracked())`
(`src/widgets/input.rs:210`). If masked mode changes only the draw, an
API key typed into a masked field appears in plaintext in every
`UiTree::accessibility_tree()` snapshot (`src/ui/tree.rs:170`) — which
is precisely the surface my band exports: the 0310 automation bus's
SemanticTree query, the 0320 wire protocol (off-process), and the 0330
MCP bridge (to an agent's context). The control-plane posture is
redaction-at-source: the server never re-filters widget state, so the
WIDGET must not surface it.
**Demand**: 0510 §5 must define masked = draw substitution AND
`access_value` substitution (same `•` form or a `"(hidden)"` marker)
while masked; the reveal toggle restores both together. Add a
validation bullet: with a masked field populated, the accessibility
snapshot text contains no plaintext fragment of the value (a leak
test, not a rendering test). This is cheap to build and impossible to
retrofit safely once consoles ship API-key forms.

### F3 (P1, 0520 + README + 0580) — "Serializable by construction" is false, and the queued Persist ask went unanswered
Three places claim control-plane state capture needs nothing:
0520 Non-goals ("the wizard's signals-only state is serializable by
construction and that is the deliberate seam"), 0580 §1/§3 ("a
control-plane (0300s) state snapshot sees a plain signal, by
construction… the named seam the 0300s control-plane band can
serialize without new API"), README dependency shape ("state
serialization should find all kit state in signals (it does, by
construction)"). Engine reality: signals are `Box<dyn Any>` arena
cells (`src/reactive/signal.rs:73-89`); there is no reflection and no
serde (`Cargo.toml:19-34`). Nothing can serialize "a plain signal".
The control-plane design (0340) is therefore a DECLARED-KEYS registry:
apps (or kits) register key + version + encode/decode closures. Signals
being the state substrate makes kit state *registrable*, not
serializable — the kit knows its value types (String/bool/choice keys),
which is exactly what makes mechanical registration possible.
Separately, the cycle-1 cross-review ask ("claim the first Persist
consumer — wizard draft survival — and pressure-test 0340's
ergonomics") is not reflected in 0520 at all: 0340 is never referenced
by number.
**Demand**: (a) fix the three phrasings ("registrable via the
control-plane 0340 registry because kit value types are constrained" —
not "serializable by construction"); (b) 0520 adds an explicit
"Persist integration (0340)" clause: opt-in wizard draft persistence —
step values + `current` + `visited` registered under keys derived from
wizard/step ids, versioned — and a validation bullet: kill the process
mid-wizard, restart, restore report offers the draft, Back/Next state
intact (this is ALSO 0340's acceptance evidence for its restore/mount
ordering, the hardest open question my cycle-1 report names); (c) 0580
references 0340 in its persistence recipe (divider position + expanded
set = two registered keys, shown in the doc).

### F4 (P2, 0500) — Combobox key routing is unresolved: a modal popup swallows the typing
0500 §3 says the Combobox trigger "is a `TextInput`" and §1 makes the
popup a modal overlay (required for `on_outside_press`, per F1). Those
two choices collide in the dispatch rules: while a MODAL tree is
visible it receives every key (`src/app/overlays.rs:354-356`) — so the
user's typing routes into the popup tree, not into the trigger
`TextInput` sitting in the tree below. The item never says where the
editor lives while the popup is open, and each resolution has costs:
(a) the editor is MOUNTED INSIDE the popup tree while open (closed
trigger renders a mirror) — works with today's engine, small visual
jump; (b) the popup is non-modal and steered by signals from the
trigger's key handler (Up/Down/Enter forwarded as highlight-signal
writes) — keeps typing in place but loses outside-press dismissal
(modal-only, overlays.rs:56-60) and needs an engine delta to extend
outside-press (or focus-loss dismissal) to non-modal trees.
**Demand**: pick (a) or (b) IN the item, and if (b), name the engine
delta explicitly as a dependency (it lands on `app::overlays`, which
0170's stability pass will freeze). The shared-popup-core framing
should also say plainly that Select (no typing) and Combobox (typing)
have different routing needs — "one popup core" is true for geometry/
dismissal, not automatically for key ownership.

### F5 (P2, 0530) — Space is double-booked: activation vs multi-select toggle
0530 §3: activation fires on "Enter/Space"; §5: "Space-toggles"
multi-select marks. With multi-select enabled both meanings claim the
same key. The ruling below (requested by this track) settles it:
**Space toggles wherever a toggle exists; Enter activates, always.**
**Demand**: encode that rule — in a multi-select table Space toggles
the row mark and never activates; in a single-select table Space may
alias Enter (or do nothing — pick one and test it); `Checkbox`
(`src/widgets/checkbox.rs:3-4`) and 0540's toggle chips already live
by it.

### F6 (P2, 0530) — Action hit-zones are invisible to the semantic tree (and therefore to automation)
0530 renders row actions as geometric hit-zones inside the table's
single draw closure (correct engineering call — the header-click
precedent, `src/widgets/table.rs:222-243`). But hit-zones are not
elements: they carry no role, no label, no focusability — they do not
exist in `accessibility_tree()`. `Table` exposes one element with one
`access_value` (table.rs:253-254). Consequence for my band: an agent
driving the admin console through 0310/0320 can see the table's
selected row but CANNOT discover that Edit/Rotate/Delete exist, let
alone which is highlighted — the admin console's defining interactions
become invisible to automation and to any future screen-reader bridge.
0530's validation section has no a11y bullet at all (0570, by
contrast, adds `Role::Tree/TreeItem` and access values).
**Demand**: add an a11y clause: the table's `access_value` (or a
per-row value function) must carry, for the selected row: the row
label/key, the available action ids, and the currently highlighted
action; validation asserts the snapshot names an action before it is
fired. Also add a track-README rule (it belongs to the whole band):
**every interactive affordance a kit widget renders must be
represented in the accessibility snapshot** — roles/labels/values at
minimum — because that snapshot is the machine-readable UI state the
control plane exports.

### F7 (P2, 0510) — enter_advances smuggles in an engine focus API without naming the dependency
0510 §4 needs "a 'focus next within this element' call — the focus
engine already computes traversal order for Tab; expose the same step
scoped to a subtree". Verified: no such API exists — `focus_next()`
(`src/ui/focus.rs:28`) is tree-global, `focus_next_in(dir)`
(focus.rs:297) is spatial. A subtree-scoped traversal step is an
`ui::focus` engine change, owned outside this kit item and subject to
0170's API budget. The same applies (more mildly) to §5: masked mode
modifies the shipped `TextInput` widget.
**Demand**: an explicit "Engine deltas" line in 0510 naming (a)
subtree-scoped focus step (ui/focus, new public API) and (b) TextInput
masked mode (existing widget, additive builder), both flagged for 0170
sequencing — so the kit item cannot appear self-contained when it is
not.

### F8 (P2, 0560) — "Zero new tokens" contradicts the tinted-ground plan
0560 §5 wants banner grounds "derive[d] at theme-build via
`theme::derive` walks… no new tokens". Verified: derivation at
theme-build produces values that must land in token SLOTS — the
`TokenSet` is a fixed 36-token model; there is no per-tone ground slot
to fill, and computing the tint in the widget is lint-forbidden
("NO color arithmetic in widget code — derived shades come from
theme::derive at theme-build time or not at all",
`src/widgets/mod.rs:8-15`). So the choices are: new tokens (a theme
governance change, not "zero"), or the fallback the item itself names.
**Demand**: commit v1 to the fallback (`surface_raised` ground +
semantic ink + tone-colored leading glyph/hairline — all existing,
audited pairs) and file tinted grounds as an explicit theme-lane
follow-up (new tokens + contrast-audit pairs, sequenced with 0170)
instead of an implementation-time "decision recorded" that can only
discover the contradiction later.

### F9 (P3, 0570) — `Role` is not `#[non_exhaustive]`; adding Tree/TreeItem is technically breaking
Verified: `Role` derives only Copy/Clone/Debug/PartialEq/Eq
(`src/ui/access.rs:30-31`) — downstream exhaustive matches break on
new variants. The item half-knows ("check 0170's non_exhaustive
stance"). **Demand**: promote that aside to a named 0170 coordination
line: either `Role` gains `#[non_exhaustive]` in the 0.2 budget
(control-plane consumers like my 0320 serializer want that too —
protocol enums should not freeze the engine's role vocabulary), or the
variants ride the same budgeted break as the other 0.2 shapes. Minor
wording nit in §1: "the widget owns fold state (`expanded:
Signal<HashSet<String>>` — bindable" — a bindable signal is app-owned
state the widget CONSUMES; say that (it matters for F3's
registrability story: fold state is a registrable key precisely
because the app owns the signal).

### F10 (P3, 0590) — The acceptance-test mechanics are unspecified
"each with a CaptureTerm acceptance test beside it" + "a test asserting
each shipped app-kit widget is imported by at least one validator".
Examples are binaries — `tests/` cannot import them, and no current
example has a beside-it acceptance test (the capture pipeline drives
built binaries under a pty instead, `examples/capture.rs:279-322`).
Feasible paths exist (a shared compose fn in `examples/common/`
consumed by both the binary and an integration test; or `#[path]`
includes; the import-law meta test can be a source-scan like the
widget-membership lint, `src/widgets/mod.rs:171-178`).
**Demand**: pick the mechanism in the item — the completion law
("consumed by a 0590 validator", enforced by a meta test) is the
band's best idea and deserves a stated implementation shape, or it
will quietly degrade to "we ran it by hand".

### F11 (P3, 0560) — Notices bridge will classify by string convention; note the 0300 seam
The bridge recipe (persistent→banner, transient→toast) must classify
`use_startup_notices` STRINGS (`src/app/notices.rs:40-45`; the
dashboard already special-cases `caps:` prefixes,
`examples/dashboard/main.rs:105-121`). Honest for v1 (the "area:
state (detail)" convention exists, `src/app/mod.rs:196-200` doc), but
brittle. **Demand**: one sentence in the recipe acknowledging the
string-convention dependency and cross-referencing control-plane 0300
(typed lifecycle/degradation events are the eventual classifier; the
recipe should migrate when 0300 lands rather than grow its own parser).

## What holds up (for the record)
- Citation accuracy is high everywhere I checked; notably 0530
  correctly identified that `Table::select` runs its ensure-visible
  `offset.update` AFTER the user callback (table.rs:163-172) — the
  same disposal hazard 0250 documents for List — and demands the
  bookkeeping-before-callbacks inversion. Correct, and the right
  general law.
- 0500's honesty about anchoring (rects only inside handlers via
  `EventCtx::current_rect`, event.rs:223-265; Table/Tabs precedents at
  table.rs:178, tabs.rs:130) is accurate and the v1 open-from-handler
  scoping is the right call.
- 0550's Tabs critique is verified (titles are `Vec<String>`,
  tabs.rs:26-32; the title walk has no overflow handling,
  tabs.rs:186-208; panel disposal semantics tabs.rs:3-8) and the
  NavList/FilterTabs split (panel-switching vs re-query) is the right
  reading of the cycle-7 router ruling.
- 0580's mounted-through-collapse default (state survives; lazy
  opt-in) correctly generalizes the Scroll-vs-Tabs state lesson
  (scroll.rs:3-9 vs tabs.rs:3-8).
- 0590's validators-grow-with-the-band + completion law is the
  strongest process idea in the study; F10 only asks it to be
  mechanically real.

## The 0250 ruling (queued ask, answered)
As the seat owning lifecycle/event surfaces, I propose the following
ruling for 0250, to be encoded once and inherited by 0500/0530/0550/
0570 (and Feed when it grows selection):

1. **Selection follows movement.** Arrows/Home/End/Page keys/single
   click MOVE the selection; `on_select` is (and is documented as) a
   selection-changed NOTIFICATION. Widgets must never wire commitment,
   navigation, or destruction to it.
2. **Activation is a distinct, explicit event.** `on_activate(target)`
   fires on: **Enter** (always — the universal commit key, and the key
   automation can always inject); **Space** only in widgets where
   Space has no toggle meaning — where a toggle exists, Space toggles
   (multi-select marks, chip-as-filter, tree-branch fold). For PURE
   toggles the toggle IS the activation, so Enter and Space
   legitimately coincide (the shipped `Checkbox` contract: "Space/Enter
   toggles", `src/widgets/checkbox.rs:3`); the rule bites in SELECTION
   widgets, where Space claimed by marking must never double as
   activate. **Mouse**: click on an already-selected item activates;
   click on an unselected item only selects. No double-click synthesis
   — `MouseKind` carries no click count (`src/ui/event.rs:95-105`); if
   a validator proves the need, click-count synthesis becomes its own
   engine item rather than a per-widget hack.
3. **Commit-on-move is opt-in, never a default.** A per-widget knob
   (0550's `activate_on_move`) is legitimate only where the committed
   act is cheap, idempotent, and non-destructive (filter re-query:
   yes; page navigation: opt-in with the disposal cost named;
   apply/destructive acts: never). Default OFF everywhere — including
   sidebars (0550's deferred default resolves to off; the item's own
   text names the per-keystroke page-disposal cost of on).
4. **Disposal-safety law (the 0250 crash class, made structural).**
   A widget completes ALL internal bookkeeping (ensure-visible math,
   offset clamps) BEFORE invoking any user callback, and user
   callbacks MAY dispose the widget's scope synchronously. Applies to
   List (`src/widgets/list.rs:232-243` moves before the callback) and
   Table (`src/widgets/table.rs:163-172` same inversion), test-pinned
   per widget.
5. **Vocabulary.** `on_select` = selection changed; `on_activate` =
   user committed this item; `on_change` = bound VALUE committed
   (Select/Checkbox class — never fires on highlight movement);
   `on_navigate` = NavList's domain alias for activate (same rules).
6. **Why Enter-first matters beyond UX**: the control-plane band
   exports UI state and injects input (0310/0320); activation must be
   reachable by one deterministic key so agents and test harnesses can
   drive any widget without mouse-position knowledge. A click-only or
   double-click-only activation would break headless drivability.

## Cycle-3 addendum — F1 resolution: the dynamic-z allocator's home
Settled with the app-kits track: **the anchored popup core lives in
their 0500 (core widget code); the engine delta is one small query on
`app::Overlays` — "the highest z among visible tree overlays" (a
top-z read over the store it already owns, the same iteration
`Overlays::dispatch` performs at `src/app/overlays.rs:303-318`) — and
0500 allocates its popup at `top_z + 1`, modal, per open.** No
allocator OBJECT, no reserved band constant beyond the existing
`MODAL_Z`/`TOAST_Z` documentation (`src/app/popups.rs:29-32`): a
query+convention is the smallest thing that nests correctly
(select-in-modal-in-modal), and it lands under 0170's API budget as
one method. Control-plane impact checked both ways: my 0310/0320 need
NO z-order knowledge — the bus drains INSIDE the driver, which already
holds the overlay store, so the SemanticTree query composes root +
visible overlay trees in z-order internally (0310 now states this),
and the wire just serializes the composed snapshot. Toasts stay above
popups by the existing band constants; if a popup above `TOAST_Z` is
ever wanted, that is a design smell to refuse, not a knob to add.

**Amendment 2026-07-22 (cycle-3 close, resolving r2-cross-review F2)**:
the last sentence above is RETIRED — it described the static-band
world, and the shipped dynamic allocator contradicts it by
construction: `Overlays::top_z()` maxes over ALL live layers (draw
layers included), so a popup opened while a toast is showing allocates
above `TOAST_Z`, and that is the CORRECT behavior, not a smell. Two
facts make it safe: (1) toasts are passive, non-interactive draw
layers (no tree, no focus, no handlers) — a popup covering one creates
zero input conflict; (2) the popup is a transient, key-owning surface
the user is actively operating — hiding it under a passive
notification would invert the interaction priority. The overlap window
is toast-lifetime × popup-open and resolves itself when either ends.
Re-imposing toast superiority would need a static ceiling below
`TOAST_Z` — exactly the band arithmetic the dynamic allocator was
built to remove, and one a modal stack reaching z 1999 would break.
The refusal that STANDS: no reserved band constants beyond the
documented `MODAL_Z`/`TOAST_Z`, and no allocator object. Reality is
now also documented at the source: `src/app/anchored_owned.rs` module
doc ("Stacking note") and the band-constant comment in
`src/app/popups.rs`.

## Cross-track answer recorded (extensions ask)
The extensions track asked whether name+description is enough action
metadata for extension-registered commands (my 0310 bus). Answered in
0310 (see its "Cross-track answers" section): **v1 = name + optional
chord + optional description — actions stay nullary** (`Actions` run
closures take no arguments today, `src/app/actions.rs:31`, and nothing
in the engine parameterizes them); an optional JSON-shaped `args`
schema HINT plus a parameterized `run_with(name, args)` is RESERVED as
needs-design, to be built only when an extension presents a concrete
parameterized command (the 0320 `invoke` wire verb reserves the field
and rejects non-empty args against nullary actions with a structured
error, so the protocol will not need a breaking change).
