# 0320 — Control wire protocol + serve seam: JSONL over stdio/unix socket, opt-in

## Metadata
- Created: 2026-07-22
- Status: Proposed
- Track: control-plane
- Depends on: 0310 (the bus this serializes), 0300 (event vocabulary);
  JSON promotion coordinated with extensions 0410 (named precondition
  below)
- Placement (settled, cycle 3 — extensions review P1-2 + their 0400
  ruling): **in-tree behind a default-OFF `control-server` cargo
  feature**, not a sibling crate. Feature off = the server module and
  its constructor DO NOT EXIST — compile-time absence is a stronger
  security stance than opt-in-by-call, and a minimal app never carries
  a listening-socket surface it did not ask for. Rationale against
  extraction now: the server ↔ VirtualTerm ↔ attach-wire co-design
  (0350/0360) is the highest-churn coupling in the band and must not
  sit across a release boundary while the protocol is unstable; the
  protocol freezes by ADR WITH the engine, so it has no independent
  release cadence (the 0400 crate-vs-feature rule). Revisit extraction
  after the protocol ADR + the 0.2 API pass.
- Completed: N/A

## ADR status
- Governing ADRs: None yet. **Needs ADR before closing** (this repo's
  third named ADR candidate after 0050/0170): the protocol schema and
  the security posture are durable public contracts; record version-1
  framing, verb set, and the trust boundary in `docs/adr/` when this is
  scheduled.

## Context
The bus (0310) is an in-process Rust API. A controller that is NOT the
same process — a test runner, another program, an agent through a
bridge (0330), the attach client (0360) — needs a wire. The family
precedent is direct: the coding-console port epic targets `abstractcode
serve`, a long-lived JSONL stdin/stdout protocol
(`docs/backlog/proposed/ports/README.md:15-18`), so JSONL is already
the dialect AbstractTUI apps will sit next to. This item defines the
protocol and the opt-in serve seam that hosts it — transport-agnostic
at the seam, two concrete transports in v1.

## Current code reality
- **JSON is in-crate, parse-only**: `src/three/gltf_json.rs` — a
  hand-rolled, depth-capped (`MAX_DEPTH = 128`, gltf_json.rs:26),
  grammar-strict DOM parser (`parse`/`parse_bytes`,
  gltf_json.rs:119-139), publicly exported (`src/three/mod.rs:22`) but
  named and documented as glTF-sized. There is NO serializer anywhere
  in the crate (`examples/capture.rs` writes its dumps by hand). The
  dependency policy hard-rules JSON stays hand-rolled
  (`docs/design/00-vision.md` "Dependency policy": serde is not on the
  allowed list; `Cargo.toml:16-18` — manifest additions only via
  integrator review).
- **Unix sockets need zero new deps**: `std::os::unix::net::UnixListener`
  is std; the crate already cfg-gates unix code
  (`src/term/mod.rs:51-52`).
- **The stdio conflict is real but NARROWER than it looks**: the unix
  terminal resolves /dev/tty first and only falls back to stdin/stdout
  as its labeled degraded path (unix.rs:599-601 `degraded()`). So a
  normally-launched interactive app leaves its pipes FREE (JSONL over
  them is legal — the `abstractcode serve` shape); the pipes are the
  terminal wire only on the fallback path, and headless serve (0350)
  has no terminal at all. The transport seam must enforce exactly this
  predicate — see §2 — not a blanket "no stdio while interactive".
- **Threading model exists**: blocking reader threads +
  `WakeHandle::post`/bus enqueue + `TerminalWaker`
  (`src/term/waker.rs:46-63`) is the engine's one concurrency shape
  (`src/reactive/source.rs:1-38`); the control server adds no async
  runtime.
- **Egress honesty precedent**: bounded ingestion with counted drops
  (`src/reactive/ingest.rs`, roadmap principle 4) — the event stream to
  a slow client must apply the same rule outward.

## Problem
No process boundary exists at all: today the only ways to drive an
AbstractTUI app are its keyboard or linking against it. Every external
consumer (harness, agent bridge, attach client) would invent its own
socket + framing + threat posture — three incompatible ad-hoc servers
in the first year, none security-reviewed.

