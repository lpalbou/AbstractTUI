# Completed: Connection lifecycle model + jittered reconnect/backoff helper

## Metadata
- Created: 2026-07-21
- Status: Completed (fix wave 3, FIXNET — graduated DIRECTLY from
  proposed/: the item's own promotion-evidence section records the
  trigger as fired with two studies' evidence, and the recommended
  "promote at the next single-writer pass" IS this pass; a paper stop
  in planned/ would have recorded no additional fact)
- Completed: 2026-07-23

## ADR status
- Governing ADRs: None (this repository has no ADR system yet). ADR impact: None by itself —
  but the API shape should be reviewed together with the 0050 transport decision so the state
  model does not accidentally encode one transport's semantics.

## Context
The standing external critique names exactly one "real unknown" in AbstractTUI's networking
story: **network-driven reactivity + reconnect under the frame loop**. Items 0010/0020 settle
the reactivity half mechanically; reconnect is the half with no in-repo precedent at all. The
reference domain shows the target shape: the agora client survives drops with an exponential
backoff loop — 0.5 s doubling to a 30 s cap, reset on clean EOF, resubscribing its desired
channel set on every reconnect (~/projects/a2a/src/agora/client/client.py:423-433, :53), with a
REST catch-up path beside the push plane. Every long-lived networked app on this engine will
need precisely this machinery, and it interacts with the frame loop's idle discipline — which is
why it deserves a designed answer rather than N hand-rolled ones.

## Current code reality
- Nothing connection-shaped exists in-crate (no state enum, no backoff helper; verified by
  reading src/reactive/, src/app/, and grepping for reconnect/backoff).
- The idle machinery a disconnected app must cooperate with:
  `App::drive_loop` (src/app/mod.rs:354-387) blocks in `wait_for_activity` with **no deadline**
  when nothing is pending, and in `wait_until(next_timer_deadline())` when one-shot timers are
  armed (mod.rs:369-380). `reactive::after` (src/reactive/animate.rs:141-146) registers a
  one-shot timer that costs **zero wakeups until due** — the natural backoff timer.
- Worker-side waiting is equally free: a background thread sleeping between reconnect attempts
  costs the UI loop nothing; it reports transitions via `WakeHandle::post`
  (src/reactive/scheduler.rs:74) and its death is surfaced loudly (`spawn_worker`,
  scheduler.rs:163; src/app/driver.rs:249-252).
- Honest degradation surfaces exist for the offline state: `use_startup_notices`
  (src/app/mod.rs:196-208), Badge/status-line widgets, and Toast for transient "reconnecting"
  notices (src/app/popups.rs).

## Problem or opportunity
Without a shared model, each app invents its own connection enum, its own backoff (usually
without jitter — thundering-herd on hub restart), and its own answer to "what does the frame
loop do while offline". The wrong hand-rolled answer is a busy-wait or a poll timer that
destroys the engine's zero-idle-cost property; the right answer (worker sleeps; UI blocks; a
state signal drives the offline rendering) is non-obvious enough that the reviews call it the
port's one real unknown.

## Proposed direction
1. A reusable connection-state model as a plain signal-friendly enum — indicative:
   `ConnState { Offline, Connecting, Online, Reconnecting { attempt: u32, retry_at: Instant } }`
   — owned by the app's UI thread, written only via posted transitions from the connection
   worker (the 0010 ownership rule applied). The UI renders it like any signal (badge, status
   line, dimmed panes).
2. A jittered exponential backoff helper (pure, no I/O): `Backoff::next() -> Duration` with
   base/cap/reset — the agora parameters (0.5 s, ×2, cap 30 s, reset on clean close) as
   defaults, plus full jitter to avoid synchronized retry storms. Usable from either side:
   worker-thread sleeps, or UI-side scheduling via `reactive::after`.
3. A documented answer to "the frame loop while disconnected": the worker owns the retry clock
   and sleeps; the UI thread stays blocked in `wait_for_activity` (idle, zero wakeups) and
   repaints only on posted state transitions. If an app wants a visible countdown, that is an
   ordinary one-shot timer, billed as such. Never a poll loop.
