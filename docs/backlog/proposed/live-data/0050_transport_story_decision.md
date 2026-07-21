# Proposed: Transport story — HTTP + WebSocket + TLS, and the dependency decision

## Metadata
- Created: 2026-07-21
- Status: Proposed
- Completed: N/A

## ADR status
- Governing ADRs: None — and note the explicit state: **this repository has no ADR system at
  all** (no docs/adr/; the dependency policy lives as a "hard rule" section in
  docs/design/00-vision.md:46-53, amended via reviews/). ADR impact: **Needs new ADR.** This
  decision is durable cross-cutting policy (dependency posture, security surface, crate
  identity) and should become the repository's first ADR, bootstrapping docs/adr/ in the
  process. Do not resolve this item by editing the vision doc alone.

## Context
The standing critique: AbstractTUI "ships no async/HTTP/WebSocket story". The engine's
dependency policy is deliberate austerity — std + five tiny crates, everything else hand-rolled
(Cargo.toml:19-34; docs/design/00-vision.md:46-53: "Nothing else without an integrator-approved
review note"). A networked app needs HTTP long-poll and/or WebSocket, and in practice TLS. The
robustness review's position (reviews/cycle11/robustness-and-chat-port.md, chat-need table):
"none in-crate (austere dependency policy is a crate policy, not an app constraint) — bring
ureq/tungstenite or a tokio runtime on background threads — fine." That is evidence for one
option, not a decision; the decision must be made explicitly and recorded, because it defines
what kind of crate AbstractTUI is.

## Current code reality
- `Cargo.toml:19-34` — dependencies: `unicode-width`, `unicode-segmentation`, `miniz_oxide`,
  plus `libc` (unix) / `windows-sys` (windows). No feature flags exist today.
- `docs/design/00-vision.md:46-53` — the hard rule, with the amendment path (integrator-approved
  review note). Hand-rolled inventory includes ANSI, input parsing, JSON, PNG, base64 — but
  nothing socket- or TLS-shaped, and hand-rolling TLS is not a credible option (security review
  burden disqualifies it).
- The engine's embedding contract already accommodates any transport without engine changes:
  I/O lives on background threads and crosses via `WakeHandle::post`
  (src/reactive/scheduler.rs:74; reviews/cycle11/robustness-and-chat-port.md §R4 names the two
  supported shapes, including app-owned loops via non-blocking `Driver::turn` +
  `wait_until`, src/app/driver.rs:222,417). There is deliberately no fd-level embedding (no raw
  fd on the `Terminal` trait) — a unified external reactor is not a supported shape.
- The reference target needs both transport styles: the agora hub speaks WebSocket push with
  REST catch-up and a ≤55 s `/inbox` long-poll fallback (~/projects/a2a/src/agora/client/
  client.py:47,156,339-443).

## Problem or opportunity
Every networked consumer must answer "which HTTP/WS client, and does the engine help?". Leaving
it unanswered produces either policy drift (a future contributor adds a convenience dependency
casually) or user confusion (the critique reads the silence as incapability). The options have
materially different costs and must not be decided by default.

## Proposed direction — options, deliberately not decided here
**Option A — engine stays transport-agnostic (documented posture).** The app brings its own
client (e.g. `ureq`, `tungstenite`, or a tokio runtime confined to background threads); the
engine offers exactly the signal-binding + wake plumbing (0010/0020) and a docs section naming
known-good MIT/Apache pairings and the background-thread confinement rule. Cost: each app makes
a transport choice; the "no async story" critique is answered by documentation, not code.
Benefit: zero dependency-policy change, zero security surface added, crate identity intact.

**Option B — optional feature-gated transport helpers in-crate** (`net` feature): minimal
HTTP/1.1 long-poll + WebSocket client behind a default-off feature. The crux is TLS: it forces
`rustls` (itself pulling `ring`/`aws-lc`) or platform TLS — a large, security-sensitive
dependency tree entering the engine's manifest even if gated. Cost: audit surface, maintenance,
policy exception that weakens the "five tiny crates" identity. Benefit: batteries-included
first-run for networked apps.

**Option C — separate companion crate** (e.g. `abstracttui-net`, own repo/versioning): the
Option-B surface without touching the engine manifest; the engine's only obligation is keeping
the 0010/0020 seams stable. Cost: a second artifact to maintain and version; discovery depends
on docs. Benefit: engine purity + a supported answer.

Evidence to date leans A (the robustness review's judgment, plus the fact that 0060 can be
built under A with off-the-shelf clients — which is itself the cheapest way to generate the
missing evidence). The decision belongs to the integrator via the ADR, informed by 0060's
experience report.

## Why it might matter
This is the fork between "a rendering engine you bring I/O to" and "a full application
framework". Both are defensible; drifting between them is not.

## Promotion criteria
Promote when 0060 (watcher milestone) completes and its experience report exists: which client
crates were used, what friction the app-side transport ownership actually caused, whether any
engine seam was missing. Write the ADR then — with evidence, not taste. If a decision is forced
earlier (e.g. an external consumer blocked on guidance), Option A can be adopted provisionally
as documentation-only, explicitly marked as awaiting the ADR.

## Validation ideas
- The ADR exists (first in docs/adr/), states context/decision/consequences, and names the
  rejected options.
- If A: docs page section (extends 0030's page) with a worked pairing and the confinement rule;
  no Cargo.toml change (test: manifest diff empty).
- If B/C: license + audit review of the chosen tree; feature-gated CI job; the engine's default
  build remains exactly the five-crate set.

## Non-goals
This item does not authorize adding any dependency, does not select client crates, and does not
build transport code. It exists so nobody does those things implicitly.

## Guidance for future agents
Do not close this from the armchair: the 0060 report is the input the decision is waiting for.
When writing the ADR, also record the fd-level-embedding non-goal (R4's honest limit) so
tokio-first expectations are managed in the same document.
