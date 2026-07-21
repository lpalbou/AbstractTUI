# AbstractTUI 0.1.0 — independent completeness audit + coding-console port evaluation

Reviewer: external, read-only. Date: 2026-07-21. Host: macOS (Apple Silicon).
Method: full source/docs/review-ledger read, plus fresh runs of the test and
perf batteries. Every claim below is tagged either **[verified now]** (I ran
or read it in this session) or **[claimed]** (asserted in docs/ledgers, not
independently re-executed here).

## 0. Ground truth established this session

- `cargo test` (default suite): **lib 910 passed / 0 failed / 16 ignored**
  (926 collected), all 30 integration suites green (adv_* through vt_style),
  doctests **32 passed / 0 failed / 23 ignored**. Ignored tests all carry
  explicit reasons (perf-release-only, live-pty/tmux-only, fuzz/soak).
  [verified now]
- `cargo test --release --test perf_budgets -- --ignored`: **12/12 pass**.
  Medians on this box: diff+present 200×60 full-change **435 µs** (budget
  2 ms), keystroke→frame via Driver **45 µs** (3 ms), shader frame 1.19 ms,
  markdown 1000-line parse 994 µs, richtext 800-para wrap 9.8 ms, 1 MB
  hostile input soup 15.5 ms, 3D brandmark frame 409 µs. README's numbers
  (~0.5 ms / ~50 µs) are consistent with my measurements. [verified now]
- Idle-zero-cost is enforced in the **default** suite, not just claimed:
  `alloc_budget.rs::diff_present_steady_state_allocates_nothing`,
  `presenter_no_change_frame_emits_and_allocates_nothing` (counting global
  allocator, 8/8 green), and `adv_app.rs::idle_app_emits_zero_bytes_across_
  idle_turns`, `adv_anim.rs::animation_completion_returns_to_zero_byte_idle`.
  [verified now]
- `grep todo!|unimplemented!` over `src/`: **zero hits**. All `#FALLBACK`
  strings are labeled runtime degradations by design, not stubs. [verified now]
- Git: 7 commits, tag `v0.1.0`; crates.io sparse index returns
  `{"name":"abstracttui","vers":"0.1.0",...}` — **the crate is genuinely
  published**. [verified now]
- Live-pty smoke (14 tests), fuzz (5), soak (1) are `#[ignore]`d; the
  cycle-10 ledger records 14/14 live smoke green on a real pty. [claimed —
  reviews/cycle10/final-status.md; consistent with the suites existing and
  being runnable]

## 1. Completeness audit — mandate clause by clause

