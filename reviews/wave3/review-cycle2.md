# Wave 3 — cycle-2 adversarial cross-review (REVIEWER)

Date: 2026-07-23 · tree: 0.2.2 working tree, wave-1 lanes landed
(CONTENT2 / READER / INPUTAV), FIXNET + INTEGRATOR in flight ·
owner: REVIEWER.

Method: adversarial code reading of all three lanes + 13 discriminating
probes through PUBLIC API in `tests/wave_c2_review.rs` (the
wave_r2_review.rs posture — probes marked CHARACTERIZATION pin current
behavior as evidence for a finding, not as endorsement). Small fixes I
made myself are marked FIXED-BY-REVIEWER; everything in peer-owned or
in-flight namespaces (feed*/markdown* = INTEGRATOR, button/net/overview
= FIXNET) is a DEMAND. Severities: P1 broken contract / data loss;
P2 real hazard reachable by plausible use; P3 honesty/doc/latent
hazard; P4 nit or observation.

Verdict up front: **no P1 anywhere.** All three lanes hold their core
contracts under hostility — the equivalence, mapping, ballistics and
sync-diff claims all survived amplified probing. The findings are two
P2 hazards (one fixed, one filed), a band of P3 honesty gaps, and
verified-good notes where the task asked specific questions.

---

## A. CONTENT2 (feed sync bridge, rich items, TimeSeries, selection)

### C-1 (P2, DEMAND → INTEGRATOR): the one-writer rule is documented but unguarded, and violations are silently permanent

`feed_sync.rs:94-101` documents "the synced feed has ONE writer — this
bridge", and that is the whole enforcement. A manual `push` onto a
synced feed is invisible to the `shown` bookkeeping: the stray item
survives every fast-path drain (probe:
`one_writer_violation_leaves_a_stray_item_across_fast_path_drains` —
two append-only drains later the stray row is still on screen,
`feed.len()` = mirrored + 1), and is evicted only when an unrelated
mutation happens to take the rebuild path. Worse than the stray-item
shape: if the app pushes a key that LATER appears in the source, the
bridge's `push` for that key lands as a replace-in-place at the old
index (`feed.rs:118-127`), so feed order diverges from source order
with `shown` claiming they agree — permanently, with no signal.

Cheap structural guard, priced: give `FeedState` a monotonically
increasing mutation counter (it already bumps `version` on every
mutation — expose the count crate-internally); the sync effect records
the counter after its own writes and, at the top of the next drain,
takes the REBUILD path when the counter moved more than its own writes
account for. Self-heals every foreign write for one `u64` compare per
drain; no API change. The probe is written to flip loudly when this
lands (update it deliberately).

### C-2 (P3, DEMAND → INTEGRATOR): NaN fingerprints re-render every drain, silently

`SyncSpec` accepts any `Fp: PartialEq` (`feed_sync.rs:38,102`) and the
docs say "any `PartialEq` value that changes whenever the rendered
output would". A float fingerprint that is ever `NaN` compares unequal
to ITSELF, so `fp != shown[at].1` (`feed_sync.rs:141`) is true forever:
the item re-renders + re-typesets on every source change with nothing
changed — the "render runs only on change" promise degrades to "every
drain" with no warning (probe:
`nan_fingerprint_rerenders_every_drain_but_stays_correct` — pixels stay
right, the cost is the finding). One rustdoc sentence in
`SyncSpec::new` (and the api.md §"Feed — syncing" block) closes it:
"float fingerprints must use bit patterns (`f32::to_bits`) — NaN never
equals itself and re-renders every drain."

### C-3 (P3, DEMAND → INTEGRATOR): the rebuild-storm cost is not named where consumers read

`docs/api.md:291-301` says order violations "take the rebuild path
inside the engine" — true, but neither api.md nor the `sync` rustdoc
names the cost shape: a rebuild re-renders EVERY visible item, so a
source that reorders on every change (a most-recent-first sort, a
live-resorted leaderboard) pays O(visible) renders + typesets per
drain, forever. The mechanics are honest (`feed_sync_tests.rs`
`shrink_and_reorder_take_the_rebuild_path` counts them); the COST
WARNING is what's missing. One sentence: "a source that reorders every
drain rebuilds every drain — for feeds ordered by mutable rank, sync a
stable order and sort at render, or accept O(visible) per change."

### C-4 (P3, DEMAND → CONTENT2-owner): `TimeSeries` restart boundary contradicts the module's own gap claim by one slot