4. Catch-up belongs to the app/transport (per-channel cursors in the reference domain), not to
   this model — the state enum must not grow transport-specific fields.

## Why it might matter
This is the named de-risking gap between "the primitives are proven" and "someone can leave a
networked app running overnight". It is also the item the 0060 watcher milestone exercises
under real failure (hub restart mid-session).

## Promotion criteria
Promote to planned/ when the 0060 watcher build starts (it needs this on day one), or when any
second real consumer appears. The API shape should be validated against a real disconnect/
reconnect cycle before the helper's surface is declared stable — which is exactly what 0060
provides; designing the full surface before that evidence exists is the risk that keeps this
item in proposed/.

## Validation ideas
- Unit: backoff sequence (base→cap, reset, jitter bounds); state-transition legality.
- Integration (CaptureTerm + injected clock via `Driver::set_clock`, src/app/driver.rs:204): a
  scripted worker posts Offline→Reconnecting→Online; assert rendered states and that idle turns
  between transitions emit zero bytes.
- Soak-shaped check under 0060: kill and restart the hub; the app reconnects, catches up, and
  the session survives without manual action.

## Non-goals
No transport code, no TLS, no socket ownership (0050 owns that decision); no automatic catch-up/
replay logic; no engine-loop changes — the existing idle machinery is sufficient by design and
this item must prove that, not work around it.

## Guidance for future agents
Re-read `App::drive_loop` and the damage contract before designing: the model must ride the
existing timer/wake machinery. Check whether 0010 landed with a helper — the connection worker
should be expressible as an ordinary 0010 source whose emitted values are state transitions.

## Promotion evidence (2026-07-22, convergence cycle 2 — the trigger has effectively fired)

The first shipped application has now hand-rolled this item once, without jitter — the exact
thundering-herd risk this item names. Evidence (FIELD study 2,
`reviews/study2/field-consumer-tensions.md` §3.5; consumer = `abstractcode-tui`):

- hand-rolled SSE parsing over ureq: `gateway/sse.rs`, 125 lines;
- reconnect loop in `runner.rs:923-1012`: **linear** backoff `500ms × consecutive_errors`
  capped at 5 s (runner.rs:1006-1008), an 8-iteration REST poll fallback, terminal-status
  probes, fatal-status classification (runner.rs:962-978) — and **NO jitter**;
- the app-class report (`reviews/study2/field-app-classes.md` class 4) adds the multiplier: an
  entity-monitoring surface runs per-entity replay streams + roster polling, making the
  hand-roll N× worse; its class-3 coordination UI rides the same lane. That report calls this
  "this study's strongest cross-track recommendation."

The item's stated promotion criterion was "the 0060 watcher build starts, or any second real
consumer appears" — the first consumer exists and a whole app CLASS is named behind it.
**Recommendation: promote to planned/ at the next single-writer backlog pass.** The 0060-first
caution stands only for declaring the API surface STABLE (validate the shape against a real
disconnect cycle); it should not keep the item in proposed/ while consumers accrete divergent
hand-rolls. 0050 (transport ADR) correctly stays gated on watcher evidence — the consumer
proved ureq-class blocking HTTP in a worker thread workable meanwhile.

## Completion report (2026-07-23, fix wave 3 — FIXNET)

Final path: `docs/backlog/completed/live-data/0040_connection_lifecycle_reconnect.md`.

**Shipped** (`src/reactive/connection.rs` + `connection_tests.rs`,
re-exported from `reactive::` and the prelude; ~460 source lines +
a split test file, both under the file budget):

- **`ConnState`** — the transport-agnostic lifecycle a UI renders:
  `Connecting / Connected / Degraded(String) / Reconnecting { attempt,
  next_in } / Closed`. Deliberately a CLOSED vocabulary (the item's
  own non-goal: the enum must never grow transport-specific fields;
  documented in-type, consistent with ADR-0003's closed-vocabulary
  clause). `next_in` is a `Duration` so "retry #2 in 1.4s" renders
  without clock math.