| Mandate clause | Verdict | Evidence |
| --- | --- | --- |
| Efficient / responsive | **DONE** | Perf table re-measured green this session (tests/perf_budgets.rs, 12/12); idle = 0 bytes / 0 allocs pinned in the default suite (tests/alloc_budget.rs, tests/adv_app.rs:55); scroll-region byte optimization referee-verified (tests/adv_scroll.rs::scrolled_log_append_property_and_bytes_won, 7.8–9× byte wins). Caveat: **timing** budgets are release-only `#[ignore]` and not run in CI (deliberate, stated in perf_budgets.rs header and ci.yml comments) — only allocation budgets gate every run. |
| Versatile widgets | **DONE** | 23 widget types in src/widgets/ (button, input, list, table, tabs, checkbox, radio, scroll, progress, spinner, badge, block, separator, code, markdown, richtext, chart {Line/Bar/Sparkline}, grid, image, viewport3d, logo); flex solver + track grid (src/layout/, examples/grid.rs); virtualized List with variable heights and prefix-sum windowing (src/widgets/list.rs:1-13); sortable Table in examples/dashboard. Exercised by examples/widgets.rs and gallery.rs. |
| Reactive effects & triggers | **DONE** | Fine-grained signals/memos/effects (src/reactive/, 13 adv_reactive tests); nine cell shaders (Shimmer, HueDrift, Pulse, Sweep, Rainbow, Vignette, Dissolve, ScanlineFade, GradientReveal — src/anim/shaders*.rs); tween/timeline/transition + `after()` one-shot timers (src/reactive/animate.rs:141); particle bursts; examples/effects.rs. Shaders cost work only where damage exists (perf_frame_with_active_cell_shader budget, verified now). |
| Mouse clicks + keyboard shortcuts | **DONE** | Click/hover/drag/wheel with capture (tests/adv_pointer.rs); SGR mouse, kitty keyboard protocol + xterm modifyOtherKeys auto-decoded (tests/adv_input.rs::kitty_and_legacy_keys_decode_exactly); Shift+Tab decoded as Tab+SHIFT (src/input/mod.rs:174); `Element::shortcut`/`shortcut_labeled` chords + KeymapHelp overlay; focus system (Tab order, traps, memory, autofocus — src/ui/focus.rs). Honest limit documented: Ctrl+Enter/Shift+Enter don't exist on legacy wires (docs/faq.md; kernel handoff §4). |
| Images | **DONE** | In-crate PNG + baseline-JPEG decoders (11 adv_jpeg tests); protocol ladder kitty → iTerm2 → sixel → unicode mosaic (half-block/quadrant/sextant/braille) with every degradation `#FALLBACK`-labeled (src/gfx/pipeline.rs); Image widget + ImageSession lifecycle (tests/adv_image.rs, adv_proto.rs); examples/images.rs. Limits (baseline-only JPEG, single sixel palette, 2-color mosaic cells) are in docs/api.md §"Stability and limits" — the ledger's known-limits list did land in the public docs. [verified now] |
| Themes | **DONE** | 26 themes verified in code (`src/theme/seeds.rs: SEEDS: [ThemeSeed; 26]`); ~36 semantic tokens (src/theme/tokens.rs); WCAG contrast audit (src/theme/contrast.rs; examples/themes.rs displays measured ratios); hot-swap through one signal (gallery example; docs/theming.md); generated per-theme hex reference (docs/captures/themes-table.md). |
| “Customizable like a React page, shareable components + events” | **DONE (idiomatic, not literal)** | The component model is plain functions: props = args/Signals, children = View args, events = callbacks (`ui::compose::Callback`), state = signals; fine-grained `dyn_view` re-render. examples/components.rs (278 lines) is the reference. Ergonomics were adversarially validated (tests/api_first_use.rs 4/4 after the RT8 prelude/padding_floor fixes — reviews/cycle8/redteam-findings.md, cycle9/react-report.md). Sharing = ordinary Rust modules/crates; there is no macro DSL or component registry — reasonable for Rust, but "like React" should be read as the signals+props model, not JSX. |
| 3D graphics + GLB models | **DONE** | src/three/: GLB parser (node hierarchies, embedded PNG/JPEG textures, vertex colors), software perspective rasterizer with z-buffer and per-triangle mips, LINEAR/STEP animation, 4-joint linear skinning with renormalization (src/three/load.rs, skin_tests.rs); Viewport3D widget with orbit/zoom events; examples/viewer3d.rs (15,452-triangle helmet; README GIF regenerable from docs/media/viewer3d.tape). CUBICSPLINE/morph targets skip with labeled warnings — documented (docs/graphics-and-3d.md §limits). Loads any GLB path; no coupling to specific asset sources. |
| Standalone, minimal MIT/Apache deps | **DONE** | Cargo.toml: `unicode-width`, `unicode-segmentation`, `miniz_oxide` + `libc` (unix) / `windows-sys` (windows). ANSI emission, input parsing, layout, signals, PNG/JPEG, glTF, and the rasterizer are all in-crate (~64.7k lines src). [verified now] |
| macOS / Linux / Windows | **PARTIAL — honestly disclosed** | macOS: verified live (suite green here; live-pty smoke suite in-tree). Linux: same unix paths, full suite in CI (ci.yml ubuntu matrix) [claimed green]. Windows: compiles + clippy-clean vs MSVC; CI runs `cargo test --lib` on windows-latest; the once-Windows-only surrogate/wake logic was extracted to `src/term/win_logic.rs` and unit-tests on every host (RT8-9 closed) — but **no interactive Windows console session has ever run**. README's platform table says exactly this ("beta event"); docs/api.md and faq repeat it. The disclosure is accurate and, if anything, conservative (final-status says "compile-verified" while CI also executes lib tests on a Windows host). |
| 10 cycles | **DONE** | reviews/cycle1/ … cycle10/ all present: per-cycle build requests, adversarial findings ledgers (RT1–RT8), integrator rulings, docs handoffs, final status. The RT ledger in cycle8 shows every finding closed or converted to a documented known limit. |
| coredoc documentation | **DONE** | Root policy set (README, CHANGELOG, CONTRIBUTING, SECURITY, CODE_OF_CONDUCT, ACKNOWLEDGEMENTS, LICENSE, llms.txt + llms-full.txt) + 7 docs pages (getting-started, architecture, api, theming, graphics-and-3d, faq, troubleshooting) + examples catalog + mdBook config (book.toml, docs/SUMMARY.md) + docs.yml Pages deploy + deterministic text captures + recorded GIFs/tapes (docs/media/). Spot-checked faithfulness: the api.md "Stability and limits" section matches the internal known-limits ledger item for item. [verified now] |
| Bonus: 2 s 3D boot identity | **DONE** | `SPLASH_TOTAL_MS: u32 = 2000` (src/boot/identity.rs:23); 3D brandmark + pure-cell 2D fallback through one player; skippable on any key; auto-disabled on non-TTY / NO_COLOR / TERM=dumb (src/boot/mod.rs, 13 adv_splash tests, examples/splash.rs). Frame costs re-measured: 3D 409 µs, 2D 101 µs. [verified now] |
| Published + CI/CD (cycle-10 addendum) | **DONE** | crates.io 0.1.0 confirmed via sparse index [verified now]; tag v0.1.0; .github/workflows: ci.yml (unix matrix + windows lib gate + fmt/clippy/rustdoc lint), release.yml (trusted publishing), docs.yml (mdBook + rustdoc Pages); one-time setup steps in .github/SETUP.md. CI runs claimed green on the push (build log; commit history shows publish-related fixes landing). |

