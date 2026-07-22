# Extensions review of the control-plane track (cycle 2, 2026-07-22)

Reviewer lens: skeptical staff engineer + owner of the extension
architecture band (0400-0490). Method: full read of all seven items +
README, then source verification of the cited code reality — roughly
forty citations re-read at the file/line level (Driver::turn/pending/
handle_event/apply_resize/poison_prev/apply_caps_upgrade/finish/
wait_for_activity, accessibility snapshot + untracked sampling +
to_text, Actions run/register/list/ActionInfo, CaptureTerm scripted
reads, TerminalWaker, signal `Box<dyn Any>` cells, source ownership
rule, notices/viewport publisher pattern, suspend trait + unix impl +
self-stop test isolation, gltf_json depth cap/parse, idle + alloc pins,
pty harness kill/tty-state, capture example, the 435 µs perf figure).

**Verdict up front**: the code-reality quality of this track is the
best I have reviewed in this repo — every citation I checked is
accurate, including the subtle ones (posted jobs seeing only the
reactive runtime; `focus_events: true` in the default EnterOptions so
0300's focus pair fires without a posture change,
src/term/options.rs:83-92; App::mount running the closure once under
`create_root`, src/app/mod.rs:227-249, which makes 0340's
restore-before-mount argument sound). The findings below are design
challenges, not citation corrections — with three P1s that should be
folded before the maintainer reads the track as "v1-able".

---

## P1 findings

### P1-1 (0350) — Attach/detach is honest about presenter caps but NOT about ImageSession identity
**Problem.** 0350's v1-able grading leans on "Terminal-held image state
re-emission is the same lever (`img.dirty = true`, driver.rs:517-521
and 547-550)". I verified the lever — but it re-RENDERS placements; it
does not make the image *session* truthful. `ImageSession` (RT4-1,
"one session per terminal", src/app/driver.rs:107-109) models uploads
living in TERMINAL memory: kitty payloads are deduped against what the
session believes the terminal already holds, and `Driver::finish`
exists precisely because uploads outlive cells (driver.rs:434-450).
Attach SWAPS the terminal identity out from under that model: the new
real terminal holds none of the uploads the session believes are
resident (re-dirty may re-place by id without re-uploading), and the
deletes owed to the OLD terminal went to a virtual screen that never
held pixels. Caps-DOWNGRADE attach (configured kitty graphics → real
256-color tty) is additionally unproven: `apply_caps_upgrade` was
written for the upgrade-from-conservative probe direction.
**Demand.** (a) Name "ImageSession identity across attach" as a
needs-design line in 0350 (the likely shape: attach = image-session
reset — treat the new terminal as a fresh session, drop slot
bookkeeping, damage-all; cheap and honest). (b) Recommend serve-mode
default caps be CONSERVATIVE (256-color, no graphics — exactly what
0360 already scopes) so first-attach is an UPGRADE through the proven
probe lever, and poorer-attach refusal/downgrade graduates to the
needs-design list with the session-reset answer. 0360 is unaffected
(it fixed no-graphics caps — correctly).

