# REDTEAM on the architecture (cycle 1)

Target: the charter (`docs/design/00-vision.md`), every module contract
(`src/*/mod.rs` + the four design docs), and `Cargo.toml`. Method: hunt
for places where two locally-correct contracts compose into a failure,
and for claims nothing currently enforces. Each finding: severity
(P0 blocks the next build wave / P1 must fix in cycle 2 / P2 should
fix), the concrete failure, and a demand on the owner.

Evidence honesty: cycle-1 docs are unusually good — most classic TUI
traps (deferred wrap, BCE, wide-pair repair, probe sentinels, arena
generations) are already named in-tree with sources. The findings below
are the composition gaps between the modules, which no single owner can
see from inside their own contract.

---

## RT1-1 (P0, REACT + RENDER): the damage pipeline has two vocabularies
and no ownership/epoch contract

The charter's core bet couples fine-grained reactivity to compositor
damage, but the seam is unwritten:

- `reactive-ui.md` §1: "the set of re-run computations IS the damage
  set"; §10: a `Dyn` re-render "damages the region and requests a
  frame" — through WHAT API? RENDER's compositor is not imported by
  REACT anywhere, and its damage entry point is not frozen.
- `layout` emits its own damage feed (`take_geometry_damage`);
  `render.md` §2.2 records damage per LAYER in frame coordinates at
  damage time. Nobody owns the translation layout-rect -> layer damage,
  and nothing says which coordinate space `Dyn` regions damage in.
- Mid-frame writes are unscheduled: effects flush synchronously on
  un-batched writes, and `WakeHandle` posted jobs run at
  `drain_posted()`. If a posted job or animation callback writes a
  signal AFTER `Compositor::flatten()` but BEFORE the presenter emits,
  which frame owns that damage? There is no frame-epoch rule saying
  "damage recorded during present lands in frame N+1, and the present
  input is immutable once flatten begins".

Failure scenario: a timer thread posts a progress update while the loop
is inside present. The effect damages a rect on a layer the compositor
has already flattened. The damage is consumed by bookkeeping for the
CURRENT frame (already emitted) and cleared — the next frame diffs
clean, the progress bar never repaints. The inverse bug (damage double
counted, full-frame repaint every frame) hides as a perf regression
instead.

Demand (cycle 2, before either side builds the loop): a one-page damage
contract co-signed by REACT and RENDER specifying (a) the single damage
entry point and its coordinate space, (b) the frame epoch rule — when a
frame's damage set is sealed, where late damage goes, (c) who translates
layout geometry damage, (d) that draw/flatten/diff/present never runs
user code (so no re-entrant damage). REDTEAM will then write the test:
signal write from a posted job during present must repaint on the NEXT
frame — no lost damage, no tearing, no double repaint.

## RT1-2 (P1, REACT): untracked signal reads in draw closures produce
permanently stale pixels

`ui` draw closures paint through `Canvas`, and nothing stops a widget
from calling `signal.get()` inside one. Draw is not a tracked
computation: the read registers no dependency (or worse — if draw runs
under some effect's tracking context, it registers a WRONG dependency
that re-runs an unrelated computation). The signal changes, no Dyn owns
the region, no damage fires, and the stale value stays on screen — the
exact bug class fine-grained reactivity claims to abolish, now silent
and permanent.

Demand: (a) the runtime gains an "in draw phase" flag; a tracked-read
attempt during draw is a debug panic naming the signal and the region
(release: count + label). (b) The widget-authoring contract in
`ui`/`widgets` docs states it: reactive reads live in `Dyn`/effects,
draw closures are pure over captured data. REDTEAM will fuzz this with
a widget deliberately reading signals in draw.

## RT1-3 (P1, REACT): synchronous effect flush during event dispatch can
dispose the subtree being routed

The kill chain, each link individually documented: (1) event routing
walks capture -> target -> bubble over instance ids; (2) handlers may
write signals; (3) un-batched writes flush effects SYNCHRONOUSLY
(`reactive-ui.md` §6); (4) a flushed `Dyn` effect disposes its previous
generation's scope — which can be the subtree containing the target
(modal closes on click). The bubble phase then walks freed instances.
`GenArena` makes stale ids detectable, but only if every step of the
walk re-validates — and handler closures already collected for the
dispatch may belong to disposed nodes.

`EventCtx` mutations being deferred until after dispatch shows the
hazard was seen — but signal writes are not `EventCtx` mutations and
flush immediately.

Demand: pick one semantics and pin it with tests: (a) wrap the whole
dispatch in `batch` so effects (and thus disposal) run after routing
completes, or (b) re-validate every instance id at every step and
document that mid-dispatch disposal skips remaining handlers. REDTEAM
cycle-2 test: capture-phase handler closes the modal containing the
target; bubble must neither panic nor fire a handler owned by a
disposed scope. (`reactive-ui.md` §12 already names adjacent risks —
credit — but the dispatch-flush composition is the live one.)

