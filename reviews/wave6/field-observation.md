# Wave 6 — adversarial field observation: the two validator apps

- Date: 2026-07-23 (evening; both apps mid-flight, launched from the wave-6 validator prompts)
- Observer scope: read-only in both consumer repos; engine repo read + this file only
- Engine baseline: abstracttui 0.2.9 published (local tree carries uncommitted
  choice_prompt churn from a parallel fix agent — every engine citation below was
  verified against stable files or the published-content match, not the churning ones)
- Apps: **A** `abstractgateway-console` (`abstractgateway/console-tui`, field-gateway
  0900–0990) · **B** `agora-tui` (`~/projects/gh/agora-tui`, field-agora 0800–0890)
- Filing count at observation time: **18** (13 gateway + 5 agora) — grown from the
  10 the observation brief named; all 18 are spot-checked below.

---

## 1. Per-app health verdicts

### App A — abstractgateway-console: HEALTHY, past its definition of done, one real flake

- **Build**: green on `abstracttui = "0.2.9"` (bumped mid-build when the engine
  released; Cargo.lock confirms 0.2.9 from crates.io). lib+bin split, ureq/serde_json
  per the brief's dependency policy.
- **Tests**: 1 lib unit + **34 headless CaptureTerm** + 1 live E2E (`#[ignore]`,
  env-gated, RAII cleanup that *restores* a pre-configured route rather than
  clearing it — better than asked). Clippy clean.
- **THE FLAKE**: the first observed run (both apps' suites executing concurrently on
  this machine) failed `runtime_knobs_render_with_provenance` with a garbled frame —
  one row read `operator_email: —  (default)ed)nsole or the API — TUI editing is a
  follow-up)`: a shorter line painted over a longer prior render without the stale
  tail being cleared. 5/5 immediate re-runs and 30/30 isolated stress runs pass —
  the failure needs system load. Root-cause analysis in §4 (it is the missed
  `Driver::set_clock` API + the 0960 draw-closure lane, not random).
- **Milestones vs brief**: all six screens exist and are wired end-to-end
  (Connection · Providers · Routes · Users & Entities · Runtimes · Review & Test —
  the brief asked for five; Runtimes was added for web-console parity). The §4
  design laws are implemented faithfully — the routes editor is a textbook
  fabricated-selection defense (`routes.rs`: mode radio default-vs-override,
  placeholder at index 0, pickers disabled in default mode, an "Applies now:"
  resolution line, "nothing configured — engine decides"). CHANGELOG records a live
  API E2E + keyboard pty smoke against a real gateway and **three same-day
  adversarial self-review cycles** (cycle 2 found and fixed an app P1 — the
  invisible-prompt key-steal that became filing 0945).
- **Structure**: 8,176 lines. Five files exceed the workspace's <600 discipline
  (providers.rs 907, worker.rs 905, routes.rs 849, ui/mod.rs 757, store.rs 732;
  tests/headless_ui.rs 1,473). Cohesion inside them is good (one screen per file);
  routes.rs and providers.rs are each one screen + its modal editors and would split
  naturally at the editor boundary.
- **Engine usage quality**: strong with one notable reimplementation (§3). Zero raw
  colors (grep-verified). Census: 18 TextInput, 6 Table, 5 ChoicePrompt, 4 Select,
  2 Combobox, 34 dyn_view regions, exactly 1 `interval` — the busy ticker, created
  only while ops are in flight and cancelled after (zero-idle honored). Every filed
  workaround exists in code exactly as filed: `open_form` closer slot (0940),
  `GuardSlot` Esc-guard (0935), `prompt_open` counter + token queue gating (0945),
  `util::clamp_selection` (0970), `fit_width` span clipping (0960), Ctrl+N/P nav
  lane (0920), root-element shortcut registration with the focus-path comment (0910).

### App B — agora-tui: HEALTHY, all three milestones executed, minor honesty debts

