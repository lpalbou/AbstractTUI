# Platform track — cycle 1 study report (control plane)

Scope: the control-plane slice of the roadmap study — programmatic/agent
control of running apps, state snapshot/restore, background mode +
attach/detach, lifecycle formalization. Band **0300–0390**, track dir
`docs/backlog/proposed/control-plane/`. Method: full read of the app
loop, terminal kernel seams, reactive crossing rules, testing rig, and
the existing backlog/roadmap; every item cites the code it stands on.
Peers this cycle: extensions (band 0400–0490) and app-kits (band
0500–0590) — cross-referenced by band only.

## What the study found (engine reality)

The engine is further along on "control plane" than its docs advertise,
because its OWN test harness needed most of the machinery:

1. **Headless drivability is shipped, not speculative.**
   `Driver::turn` is a non-blocking full frame pass
   (src/app/driver.rs:222); `App::run_on` runs the real blocking loop
   against any `Terminal` object (src/app/mod.rs:348); `CaptureTerm`
   (+`VtScreen`) proves the in-memory-terminal seam end-to-end
   (src/testing/capture.rs, src/testing/vt.rs); the canonical headless
   harness is even a doctest (src/app/mod.rs:85-118).
2. **A machine-readable UI state exists**: `UiTree::accessibility_tree`
   snapshots role/label/value/focus/bounds/depth with untracked value
   sampling (src/ui/tree.rs:170, src/ui/access.rs:97-104) — exactly the
   observation surface an external controller needs.
3. **Run-by-name commands exist**: the `Actions` registry
   (src/app/actions.rs:125-148) with collision refusal and re-entrancy
   safety — "invoke registered app commands" is shipped minus discovery
   metadata.
4. **The injection point exists but is private**: `Driver.pending`
   (driver.rs:117-119) feeds the ONE correct routing path
   (`handle_event`: overlays → tree → actions → default quit,
   driver.rs:452-506). Any bus/protocol must enter there; entering at
   `UiTree::dispatch` would silently bypass modal routing.
5. **The thread-crossing law is already strict and named**: single UI
   thread, `WakeHandle::post` as the one crossing
   (src/reactive/source.rs:5-10), waker interruption of blocked reads
   (src/term/waker.rs). Every control-plane design in this track rides
   it; note that posted jobs see only the reactive runtime — commands
   touching App/Driver need a driver-drained queue (the `pending`
   pattern), which shaped item 0310.
6. **Attach/detach levers pre-exist**: resize already poisons the
   previous frame and re-presents everything (driver.rs:508-533,
   560-565 — an attaching terminal is exactly "a screen we must not
   trust"); capability change mid-session exists
   (`apply_caps_upgrade`, driver.rs:538-554); terminal-held images
   re-emit via the dirty flags both paths already set.
7. **The honest persistence constraint is structural**: signal values
   are `Box<dyn Any>` arena cells (src/reactive/signal.rs:73-89); no
   reflection, no serde (Cargo.toml:19-34; docs/design/00-vision.md
   dependency policy) — general auto-serialization is impossible
   without user participation. The design must be a declared-keys
   registry, and the items say so plainly.
8. **No new dependencies are needed anywhere in the track**: JSON
   parsing is in-crate (src/three/gltf_json.rs — parse-only; a writer
   must be added), unix sockets are std, the concurrency model is
   blocking threads + waker.

## Items drafted (7 + track README)

| ID | One-liner | Feasibility verdict |
| --- | --- | --- |
| 0300 | App lifecycle events (boot/ready/resize/caps/focus/suspend/resume/quit/shutdown + custom events) as one subscribable surface | **v1-able** — all emission points exist; subscription mechanics proven in-repo (notices/viewport signal pattern) |
| 0310 | Automation bus: inject input / query semantic tree + screen text / invoke actions / subscribe — the in-process API | **v1-able** — composes existing seams; opens the private `pending` injection path correctly |
| 0320 | Control wire protocol (JSONL) + opt-in serve seam: unix socket / headless stdio / fd-pair | **v1-able** on unix; windows named pipes **needs-design** (deferred); needs the repo's protocol+security ADR |
| 0330 | MCP bridge note: agent access as an external client of 0320; zero core-crate deps | **trivial-after-0320**, strictly out-of-crate |
| 0340 | Persist registry: declared keys, atomic snapshots (phase-U consistency), restore-on-start, crash marker, per-key versioning | **v1-able** core; migration-hook shape + restore/mount ergonomics **needs-design** against a real consumer |
| 0350 | Background mode + attach/detach design (VirtualTerm, attach wire, client, session semantics) | core **v1-able**; caps re-negotiation + session ownership **needs-design**; multi-viewer + windows **research** |
| 0360 | Milestone: attach proof — one headless app, one client, unix, fixed caps; experience report feeds the ADR | **v1-able** as scoped (~2-4 days) |

Sequencing: 0300 → 0310 → 0320 → {0330, 0360}; 0340 independent after
0300; 0350 design before 0360, report folds back before any wire
freeze. The README carries the full "what we will NOT do" list (no RCE
surface, no TCP in v1, no in-core MCP, no signal auto-serialize
pretense, no tmux replacement, no async runtime).

## The 3 hardest open questions

1. **Capability identity of a detached session.** A headless app must
   render for SOME `Capabilities` before any terminal exists, and
   attaching terminals will disagree with it (color depth, kitty
   keyboard, cell pixel size for images). First-attach-fixes-caps is
   the honest v1; live per-attach re-negotiation touches enter-time
   postures (kitty flags are pushed at session enter,
   driver.rs:138-145) and image placement geometry — is a mid-life
   DOWNGRADE ever acceptable, or does a poorer terminal get refused?
   Needs a maintainer ruling with 0360's evidence on the table.
2. **Session ownership + crash semantics.** Who daemonizes the headless
   process, what SIGHUP means for it, how a stale socket from a crash
   is distinguished from a live detached session (lock-file liveness is
   proposed), and how 0340's crash marker composes so that
   "server crashed while detached" resumes rather than resurrects
   stale state. This is the least engine-shaped, most operational part
   of the track — and the part tmux spent years hardening.
3. **Restore/mount ordering ergonomics (0340).** Restored bytes must be
   readable INSIDE the mount closure (signals are created there,
   src/app/mod.rs:227-249), which forces a pre-mount load handing a
   queryable `Restored` view into app code. The shape is easy to get
   subtly wrong (keys read before registration, partial restores,
   version-migrate placement) — it should be prototyped against a real
   consumer (an app-kit wizard, band 0500, is the ideal guinea pig)
   before the registry API freezes.

## Asks for the cross-review cycles

- **extensions (0400–0490)**: does the bus verb set (0310) suffice as
  the neutral surface for extension-registered actions/events, or do
  extensions need registration metadata beyond name+description?
- **app-kits (0500–0590)**: claim the first `Persist` consumer (wizard
  draft survival) and sanity-check the 0340 registry ergonomics against
  a concrete kit; confirm whether admin/chat patterns want
  `Detached`-aware behavior (pausing pollers) as a documented kit
  convention.
- **Integrator**: the overview.md fold (counts/ledger/bands) is
  deliberately deferred until all three tracks stabilize — single
  writer, one pass.