### Marketing-vs-code deltas found (all minor)

1. README: "enforced by in-tree perf budgets … not aspirations" — true for
   **allocation** budgets (default suite) and true that timing budgets exist
   and pass, but timing budgets are manual release-mode runs, not CI gates.
   One sentence of nuance would make this exact.
2. README "Twelve runnable examples": 11 product examples + the `capture`
   tool = 12 runnable programs. Defensible, borderline.
3. Windows wording drifts between documents ("compile-verified only" in the
   final ledger vs lib-tests-on-Windows-CI reality) — the public README is
   the accurate one, and errs conservative.

Nothing found where the code is weaker than the public claim.

### WHAT REMAINS (ranked)

- **P0 — Windows interactive verification.** The only mandate clause not
  proven end-to-end. Next step: one session on Windows Terminal + legacy
  conhost running `examples/hello`, `dashboard`, `widgets` (input, resize,
  suspend/restore, mouse), then flip the README table. The extracted
  win_logic tests reduce risk; they do not replace the run.
- **P1 — Perf-timing regression gate.** Timing budgets exist and pass but
  nothing runs them automatically; a scheduled release-mode CI job (generous
  budgets, quiet runner) would catch drift. Same for the ignored fuzz_big
  (5 tests) and 10k-frame soak.
- **P1 — MSRV.** No `rust-version` in Cargo.toml (named in .github/SETUP.md
  as a follow-up). Declare and pin a CI job.
- **P2 — 23 `ignore`-fenced doctests** never compile-checked (RT8-8 was only
  partially executed — Image converted, most widget fences remain). Rot risk
  against future API changes.
- **P2 — Syntax highlighting is demo-grade**: single-line, C-like only
  (src/text/highlight.rs states this honestly). Fine for the library; a real
  consumer needs more (see Part 2).
- **P2 — List multi-row item content** deliberately deferred
  (src/widgets/list.rs:11-13) — becomes load-bearing for the console port.

### abstractcoder side-project status

**Confirmed inert and not claimed as done.** `../abstractcoder/` contains
exactly: ARCHITECTURE.md (charter), Cargo.toml, `src/model.rs`,
`src/protocol/types.rs` — no lib.rs/main.rs, so it is not even a buildable
crate yet; three checkpoint commits. The build was explicitly put on hold by
the operator ("for abstractcoder: wait" — untracked/partial.log:841, 984) and
the status ledger marks it "charter seeded; ON HOLD". No public AbstractTUI
document claims it exists as a product. [verified now]

## 2. Coding-agent console port: feasibility on AbstractTUI

