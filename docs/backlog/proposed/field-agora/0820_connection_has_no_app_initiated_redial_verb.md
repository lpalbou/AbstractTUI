# Proposed: Connection has no app-initiated re-dial from Connected/Degraded — planned transport switchovers must masquerade as failures

## Metadata
- Created: 2026-07-23
- Status: Proposed (field-agora, agora-tui build)
- Severity: P3 — workaround holds but mislabels a planned transition for one badge-frame
- Class: API gap

## Context
agora-tui runs `reactive::connection` over a WS dial with a REST long-poll
fallback: when the WS handshake fails but `/healthz` answers, the attempt
reports `degraded("long-poll fallback (WS unavailable)")` and serves
`/inbox?wait` — the exact `ConnState::Degraded` story. Each poll cycle
also probes whether WS came back, and here is the gap: when it does, the
app WANTS "drop this attempt, dial again now" — a planned, healthy
switchover. The only lever that re-enters the dial loop from a live
attempt is `events.failed(reason)`:

- `Connection::retry_now()` is a no-op unless the state is already
  `Reconnecting` (src/reactive/connection.rs:462) — correct for its
  purpose (skip a pending wait), unusable as "re-dial now".
- `Connection::close()` is terminal by contract (`:449`).
- So the long-poll worker reports `failed("WS restored — re-dialing
  push")`, and the header truthfully-but-absurdly shows
  "○ reconnecting (attempt 1) in 0.2s" for a moment on what is actually
  an UPGRADE. The backoff attempt counter also increments for a
  non-failure.

The same verb would serve credential rotation and subscription-set
changes (agora-tui pins its watched-channel set per session today partly
because there is no clean "re-dial with fresh state" path).

## Current code reality (0.2.8)
- `src/reactive/connection.rs:462` — `retry_now` early-returns unless
  `Reconnecting`.
- `:342` — `Report::Failed` is the only report that supersedes the live
  attempt's generation and schedules a dial; there is no
  `Report::Redial`-class transition (and no UI-side equivalent).
- `ConnState` is deliberately closed (":65 — growing this enum would be
  a breaking change this crate refuses") — this ask is about a VERB, not
  a state: a planned re-dial can render as the existing `Connecting`.

## Repro
Any dial fn with two transports of different quality: connect via the
fallback, then try to promote to the primary when it recovers. The only
paths are (a) stay degraded forever, or (b) report a fake failure.

## Workaround in the field (delete when fixed)
`src/hub/transport.rs::long_poll` in agora-tui: on a successful WS probe,
`events.failed("WS restored — re-dialing push")` and return. An engine
`Connection::redial_now()` (or `ConnectionEvents::superseded(reason)`)
that bumps the generation, skips the backoff draw (or resets it), and
goes straight to `Connecting` would let the app delete the fake-failure
label and keep the attempt counter honest.