`chart_time.rs:80-83`: `missed >= capacity` clears the ring. The module
doc claims missed slots pad NAN "so the chart draws a HOLE for the
pause instead of silently compressing the x-axis", and the `push` doc
says "a pause longer than the whole window restarts it". Both
statements are false at the boundary: `missed == capacity` is exactly
the window (not longer), and `[NAN × cap-1, v]` is representable in the
SAME bounded work the padding loop already does — but the code
restarts, so one extra slot of silence flips the display from "window
of hole" to "a single dot at span zero" (probe:
`timeseries_restart_boundary_sits_at_exactly_capacity_missed_slots`
pins both sides of the boundary). The restart itself is a deliberate,
test-pinned design (`pause_longer_than_the_window_restarts_bounded`),
so I did not change behavior; pick one: pad `missed.min(capacity - 1)`
NANs (uniform behavior, doc becomes true, kills the discontinuity), or
correct both doc sentences to name the `>= capacity` restart.

### C-5 (P3, FIXED-BY-REVIEWER, doc-only): jittered pushes at `interval == cadence` fabricate gaps and lose samples

A producer pushing on a wall clock at the ring's own cadence straddles
slot boundaries under jitter: two samples coalesce into one slot
(latest wins — the earlier sample is lost) and the skipped neighbor
pads NAN, which the chart contract renders as a PAUSE THAT NEVER
HAPPENED. The dashboard is immune (virtual-clock `interval` drives
jitter-free timestamps), but nothing told an app author to either do
that or choose `cadence` above their push jitter. Added the CADENCE
CHOICE paragraph to the `chart_time.rs` module doc and pinned the
mechanism (probe:
`timeseries_jittered_pushes_at_cadence_produce_phantom_gaps` — 4 pushes
at 0/101/199/305 ms on a 100 ms cadence: sample 2.0 lost, slot 2 a
phantom hole).

### C-6 (P4, DEMAND → CONTENT2-owner): 32-bit `usize` wrap in the missed-slot count

`chart_time.rs:79` casts `slot - self.next_slot` (u64) to `usize`. On a
32-bit target a >2³²-slot pause wraps to a small count and pads a few
bogus NANs instead of restarting. Cosmetic and astronomically rare;
`.min(self.capacity as u64)` before the cast closes it for free.

### C-7 — VERIFIED GOOD: `selected_key` × sync rebuild

The task's question ("does a rebuild preserve selection highlight?") —
yes, correctly: selection is a KEY signal resolved at draw through the
rebuilt index (`feed.rs:482-493`), so the highlight follows the item to
its new band after a reorder rebuild, `row_of` agrees with the pixels,
and a key that leaves the source highlights nothing (probe:
`sync_rebuild_preserves_selection_highlight_by_key`, all three
phases). No stale-index class here — the design (key-addressed, not
row-addressed) is right.

### C-8 (P3 → DISCHARGED mid-cycle): in-flight `feed_typeset.rs:216` unreachable-pattern warning failed the tree-wide lint gate

`match` over `DocBlock` gained `DocBlock::Task(_) => false` while
keeping `_ => false` — rustc warned `unreachable_patterns` and
`clippy -D warnings` failed the whole tree on it during this review's
early battery runs. INTEGRATOR resolved it before the final battery
(`#[allow(unreachable_patterns)]` on the wildcard with a rationale
comment — the right shape: the enum is `#[non_exhaustive]`, so the
wildcard must stay for the doc-block wildcard precedent). Recorded for
the timeline; no action remains.

---

## B. READER (doc vocabulary, stream session, outline, search, images)

### R-1 — VERIFIED GOOD: streamed-vs-batch equivalence under amplification

Ran the lane's contract wider than its own rig: a 14-document hostile
corpus (code-span pipes, `\|` at cell edges, alignment-row lookalikes
with matching AND mismatching cell counts, CRLF tables, ZWJ emoji in
cells, tables abutting images/tasks/fences, unresolved header
candidates at EOF, header/delimiter split by a blank line) × per-char
chunking + whole + 6 fresh-seed randomized chunkings each (probe:
`doc_stream_equivalence_holds_over_hostile_corpus_and_fresh_seeds`).
Zero divergence, zero panics. The seal's worst-case rules
(`md_doc_stream.rs:149-238` — candidate-pending, table-open, joins-para
worst-casing) held everywhere I could think to cut. CRLF⇄LF parse
equality also pinned (`crlf_tables_parse_identically_to_lf` — both the
batch `str::lines` path and the stream's `\n`-split + `trim_end`
agree).