Target: a port of `../abstractcode`'s console (fullscreen_ui.py ~4,368 lines,
prompt_toolkit) driven by `abstractcode serve` JSONL events (docs/cli.md
§"Headless mode": commands `prompt/approve/answer/steer/cancel/status/quit`;
events `ready/run_started/phase/cycle/thought/tool_call/tool_result/denied/
approval_required/ask_user/status/steer_queued/ack/error/llm_call/final`).
The existing abstractcoder charter (ARCHITECTURE.md) already maps this
correctly: reader thread per child → `WakeHandle::post` → signals → widgets.

### 2a. Already supported well (widget/API named)

| Console need | Engine surface | Assessment |
| --- | --- | --- |
| Async/subprocess event source into the reactive loop | `reactive::WakeHandle::post` (scheduler.rs:74) + `TerminalWaker` self-pipe integration; posted closures run on the UI thread; N events per frame coalesce into one render. Proven by tests/adv_app.rs::`cross_thread_post_lands_exactly_one_frame_later` and `spawned_worker_panic_surfaces_as_app_error`. | **Ready.** This is the architectural risk in most TUI stacks and it is already solved and test-pinned here. |
| Approval prompt flow (y/n/a/e/q) | `app::Modal` (focus-trapped, `share()`, `on_outside_press`), `Element::shortcut` chords, Button, KeymapHelp | **Ready.** Focus trap prevents composer keystrokes leaking into the approval. |
| Session tabs | `widgets::Tabs` (lazy per-tab panels, `active` signal, `on_change`) | **Ready.** |
| Right-hand detail/timeline panel | Row layout + `grow` + virtualized `List` (variable heights, `scroll_to`, sticky `selection_key`) + `Scroll` | **Ready** for line-oriented timelines. |
| Status/cache meter | Element row + Badge + Progress + `Sparkline` (widgets/chart.rs:144) + `dyn_view`; `llm_call` event fields map directly | **Ready.** |
| Permission-mode switch | Signal + Badge; Shift+Tab arrives as Tab+SHIFT on every terminal (input/mod.rs:174); footer dropdown = `Overlays::layer` + `on_outside_press` | **Ready.** |
| Tool-call cards with live status | Block + Spinner + Badge + per-card status signal in `dyn_view`; fold/expand = a bool signal | **Composable today**; a packaged card is P2 sugar. |
| Transcript byte efficiency (SSH) | Presenter scroll-region optimization: `FrameDiff::compute_scrolled` + `emit_scrolled`, referee-verified 7.8–9× byte reduction on log-append/list-scroll (tests/adv_scroll.rs) | **Ready** — directly benefits an append-heavy transcript. |
| Theming incl. code colors | 26 themes; `code_token_color` maps TokenKind→theme inks | **Ready.** |
| End-to-end testability | `CaptureTerm` + `Driver::turn` + `VtScreen` referee; multi-app stacking proven (tests/integration_matrix.rs) | **Ready** — the console can be developed headlessly. |

### 2b. Gaps — the prioritized improvements list

**P0-1 — `Transcript`/`Feed` widget (virtualized, append-only, mixed rich
blocks).** The gap: `MarkdownView` typesets its **whole source** and caches
per width inside one element instance (widgets/markdown.rs:108-110); any
source change (each streamed token) rebuilds and re-parses O(document), and
`MarkdownView::rows()` pays the fold a second time for the scroll clamp.
`List` is virtualized with variable heights but items are single-row Strings
— "wrapped multi-row item CONTENT is a later decision" (widgets/list.rs:11).
At 1,000 lines a re-parse is ~1 ms (measured), so a long session streaming at
30–100 events/s multiplies into whole-core burn. Proposal: an append-only
item feed — `push(TranscriptItem)` where each item owns its typeset rows
(RichLine cache keyed by width), prefix-sum row index reused from List's
windowing, only the **open tail item** re-typesets during streaming, closed
items freeze; sticky-bottom + scrollback offset signal. Every ingredient
(RichText layout, prefix sums, Scroll, damage) exists in-repo; this is the
one load-bearing new widget for the port.

**P0-2 — Streaming/incremental markdown session.** `render::md::parse`
(md.rs:157) is a clean block parser but whole-document. Proposal:
`md::StreamSession` — feed text deltas; only the trailing open block (the
one still receiving tokens) re-parses; completed blocks freeze into typeset
rows (feeding P0-1). Also gives correct mid-fence behavior while a code
block streams. Without it, the port either re-parses per token or renders
streaming text plain until turn end (what abstractcode does today with its
conservative renderer — acceptable fallback, but the engine can do better).

