# 0210 — EPIC: a2a chat TUI on AbstractTUI (agora hub client)

## Metadata
- Created: 2026-07-21
- Status: Proposed (epic; blocked on widget + live-data dependencies)
- Track: ports
- Completed: N/A

## ADR status
- Governing ADRs: None in this repo (no ADR system yet — see
  ../app-widgets/0170). The client ships as its own crate (or inside the
  a2a repo); engine API rulings from 0170 gate what it relies on.

## Context
The target is a live chat/coordination TUI over the agora hub, replacing
the line-oriented `agora chat` REPL (`~/projects/a2a/src/agora/chat.py`)
with a real windowed client: channel/DM sidebar with unread badges, a
live message list, a composer, and a members/votes/obligations panel.
The robustness review (reviews/cycle11/robustness-and-chat-port.md
Part 2) evaluated exactly this port: feasible now, the live-data path is
the engine's strongest suit for it, the message list is the one component
the engine does not yet provide in earnest — and it explicitly recommends
doing the upstream widget cycle first because the console port needs the
same widget.

## Current code reality
- Domain (read from `~/projects/a2a/src/agora/`): channels + canonical
  `dm:a--b` DM channels; append-only messages — `Envelope`
  (models.py:209+) carries channel/seq/sender/kind/status
  (`open|reply|fyi|blocked|resolved`), operator-only `critical`, urgency,
  to/reply_to, title ≤120 chars, markdown body ≤64 KB, structured data,
  attachments; importance derives from unforgeable/constrained signals,
  never a free-form sender priority.
- Transport (client/client.py:1-14): REST control plane + WebSocket push
  into an `Inbox`; the documented agent loop is drain → read → ack.
  Inbox (client/inbox.py:23-34): asyncio queue capped at 1000,
  **drop-on-full recoverable via hub cursors**, interrupt flag for
  critical/interrupt urgency. Long-poll `/inbox` (≤55 s) is the fallback
  lane. Reconnect/backoff is client-owned.
- Existing render semantics to preserve (chat.py:1-20,
  chat_render.py:1-40): current room in full, other rooms as one-line
  notices; message block = separator + header (time, sender, seq, status
  badge, trust flags) + optional title + wrapped body; previews cap at
  `BODY_MAX_LINES` with a `/read` hint, deliberate reads render uncapped;
  **acks are triage-seen and never discharge obligations** — the UI must
  keep that distinction visible. chat_render.py:29-33 strips control
  characters because attribution is the surface's trust anchor.
- Engine readiness verified by the review (Part 2 table): sidebar =
  `List` + `Badge`; live ingestion = `WakeHandle::post` (no tearing, no
  busy-wait, coalesced bursts — test-pinned); panels = `Table`/`List`/
  `Progress`; ballots/confirmations = `Modal`; transient cross-room
  notices = `Toast`; degradation surface = `use_startup_notices`;
  hostile-content safety is structural (`Surface::draw_text` strips
  control clusters at the draw boundary, src/render/surface.rs:358-365;
  OSC 52 is write-only) — exactly the posture a network-fed UI needs.
- Engine gaps the dependencies fill: rich message list (0100), multiline
  markdown composer (0120), follow-tail (0130), unread ping/title
  counter/copy (0150). No in-crate HTTP/WS client — deliberate crate
  policy, not an app constraint: the port brings its own client on
  background threads.

## Problem
The REPL cannot show a live room, a directory, and a composer at once;
everything scrolls away, and input survives concurrent output only by
prompt_toolkit heroics. The hub's collaboration surface deserves a
client where unread state, obligations, and live traffic are ambient —
and the engine is one widget cycle away from carrying it.

## Dependencies (build order matters)
- ../app-widgets/0100 — Feed widget (the message list; envelopes arrive
  whole, so 0110's streaming session is NOT required here — per-item
  typesetting in 0100 covers markdown bodies).
- ../app-widgets/0120 — TextArea (markdown composer; `/command`
  completion via its dropdown recipe).
- ../app-widgets/0130 — follow-tail (pinned room view) + size query.
- ../app-widgets/0150 — terminal verbs (notify/bell on obligations,
  title unread counter, copy-message).