## RT1-4 (P1, RENDER): wide-pair and pooled-glyph invariants must
survive blits, and pool ownership must be explicit in types

`render.md` §2.1: pool ids are surface-local; blit/flatten "adopt"
pooled glyphs through the owning surface's pool. Two composition
hazards:

- A blit whose clip edge slices a wide pair (leader in, continuation
  out — or the reverse) at BOTH source and destination simultaneously.
  Write-path repair handles the destination; the SOURCE region read
  must also see a consistent pair (reading half a pair and repairing at
  the destination turns one glyph into a styled blank — correct — but
  reading a continuation whose leader was outside the source rect must
  not resurrect the leader).
- Pool adoption failure: destination pool full during blit/flatten ->
  REPLACEMENT glyph. That is the documented labeled degradation, but a
  flattened frame handed to diff carries cells from MANY surfaces; if
  `Frame` does not carry exactly one pool, a pool id in a frame cell is
  meaningless. Nothing in the contract says the flattened frame owns a
  private pool.

Demand: (a) make pool ownership a type-level fact (`Frame { cells,
pool }` or equivalent) so "which pool resolves this id" is never
ambient knowledge; (b) a `Surface::debug_validate()` (pairs intact,
pool ids in range) REDTEAM can call in property tests; (c) cycle-2
property test (REDTEAM builds it): random blits of ZWJ-bearing content
across surfaces at hostile clip edges — no orphan halves, no
cross-pool id leaks, no panic.

## RT1-5 (P1, RENDER + KERNEL): the last-column strategy rests on
deferred autowrap plus presenter-state purity — both unverified where
they are weakest

The strategy in `render.md` §2.4 (write the last column, rely on
deferred wrap, invalidate the virtual cursor after) is correct for
xterm descendants, and the rig's VT model implements exactly those
semantics to hold the property test together. Two unverified legs:

- ConPTY/Windows: conhost's deferred-EOL behavior has a documented
  history of quirks (delayed wrap + clear interactions). If any
  supported Windows terminal scrolls on a bottom-right write, the alt
  screen scrolls one row and the presenter's prev-frame model is wrong
  about EVERY row until a full repaint — self-sustaining corruption,
  not a one-frame glitch, because diff trusts prev.
