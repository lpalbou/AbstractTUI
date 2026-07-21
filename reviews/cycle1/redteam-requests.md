# REDTEAM requests (cycle 1)

What the rig needs from other owners, and what the rig promises back.
Full findings live in `redteam-on-architecture.md`; this file is only
the actionable asks.

## To KERNEL

1. **`Terminal` trait freeze.** `CaptureTerm` (src/testing/capture.rs)
   is deliberately a standalone struct this cycle: the trait landed
   mid-cycle and its `read(&mut self, deadline) -> Result<TermRead<'_>>`
   borrowed-buffer + deadline semantics deserve a scripted-clock design
   on the test side, not a rushed impl that ossifies your signature.
   Early cycle 2: declare the trait stable (or change it now), and
   `CaptureTerm` gains the impl the same week — the surface already
   mirrors it method-for-method (enter/leave/size/read/write/flush).
   One design note: scripted reads need a virtual clock ("Idle until
   deadline" must not sleep in tests) — if the trait ever grows a time
   source, make it injectable.
2. **Probe gating + late replies** (RT1-6): skip active probing on
   `dumb`/empty TERM; pin a test that a caps reply arriving after probe
   completion is dropped as a caps event (never surfaces as text or
   `Unknown`). The rig's scripted terminal will exercise the 2-seconds-
   late reply case in cycle 2.
3. **`deferred_wrap` caps bit** (RT1-5): one manual ConPTY verification
   run in cycle 2 decides whether the presenter's skip-last-column
   fallback must activate on Windows.

## To RENDER

1. **One palette source** (RT1-7): your 16-color downlevel table must BE
   `testing::palette::SYSTEM_16` — import it, or add a test asserting
   element-wise equality with your copy. Two hand-typed xterm tables
   will drift, and then the diff/present property test lies.
2. **Presenter sequence set** (doctrine §1): the VT model understands
   exactly the render.md §2.4 emission set (incl. SGR 5/8/25/28, 4:3
   undercurl via colon sub-params, OSC 8 with id=, DECSET/DECRST, ECH,
   SU/SD, DECSC/DECRC). If the presenter grows a sequence (DECSTBM
   scroll regions, IL/DL, ICH/DCH), file it with REDTEAM in the same
   cycle — the model extends in days; silent emission of unmodeled
   bytes fails your property run with `unknown_seq_count > 0`.
3. **ZWJ policy sync** (RT1-7): the model will adopt your
   `cluster_width` ZWJ-merging convention when the presenter lands so
   both sides judge one convention; ping REDTEAM when `text::
   cluster_width` is final.
4. **`Surface::debug_validate()`** (RT1-4): pairs intact + pool ids in
   range, `#[cfg(any(test, debug_assertions))]` is fine — the blit
   property tests want a cheap oracle from your side of the fence.

## To REACT

1. **Dispatch/flush semantics** (RT1-3): decide batch-the-dispatch vs
   revalidate-every-step and say so in reactive-ui.md — the cycle-2
   unmount-during-routing tests will pin whichever you choose.
2. **Draw-phase read guard** (RT1-2): a debug flag on the runtime
   ("drawing now") that makes tracked reads loud. REDTEAM will
   contribute the test widget that tries it.

## To GFX3D

1. **Hostile GLB fixtures** (RT1-8): REDTEAM ships a seeded GLB mutator
   (fuzzish-style: truncations, stride/offset/alignment mutations of a
   minimal valid GLB) early cycle 2 — write the accessor extraction
   validation tests-first against it.
2. **Presenter custody** (RT1-5b): protocol emissions route through the
   presenter's external-write bracket once RENDER defines it — never a
   raw `Terminal::write` from gfx.

## To DESIGN

1. **Runtime audit** (RT1-9): `Registry::register` runs the contrast
   audit at runtime; decide refuse-vs-label and REDTEAM will pin both
   the pass and the violation path.
2. **Splash pacing** (RT1-10): frame-drop pacing + between-frame skip
   checks + hard wall-clock cutoff; the rig's `CaptureTerm` can script
   a slow-flush terminal to test all three once the splash exists.

## To the integrator

1. Nothing needed in `Cargo.toml` — the rig added zero dependencies and
   zero features. Please keep `pub mod testing;` unconditional in
   `src/lib.rs` (not behind a feature): other owners' unit tests import
   it, and a feature gate would silently split the test surface.
2. `tests/goldens/` now exists and is content-addressed by test name;
   goldens are reviewed artifacts (see docs/design/testing.md §5) — if
   a future CI lands, `UPDATE_GOLDENS` must NOT be set there.
3. Shared-target-dir etiquette observed: this cycle's heavy builds were
   `cargo check --tests` + one `--release` perf run.

## Standing promise

Any module can call the rig from its own unit tests today:
`crate::testing::{VtScreen, CaptureTerm, assert_snapshot, Rng,
hostile_corpus, time_median}`. If the rig itself blocks you (missing
sequence, missing assertion surface), file it in your requests doc and
REDTEAM treats it as a same-cycle obligation — the referee must never
be the bottleneck.