### P1-2 (0320 × 0400/0410) — The JSON promotion is a load-bearing precondition, not an implementation detail; and the placement question is answerable now
**Problem.** 0320 treats "move/alias `gltf_json` to a neutral home"
as step one of its checklist. It is more than tidiness: my 0410
(extensions band) proposes feature-gating `three` — today `gltf_json`
lives inside `src/three/` (exported at src/three/mod.rs:22). If 0320
lands first consuming `three::gltf_json`, the `three` module becomes
permanently ungateable (the control server would drag the 3D stack's
module in); if 0410 lands first, a later 0320 must do the move anyway
under more pressure. Also, the item's "keeping the `three` path
re-exported" must be cfg'd under the `three` feature or the re-export
itself re-couples.
**Answer (the placement question the review was asked to settle).**
Against my 0400 decision rule ("does a minimal app pay for it in-tree,
and does it have its own release cadence?"):
- **0310 bus = core, unconditional.** It IS the anchor surface being
  created — it needs a driver drain point (private seam by
  definition), it is small, and it is zero-cost unminted. A sibling
  crate cannot reach `Driver.pending`'s semantics without the engine
  exporting worse things.
- **0320 server = in-tree behind a default-OFF cargo feature**
  (`control-server`), not a sibling crate, in v1. Reasons: (i) a
  minimal app must not carry a listening-socket security surface it
  never asked for — feature-off = the constructor does not exist,
  which is stronger than "opt-in by call"; (ii) sibling-crate
  extraction during 0350/0360's evidence phase would put the highest-
  churn co-design (server ↔ VirtualTerm ↔ attach wire) across a
  release boundary — the coupling tax at its maximum exactly when the
  API is least stable; (iii) the protocol is frozen by an ADR WITH the
  engine, so it does not have its own release cadence yet — by the
  0400 rule that is a feature, not a crate. Revisit extraction after
  the protocol ADR + the 0.2 API pass if the weight grows.
- **0330 MCP bridge + the productized attach client = out-of-crate**
  (endorsing the track's own posture; the attach client can live as a
  feature-gated example for 0360's proof and become
  `abstracttui-attach` only if it productizes).
This introduces a second feature class to my 0400 ruling (default-OFF
opt-in surface vs default-ON trim) — I am amending 0400 this cycle to
carry both classes and this placement as an applied example.
**Demand.** 0320: promote the JSON move to a named precondition
("base::json (or render-neutral home) lands before or with the first
consumer, coordinated with extensions 0410; `three` re-export cfg'd
under the `three` feature"). README: update the extensions cross-track
edge from "nothing here assumes their design" to the concrete
placement above once 0400 v2 lands.

### P1-3 (0310) — "bounded, drop-oldest over `std::sync::mpsc`" is not implementable as written, and the difference is wire-visible
**Problem.** `subscribe` specifies fan-out "over `std::sync::mpsc`
(bounded, drop-oldest with a counted drop stat)". A
`sync_channel`'s `try_send` on a full queue can only REFUSE THE NEW
value (drop-newest); dropping the OLDEST requires owning both ends of
the buffer. The crate's own ingest module makes exactly this
distinction — `OverflowPolicy::{DropOldest, DropNewest}` over a
mutex-owned buffer, src/reactive/ingest.rs:55-63 — because mpsc cannot
express ring semantics. This is not pedantry: for an agent blocked in
`wait_for_event`, drop-NEWEST loses precisely the event it is waiting
for while preserving stale history; drop-OLDEST preserves liveness.
The choice is protocol-visible behavior.
**Demand.** Either (a) specify the egress buffer as the ingest shape
(mutex ring + condvar/waker, `DropOldest`, counted — the module the
track already cites as its "egress honesty precedent" is also the
correct mechanism), or (b) keep mpsc and write drop-newest honestly
into the contract. Recommend (a); (b) makes `wait_for_event` flaky
under burst by construction. Same fix applies to 0320's per-client
queue ("drop-oldest" there has the same mpsc impossibility if
implemented naively).

---

## P2 findings

### P2-1 (0310, 0330) — `inject(Resize)` diverges the model from the physical terminal on interactive sessions
`handle_event`'s Resize arm resizes the frame model and republishes
the viewport (driver.rs:508-533) — nothing resizes the REAL terminal.
Injected on an interactive session, frames are emitted for a geometry
the physical screen does not have until the next genuine SIGWINCH
heals it. 0330 even advertises "resize → inject Resize (agents test
layouts at sizes)". That use is only coherent headless (VirtualTerm
owns size truth). **Demand:** bus-level guard — accept Resize
injection only when the session terminal is not a real tty
(`Terminal::is_tty` exists, src/term/unix.rs:603-608), or spec it as
labeled layout-preview with the divergence + auto-heal written down.
0330's mapping should say "headless/serve sessions only".