### R-2 — VERIFIED GOOD (pinned): GFM cell-splitting semantics

Pipes inside code spans SPLIT cells (GFM: only `\|` protects — pinned
so nobody "fixes" it into CommonMark drift), matching-cell-count
delimiter lookalikes open tables from prose (GFM behavior), mismatched
counts stay prose, delimiter-shaped BODY rows stay rows (probe:
`code_span_pipes_split_cells_and_matching_lookalikes_open_tables`).
`split_row_cells`'s escape-parity walk (`md_doc.rs:347-399`) is
correct on `a\\|` vs `a\|` tails — read carefully, no counterexample
found.

### R-3 (P3, DEMAND → INTEGRATOR): image cache file identity is (size, mtime) — the known same-mtime-rewrite hole

`markdown_image.rs:226-235`: `file_signature = len ^ mtime_ns.rot17`.
No inode. A same-length rewrite on a 1-second-granularity filesystem
(HFS+, NFS, FAT) or under mtime-preserving tooling (`rsync -a`, `tar`)
serves STALE pixels forever — this is byte-for-byte the
JsonFileRunStore scan-memo adversary class already on the workspace
record ("mtime alone is NOT file identity"; the fix there keyed
`(mtime_ns, st_ino, st_size)`). For a doc reader the blast radius is
cosmetic (wrong pixels, not wrong data), hence P3 not P2 — but the fix
is one `#[cfg(unix)] use std::os::unix::fs::MetadataExt` fold of
`meta.ino()` into the hash, in the metadata read already being paid.
The reachable half of the contract is pinned green (probe:
`image_rewrite_invalidates_the_decode_cache` — content+size rewrite
re-probes and re-decodes).

### R-4 (P4, observation): mosaic LRU is entry-bounded, not byte-bounded

`MOSAIC_CACHE_CAP = 32` grids, each up to `width × MAX_IMAGE_ROWS`
cells (`markdown_image.rs:47,215`). At a 300-col terminal that worst
cases ~32 × 60k cells × ~9 B ≈ 17 MB — tolerable, but the bound is
accidental (two caps multiplying), not chosen. Fine to leave; worth a
comment if images ever go protocol-sized. Move-to-back LRU on hit is
correct (verified the `remove`/`push` order).

### R-5 — VERIFIED GOOD (pinned): slug dedup vs literal `-N` collisions

Both directions match the GitHub probing rule: literal `x-1` between
duplicate `x`s pushes the generated suffixes past it (`x, x-1, x-2,
x-3`), and a literal arriving AFTER its generated twin probes deeper
(`x, x-1, x-1-1`) (probe:
`slug_dedup_survives_literal_suffix_collisions_both_orders`). O(n²) in
pathological heading counts — irrelevant at document scale.

### R-6 — VERIFIED GOOD (pinned): text↔cells mapping at wide-glyph boundaries

The task's specific worry — match cells for CJK/emoji content: a `本`
match inside `日本語` reports cells `(2,4)` (both columns of the
matched glyph, offset past the preceding wide glyph), a ZWJ emoji
(`👍🏽`) match covers its whole cluster in bytes and both its columns in
cells, and a match ENDING the row closes at the true end column
(probe: `wide_glyph_matches_report_two_column_cells`). The mapping and
`draw_rows` share span walk + segmentation
(`markdown_search.rs:177-211`), so mapping-vs-pixels drift is
impossible by construction; the existing round-trip test already
covered the second-cell-of-wide-cluster mouse direction. One
observation (P4, no action): a grapheme cluster SPLIT ACROSS STYLED
SPANS (`**e**` + combining mark) segments per-span in both the mapping
and the renderer — consistent with the pixels, merely not
unicode-ideal.

### R-7 (P4, observation): `probe_file_uncached` reads the whole file on unprobeable input

`markdown_image.rs:262-285`: for a non-image file the ladder ends by
reading the REST of the file into memory before giving up (a 2 GB
`![x](video.mp4)` costs a full read + allocation at typeset). The
64 KB / 2 MB ladder steps are right; consider capping the final step
or trusting `probe_dimensions`'s failure after step 2. Low urgency —
reader inputs are the user's own documents.

---

## C. INPUTAV (key state, push-to-talk, meter/scope)