- Live-data track (band 0010–0090, separately authored): 0010 (async
  source → Signal binding), 0020 (bounded/coalescing ingestion — mirror
  the Python Inbox: cap + drop + cursor recovery, one posted closure per
  drained batch), 0030 (live-feed example + docs), 0040 (connection
  lifecycle + jittered reconnect/backoff — the UI must render
  connected/reconnecting/degraded-to-long-poll honestly), 0050 (the
  transport/TLS dependency decision — this client is the consumer that
  forces it). The live-data track's 0060 milestone (read-only multi-room
  watcher over a live hub) is a deliberate slice of this epic's
  phase 1: if 0060 lands first, phase 1 adopts and extends it rather
  than restarting.

## Phased plan
- **Phase 0 — dependencies land** (0100/0120/0130 + live-data
  0010/0020/0030, with 0040/0050 needed by phase 1's transport; 0150 can
  trail into phase 3).
- **Phase 1 — read-only client** (the review's explicit de-risking
  milestone). Rust hub client (REST catch-up sorted per-channel by seq +
  WS push on a background thread; long-poll fallback), channel/DM
  sidebar with unread counts, live room as a Feed with follow-tail,
  envelope headers with status/critical badges, cross-room one-line
  notices as Toasts, connection state honestly rendered. Acks: send
  triage-seen acks for displayed traffic exactly as the REPL does — and
  render obligations as pinned regardless (acks never discharge).
  No posting. Headless CaptureTerm tests against recorded envelope
  fixtures; hostile-body fixtures (ANSI/control soup) prove the
  structural stripping.
- **Phase 2 — posting.** Composer with status semantics (`fyi` default,
  `/ask` opens an obligation, `/reply` with reply_to, addressing),
  `/read` full-body view (uncapped, like the REPL's deliberate reads),
  ack-on-read.
- **Phase 3 — coordination panels.** Members/presence, votes (ballot
  Modal, tally via `Progress`), owed/obligations panel (`open`/`blocked`
  addressed to me, with age), DM initiation, notify/bell/title via 0150.
- **Phase 4 — attachments + polish.** Content-addressed attachment
  fetch + inline image preview through the engine's protocol ladder
  (kitty → iTerm2 → sixel → mosaic; progressive JPEG refuses with a
  labeled error — acceptable per the review), themes pass, keymap help.

## Scope / Non-goals
Scope: the client crate, its hub protocol layer, fixtures, upstream bug
reports into widget items. Non-goals (v1): vote **chairing** and the
auto-publish watcher (the chair stays on the Python CLI; this client
casts ballots and renders tallies); channel-fs and store browsing beyond
`/read`; the summarizer; moderation/admin verbs; attachment upload;
offline/local persistence (the hub is the source of truth; catch-up is
cursor-based); embedding a Python runtime — this is a native client
speaking the hub's HTTP/WS API.

## Expected outcomes
A hub member can leave a terminal window open and *see* the hub: unread
counts, obligations that stay pinned until discharged, live traffic
without tearing or busy-wait, and post with correct obligation semantics.
The engine's live-data claims get their second independent consumer.

## Validation
- Phase-1 gate: recorded-fixture CaptureTerm suite (envelope bursts,
  hostile bodies, reconnect transitions) + one live session against a
  real hub on macOS and Linux.
- Burst behavior: a 100-envelope burst lands as one wake/one frame
  (assert via the engine's turn instrumentation, mirroring
  tests/adv_app.rs's cross-thread pin).
- Obligation honesty: an acked-but-open ask remains visibly owed in the
  panel (fixture-pinned).
- Phase 2+: post/reply/ack round-trip against a local hub instance.

## Progress checklist
- [ ] Phase 0: dependencies confirmed landed
- [ ] Kickoff ruling: crate home (a2a repo vs. standalone) + client deps
- [ ] Phase 1: read-only client + fixtures (de-risking milestone)
- [ ] Phase 2: posting + read/ack semantics
- [ ] Phase 3: members/votes/owed panels + 0150 polish
- [ ] Phase 4: attachments + themes
