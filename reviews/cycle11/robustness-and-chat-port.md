# Cycle 11 — independent robustness review + chat-CLI port feasibility

Scope: read-only second opinion on abstracttui 0.1.0 (crates.io-published) as a
foundation for large applications, plus a feasibility and gap analysis for
porting the agora chat surface (`~/projects/a2a`, `agora chat`) onto it.
Method: source reading (src/, tests/, examples/, docs/), the cycle 1-10 review
ledgers, the operator session log (untracked/partial.log), and a verification
battery run on this machine. Where docs and code disagree, code wins; every
"verified" below is something I executed or read directly, everything else is
labeled as claimed.

## 0. What I ran (2026-07-21, macOS Apple Silicon)

| Check | Result |
| --- | --- |
| `cargo test` (default suite) | green — lib 910/0 (16 ignored), ~35 integration suites all 0 failed, doctests 32/0 (23 ignored); consistent with the cycle-10 ledger's 1242 |
| `cargo clippy --all-targets` | zero warnings |
| `cargo check --target x86_64-pc-windows-msvc` | clean |
| `cargo test --test fuzz_big -- --ignored` | 5/5: parser 20,000 hostile chunks 0 panics + 2,000 split-invariance cases clean; PNG 5,000 hostile vectors 0 panics; GLB 5,059 mutants 0 panics (130 loaded / 4,929 rejected); JPEG 3,000 mutations 0 panics; markdown+highlighter 5,000 each 0 panics |
| `cargo test --test live_smoke -- --ignored` | first run 13/14 — `live_widgets` FAILED; passes in isolation; serial rerun 14/14; warm parallel rerun 14/14 in 4.05s (see finding R5) |
| Not run | perf_budgets (release-only), soak, tmux_live (needs tmux session) — results below for those are ledger claims |

Also verified directly: the repo is really published (git remote lpalbou/abstracttui;
the session log records the crates.io publish, the v0.1.0 release, and live CI
run IDs including an initially FAILING Windows job — see R2).

---

## Part 1 — Robustness & integration audit

### R1. Cross-platform truth (P1 for Linux-first users, otherwise P2)

The real split, confirmed from `.github/workflows/ci.yml` and the term/ code:

- **macOS**: full default suite + the live-pty suite (`tests/live_smoke.rs`,
  `src/term/rt5_live_tests.rs`, `src/term/tmux_live_tests.rs`) executed here —
  I re-ran live_smoke myself (14/14 warm). Genuine controlling-terminal
  coverage: signal resize, suspend/resume, keystroke acceptance.
- **Linux**: ubuntu CI runs the full *default* suite (session log records the
  green matrix). The live-pty suites are `#[ignore]`d and ci.yml never passes
  `--ignored` — **no record anywhere of the pty suite executing on Linux**.
- **Windows**: better than "compile-checked only" — ci.yml has a
  `windows-latest` job running `cargo build --lib` + `cargo test --lib`, and
  the session log (lines ~1141-1290) shows 897 lib tests executing on a real
  Windows runner. That job's first run FAILED (3 tests: the SGR-mouse
  heuristic claimed mouse on bare conhost) and was fixed in d6dfb00 — evidence
  the gate is real, not decorative. What has never happened is a live
  interactive console session: `WindowsTerminal::enter` (VT mode flags),
  the auto-reset wake event, and console resize records are untested against
  a real console.