## What we want
1. **Protocol v1 (JSONL, one object per line)**, mapping 1:1 onto bus
   verbs — the protocol is a SERIALIZATION of 0310, never a second
   behavior surface:
   - `hello` (server → client on connect): protocol version, app name,
     capability list of enabled verb groups.
   - `inject` (client → server): semantic input — key with mods, mouse,
     paste, resize; NEVER raw escape bytes (parsing hostile bytes is
     the engine's job at its real input seam, not the protocol's).
   - `query` / `reply` (id-correlated): the 0310 query set; the
     semantic tree serializes from `AccessSnapshot` rows
     (`src/ui/access.rs:97-104`) — role/label/value/focused/bounds
     /depth, machine shape first, `to_text` form available for humans.
   - `invoke` / `result`: run a registered action by name
     (`src/app/actions.rs:125`); unknown action = structured error.
   - `subscribe` / `event`: 0300 lifecycle + custom events; per-client
     bounded egress, drop-OLDEST, drops counted and REPORTED in-band
     (an event `{"type":"dropped","count":N}` — the labeled-degradation
     rule on the wire). **Mechanism (settled, extensions review
     P1-3)**: a mutex-owned ring with the crate's own
     `OverflowPolicy::DropOldest` semantics
     (`src/reactive/ingest.rs:55-63`) + a writer-thread wakeup — NOT
     `std::sync::mpsc`, whose `try_send` can only refuse the NEW value
     (drop-newest). The difference is wire-visible: for a client
     blocked in a wait-for-event, drop-newest loses exactly the event
     it is waiting for while preserving stale history; drop-oldest
     preserves liveness. Custom-event payloads are OPAQUE strings
     end-to-end — the server never parses or validates payload content
     (apps may adopt JSON by convention; schema-validating app events
     server-side would turn them into a protocol-versioning problem).
   - `error`: every malformed line gets a structured reply naming the
     defect; the connection survives malformed input (parser armor:
     depth cap + line-length cap, mirroring `MAX_STRING_LEN` thinking
     in `src/testing/vt.rs:22-25`).
2. **The serve seam**: `ControlServer::spawn(bus, transport)` where
   transport is an enum — `UnixSocket(path)` | `Stdio` | `FdPair(read,
   write)` (embedders/tests). One accept thread, one reader + one
   writer thread per client, all blocking, all joined on shutdown.
   Client cap (small N, refuse-with-error above it). **Read-only serve
   config (extensions review P3-3)**: the constructor takes a verb-group
   mask (e.g. queries+subscribe only, inject/invoke disabled); the
   `hello` advertises exactly the enabled groups and disabled verbs
   answer with a structured `error` — the cautious posture is enforced
   at the trust boundary, not requested past it by polite clients.
   **The stdio predicate (corrected, extensions review P2-2)**: stdio
   transport is legal iff the process's stdin/stdout are FREE — i.e.
   the terminal resolved /dev/tty rather than taking the
   stdin/stdout fallback (the labeled degraded path,
   `src/term/unix.rs:599-601`), or no terminal session exists at all
   (0350 headless serve). "Refuses when a session is entered" was both
   too strict (a /dev/tty-resolved interactive app can legitimately
   serve JSONL over its free pipes — the `abstractcode serve` shape)
   and unenforceable from a bus handle. Plumbing: `App::run_prepared`
   already reads the concrete terminal's degradation state
   (`src/app/mod.rs:323-331`); it additionally records the
   resolved-tty fact where the bus can expose it to the constructor —
   a guard must SEE the fact it guards on.
3. **Security posture v1 (documented in-repo, enforced in code)**.
   The threat model, stated plainly:
   - **Trust boundary = the local user account.** The control socket
     lives in a 0700 directory, socket file 0600, owner-only. Anyone
     who can open it already runs code as the same uid (and could
     ptrace the process or read its memory) — the socket adds no
     privilege that uid does not have. Cross-user and remote attackers
     are OUT of this surface by construction: no TCP, no abstract-
     namespace sockets (filesystem path only, so permissions apply).
   - **File permissions ARE the authentication in v1** — deliberate,
     documented, and re-verified at bind time (the server refuses to
     serve a pre-existing path and re-checks dir/file modes after
     bind). No token handshake in v1: a token stored where the same
     uid can read it authenticates nothing the permissions don't; if
     a multi-user or brokered posture is ever wanted, authentication
     is a NEW reviewed item, not a patch.
   - **Capability, not code**: verbs are the closed set above — no
     eval, no filesystem verbs, no process spawn; `invoke` reaches
     only actions the app registered. Opt-in only (no env-var
     auto-enable — the app calls the constructor).
   - **What the wire carries**: everything the semantic tree and
     screen text carry — so redaction is at the WIDGET source (masked
     inputs mask `access_value`; see 0310 "Cross-track answers"), and
     the threat note must say the event/query stream shows whatever
     the user sees.
   A one-page threat note in `docs/` ships WITH the feature, not
   after, containing exactly the four bullets above plus the client
   cap and drop-counter behavior.