### I-1 (P2, FIXED-BY-REVIEWER): a Full→Degraded fidelity flip stranded the down-set — the stuck-hold/stuck-mic class, now structurally closed

`publish_fidelity(false)` flipped the signal and left `frame.down`
populated. On a Degraded wire releases are deliberately not processed
(`keys.rs` release arm gates on `full`), so any key in the down-set at
downgrade would read `is_down == true` FOREVER — and `PushToTalk`'s
Hold branch stops on `released || !down`, so a held capture would keep
"recording" with no gesture able to end it (the exact 0610 stuck-mic
privacy class). Unreachable from today's driver — the probe fold only
ever SETS `kitty_keyboard` (`term/probe.rs:134-139`) and
`apply_caps_upgrade` only raises flags — but the contract table
(`keys.rs` module doc: "Degraded ⇒ down-set always empty") was held
incidentally, not structurally, and "an invariant enforced in one lane
and violated in a lane added later is the recurring defect class"
(cycle-13 adversary, on the record). Fix: the downgrade path drains
the down-set into synthesized release edges (the focus-loss rule,
without claiming a focus event) + unit test
(`downgrade_drains_the_down_set_toward_not_held`). ~12 lines, INPUTAV
file, not in any peer's in-flight set.

### I-2 (P2, DEMAND → whoever lands a suspend binding; cc INPUTAV): `Terminal::suspend` bypasses key-state hygiene