Docs honesty: the README platform table and faq.md state the Windows caveat
plainly ("has not yet been run on a live Windows console. Treat the first
Windows run as a beta event") — accurate, even slightly understated given the
897-test CI gate. The **Linux row overstates**: "Verified — same unix code
paths and pty coverage" reads as if the pty suite ran on Linux; it did not.
The shared-`unix.rs` argument is real but RT5-1 itself proves intra-unix
platform quirks exist (Darwin's poll(2) rejecting the `/dev/tty` alias) — the
symmetric Linux-only quirk class is exactly what an un-run live suite misses.
Fix: run the ignored suite once on Linux and record it, or soften the wording.

Where a Windows user hits a wall today: `suspend()` returns Unsupported
(documented; hide Ctrl+Z), bare conhost gets no mouse (honest degradation
post-fix), and the first live console run may surface wake/resize bugs the
compile+unit gate cannot. `win_logic.rs` mitigates the worst of this: the
UTF-16 surrogate pairing (incl. batch-straddling pairs), conhost zero-repeat
clamp, wake latch and resize dedupe are extracted platform-independent and
unit-tested on every host (6 tests, I read them).

### R2. Parsers under untrusted data — real, not claimed (verified)

The fuzz story survives independent execution. I ran the enlarged campaigns
myself (table above): seeded, reproducible, totality-asserting (never panic,
always a Result), with split-invariance for the input parser (any chunking of
the same bytes yields the same event stream — 2,000 cases clean). Beyond the
ignored campaigns, hostile-input tests run in the *default* suite every build
(adv_input boundary fuzzing, adv_jpeg/adv_gfx ratchets, `alloc_budget`'s
JPEG dimension-bomb test asserting the 65535×65535 rejection path allocates
< 64 KB — the guard fires *before* attacker-sized allocation).

Two security properties that matter for a network-fed chat UI, both verified
in code:

- **Escape injection through drawn text is structurally dead**:
  `Surface::draw_text` strips control clusters at the draw boundary
  (src/render/surface.rs:358) and the presenter emits only its own escapes —
  a hostile message body becomes glyphs in cells, never wire bytes.
- **Hostile terminal bytes cannot forge keystrokes**: unknown escape sequences
  surface as `Unknown` events; bracketed paste is fuzz-hardened with embedded
  escapes neutralized as content (kernel handoff §10, backed by adv_input).
- OSC 52 clipboard is write-only by design (read form = exfiltration vector,
  never emitted) — the right call for a chat client.

Residual known limit RT5-2 (JPEG scan selectors unvalidated against SOF ids)
is documented and harmless for baseline files. Acceptable.

### R3. Idle-zero-cost and the damage model — test-enforced (verified)

Not marketing. The specific pins, all of which I read and all of which ran
green in my battery:

- `tests/adv_app.rs::idle_app_emits_zero_bytes_across_idle_turns` — 16 idle
  turns through the real Driver: zero bytes, zero flushes, `turn.idle` true.
- `alloc_budget::diff_present_steady_state_allocates_nothing` — the RT2-1
  regression pin (was 3,643 allocs/frame at first filing, now 0) under a
  per-thread counting global allocator.
- `alloc_budget::presenter_no_change_frame_emits_and_allocates_nothing`.
- Zero-wakeup idle is structural, not asserted: `App::drive_loop` blocks in
  `wait_for_activity` with no deadline when no timers are pending
  (src/app/mod.rs:369-380); animations pace at 16 ms only while
  `frame_tasks_pending() > 0`.

docs/architecture.md names these exact tests — docs and code agree here.

### R4. Embedding reality — the crux for both ports (verified, one doc gap)

The architecture is a single-threaded reactive graph + an explicitly
separable frame loop, and the separation is genuine:

- `Driver::turn` is public and **never blocks**; `wait_for_activity` /
  `wait_until(deadline)` are the only blocking edges; `RunConfig` injects
  caps/enter/probe. `App::run_on` accepts any `Terminal` impl. Tests drive
  the production pipeline frame-by-frame against `CaptureTerm` (the doctest
  on `App` shows the exact harness).
- Cross-thread ingress is one mechanism: `WakeHandle::post(closure)` queues
  onto a `Mutex<Vec<Box<FnOnce>>>`, flags an `AtomicBool`, and fires the
  terminal waker (unix: self-pipe write; windows: auto-reset event). The
  blocking read returns `TermRead::Wake`; the next `turn` drains all posts in
  phase U, where the closures run with full runtime access. Wakes and frame
  requests both coalesce — a burst of N messages is one wakeup, one frame.
- The epoch rule makes this tear-free by construction: user code runs only in
  phase U, the damage set seals at layout, cross-thread writes arrive only as
  posted jobs. Pinned by
  `tests/adv_app.rs::cross_thread_post_lands_exactly_one_frame_later`.
- Misuse fails loud: `Signal` handles are `Copy + Send` but carry their
  minting runtime's id — using one on the wrong thread is a named panic
  (src/reactive/runtime.rs:160), never silent aliasing.
- Worker death is not silent: `spawn_worker` catches panics and posts a
  labeled failure that `Driver::turn` returns as an app error (RT1-15b).

Two honest limits:

1. **No fd-level embedding.** The `Terminal` trait exposes no raw fd, so you
   cannot select on the terminal from an external reactor (tokio/epoll). The
   supported shapes are: (a) engine owns the UI thread, async I/O lives on
   other threads and posts through `WakeHandle` — the recommended, test-proven
   shape; or (b) the app owns the loop, calling non-blocking `turn()` +
   `wait_until(deadline)` at its own cadence. Both are fine for the target
   ports; a tokio-first application wanting one unified reactor is not the
   supported shape.
2. **The pattern is invisible in the docs (P1).** `WakeHandle`, `spawn_worker`
   and the post-drain contract appear in **zero examples and zero docs/*.md
   pages** (grepped; only rustdoc and architecture.md's one conceptual
   sentence). The dashboard example fakes its feed with `reactive::after`
   timers. For the two target applications this is the single load-bearing
   API, and today you find it only by reading src/reactive/scheduler.rs.

### R5. Test-rig flake observed (P2, characterized)

My first parallel `live_smoke -- --ignored` run failed `live_widgets` (13/14)
where the cycle-10 ledger records 14/14. In isolation it passes; serial and
warm parallel reruns pass 14/14 (4 s). Mechanism: on a cold target the suite
builds example binaries inside the test bodies while 14 pty cases run
concurrently; build-lock serialization can push one case past its 8 s
deadline (the suite comment even names this hazard — "runs the prebuilt
binary (no cargo latency inside the deadline window)" — but the first-ever
invocation still pays the build). Not a product bug; it is the documented
"perf numbers are load-sensitive" caveat manifesting in the rig. Worth a
prebuild step in the harness before the spawn loop.

### R6. API stability & ergonomics at 0.1.0 (P2)

- The prelude is curated and the two-`Style` trap is resolved the right way:
  `layout::Style` is exported as `LayoutStyle`, `render::Style` is
  deliberately absent (prelude.rs:7-10; executed cycle 9 for RT8-1). The full
  paths still collide for anyone importing both modules — livable.
- `#[non_exhaustive]` protects only the input enums (`Event`, key types).
  `term::Capabilities` is a public struct with ~20 public fields and is NOT
  non_exhaustive — adding a capability field is semver-breaking for
  exhaustive literals. In practice construction goes through
  `default()`/`detect_env()`, but the footgun is there.
- Widgets are builder types with private fields — additive evolution is safe.
  Components are plain `fn(Scope, Props) -> View` with `Callback<T>` props;
  the shareable-component story needs no registry or trait objects and reads
  well (ui::compose docs carry the pattern).
- Churn forecast: the crate's own v1-honesty notes name the breaking changes
  most likely to come, and they land exactly where a chat app leans —
  `Scroll::content_size` ("when a layout-query surface lands, the hint
  becomes optional — request filed") and `List` variable-height content
  ("wrapped multi-row item CONTENT is a later decision"). Expect 0.2 breakage
  in scroll/list; the rest of the surface looks settled (the `Terminal` trait
  is declared stable, the damage contract is stable and enforced by test).

### R7. Known-limits list vs the target apps

Confirmed in docs (faq/README/final-status) and code. Judgment per limit:
JPEG baseline-only — attachment previews of progressive JPEGs refuse with a
labeled error and the UI degrades; acceptable (P2 nicety). Sixel single
palette per emission — chat shows at most an image or two; acceptable.
Animation LINEAR/STEP and mosaic 2-color/cell — irrelevant to both ports.
Perf load-sensitivity — documented honestly, and I reproduced its rig-level
manifestation (R5). **None of the published known limits blocks the chat or
console ports.** The blocking gaps are missing widget capability, below.

### Ranked findings (part 1)

- **P1 — Linux live-pty coverage never executed; README Linux row implies it
  was.** Run it once on Linux or reword.
- **P1 — the background-feed pattern (WakeHandle/spawn_worker) is
  undocumented and unexampled**, despite being the load-bearing API for any
  real application with I/O.
- **P2 — Capabilities not non_exhaustive** (semver footgun).
- **P2 — live_smoke cold-start flake** (prebuild before spawning).
- **P2 — Linux/Windows wording nits aside, platform docs are honest**; the
  Windows CI gate is stronger than the ledger's own "compile-verified only"
  summary.

---

## Part 2 — Chat-CLI port feasibility (agora)

### The domain being ported

From ~/projects/a2a/src/agora (read: chat.py, chat_render.py, models.py,
client/client.py, client/inbox.py, listen.py, vote.py): channels + canonical
`dm:a--b` DM channels; append-only messages (`Envelope`: channel/seq, status
`open|reply|fyi|blocked|resolved` + critical + urgency, to/reply_to, title
guaranteed-read ≤120 chars, markdown body ≤64 KB, structured data,
content-addressed attachments); an inbox of envelopes with an interrupt flag
(deliver/drain/wait); transport = WebSocket push with reconnect/backoff +
REST catch-up sorted per-channel by seq + `/inbox` long-poll (≤55 s) as
fallback; acks are triage-seen and never discharge obligations. The existing
`agora chat` REPL renders: a channel directory with stats and unread counts,
the current room in full with sender/status attribution, other rooms as
one-line notices, slash commands (/ask /critical /reply /read /dm /vote
/tally /fs /board /owed /members …), chaired votes with an auto-publish
watcher, and a prompt_toolkit input line that survives concurrent output.

### What AbstractTUI already gives you (verified against src/widgets, src/app)

| Chat need | Engine surface today | Verdict |
| --- | --- | --- |
| Channel/DM sidebar with unread badges | `List` (virtualized, variable heights, sticky selection by key, `scroll_to`) + `Badge` | ready |
| Live inbox → UI without tearing/busy-wait | `App::wake_handle().post(...)` + coalescing waker + phase-U drain; loud panic on misuse; `spawn_worker` for labeled worker death | ready (pattern), P0 docs gap |
| Message list: wrapped markdown, authorship, timestamps, status badges | `MarkdownView` (full token typeset, `rows()` exposes the wrap fold for scroll math), `RichTextView` (span patches), `Badge`; but see the gap below | **the gap** |
| Composer | `TextInput`: cluster-atomic editing, selection, word jumps, whole-paste insertion, `on_submit` — single-line only | partial |
| Right panel (members, votes, obligations) | `Table` (sortable), `List`, `Progress` for tallies | ready |
| Vote ballots, help, confirmations | `Modal` (owns input while visible, `MODAL_Z`), `on_outside_press` | ready |
| Transient notices ("message in #design") | `Toast::show(overlays, cx, vp, text, Duration)` | ready |
| Startup/degradation surface | `use_startup_notices` (reactive store, late pushes propagate) | ready |
| Terminal notification ping, title unread count, copy message | `Terminal::notify` (OSC 9/99, caps-gated), `bell`, `set_title`, `clipboard_copy` (OSC 52 write-only) exist on the trait — **unreachable from component code** while `App::run` owns the terminal (grepped src/app, src/ui: no pass-through) | gap |
| Hostile content safety | control clusters stripped at draw; parser hostile-byte hardened | ready (verified, R2) |
| HTTP/WS client | none in-crate (austere dependency policy is a crate policy, not an app constraint) — bring ureq/tungstenite or a tokio runtime on background threads | fine |

### The live-data path — is it ready?

**Yes.** This is the strongest part of the engine for this port. The shape:

```rust
let msgs = cx.signal(Vec::<Envelope>::new());   // UI-thread state
let wake = app.wake_handle();                    // Clone + Send + Sync
std::thread::spawn(move || {                     // or spawn_worker (labeled death)
    // blocking WS read / long-poll loop, reconnect with backoff (client-owned)
    for batch in hub_events {
        wake.post(move || msgs.update(|m| m.extend(batch)));
    }
});
```

Assessment against the failure modes that matter:

- **No tearing**: posted closures run on the UI thread in phase U; the damage
  set seals at layout; a post landing mid-frame wakes the loop and lands in
  the *next* frame exactly once (test-pinned, R4).
- **No busy-wait**: idle blocks in the terminal read with zero wakeups; the
  self-pipe/event waker interrupts it. A message burst coalesces into one
  wake and one frame.
- **Backpressure**: the posted queue is an unbounded `Vec` (scheduler.rs) —
  a flooding producer grows memory between turns. The agora client already
  solves this shape on its side (Inbox caps at 1000, drop + cursor recovery);
  the Rust port should mirror it: coalesce in the producer thread and post
  ONE closure per wake carrying the drained batch, not one per envelope.
- **Failure visibility**: a dead poller thread via `spawn_worker` surfaces as
  a labeled app error instead of a silently frozen inbox.

What a clean "async data source → reactive signal" binding looks like: it is
three lines away from existing today (above). The missing piece is not
machinery — it is a name, an example, and a paragraph. A `feed` helper that
packages producer-side batching + `spawn_worker` + a `Signal<Vec<T>>` would
close it, but even `examples/feed.rs` alone would.

### Improvements, prioritized

**P0-1 — a rich, appendable message list.** The one real engineering gap.
Today `List` is `Vec<String>` with the label on the item's first row only
(module doc: multi-row content "a later decision"), `Scroll` needs a manual
`content_size` hint, and `dyn_view` replaces its whole subtree per change —
there is no keyed reconciliation, so a naive `dyn_view` over the message
vector rebuilds every message view on each arrival. A port CAN ship today by
hand: window the transcript (render the last N messages, like agora chat's
own BODY_MAX_LINES posture), compute heights via `MarkdownView::rows(width)`,
sum into `Scroll::content_size`, and rebuild the window per arrival (layout
solve is ~112 µs for 480 children; a 50-message window is fine). But this is
the port's largest cost and it is throwaway. Proposed engine surface, either:
(a) `List` items as measured `View` content (the already-filed direction), or
(b) a keyed-children primitive (`keyed(style, iter, key_fn, render_fn)`)
plus the layout-query that makes `content_size` optional (also already
filed). **Identical need in the coding console** (streaming agent/build log)
— this widget pays for itself twice.

**P0-2 — document + exemplify the background-feed pattern.** An
`examples/feed.rs` (thread appending lines through `wake_handle().post`,
follow-tail scroll) and a docs/api.md section on `WakeHandle`/`spawn_worker`/
the batching idiom. Cheapest high-leverage item on this list; both ports
start here on day one. (Consider re-exporting `WakeHandle` in the prelude.)

**P1-1 — multi-line composer.** agora bodies are markdown; a single-line
`TextInput` forces one-liners or external editors. Proposed: `TextArea` (or
`TextInput::multiline(max_rows)`) with Alt+Enter/Shift-Enter newline (kitty
protocol already disambiguates where supported; kernel handoff §4 names the
legacy limits honestly), grow-to-content up to a cap. Console overlap: low
(command input is single-line).

**P1-2 — app-level terminal verbs.** Chat needs an unread ping
(`notify`/`bell`), a title unread counter (`set_title`), and copy-message
(`clipboard_copy`). All four exist on `Terminal` but components cannot reach
them. Proposed: a posted terminal-request queue on `Overlays`/`Actions`
drained by the driver during present (preserves the one-writer rule and the
one-flush contract), caps-gated exactly like the verbs already are. Console
overlap: title = build status, bell on failure.

**P1-3 — follow-tail idiom.** Chat and log views both want "pinned to bottom
unless the user scrolled up". Buildable today against `Scroll`'s offset
signal + `List::scroll_to`, but every consumer will write the same
edge-cased code. Proposed: `Scroll::follow_tail(Signal<bool>)`.

**P2 — smaller items.** Bound or document the posted-job queue growth
(producer-side batching guidance); progressive-JPEG acceptance for
attachment previews (currently a labeled refusal — fine); an `interval`
helper beside `reactive::after` (the dashboard hand-rolls re-arming);
`Capabilities` non_exhaustive before the surface calcifies; run the live-pty
suite once on Linux and record it.

### Feasibility verdict for the chat port

Feasible now, with eyes open. The hard problem — a network thread feeding a
terminal UI without tearing, busy-waiting, or silent death — is solved,
test-enforced, and better-engineered here than in most established TUI
stacks; it is merely undocumented. The security posture for untrusted
content (draw-boundary control stripping, hostile-byte parser, write-only
clipboard) is exactly what a hub client needs. The composer, sidebar,
tables, modals, toasts, themes and notices are ready. The message list is
the one component the engine does not yet provide in earnest: budget the
port as "one hand-rolled windowed transcript view now" or "one upstream
widget cycle first", and prefer the latter if both the chat CLI and the
coding console are coming — it is the same widget.