- External writers: `gfx` protocol emissions (kitty APC, iTerm2 OSC,
  sixel DCS) move the REAL cursor in protocol-specific ways the
  presenter's virtual cursor knows nothing about. `Presenter::
  invalidate()` exists but no contract says who calls it, when, and
  what happens if a frame interleaves cell runs with an image payload.

Demand: (a) KERNEL: a `deferred_wrap` capability (env default true;
verify on Windows in cycle 2 with a real ConPTY session — one manual
run is acceptable evidence; if broken, presenter's documented
skip-last-column fallback activates via caps); (b) RENDER: all bytes
reach the terminal THROUGH the presenter (an `external_write` bracket
that flushes pending state and invalidates after), never around it;
GFX3D signs this too. REDTEAM will assert via `CaptureTerm` that no
byte reaches the terminal outside presenter custody in integration
tests.

## RT1-6 (P1, KERNEL): probe timing is safe against mute terminals but
not against slow startup composition

The sans-IO probe + DA1 sentinel + 500 ms deadline design is sound
(mute terminals cannot hang it). The composition problem is WHERE those
500 ms sit: caps decide truecolor vs 256 and the graphics channel, the
boot splash needs both, so the obvious wiring is enter -> probe ->
splash -> app, which on a mute terminal is 500 ms of BLACK before a 2 s
splash — a perceived hang at every launch, in the one code path every
user sees first.

Demand: (a) startup sequencing documented: first paint uses env-pass
caps immediately; probe results arriving later UPGRADE (re-render is
one damage-all frame) — or the splash budget absorbs the probe (start
mosaic, upgrade mid-splash). (b) The probe is skipped entirely for
`TERM=dumb`/empty (env pass already knows; emitting query bytes at a
dumb terminal violates its own "no escapes at dumb terminals" rule —
currently only implicit). (c) A late reply (multiplexer passthrough
answering after the sentinel) must be dropped by the parser as a caps
event, never leaked as text — pin with a test. REDTEAM will script a
terminal answering DA1 instantly and everything else 2 s late.

## RT1-7 (P1, RENDER, touches all): one width policy is pinned, but
real terminals disagree with ANY policy — the presenter must be
defensive after risky clusters

`render.md` §2.5 defines cluster width (VS16/ZWJ -> 2, cap 2); the rig
pins unicode-width + VS16 widening and will adopt ZWJ merging when the
presenter lands, so the property test judges one convention
consistently. The unfixable residue: the REAL terminal's width opinion
decides where the cursor physically ends up. xterm renders a ZWJ family
as its components (4+ columns); kitty/wezterm render 2; unicode-width
tracks a specific Unicode version, terminals track others. Any
disagreement desynchronizes the presenter's virtual cursor MID-RUN, and
every subsequent glyph in that run lands shifted — the classic
mystery-smear.

Demand: after emitting any risky cluster (contains VS16, ZWJ, or an
ambiguous-width char), the presenter invalidates its virtual cursor so
the next emission starts with absolute CUP. Bounded byte cost, kills
the drift class structurally. Plus: the 16-color downlevel table must
be THE `testing::palette::SYSTEM_16` table (import it or cross-pin
equality in a test) — two hand-typed copies of the xterm palette WILL
drift and the property test would then lie in both directions.

## RT1-8 (P1, GFX3D): GLB accessor extraction — the hostile-input rules
must be written before the code

Cycle-2 work lands vertex extraction (strides, component conversion).
The header/JSON layer already does checked arithmetic; the accessor
layer inherits sharper edges: (a) `byteOffset` alignment — real files
violate the spec's 4-alignment; reads must be `from_le_bytes` on byte
slices, never pointer casts (the no-unsafe rule covers this — keep it
that way when the rasterizer wants speed); (b) stride games — stride <
element size, stride not a multiple of component size, stride*count
overflowing the view — each must reject with a named error, not
truncate; (c) endianness: glTF is little-endian by spec, all our
targets are LE, but the assumption belongs in a comment + a
`from_le_bytes`-only rule so a future BE port fails loudly in review,
not silently in rendering; (d) sparse accessors and non-mode-4
primitives reject loudly (already stated — keep it pinned by negative
fixtures).

Demand: land the validation rules as tests-first against a hostile
fixture pack; REDTEAM ships a GLB mutator in `fuzzish` style (seeded
truncations, stride/offset mutations of a valid minimal GLB) in cycle
2 to generate it.

## RT1-9 (P1, DESIGN): the contrast audit gates registered themes at
test time — runtime paths can still mint sub-contrast tokens

`contrast.rs` floors + the decisive-ground rule are exactly right, and
CI pins them for every SEEDED theme. Two bypasses: (a) anything
REGISTERED at runtime (the registry is a runtime object; apps can add
themes) skips CI by definition — `register` must run the audit itself
and refuse or label violations at runtime; (b) any DERIVED color
computed outside theme-build time (hover washes, disabled tints,
selection blends done per-widget with `Rgba::lerp/over`) is unaudited
by construction. One `lerp(bg, 0.5)` in a widget default and the
"contrast floors are test-pinned" claim is false in the shipped
product.

Demand: (a) `Registry::register` audits at runtime (returns the
violation list; caller chooses refuse-or-label). (b) Rule in the theme
doc + review checklist: no color arithmetic outside `theme::derive`;
widgets consume tokens only. REDTEAM cycle-2 grep-test: `Rgba::lerp|
over|luminance` call sites outside `theme`/`render` compositing get
flagged. (c) `text_faint` (2.5:1) is for decoration only — widget
defaults must never put content text on it (review rule, cheap to
check).

## RT1-10 (P1, DESIGN + GFX3D + REACT): the boot splash is a liveness
risk on slow terminals

2 s budget, skippable, non-TTY guard — good. What is missing is
BACKPRESSURE: the splash renders 3D through the gfx ladder; on a 200 ms
RTT ssh link or a slow terminal, pixel-protocol frames (kitty/sixel
payloads are tens of KB) saturate the write path. If frames are queued
rather than dropped, "2 s" becomes 10+ s of un-skippable animation —
the skip key is only seen when the loop returns to poll between writes.

Demand: (a) wall-clock pacing with frame DROP (never queue: if the
write of frame N hasn't flushed by N+1's deadline, skip ahead); (b) the
skip check runs between every frame write, and a hard wall-clock
deadline (~2.5 s) cuts to the app unconditionally; (c) the splash's
TTY check must test the handle the engine actually renders to (KERNEL
opens `/dev/tty` even when stdout is a pipe — `isatty(stdout)` is the
wrong question and would skip the splash in legitimate interactive
sessions, or worse, show it in captured ones); (d) composition with
RT1-6: splash starts on env-pass caps, never waits for the probe.

## RT1-11 (P2, GFX3D): sixel palette registers are shared state on real
terminals

The plan (median-cut to 16–64 registers + dithering) handles the
fidelity claim honestly. The trap is that `#Pc` register DEFINITIONS
are screen-global on xterm-class implementations: two images visible
simultaneously, each defining its own optimal palette into registers
0–63, recolor each other — the second emission clobbers the first
image's registers. Also: `Pu=2` RGB is 0–100 scaled; quantize with
rounding and label the loss (already noted in the doc — keep it).