`term/mod.rs:176-190` ships an app-callable job-control suspend ("the
app's Ctrl+Z binding"). While the process is STOPPED, key releases are
unobservable; on resume a key released-during-suspend stays in the
down-set with no repeat ever coming to correct it → stale hold, and a
PTT `Held` capture resumes "recording" though the chord is up (focus
loss does NOT cover this: Ctrl+Z keeps the terminal window focused, so
no FocusLost arrives). Nothing binds suspend today — the driver never
calls it — so this is a seam demand, not a live bug: the suspend
orchestration, wherever it lands, must clear the down-set the way
focus loss does (fail toward not-held; `publish_fidelity`'s new drain
is the reusable shape). The keys module doc should name suspend beside
focus loss when that lands.

### I-3 — VERIFIED GOOD (pinned): fidelity flip mid-hold is honest end to end

The task's scenario, driven through the REAL driver + probe replies
(probe: `fidelity_flip_mid_hold_recovers_via_repeat_without_faking`):
legacy press (Degraded — press edge honest, no fabricated hold) → 0293
probe proof lands mid-hold → fidelity flips Full → `is_down` stays
FALSE (the honest gap: the hold predates the protocol; never invented)
→ the held key's first kitty REPEAT proves the hold
(`keys.rs:337-349`, deliberately WITHOUT a press edge so capture
surfaces can't auto-start) → wire release ends it. The
repeat-as-proof-of-down branch is the load-bearing recovery and it
behaves exactly as documented.

### I-4 — VERIFIED GOOD: FocusLost during Latched mode

`step()` checks focus FIRST, before any fidelity branch
(`push_to_talk.rs:170-174`), so a Latched capture stops with
`StopReason::FocusLost` on any wire; pinned by
`latch_mode_stops_on_focus_loss_too`. A press edge arriving in the
SAME turn as the focus loss is discarded (the `return` after stop) —
the safe direction for a mic; a fresh press next turn starts cleanly.
The upgrade-mid-latch case (latch survives, toggles off on next press)
is deliberate and pinned by the lane's own test.

### I-5 — VERIFIED GOOD: meter equal-write re-arm and "jump then hold"

The claim "equal re-writes re-arm nothing" holds by exact arithmetic:
`set_targets` only reports change on real target/display/peak movement
(`meter.rs:105-130`), `advance`'s `max()` clamps land display ON
target and peak ON display, so `settled()` is plain equality — no
epsilon residue to strand a task. "Level jumps then holds" cannot
stick: the attack lands display exactly on target synchronously, so
the meter is settled the moment it repaints (no task needed); a
subsequent fall arms exactly one task (`task_live` latch), which drops
at the fixpoint. Peak-hold stamping across parked stretches is also
correct — `peak_since` is None-cleared on every peak rise and
`get_or_insert(now)` re-anchors at the first fall after a park, so a
parked meter never fast-drops a fresh peak. Pinned by the lane's
`equal input re-arms nothing`, the reanchor test, and the alloc-budget
idle pin; I found no hole to write a new probe against.

### I-6 — VERIFIED GOOD (pinned): the physical-fact rule vs the selection layer

The tap sits at the TOP of `handle_event`, pre-conversion and
pre-routing (`driver.rs:626-633`), so key state observes keys any
consumer later claims. Pinned through the real path the task named:
with a selection visible MID-DRAG, `c` is consumed by the selection
layer (copy + clear — `selection.rs:329-332`) and the key-state press
edge is still observed (probe:
`key_state_observes_keys_the_selection_layer_consumes`). Should it see
them? Yes — key STATE is a physical fact; routing decides meaning, not
existence. One wrinkle worth knowing (not a defect): since 0290 every
copy ends the gesture, so selections only claim keys mid-drag —
release-copied selections never swallow the app's next `c`.

### I-7 (P4, observation): Degraded auto-repeat flaps a latch

Documented in the lane's own module doc ("holding the chord on a
Degraded wire toggles repeatedly — the truthful label is the
mitigation"): OS auto-repeat on legacy wires arrives as more presses,
each toggling the latch. No mechanical fix exists that doesn't
re-introduce the forbidden repeat-timeout inference; the honesty
posture is right. Noted so nobody "fixes" it later.

---

## D. Cross-lane / process

### X-1 (P4): probe suite placement

The 13 probes live in `tests/wave_c2_review.rs` (public-API only, no
peer files touched). Two are CHARACTERIZATION pins (C-1, C-2) that
flip loudly when the demanded guards land — update them deliberately
in the same change, per their comments.

### X-2 (note): battery ran during peer churn

FIXNET and INTEGRATOR were in flight during this review; the
verification battery below reflects the tree as of its timestamps.
C-8 (feed_typeset) was fixed by INTEGRATOR mid-cycle; two FIXNET
in-flight items (`connection.rs` clippy lint + rustfmt diff) are the
churn that gates the tree at close — both named in the handoff.

---

## E. Verification battery (2026-07-23, ~03:45–03:55 local; host load avg ~6–8)

| gate | result |
| --- | --- |
| whole-tree `cargo test` | **1,636 passed / 0 failed** (lib 1,198 + integration suites + doctests; 18+ ignored suites excluded by design) |
| `cargo clippy --all-targets -- -D warnings` | **FAILS on FIXNET's in-flight** `src/reactive/connection.rs:158` (`pub fn next` → `should_implement_trait`). With that one lint allowed: **exit 0, nothing else** — all three wave-1 lanes and my files are clippy-clean |
| `cargo fmt --check` | **FAILS on the same in-flight file** (`connection.rs:460` diff); rest of the tree clean |
| alloc pins (`cargo test --test alloc_budget`) | **10 passed / 0 failed** (incl. INPUTAV's parked meter/scope/key-state idle pin) |
| `cargo semver-checks --baseline-version 0.2.2` | **196 checks: 196 pass, 57 skip** — everything additive vs the published release |
| `perf_budgets` (release, serial) | **12 passed / 0 failed.** Medians: diff+present 200×60 155 µs (budget 2 ms), parser 1 MB soup 5.7 ms (50 ms), shimmer frame 425 µs (3 ms), splash 2D 36 µs (2 ms), brandmark 3D 150 µs (8 ms), keystroke→frame 137 µs (3 ms), VT referee 503 µs (3 ms), grid solve 37 µs (3 ms), md parse 1k-line 318 µs (20 ms), richtext wrap 3.3 ms (20 ms); pool cap 4096 holds, link churn 4,465 refusals no wrap |
| `perf_app_surfaces` (release, serial, ratchets ACTIVE) | **7 passed / 0 failed.** Feed token frame 277 µs + 73 B (ratchet 110), select popup 52 µs + 301/254 B (452/381), selection drag 834 µs + 260 B (390), composer keystroke 100 µs + 465 B (698), codeview scroll 381 µs + 255 B (383 — byte meter added this cycle), scroll-guard phases 172/1,758 B (258/2,637), startup warm release 52/51 ms (1,500 ms ceiling) |
| review probes (`wave_c2_review`) | **13 passed / 0 failed** (re-run after peer churn) |
| `keys` unit tests (with the I-1 fix) | **10 passed / 0 failed** |

Scheduled-gate inputs timed for the workflow design: `fuzz_big`
release = **0.04 s** (6 campaigns), soak release = **3.7 s**,
both perf suites ≈ 10 s each after compile — the weekly job fits its
60-minute timeout with a cold cache several times over.