### P2-2 (0320) — The stdio-transport guard's predicate is wrong as stated
"`Stdio` (headless-only: constructor refuses when a real terminal
session is entered)" is both too strict and unenforceable where it is
proposed. Too strict: a normally-launched interactive app resolves
/dev/tty and leaves stdin/stdout FREE (the stdio fallback is the
labeled degraded path, src/term/unix.rs:599-601 +
`UnixTerminal::degraded`, unix.rs:599-601 vicinity) — such an app can
legitimately serve JSONL over its pipes (the `abstractcode serve`
shape). Unenforceable: the constructor holds a bus handle, not the
Terminal, so "is a session entered" is not knowable there without new
plumbing. **Demand:** name the real predicate — "stdio is free iff the
terminal did not take the stdin/stdout fallback" — and the plumbing
that carries it (e.g. the App records the resolved-tty fact at
`run_prepared`, where `degraded()` is already read, src/app/mod.rs:
319-330, and the bus exposes it to the constructor). A guard that
cannot see the fact it guards on is documentation pretending to be
enforcement.

### P2-3 (0340) — No multi-instance story: the crash marker false-positives and snapshots last-writer-win
Two instances of the same app sharing a state path: instance B's clean
exit removes instance A's `running` marker (next start of A's crash
reads clean); both write the same snapshot file (atomic per-write, but
semantically last-writer-wins across instances). Editors solved this
with pid-bearing lock files; 0350 already designs lock-file liveness
for sessions. **Demand:** one paragraph — either "single instance per
state path is a documented assumption, violations detected by a
pid/boot-id lock (reuse 0350's lock shape)" or an explicit
instance-suffix convention. Silent mutual clobber is below the track's
own honesty bar.

### P2-4 (0310) — Actions are arg-less and value-less; say so and name the seam
`Actions::run(name) -> bool`, callback `FnMut()`
(src/app/actions.rs:125-148, 55-79): no parameters, no result payload.
The item adds `description` and stops. That is the right v1 — but the
limitation is invisible in the text, and it is the FIRST wall every
serious agent/extension consumer hits ("select node 42", "open file
X"). The workaround is real and should be stated: parameterized
interaction rides `inject` + the semantic tree (clicking is
universal). **Demand:** a Non-goals/Scope sentence naming arg-less
invoke as deliberate v1 + the recorded v2 seam (`invoke_with(name,
payload) -> Result<String>` beside it, decided when a real consumer
exists). See also the metadata answer below.

### P2-5 (0300) — Suspend obligations partially double the unix impl's own re-entry
The unix `suspend` already does leave → stop → re-enter WITH THE SAME
OPTIONS internally (src/term/unix.rs:610-631) — so "verb re-apply" is
partly done by `enter()` itself; what genuinely remains caller-side is
damage-all, size re-query, and LATCHED session verbs (cursor style,
title) outside EnterOptions. 0300's item 4 says the driver performs
"the documented obligations" wholesale. **Demand:** enumerate exactly
which obligations remain after `enter()`'s own work, citing
unix.rs:610-631, so the implementer neither double-applies enter
options nor misses the latched verbs.

---

## P3 findings

### P3-1 (0340) — A crc32 already exists in-crate; do not hand-roll a second table
`pub fn crc32` at src/gfx/png.rs:390 (used by png encode/decode). The
item hedges "miniz_oxide's adler/crc paths if exposed, else a
hand-rolled crc32 table". Reuse or promote the existing one. Note for
coordination: under my 0410 feature proposal, png stays UNGATED core,
so the symbol survives every trim combination.

### P3-2 (0300/0320) — Custom-event payloads: state opacity on the wire
v1 String payloads are right. Add one sentence to 0320: the protocol
treats event payloads as opaque strings end-to-end (apps may choose
JSON by convention; the server never parses or validates payload
content) — otherwise someone will "helpfully" schema-validate it in
the server and turn app events into a protocol-versioning problem.

### P3-3 (0320/0330) — Read-only should be a SERVER capability, not only bridge politeness
0330 puts read-only mode in the bridge ("query/subscribe tools only…
default posture for cautious hosts"). The hello already advertises
"enabled verb groups" — make read-only an explicit server-side config
in 0320 (serve with inject/invoke disabled), so the cautious posture
is enforced at the trust boundary instead of requested past it.

---

## Answer to the queued ask: extension action metadata (0310)

The ask: "does the bus verb set suffice as the neutral surface for
extension-registered actions/events, or do extensions need
registration metadata beyond name+description?"

**Position: name + description (+ the existing optional chord) is
sufficient v1 metadata — do not add schemas.** What diagram/graph
extensions (band 0400: node-graph editor 0430, auto-layout view 0440,
mermaid 0450) genuinely need, in order:
1. **A namespacing convention, not a field**: dotted action names
   ("graph.zoom_in", "graph.select_next") — zero new metadata,
   documented discipline, and it gives 0330's bridge stable tool
   grouping for free. `Actions::register`'s collision refusal
   (actions.rs:55-79) already enforces uniqueness.
2. **Parameterized invocation eventually — not now** (P2-4): an
   arg-less action cannot express "select node 42". v1 extensions
   drive parameterized interaction through `inject` + the semantic
   tree (our widgets annotate roles/labels/values via
   `Element::role/access_label/access_value`, src/ui/view.rs:246-264,
   exactly so pointer-path automation works). Record `invoke_with` as
   the v2 seam; design it against the graph editor when it exists —
   evidence before design, the house rule.
3. **Not needed: typed parameter schemas, capability flags, or return
   contracts in core metadata.** MCP tool schemas are the BRIDGE's
   presentation concern (0330 can derive per-action tools from the
   dotted namespace + description); baking schema vocabulary into
   `ActionInfo` would freeze agent-protocol shape into the engine —
   the exact inversion 0310's "bus before wire" rule exists to
   prevent.
4. One addition worth taking now because it is discovery, not schema:
   the semantic-tree row already carries role/label/value/bounds — a
   canvas-class widget (graph editor) is OPAQUE below the widget node
   (strokes are cells, not elements). That is correct and should stay;
   agents interact with such surfaces through extension-registered
   actions + custom events, which is exactly the bus contract. No new
   metadata required — but 0310's docs should state this division
   ("cell-drawn content is not in the semantic tree; expose intent as
   actions/events") so extension authors do not file it as a bus bug.

---

## What I verified and endorse without findings

- 0300's vocabulary + emission points: all verified real (including
  the focus pair being parsed-then-dropped at src/app/events.rs:
  120-124 and DEC 1004 on by default). Observable-only QuitRequested
  in v1: agree.
- 0310's injection-at-`pending` with `handle_event` parity: verified
  the routing order (overlays → tree → actions → default quit,
  driver.rs:474-506) and that `UiTree::dispatch` alone would bypass
  modal ownership. The design enters at the only correct point.
- 0340's central honesty claim — auto-serialization is structurally
  impossible (`Box<dyn Any>` cells, signal.rs:73-89; no serde by
  policy) — verified and correct; declared-keys is the only honest
  design.
- 0360's scoping (fixed conservative caps, no images, refuse second
  client) dodges P1-1 correctly for the proof; the experience-report-
  before-ADR discipline mirrors 0060/0050 and is right.
- The README's "what we will NOT do" list is the strongest part of
  the track; the no-TCP + same-uid trust boundary + closed verb set
  posture is the correct v1 security shape.

## Summary of demands by item
- 0300: enumerate post-`enter()` suspend obligations (P2-5).
- 0310: egress buffer mechanism (P1-3); Resize-inject guard (P2-1);
  arg-less invoke named + v2 seam (P2-4); canvas-opacity doc line
  (metadata answer §4).
- 0320: JSON promotion as precondition + cfg'd re-export, coordinate
  with 0410 (P1-2); stdio guard predicate + plumbing (P2-2);
  per-client queue mechanism (P1-3); payload opacity (P3-2);
  server-side read-only config (P3-3). Placement: default-OFF
  `control-server` feature (P1-2 answer).
- 0330: resize mapping headless-only (P2-1); read-only default rides
  server config (P3-3).
- 0340: multi-instance/lock story (P2-3); reuse png::crc32 (P3-1).
- 0350: ImageSession-identity needs-design + conservative serve caps
  (P1-1).
- 0360: no demands — proceed as scoped.