- **`Backoff`** — pure full-jitter exponential schedule: uniform in
  `[0, min(cap, base × 2^attempt)]`, defaults base 500 ms / ×2 / cap
  30 s (the agora client's parameters, per the item), `reset()` on
  success, `seeded(n)` for deterministic tests, `ceiling()`/`attempt()`
  as the honest observables. PRNG is the crate's own xorshift64 shape
  (the particles precedent) — zero new dependencies.
- **`connection(cx, backoff, dial)`** — the machine: state as a
  `Signal<ConnState>` owned by `cx`; `dial` runs on the UI thread once
  per attempt (birth + each retry) and spawns the app's transport work;
  the `Clone + Send` **`ConnectionEvents`** reporter crosses threads on
  the posted-jobs lane (`connected`/`degraded`/`failed`/`closed`, plus
  `is_closed`/`is_current` worker stop conditions). Retries ride the
  EXISTING timer heap (`arm_timer_at`/`cancel_timer`, the interval
  precedent — injected test clocks stay authoritative); cancellation =
  `close()` / `retry_now()` / scope death (`on_cleanup` cancels the
  armed one-shot and drops the dial fn; the cleanup touches only
  Rc-held state, so it is safe after the arena freed the nodes).
- **Generation stamping** — accepting a failure supersedes the attempt
  (gen bump BEFORE scheduling), so a zombie worker's late reports are
  inert-and-counted (`stale_reports`, the `dead_sends` convention)
  and can never flip the live attempt's state.
- **Docs**: `docs/live-data.md` § "Connection lifecycle" (mermaid state
  diagram, worker-thread example, the full-jitter/thundering-herd
  rationale, the three enforced rules); `docs/api.md`
  § `reactive::connection`; CHANGELOG under `[Unreleased]`.

**Honest non-goals held**: no network I/O, no threads, no TLS, no
socket ownership (0050's question — the dial fn is the seam); no
catch-up/replay (transport policy; stated in both docs); no engine-loop
changes (the item demanded the existing idle machinery suffice — it
did: one armed one-shot while reconnecting, nothing at all while
`Closed`).

**Validation** (14 tests green first run):

- Backoff: ceiling monotone base→cap (`backoff_ceiling_grows_monotone_
  to_the_cap`), draws within `[0, ceiling]` and under the cap across
  seeds with real variance (`backoff_draws_stay_within_the_jitter_
  bounds`), reset re-bases, zero-base degeneracy safe.
- Machine: state-sequence GOLDEN under scripted failures (birth →
  fail → `Reconnecting{1, ≤base}` → timer-fired redial → connect →
  degrade → fail (schedule RESET by the connect: attempt 1 again) →
  close; exactly 7 transitions), degraded-from-Connecting counts as
  impaired connect, cancel-mid-reconnect removes the armed entry
  ENTIRELY (`next_timer_deadline() == None` — a dead connection may
  not bound the idle sleep), scope-disposal-mid-reconnect ditto plus
  inert-counted late reports, stale-attempt reports refused while the
  live attempt's land, transport clean close terminal, `retry_now`
  consumes the wait, re-entrant close from inside `dial` (no borrow
  collision, dial fn dropped not restored), Send contract pinned
  end-to-end through a real thread.
- **Zero idle cost when Closed** pinned on all three loop meters: no
  armed timer, no pending posted jobs, no frame tasks, and a far-future
  clock fires nothing.

**Stability note**: the 0060 watcher milestone remains the surface's
first REAL disconnect/reconnect exercise (the item's stability caution
stands — this API is shipped additive, not declared 1.0-frozen; 0170
audits it with everything else).

**Follow-ups revealed**:

- The consumer migration (`abstractcode-tui` runner.rs:923-1012 linear
  no-jitter hand-roll → `connection` + `Backoff`) is the ports-track
  proof, on their next engine bump — same acceptance shape as 0297's
  retire-deferral deletion.
- If the 0060 watcher wants a visible countdown ("retry in 3…2…1"),
  that is an app-side `interval` per the docs; if it turns out every
  consumer builds one, a `Reconnecting`-aware countdown helper is a
  one-evening follow-up — file only on the second real request.
