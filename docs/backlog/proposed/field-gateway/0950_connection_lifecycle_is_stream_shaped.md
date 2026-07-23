# Proposed: reactive::connection assumes a persistent transport — probe-shaped clients cannot adopt it

## Metadata
- Created: 2026-07-23
- Status: Proposed (field-gateway, gateway-console build)
- Severity: P3 — evidence note; the hand-rolled enum is small
- Class: API-fit evidence (for the connection-lifecycle surface)

## Context
The gateway console was expected to be a natural `reactive::connection`
consumer (the engine guide names it "the gateway link"). It is not, and
the reason is structural: the console's transport is request/response
(probe on demand, then discrete admin calls), not a stream. The
lifecycle vocabulary doesn't map:

- `Reconnecting { attempt, next_in }` — there is nothing to reconnect;
  a failed probe waits for the OPERATOR to fix the URL/token, not for a
  backoff timer. Auto-retry against a wrong token would even be hostile
  (hammering 401s).
- `Degraded(reason)` — no stream to degrade.
- What the console DOES need the engine has no word for:
  `Unauthorized` as a first-class peer of `Unreachable` (the two states
  an operator fixes differently — the app's whole connection screen is
  built on that distinction).

The console hand-rolled a five-variant `ConnPhase` (NotConnected /
Probing / Connected(identity) / Unauthorized / Unreachable) in ~20
lines; `connection()` + `Backoff` would have been more code and wrong
semantics.

## Current code reality
- `src/reactive/connection.rs:484` (0.2.8): `connection(cx, backoff,
  dial)` — the dial-fn/retry-timer/stale-report machinery is all about
  keeping ONE transport alive. Correct for its class (the docs are
  explicit); no fit for request/response config tools.

## Repro
Not a defect — an adoption mismatch: try to express "operator typed a
wrong token; show 401 and wait for edits" in `ConnState`. The closest
is `Degraded("401")` with retries disabled, which misuses both words.

## Workaround in the field (delete if the surface ever widens)
The app-local `ConnPhase` enum (src/store.rs). No engine change is
necessarily wanted — this is filed as validator evidence: the second
app on the engine did NOT consume `reactive::connection`, and the
reason is shape, not quality. If more probe-shaped apps appear, a tiny
`probe`-class helper (state enum + "probe once, report" and NO retry
machinery) may deserve to exist beside it; one consumer is not enough
to justify it.