**P0-3 — Multiline composer (`TextArea`).** `TextInput` is single-line by
design (widgets/input.rs:1). The console needs: multi-row editing,
up/down history recall with the row-boundary semantics abstractcode has
(fullscreen_ui.py:148-163), block paste (engine's `Paste` event already
arrives whole — input hardening done), Enter-submit vs newline (note:
Shift+Enter is indistinguishable on legacy wires — kernel handoff §4 — so
follow abstractcode's Esc+Enter or option-key convention), and a completion
dropdown for `/commands` and `@file` mentions anchored at the caret
(`Overlays::layer` + `on_outside_press` exist; the composer must expose the
caret cell). TextInput's `ClusterMap` grapheme machinery (input.rs:44-96) is
directly reusable.

**P1-4 — Language lexers behind the existing `Highlighter` seam.** The trait
(text/highlight.rs:41) is the right plug point but the only impl is a
single-line C-like demo lexer (rust/c keyword presets, no cross-line state —
honestly documented). The console renders diffs, Python, JS, TOML, shell.
Proposal: add a stateful variant (carry per-line lexer state down the block,
matching how CodeView iterates) + presets for python/js/toml/diff. Diff
tinting matters most (tool-result previews).

**P1-5 — Bounded/coalescing event-ingestion helper.** `WakeHandle::post`
pushes into an unbounded `Vec` under a mutex and fires the waker **per
post** (scheduler.rs:74-81). Rendering coalesces fine (one frame per drain),
but a flooding child (rapid `tool_result` chunks) means unbounded queue
growth and lock/pipe churn. The fix is a documented pattern or tiny helper:
reader thread batches lines (drain-all-available per read), posts **one**
closure per batch, and applies a drop/coalesce policy for superseded
progress events. Engine change optional; the recipe must exist or every
consumer rediscovers it under load.

**P1-6 — Transcript text selection/copy.** OSC 52 copy exists (write-only,
term/verbs; clipboard-read deliberately refused — good). There is no mouse
selection model over rendered content. abstractcode ships command-based
copy (`/copy`), which ports cleanly; mouse drag-selection would need a
selection layer + cell-text extraction (Surface knows its cells; the
snapshot module is the extraction seam). Command-copy first, mouse P2.

**P2-7 — Clickable links/paths in the transcript.** Cells carry OSC 8
hyperlink ids (render/cell.rs:296-310, 65k cap with labeled degradation);
markdown styles links but app-level "click file:line → open in detail
panel" needs link-id hit-testing exposed through the event path.

**P2-8 — Packaged `ToolCallCard` + `ApprovalBar` widgets.** Pure sugar over
2a composition; worth shipping in the console crate first, upstreaming if
they generalize.

**P2-9 — A worked subprocess example in-repo** (spawn child, reader thread,
`WakeHandle::post`, quit teardown). The abstractcoder charter prescribes the
pattern; an `examples/agent_feed.rs` would pin it against regressions and
teach it.

### 2c. The "architecturally hard" list — assessed

- **Streaming into a wrapped scroll region without reflow cost**: not
  architecturally hard here. Damage tracking + the scroll-region emitter
  already make repaint and bytes proportional to change; what's missing is
  incremental **typesetting** (P0-1/2), which is widget-layer work with
  in-repo foundations (measured: full re-wrap of an 800-paragraph document
  is 9.8 ms — affordable on resize; per-token re-parse is the only real sin).
- **Reactive loop beside an async/subprocess source**: solved primitive
  (WakeHandle + waker + phase-ordered Driver loop), test-pinned, including
  worker-panic surfacing. The port's reader threads are ~100 lines.
- **Back-pressure from a fast event stream**: engine coalesces rendering by
  construction; queue growth is the app's to bound (P1-5). No engine
  redesign needed.

### 2d. Port verdict

**Yes, with 3 P0 additions** (Transcript/Feed widget, streaming markdown
session, multiline composer) plus 2 P1s that determine day-2 quality
(lexers, ingestion recipe). Everything else on the console's requirement
list — approvals, tabs, meters, permission switching, detail panel, theming,
async integration, headless testing — is composition over already-shipped,
test-pinned surfaces. All three P0 items have direct foundations in-repo
(List's prefix-sum windowing, md.rs's block parser, TextInput's ClusterMap),
so none is a research project; each is a bounded widget/module with an
obvious test harness (CaptureTerm + VtScreen).
