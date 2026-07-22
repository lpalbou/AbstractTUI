# 0340 — Persist: declared state keys, atomic snapshots, restore-on-start, crash marker

## Metadata
- Created: 2026-07-22
- Status: Proposed
- Track: control-plane
- Depends on: 0300 (SuspendPending/Shutdown flush hooks); independent of
  0310/0320
- Completed: N/A

## ADR status
- Governing ADRs: none yet. **Needs ADR before closing**: the snapshot
  container format + compatibility policy is a durable on-disk contract
  (second ADR candidate in this track after 0320's protocol).

## Context
"Full app-state snapshot/restore" — the honest version. Long-lived apps
(the roadmap's chat, console, dashboard classes) accumulate state a
crash or reboot should not erase: composer drafts, scroll positions,
selected room/tab, collapsed sections, session identifiers. Terminal
users already expect this from editors (vim swap, emacs desktop-save);
app-kit patterns (band 0500 — wizards, multi-step forms, admin panels)
are the first consumers by construction: a wizard that loses six steps
of input to a crash is not shippable.

## Current code reality
- **General auto-serialization is impossible, structurally.** A
  `Signal<T>` is a `Copy` index into the runtime arena
  (`src/reactive/signal.rs:40-43`); the value cell is
  `Rc<RefCell<Box<dyn Any>>>` (`cell_of`, signal.rs:73-89 matching
  `NodeKind::Signal { value }`). Rust has no reflection; the dependency
  policy excludes serde (`docs/design/00-vision.md` "Dependency
  policy"; `Cargo.toml:19-34`). Without user participation the engine
  cannot enumerate, let alone encode, signal values — any design
  claiming otherwise would be dishonest. Apps must DECLARE what
  persists and how it encodes.
- **The phase structure gives a correct snapshot moment for free**:
  phases L..S run no user code (`docs/design/01-damage-contract.md` §1),
  so state read in phase U (or at the turn boundary after S) is
  self-consistent by construction — no torn reads, no locks.
- **Flush hooks arrive with 0300**: `SuspendPending` (before SIGTSTP,
  `src/term/unix.rs:610-634`) and `Shutdown` (before `Driver::finish`,
  `src/app/driver.rs:434-450`) are the two moments a snapshot must be
  able to ride.
- **Restore has a natural seam**: `App::mount` runs the component
  closure once under the reactive root (`src/app/mod.rs:227-249`);
  restored values must be available BEFORE mount reads them (signals
  are created inside the closure), so restore is a pre-mount load
  handing values to app code, not a post-hoc signal mutation.
- **Atomic write pattern, family-proven**: the runtime store writes to a
  unique tmp then renames —
  `abstractruntime/src/abstractruntime/storage/json_files.py:264-282`
  (`tmp = p.with_name(f"{p.name}.{uuid}.tmp")` … `tmp.replace(p)`, with
  best-effort tmp cleanup on failure), and its scan-memo notes
  (json_files.py:101) record why replace-into-place is load-bearing
  (every save mints a new inode; readers can never observe a torn
  file). Same shape here, in Rust: write + fsync + rename.
- **Startup notices carry restore honesty**: labeled degradations
  already have a surface (`App::push_startup_notice`,
  `src/app/mod.rs:196-200`) — a skipped or version-refused key reports
  there, never silently.

## Problem
There is no persistence anywhere in the engine: a crash loses
everything, every app hand-rolls config/state files with its own
atomicity bugs (the torn-file and stale-tmp classes the runtime store
had to learn the hard way), and nothing distinguishes "clean exit" from
"crashed" on next start.

## What we want
1. **A `Persist` registry** on `App` (opt-in): apps register keys —
   `persist.register(key, version, read_fn, write_fn)` where `read_fn:
   Fn() -> Vec<u8>` samples app state (untracked reads, phase-U-called)
   and `write_fn: FnOnce(u8 /*version*/, &[u8]) -> Result<()>` applies a
   restored value. Encoding is the APP's (its own format, or the
   promoted in-crate JSON once 0320 lands — no coupling either way).
   Key naming discipline: dotted namespaces ("composer.draft"),
   collisions refused loudly (the `Actions::register` rule,
   `src/app/actions.rs:55-79`).
2. **Snapshot engine**: `snapshot_now()` plus automatic triggers —
   always on `Shutdown` and `SuspendPending` (0300), optionally on a
   dirty-marking debounce the app opts into (`persist.mark_dirty()` +
   engine-side one-shot timer — billed like any timer, zero cost when
   unused). Collection runs in phase U (consistent by the phase
   argument above); the WRITE happens on a helper thread —
   encode-then-hand-off, one in-flight snapshot, newest-wins
   coalescing (the `latest_source` shape,
   `src/reactive/source.rs:51-57`), so the UI thread never blocks on
   fsync.
3. **Container format v1** (engine-owned, small, hand-rolled): single
   file, magic + container version + per-key entries (key, app version,
   length, checksum, bytes). Checksum: **reuse the crate's existing
   CRC-32** — `gfx::png::crc32` (compile-time table, no lazy-init
   state, `src/gfx/png.rs:388-390`), promoted or re-exported rather
   than hand-rolling a second table (extensions review P3-1; their
   0410 keeps png UNGATED core, so the symbol survives every trim
   combination). Atomic write: unique tmp + fsync + rename, tmp
   cleanup on failure — the runtime-store pattern cited above.
4. **Restore-on-start + crash marker**: `Persist::load(path)` BEFORE
   `App::mount`, returning a report — `FirstRun` / `CleanPriorExit` /
   `CrashDetected` (a `running` marker file created at snapshot-engine
   start and removed on clean shutdown; marker present at load =
   crash) — plus per-key outcomes (restored / version-mismatch /
   checksum-failed / decoder-error). The APP decides policy (silent
   restore, or a "restore previous session?" modal — `app::Modal`
   exists, `src/app/popups.rs`); refused keys surface as startup
   notices. The engine never auto-applies state the app did not accept.
   **Multi-instance rule (extensions review P2-3)**: one state path =
   one live instance, ENFORCED, not assumed — the naive marker
   false-positives (instance B's clean exit removes A's marker) and
   snapshots last-writer-win across instances. v1: the `running`
   marker is a pid-bearing lock (created exclusively at snapshot-engine
   start; a live-pid marker at load means "another instance owns this
   path" and `load` refuses with a labeled report variant, while a
   dead-pid marker means crash). This is 0350's lock-file-liveness
   shape at file granularity — one design, two consumers; apps wanting
   N instances use N paths (an instance-suffix convention documented,
   not invented by the engine).
5. **Versioning, minimal and honest**: per-key `u8` version; on
   mismatch the write_fn receives the stored version and bytes and may
   migrate or refuse; container-version mismatch refuses the whole file
   with a labeled notice (no silent partial reads).
6. **Docs**: one page — what persists (declared keys only), when
   (triggers), where (app-chosen path; a conventional per-app default
   helper), what crash detection means and does not mean.

## Scope / Non-goals
Scope: registry, snapshot engine + triggers, container v1, atomic
writer thread, restore + crash marker, notices, docs, example (the
dashboard example persisting its theme + a draft).
Non-goals: automatic signal discovery (impossible, above); encoding
opinions (bytes in, bytes out); multi-file/journal stores, compaction,
history (single-slot snapshot v1 — a journal is a later item IF an app
class demands undo-across-restart); cross-machine sync; encryption
(the file has the same trust boundary as the app's own config);
UI-widget auto-persistence (scroll positions etc. persist only if the
app wires a key — widget-level sugar can come later on evidence).

## Feasibility
**v1-able core; two needs-design details.** The registry, triggers,
atomic writer, marker and notices compose from existing engine shapes
(phase U consistency, one-shot timers, popups, notices) plus ~200
lines of container code. Needs-design before build: (a) the exact
migration-hook signature (pass-stored-version-to-write_fn, chosen
above, vs a separate migrate registry — decide against real consumer
code from band 0500); (b) restore/mount ordering ergonomics — restored
bytes must be readable INSIDE the mount closure (values pre-mount,
handles created during mount); the clean shape is `Persist::load`
returning a typed `Restored` handle the closure queries by key, and it
should be prototyped against a real app before freezing. No research
items: everything here is settled engineering.

## First consumer (named cycle 2; accepted by app-kits cycle 3)
The app-kits **wizard (0520)** is the claimed AND accepted first
consumer (reviews/study/platform-on-appkits.md F3; their 0520 §7 now
specifies the registration: ONE key per wizard — `wizard.<id>` — whose
`read_fn` samples step values + current step + visited flags). It is
the ideal pressure test because the kit CONSTRAINS its value types
(String/bool/choice keys), making mechanical registration honest, and
because its kill-mid-wizard/restart/offer-the-draft journey exercises
this item's two needs-design edges (migration-hook signature;
restore-before-mount ergonomics) against real consumer code. 0520's
resume-after-crash acceptance case doubles as this item's acceptance
evidence.

## Expected outcomes
Crash-safe drafts and session state for every app class; app-kit
wizards (band 0500) get resume-after-crash as a pattern, not a
project; the ports (0200/0210) persist composer drafts and session
selection with a handful of registry lines; "clean exit vs crash" is a
first-class, testable distinction.

## Validation
- Unit: container round-trip (incl. checksum corruption → per-key
  refusal), version mismatch paths, collision refusal, marker
  lifecycle.
- Crash simulation: kill -9 between snapshot and clean shutdown in a
  pty harness (`src/testing/pty.rs` spawn + kill, pty.rs:305-315) →
  next start reports `CrashDetected` and restores the last snapshot;
  torn-write simulation (tmp file present, no rename) → prior snapshot
  intact, tmp cleaned.
- CaptureTerm acceptance: Shutdown trigger writes exactly one snapshot;
  suspend trigger writes before SIGTSTP delivery (unix live-pty,
  ignored by default like `src/term/unix.rs:233-241`).
- Idle pins: an app with a registry but no dirty marks adds zero
  wakeups (`tests/adv_app.rs:54` extended).

## Progress checklist
- [ ] Registry (keys, versions, collision refusal)
- [ ] Snapshot collection in phase U + writer thread (newest-wins)
- [ ] Container v1 + checksums + atomic replace
- [ ] Crash marker + load report + per-key outcomes as notices
- [ ] 0300 trigger wiring (Shutdown, SuspendPending) + dirty debounce
- [ ] Example + docs + crash-simulation tests
- [ ] ADR: container format + compatibility policy
