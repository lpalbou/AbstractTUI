# 0688 — Detection/transport robustness: strict probe parse; big single-frame payloads under tmux

## Metadata
- Created: 2026-07-22
- Status: Proposed
- Track: media-av (band 0600–0690)
- Completed: N/A
- Depends on: nothing.
- Promotion trigger: bundled into the next caps/probe wave (KERNEL
  lane), or the first tmux+iTerm2 field report of a vanishing large
  image.

## ADR status
- Governing ADRs: ADR-0001. ADR impact: none.

## Context
Two robustness gaps found (not fixed — outside the study-2 fix
whitelist) during the image-truth audit:

1. **Substring probe parse**: `ActiveProbe` accepts a kitty graphics
   reply when the raw payload `contains("OK")` and
   `contains("i=4242")` (src/term/probe.rs:172-187). `i=42421` matches
   the id check by substring; any reply text containing "OK" passes the
   status check. All OUR emissions are `q=2` (reply-suppressed), so no
   in-engine traffic can collide today — but a third-party program
   sharing the terminal (a shell plugin drawing kitty images with
   `q=0`, ids in the 4242x range) could theoretically flip
   `kitty_graphics` on a terminal that never proved it, or the wrapped
   id `4343` equivalent could fake tmux passthrough.
2. **Single-frame >1 MiB payloads under tmux**: the study-2 fix wraps
   kitty emissions per APC escape (~4 KiB each — safe), but iTerm2 OSC
   and sixel DCS are ONE escape by construction; over ~1 MiB tmux
   discards them wholesale (tmux#487 input cap). iTerm2 defines a
   chunked `MultipartFile`/`FilePart`/`FileEnd` variant for exactly
   this (docs/design/gfx-three.md:90-91 already records it); sixel has
   no chunked form (a >1 MiB sixel through tmux is honestly
   impossible — must be labeled, not attempted).

## Current code reality
- Probe: src/term/probe.rs:172-187 (`KittyGraphics` arm), tests at
  probe.rs:345-364 cover wrong-id/error cases but not
  substring-prefix collisions.
- Transport: `tmux_wrap_per_escape` (src/gfx/pipeline.rs) with the
  1 MiB rationale in its doc comment; iTerm2 emitter is single-OSC
  (src/gfx/proto/iterm2.rs).

## Problem
Capability lies are the worst failure class this engine has (its whole
detection design exists to avoid them), and a silently discarded image
is the exact "byte-correct but never appears" symptom the study chased.

## What we want
1. Probe: parse the reply control data into k=v pairs; require exact
   `i == "4242"`/`"4343"` and the message segment (post-`;`) to equal
   `OK`. One function, reused by both id checks; collision tests.
2. iTerm2 under tmux: emit `MultipartFile`/`FilePart`/`FileEnd` chunks
   (each part wrapped) when `wrap == Some(Tmux)` and the payload
   exceeds a safe threshold (~512 KiB); plain single-frame otherwise.
3. Sixel under tmux over the cap: refuse with a labeled `#FALLBACK`
   (drop to mosaic through the ladder) instead of emitting bytes tmux
   will discard.

## Scope / Non-goals
Scope: the strict parse, multipart iTerm2, sixel size guard + label.
Non-goals: re-litigating detection policy (correct today), kitty (safe
since the per-escape fix).

## Expected outcomes
No path can flip a graphics capability without an exact-id OK, and no
image silently vanishes at the multiplexer.

## Validation
- Probe unit tests: `i=42421;OK`, `i=4242;ENOTSUP OK?` and friends stay
  inert; exact replies still flip.
- Pipeline tests: >threshold iTerm2 payload under tmux → multipart
  frames, byte-exact unwrap; oversized sixel under tmux → mosaic +
  label.

## Progress checklist
- [ ] Strict reply parse + collision tests
- [ ] iTerm2 multipart lane under tmux
- [ ] Sixel size guard + label