- **Build**: green on `abstracttui = "0.2.8"` (lock confirms; did not bump to 0.2.9 —
  harmless, 0.2.9 is ChoicePrompt-only and the watcher opens no prompts).
  ureq + serde_json + tungstenite per the brief. Clippy clean, no `unsafe`.
- **Tests**: **88 total** (24 unit + 28 `probe_c2` + 36 `watch_ui`), 0 failures.
  Test names are genuinely adversarial: shuffled-burst seq ordering, dedup across
  catch-up overlap, split-first-fill fabricates no unread, hostile bodies inert,
  lane overflow labeled, **`quiet_app_emits_zero_bytes_between_events`** (the idle
  law pinned from the app side — 16 turns, byte-count equality).
- **Milestones**: M1 (tail) done; M2 (multi-pane, sidebar List + unread, presence,
  toasts, tombstones, status badges) done; M3 (reconnect honesty) done **with a
  real kill/restart drill on a scratch hub** — the experience report records
  attempts 1..7 with live-ticking jittered countdowns, exactly-once redelivery of
  messages posted during the outage, and the degraded long-poll lane. The
  experience report (`docs/experience-report.md`, the milestone's DoD artifact)
  exists and is substantive.
- **Honesty debts (theirs)**: the report's validation list claims "Idle soak:
  multi-hour run … byte growth tracked" while line 125 admits "(pending:
  byte-growth samples from the 2h live-hub soak)" — the claim is ahead of its
  evidence. `tests/probe_c2.rs` is headed "TEMPORARY cycle-2 adversarial probes —
  DELETE BEFORE FINISH" and still ships. The directory is not a git repository at
  all (never initialized — consistent with its "never run git" rule, but the
  operator should know there is no history).
- **Structure**: 3,871 lines, every file under 600 — the discipline held. One
  structural wart: binary-only crate, so integration tests import src via
  `#[path = "../src/..."]` includes (modules compile twice, `#[allow(dead_code)]`
  noise); the lib+bin split App A used is the standard fix.
- **Engine usage quality**: exemplary — the best consumer read of the engine so far
  (§3). `reactive::connection` + `Backoff::default()` with generation-checked
  workers; `bounded_source(1000, DropOldest)` with `stats.dropped` rendered in the
  header; `latest_source` for sidebar lanes with worker-side change detection (a
  quiet hub posts *nothing* to the UI thread); `FeedState::sync_with` keyed
  `"{channel}:{seq}"` with a rev fingerprint for in-place body fills;
  `Scroll::follow_tail`; `List.selection_key`; `use_startup_notices`; engine
  `retry_now()` correctly wired to the `r` key. Zero raw colors.

---

## 2. Filed-item verification (all 18, against engine source)

Method: every engine `file:line` cite opened and read; behavioral claims checked
against the actual code path; the two "does an existing API already cover this?"
questions from the observation brief answered explicitly. Line numbers drift ±a few
lines against the dirty local tree; **content matched in every case**.

| ID | Claim verified? | Notes |
| --- | --- | --- |
| 0800 | **Real** | `notices.rs` store is thread-global grow-only `Signal<Vec<String>>` (`publish_notice` only pushes, no cap); `driver.rs:396-410` fans per-frame collapse diagnostics *and* image-ladder notes into it mid-session. The "startup" name is honestly wrong for what the lane carries. |
| 0810 | **Real** | `list.rs:69` `items: Vec<String>`; row painting is a single print with the row ink pair; `Badge::element` exists but nothing accepts it row-scoped. Direct 0550 NavList evidence. |
| 0820 | **Real — and it does NOT overlap `retry_now`** (the brief's question): `connection.rs` `retry_now` explicitly early-returns unless `matches!(state, Reconnecting)` — "an in-flight attempt is not restarted" is in its rustdoc. The filing pre-checked this itself. `Report::Failed` is the only live-attempt-superseding path. The app's fake-failure workaround (`transport.rs::long_poll` reporting `failed("WS restored — re-dialing push")`) is real and does inflate the attempt counter. |
| 0830 | **Real — and it does NOT overlap `Reconnecting{next_in}`** (the brief's other question): `next_in` is the jitter draw at transition time; `arm_retry` computes the true deadline (`now + next_in`) straight into `arm_timer_at` and nothing exposes it. The app's workaround (deadline-capture effect + scoped 500ms interval that dies with the Reconnecting region) is exactly as filed in `header.rs:70-102`. |
| 0840 | **Real** | getting-started's "Layout basics" (line 113) never mentions `basis` or the intrinsic-basis/leftover interaction; api.md names `basis` once as vocabulary (line 139) and the knowledge lives only in the modal-overflow Scroll note (line 502). The app's one-line fix + citing comment is at `panes.rs:250-256`. |
| 0900 | **Real** | `table.rs:386` `solve_columns`: `Cells` clamp against `remaining` in declaration order; flex shares only `if any_flex && remaining > 0` — oversubscription zeroes the flex column silently, later fixed columns also clamp. The cycle-2 addendum (paired width-branch desync hazard across five tables) is honest added cost. |
| 0905 | **Real** | `select.rs:273-281` `write_value` early-returns on equal index; no companion commit event exists. The app shipped the "Retry model discovery" button instead. |
| 0910 | **Real, docs half slightly overstated** | Dispatch is focus-path-only (`tree.rs:684+` walks `path`; `:292` "No focus = the root's shortcuts"); Actions run last (`driver.rs:738-751`). *Nuance the filing misses:* `Element::shortcut`'s rustdoc (`view.rs:218-220`) does say "Resolution walks root -> focus path" — the scope fact exists in compressed form; what is genuinely unstated is the consequence (off-path registrations never fire; use Actions for globals). Still a real docs gap, mildly overclaimed. |
| 0920 | **Real** | Same mechanics; correct read that this is convention material for the 0520 wizard kit, not a bug. |
| 0930 | **Real** | `button.rs:79` `disabled: bool`, build-time only; no `Signal<bool>` overload anywhere in the widget set. The granularity dance it forced is documented in routes.rs's own comments. |
| 0935 | **Real (evidence class)** | No form/field-group state container exists; the GuardSlot pattern is ~25 lines × three forms as filed. |
| 0940 | **Real** | `popups.rs:63+`: `build` runs before `Modal { layer, scope }` is constructed; `share()` needs the first handle. The slot dance is the only way today; ChoicePrompt's internal resolver proves the callback machinery exists. |
| 0945 | **Real** | `overlays.rs` dispatch: `targets.sort_by_key(Reverse(z))` is stable, so equal-z iteration order is insertion order — the FIRST (= oldest) modal in the walk wins keys, while painting favors the newest. No modal-count/top-modal query exists. This is the **second consumer app** to hit the equal-z key/paint disagreement (abstractcode-tui hit the sibling case 2026-07-22). Cite `choice_prompt.rs:380` sits in the churning file; the load-bearing claims verified against stable overlays.rs/popups.rs. |
| 0950 | **Real (evidence, correctly self-limiting)** | `connection()` is persistent-transport shaped by design; the filing itself argues *against* an engine change until more probe-shaped consumers exist. Right call. |
| 0960 | **Engine claim real; context claim FALSE** | draw.rs: closures paint into a `ClippedCanvas` scoped to the *damage* rect; `clip_overflow` is opt-in and absent from api.md (grep-verified). **But** "there is no standalone rich-line widget" is wrong: `RichTextView` (`widgets/richtext.rs`) with `.wrap(false)` is exactly that, clips spans at the element's right edge (`draw_rich_lines` → `print_span_clipped`, with a comment naming the very discipline draw closures lack), and App B uses it for the identical chrome-line job. App A's whole `line()`/`field()`/`fit_width` machinery (`util.rs:27-100`) reimplements it. Part engine footgun, part missed API — see §3/§4. |
| 0970 | **Real** | `table.rs`: the only `selection.set` is in the key-nav arm (line ~159); rows supplied at build never clamp the bound signal. The filing's warning about the read-time-clamp "fix" targeting an unseen row is the sharpest destructive-misfire observation in the set. |
| 0980 | **Real, one detail wrong** | `table.rs:194-208`: the `s` arm calls `ctx.stop_propagation()` whenever the table has columns, handler bound or not — key eaten, confirmed. *Detail wrong:* "the table's sort indicator cycles with no app-side effect" — without a handler nothing cycles (the indicator is driven by the app's `sorted` prop); the key simply vanishes. Doesn't change the fix. |
| 0990 | **Real (evidence class)** | No correlate-by-id completion primitive; the app's form_id + single-slot + claiming-effect convention is as filed, and its hazards were found by the app's own cycle-1 review. |

**Already-fixed-by-0.2.9 check**: none of the 18. 0.2.9 shipped ChoicePrompt
dual-spelling key folds + a body slot (commit 1f05621) — orthogonal to every filing.
The in-flight choice_prompt churn may land 0945-adjacent work; nothing shipped yet.

Filing hygiene overall: **excellent**. Both apps pre-checked the obvious "does an
existing API cover this?" objections inside the filings themselves (0820 cites
`retry_now`'s guard; 0830 cites the arm-site; 0950 argues against its own fix). The
two factual defects found (0960's context line, 0980's indicator detail) don't
change any fix decision — but 0960 needs its context corrected before someone
builds "a standalone rich-line widget" that already exists.

---

## 3. Engine-usage quality: idiomatic or fighting?

**App B is the reference consumer.** Its transport module is a 1:1 mapping onto the
engine's architecture rule (workers never touch signals; generation-stamped
reports; lanes in, ConnectionEvents out), and its state fold (`state.rs`) is the
sanctioned bounded_source pattern with an idempotent re-walk the code comments
justify. It hand-rolled nothing the engine covers: follow-tail is engine
(`Scroll::follow_tail`), sync is engine (`sync_with` + rev fingerprints), sticky
selection is engine (`selection_key`), the countdown/redial/badge-slot/notice-fold
hand-rolls are precisely its four filings. One deliberate near-fight that is
actually sound: the sidebar poller is a worker thread parked on a hand-rolled
condvar `StopGate` instead of the brief-suggested `reactive::interval` — correct,
because `interval` fires on the UI thread (a network poll can't ride it directly)
and worker-side change detection means a quiet hub wakes the UI zero times, which
is *stricter* than the interval shape. The engine could bless this pattern in
docs (see prompt feedback).

**App A is idiomatic in the large, with one significant reimplementation.**
The worker/store/screens architecture follows the live-data law exactly
(one worker owns HTTP; results are posted closures; verify-after-write lives in
the worker so screens cannot forget it). Select/Combobox/Table/ChoicePrompt/
TextInput usage is textbook, the busy ticker is the only interval and it exists
only while ops run. The reimplementation: `util::line()`/`field()` — multi-ink
chrome rows as raw `Element::draw` closures with hand-rolled cell-width
truncation — duplicates `RichTextView.wrap(false)`, which App B uses for the same
job. That miss produced the 0960 bleed, the `fit_width` machinery, and (likely)
the flake's stale-overlap frame: `line()` prints text without filling its
background, so its cells depend on ancestors' fills and damage-rect luck — the
exact lane `RichTextView`'s fill + span clipping guards. Not fatal, pervasive
though: ~every screen renders through `line()`.

**Zero-idle**: both apps honor it and App B test-pins it. **Tokens**: both apps are
raw-color-free (grep-verified; App A's util module even declares the rule in its
header). **Layout gymnastics**: none found beyond the justified basis(0) idiom
(filed as 0840's docs ask).

---

## 4. Unfiled tensions worth items (the hunt's yield)

1. **Headless suites are wall-clock-load-sensitive because nobody knows
   `Driver::set_clock` exists** — the strongest new finding of this observation.
   The engine HAS the API (`driver.rs:271`, "Tests drive turns on synthetic time
   instead of real sleeps") — but it appears in **neither docs/ nor
   llms-full.txt** (grep-verified), and both launch prompts pointed consumers at
   exactly those two surfaces. Consequences observed: App A's `press_escape` does
   a real `std::thread::sleep(45ms)` to cross the bare-ESC deadline (harness,
   `headless_ui.rs:113-118`), toast/interval timers ride `Instant::now()` through
   `Driver::turn`, and the one observed test failure happened exactly when both
   suites ran concurrently (CPU contention shifting frames across wall-clock
   deadlines → different damage sequences → the stale-overlap frame). Fix is a
   docs/testing-guide item plus consideration of surfacing time control on the
   CaptureTerm/Driver harness path. Evidence chain: 1 failure in the loaded run,
   0 in 35 unloaded runs, garbled frame captured in this observation's log.
2. **`ConnectionEvents` offers no cancellable wait** — App B's workers must
   poll-slice sleeps at 250ms to honor close/supersede promptly
   (`transport.rs:52-62`); every transport worker on the engine will rewrite this
   loop. A waiter (condvar or `wait_closed(timeout)`) on the events handle would
   delete it. (App B's own `StopGate` proves the shape they wanted.)
3. **`RichTextView` as the chrome-line widget is untaught** — api.md names it once
   inside the reader-widget list ("wrapped styled spans"), reading like a
   document viewer. App A reimplemented it with draw closures and filed 0960's
   false "no standalone rich-line widget" line as a result. One api.md paragraph
   + a getting-started snippet ("a styled status line = RichTextView with
   wrap(false) + fill") converts 0960's workaround into a deletion.
4. **Feed items materialize theme inks at sync time** (App B `panes.rs:192`,
   `render_item(r, &current_theme().tokens)` inside the SyncSpec): a runtime theme
   switch would leave stale-colored items until each row's rev changes. Latent
   today (no theme-switch key in either app; both set theme at boot) — but the
   engine's one-theme-signal law implies runtime switching is a supported story,
   and the sync-item cache sits outside it. Worth a docs note or a
   `FeedState::retint`/resync hook before a consumer ships a theme toggle.
5. *(Weak)* the bounded_source fold re-walks the retained window per drain (App B
   `state.rs:206`, self-justified as idempotent). At cap 1000 this is fine; a
   consuming-drain variant would make O(new) folds natural. Docs-line strength,
   not an item yet.

---

## 5. Ranked fix queue (18 filings; consumer-blocking > honesty > polish)

1. **0945** — equal-z modal key/paint disagreement (+ no prompt introspection).
   Second app to hit the class (abstractcode-tui 2026-07-22 precedent); it enabled
   an app-level P1 where Enter could fire "Rotate token" on an *invisible* prompt.
   The core fix is one rule: **paint order and key order must agree** (newest-wins
   at equal z). A queryable modal/prompt count is the secondary ask.
2. **0970** — Table (and List) must clamp bound selections when rows shrink. Every
   CRUD screen hits it on delete-last-row; the "obvious" app-side read-time clamp
   is a destructive-misfire trap. Widget-owned invariant, small change.
3. **0900 (+ addendum)** — silent flex starvation under oversubscribed fixed
   columns; the addendum's per-column `min_width` + drop-priority is the shape
   that deletes ~90 lines of paired-breakpoint convention per app.
4. **0930** — reactive `disabled(Signal<bool>)` on Button/Select/Combobox (or the
   0510 kit owning it). It dictated App A's whole form architecture; focus loss on
   rebuild is a class defect every validation-gated form will meet.
5. **0905** — `on_commit` firing on every popup commit beside the value-change-only
   `on_change`. Dead retry gestures; the Popup's `DismissReason::Committed`
   vocabulary already exists one layer down.
6. **0910** — docs sentence in `Element::shortcut` + api.md naming the focus-path
   scope and pointing at Actions; optional debug-build orphan-shortcut walk.
   Cheapest hour-per-consumer saving in the queue.
7. **0960** — default-clip draw-closure output to the element rect (opt-out for
   deliberate overdraw), or failing that a loud docs warning — **plus** correct the
   filing's context and teach RichTextView (§4.3). Likely implicated in App A's
   observed flake frame.
8. **0800** — split the diagnostics lane from startup notices (kind/severity or a
   `use_diagnostics` hook) and bound the vec. Long-lived apps otherwise inherit an
   unbounded, mislabeled firehose.
9. **0840** — one-paragraph grow-vs-intrinsic-basis addition to "Layout basics".
   Diagnosed by an adversarial reviewer, not the builder — meaning the next
   builder loses the same session.
10. **0980** — claim `s` only when `on_sort_requested` is bound (the engine's own
    unbound-Enter/Space precedent, quoted in the filing).
11. **0830** — `Connection::retry_deadline() -> Option<Instant>` accessor; the
    engine computes the deadline at arm time and drops it. Every honest reconnect
    countdown re-derives it with post/drain skew.
12. **0820** — `redial_now()`/`superseded(reason)` verb for planned transport
    switchovers (verified: `retry_now` cannot serve it). Keeps attempt counters
    honest on upgrades; also serves credential rotation and subscription changes.
13. **0810** — List row accessory slot (or 0550 NavList promotion): tone-carrying
    unread chips are inexpressible today.
14. **0940** — hand `build` a lightweight close handle in `Modal::open`
    (ChoicePrompt's resolver proves the machinery). Deletes the slot dance from
    every form-owning app.
15. **0935 / 0990 / 0920** — not standalone fixes: fold as requirements into the
    0510 form kit (dirty tracking, submit-completion correlation) and 0520 wizard
    kit (input-immune nav chord convention + footer wording). The two apps'
    conventions are the kit's spec, already written.
16. **0950** — hold, as its own author argues; revisit at the second probe-shaped
    consumer.

---

## 6. Top-5 engine insights from the field

1. **Discoverability is now a bigger tax than missing capability.** Four
   independent misses where the API existed but the teaching didn't:
   `RichTextView` (App A rebuilt it), `Driver::set_clock` (both harnesses
   wall-clock-fragile; absent from llms-full.txt entirely), Actions-for-global-keys
   (found "only by reading the engine's own harness", 0910), grow-vs-basis (0840,
   knowledge existed only in a modal-overflow note). llms-full.txt is the de-facto
   consumer index — an API absent from it functionally does not exist.
2. **Equal-z overlay routing is a cross-app defect class, not an incident.** Two
   consumer apps, two different compositions (abstractcode-tui modal replacement;
   gateway-console prompt+queued-modal), same root: paint order and key order
   disagree at equal z. The invisible-key-owner failure is destructive-misfire
   grade. This is the queue's #1 for a reason.
3. **The Table family assumes cooperative data; CRUD apps are not cooperative.**
   Three of the four table filings (0970 selection, 0900 widths, 0980 key claims)
   are widget-owned invariants the widget doesn't own yet. The 0530 upgrades
   should be invariant-first (selection validity, width honesty, key claims
   conditioned on handlers) before feature-first (badges, row actions).
4. **Forms are the largest un-owned surface.** 0930+0935+0940+0990+0920 are one
   coherent kit spec written twice in app code: reactive enablement, dirty
   tracking, close handles, submit correlation, nav chords. App A is the 0510/0520
   promotion trigger its launch prompt predicted — the evidence is filed and
   consistent.
5. **`reactive::connection` held under its first real transport.** Kill/restart
   drills, generation-stamped zombie suppression, full-jitter draws rendering
   visibly decorrelated — the architecture survived contact. Everything found is a
   verb or an accessor (0820, 0830, the events-waiter in §4.2), not a shape
   problem. That is the 0050 transport-ADR evidence the milestone wanted, and it
   is positive.

---

## 7. Prompt-quality feedback (for the next prompt-writing wave)

**What worked (both prompts):** the feedback-protocol section produced 18
well-formed filings with correct band discipline, README tables, severity
vocabulary, and workaround-to-delete framing — the skeleton-inline decision paid
off. The API grounding sections were largely load-bearing: App A's live E2E
matches §2 of its brief endpoint-for-endpoint; App B's §2.2 membership-gate
warning was heeded exactly (memberships outside the binary).

**Prompt-induced errors found:**

1. **Agora brief §2.5 named a fallback lane that cannot work** — "`GET /inbox?wait=55`
   in a loop … keeps the watcher live at zero busy-wait." The builder proved it
   false against hub source (inbox serves the first 100 rows after the seat's
   *ack cursor*; a never-acking watcher goes permanently stale past 100 unread,
   and a non-empty inbox returns instantly — a busy loop, not a park) and built a
   paced `/channels`-diff lane instead, documenting the correction in
   `api.rs:160-168` and the experience report. Best possible failure mode — the
   validator validated the brief — but the error was ours.
2. **Agora brief suggested `engine interval` for the sidebar refresh** — `interval`
   fires on the UI thread; a network poll can't ride it directly. The builder's
   worker+condvar shape is stricter than the suggestion (zero UI wakes when
   nothing changed). The next prompt should teach the worker-side-poll +
   change-detect shape as the sanctioned network cadence pattern.
3. **Gateway brief's §5 "load-bearing APIs" list omitted `RichTextView`** — and
   the app hand-rolled chrome lines with draw closures, leading to the 0960 bleed
   and a false claim in that filing. The omission is upstream of the defect. The
   sibling prompt (agora) listed Feed's rich-line machinery, and that app used
   RichTextView correctly — strong evidence the API lists steer builders more than
   the docs do.
4. **Neither prompt taught the headless-time story** (`Driver::set_clock`) — both
   harnesses are wall-clock-sensitive, one observably flaked. Add one line to the
   testing-pattern paragraph of every future launch prompt; add it to
   llms-full.txt regardless.
5. **No engine-release policy mid-build**: 0.2.9 shipped mid-flight; App A bumped
   same day, App B stayed on 0.2.8. Harmless here, divergence-prone in general —
   one sentence ("when the engine releases during your build, bump unless the
   changelog names a break") closes it.

**Budget note**: the agora brief sized the watcher at ~2 days and asked for overrun
to be treated as a finding; the app landed M1–M3 in a single session — the estimate
was conservative, no finding owed.

---

## 8. Are the validator apps doing their job?

**Yes — decisively, and the evidence is specific.** The job was (a) ship real
consumers, (b) surface engine truth. On (b): 18 filings of which 16 verified fully
accurate and 2 carry minor factual defects that don't change the fix; every filing
carries a shipped workaround the engine fix can delete; two filings (0820/0830)
pre-answered the "doesn't an existing API cover this?" objection correctly, which
this observation confirmed against source. The field found one defect class the
engine's own suites never could (equal-z key/paint disagreement under *composed*
app modals — now hit by two independent apps), produced the first live evidence
for the 0050 transport ADR, and wrote the 0510/0520/0530 kit specs in the form of
working app conventions. App A's three same-day adversarial self-cycles are the
strongest sign the loop is healthy: filings 0905/0935/0945 came out of its own
reviews, not the engine team's.

**Where the validators are NOT yet doing their job:** neither app exercises
runtime theme switching (the retint lane — §4.4 — has zero field coverage), mouse
interaction is nearly untouched (App B is keyboard-only by design; App A barely),
the media/image lanes have no second consumer, and both test suites are
wall-clock-fragile in a way the engine already solved (`set_clock`). App B owes
two hygiene items before its report is citable as milestone evidence: the pending
soak numbers its summary already claims, and deleting the self-labeled temporary
`probe_c2.rs`.

**Net:** the field program is returning more engine truth per day than any
internal review wave so far, and the highest-leverage engine work it points at is
not code first — it is (1) the equal-z routing rule, (2) Table invariants, and
(3) making llms-full.txt carry the APIs consumers keep rebuilding.