Demand: pick a strategy before the sixel emitter lands: one engine-wide
palette allocation (register ranges partitioned per image), or
re-emission of all visible sixel imagery when palettes change, or
document single-image-at-a-time as a v1 limit (labeled). REDTEAM will
add a two-image golden once the emitter exists.

## RT1-12 (P2, KERNEL): Windows resize + mouse degradation edges

(a) `WINDOW_BUFFER_SIZE_EVENT` is the wakeup, but a missed/coalesced
event (classic conhost window-only resizes) leaves the app blind until
the next input byte. Cheap insurance mirroring the unix "ioctl is
ground truth" posture: re-read the screen-buffer size after EVERY wait
wake and on every deadline expiry, compare cached, synthesize
`TermRead::Resize` on change. (b) Classic conhost (pre-Windows-Terminal)
does not translate mouse to VT sequences under
`ENABLE_VIRTUAL_TERMINAL_INPUT` in older builds — mouse silently dead.
The caps env pass claims `sgr_mouse` for anything not dumb; on Windows
gate it on WT_SESSION/ConPTY version or mark it "labeled degradation"
so a widget can drop hover affordances honestly.

## RT1-13 (P2, verified clean): dependency licensing

Verified from the lockfile metadata this cycle: miniz_oxide 0.8.9 =
`MIT OR Zlib OR Apache-2.0`; unicode-segmentation 1.13.3 and
unicode-width 0.2.2 = `MIT OR Apache-2.0`; libc, windows-sys (+
windows-targets family, windows-link) = `MIT OR Apache-2.0`; adler2
(miniz_oxide's only transitive dep) = `0BSD OR MIT OR Apache-2.0`. All
compatible with the crate's MIT license; no copyleft anywhere in the
tree. No action needed; re-verify on any version bump (one `cargo
metadata` pass — REDTEAM will keep a pinned test if the tree ever grows
build-deps).

## RT1-14 (P2, RENDER): glyph pool and link table growth are unbounded
by contract

Pools "expected to stay tiny", dedup by linear scan, no compaction
(documented as a cycle-2 hook — good). The workload that breaks the
expectation: a chat/log viewer streaming unique ZWJ emoji and unique
URLs. Pool: unbounded growth + O(n) scan per spill = quadratic churn.
Links: `Cell.link` is a u16 — 65,535 unique URIs per surface, then
what? Wrap = silent mislink (worse than drop).

Demand: define the cap behavior now, implement when the churn bench
demands it: pool -> cap + labeled REPLACEMENT (already the exhaustion
path) or generation-based compaction on `fill`; links -> cap + drop
hyperlink with one labeled warning. REDTEAM ships the churn bench
(100k unique clusters/URIs) in cycle 2 so this is measured, not argued.

## RT1-15 (P2, REACT): failure diagnostics for the two loud-stop paths

(a) The 100k effect-flush ceiling converts infinite loops into a panic
— but 100k user closures first (possibly seconds of freeze), and the
panic names no culprit. Cheaper and sharper: per-effect run count per
flush; any single effect exceeding ~1k runs panics naming ITS creation
site (store a `&'static str` label or creation index). (b) A
wrong-thread handle use panics on the BACKGROUND thread; default Rust
behavior kills that thread only — the app symptom is "images silently
stopped loading". Demand: `drain_posted` documents that posted jobs run
on the UI thread; background threads get a documented
`catch_unwind`-at-spawn pattern (or the spawn helper lives in
`reactive::scheduler` and installs it), so a dead worker surfaces as a
labeled app error, not silence.

## RT1-16 (P2, cross-cutting): three small contract holes, one line each

- KERNEL/RENDER: one `flush` per presented frame (write may be called
  many times; flush exactly once at frame end). Pin it: `CaptureTerm`
  counts flushes; torn frames on non-2026 terminals otherwise.
- Charter self-contradiction: "Idle: zero wakeups" vs "a blinking
  cursor damages one cell". If the engine animates the cursor, idle
  isn't zero-wakeup. Decide: terminal-native cursor (DECSET 25 + CUP
  parking — zero engine wakeups, the presenter already parks the
  cursor) as default; composited cursor = an ANIMATION, honestly
  frame-paced. Both sides billed correctly.
- REACT/DESIGN: theme switching granularity — one theme signal (switch
  = full-frame damage, simple, correct) vs per-token signals (fine
  damage, N×tokens edges). Pick one this cycle; widgets are being
  written against an unstated answer right now.

---

## Priority order for cycle 2 (REDTEAM's view)

RT1-1 is the only P0: it sits exactly where the charter's novelty lives
and both owners are building toward it from different assumptions. The
P1 block (RT1-2..RT1-10) is all cycle-2-must-fix, roughly ordered by
blast radius. The P2s are recorded so nobody discovers them as
"surprises" in cycle 5.