4. **JSON promotion is a NAMED PRECONDITION, not a checklist step**
   (extensions review P1-2; their 0410 records the same edge from the
   other side): the parser lives inside `src/three/` today
   (`gltf_json`, exported at `src/three/mod.rs:22`) and extensions 0410
   proposes feature-gating `three`. Whichever of {0410's gate, this
   item} ships first, the move to a render-neutral home (`base::json`
   or equivalent) lands BEFORE or WITH it — otherwise the control
   server dragging `three` in makes the module permanently ungateable,
   or the gate strands the server's parser. The legacy
   `three::gltf_json` path stays re-exported UNDER the `three` feature
   (an unconditional re-export would re-couple). On top of the
   promoted parser: a small emit module — string escaping,
   finite-number formatting, no reflection: hand-written `to_json` per
   protocol type.
5. **Conformance fixture**: a scripted-transport test suite (FdPair over
   in-memory pipes) pinning every verb, every error shape, and the
   drop-counter behavior — the fixture 0330's bridge and 0360's client
   develop against.

## Scope / Non-goals
Scope: protocol v1, the three transports, the JSON writer + parser
promotion, security note, conformance fixture, docs page.
Non-goals: TCP/TLS/HTTP/WebSocket (explicitly NOT this item — and
deliberately independent of live-data 0050, which decides app-DATA
transports); authentication schemes beyond fs permissions (needs its
own review if ever wanted); windows named pipes (needs-design: the
`windows-sys` feature set in `Cargo.toml:28-34` lacks Win32 pipe APIs —
defer to a follow-up with its own manifest review); protocol v2
concerns (streaming frame mirrors — that is 0350's channel, designed
there).

## Feasibility
**v1-able on unix + free-pipes stdio + fd-pair; windows transport
needs-design (deferred).** Everything rides existing machinery: the
parser exists (promotion is a move — a named precondition coordinated
with extensions 0410), the writer is a day of code, the threading
shape is the engine's own, the socket is std, the egress ring is the
ingest module's own overflow semantics pointed outward. The genuinely
new engineering is protocol DISCIPLINE: id correlation, error grammar,
bounded ring egress, and the free-pipes stdio predicate with its
plumbing. Risks to name honestly: (a) protocol stability — hence the
ADR gate; (b) the `gltf_json` promotion touches `three`'s public path —
coordinate with 0170's API budget AND 0410's gate ordering; (c) a
slow/hostile client must never block the UI thread — enforced
structurally (writer thread owns the socket write; DropOldest ring
between UI and writer; reader thread owns parsing).
Idle cost: `control-server` feature off = the module does not compile
into the binary; feature on but server not constructed = nothing
exists; server on with zero clients = one accept thread blocked in
`accept()`, zero UI wakeups — the `tests/adv_app.rs:54` pin extended
to a serve-enabled app must stay green.

## Expected outcomes
Any process can drive any consenting AbstractTUI app with a shell
one-liner (`socat` to the socket); the MCP bridge (0330) and attach
client (0360) are thin clients of a reviewed surface; both port epics
gain scriptable end-to-end acceptance harnesses.

## Validation
- Conformance fixture (FdPair): hello/inject/query/invoke/subscribe
  round-trips; malformed-line survival; oversized-line rejection; drop
  counting under a stalled reader; concurrent clients.
- CaptureTerm + socket acceptance (unix): a real UnixListener session
  driving a modal interaction end-to-end; socket file mode asserted
  0600; second-client-over-cap refused with a structured error.
- Idle pins: serve-enabled idle app = zero UI wakeups/bytes.
- Docs: protocol page rendered from the same constants the code uses
  (no drift between doc and wire).

## Progress checklist
- [ ] PRECONDITION: JSON promotion to a neutral module (with 0410;
      `three` re-export cfg'd under the `three` feature) + emit module
- [ ] `control-server` default-OFF feature scaffolding (module absent
      when off; idle pins compiled both ways)
- [ ] Protocol types + (de)serialization + version handshake
- [ ] Transport seam (UnixSocket / free-pipes Stdio + resolved-tty
      plumbing / FdPair)
- [ ] Reader/writer thread pair + DropOldest ring egress with counted,
      in-band-reported drops
- [ ] Security: perms, client cap, closed verb set, verb-group
      read-only mask, threat note
- [ ] Conformance fixture + acceptance tests + idle pins
- [ ] ADR: protocol v1 + trust boundary
