# Future app-class gap check (FIELD, study 2)

Method: for each of the maintainer's five named future surfaces, read the
existing backlog (overview.md + the app-kits, extensions, control-plane,
live-data, ports tracks in full or at item depth) and answer: does a home
already exist? Verdicts are COVERED / GAP(named) / NEEDS-ITEM. COVERED beats
new items — this report files nothing outside my band; the two NEEDS-ITEM
findings are handed to the integrator with the band they belong to.

## Verdict table

| # | App class | Verdict | Where it lives / what's missing |
| --- | --- | --- | --- |
| 1 | Gateway console (config/admin) | **COVERED** (one named fold-in) | 0510+0520+0530+0540+0550+0560+0590; fold the route-editor default-vs-override pattern into 0510/0590 |
| 2 | Workflow visualization (flow editor) | **COVERED** | 0420 (canvas) → 0440 (auto-layout view) → 0430 (interactive editor); sequencing already load-bearing in overview |
| 3 | Collaborative coordination UI (continuum-style) | **COVERED** | 0210 epic + 0550 (sidebar/tabs) + 0560 (attention banner) + 0580 (member rail) + 0540 (counts) + live-data 0040/0050 |
| 4 | Entity monitoring/management | **COVERED-with-dependency** | Widgets covered (Progress/Badge/0530/0540; graph = 0440 v1.5); the SSE lane NEEDS 0040 promoted — its trigger has fired (consumer evidence) |
| 5 | Realtime monitoring (production monitors) | **PARTIAL** | Charts/badges/tabs/banners/interval covered; GAP(named): time-axis + history-window charts (NEEDS-ITEM, app-widgets band); GAP(named): severity-tinted feed lines = `FeedBlock::Rich` (first-app band); gauge = minor, note only |

## 1. Gateway console class — COVERED

Every sub-need has a named item, and the admin console is already the 0590
validator that binds them:

- **Tables + badges + actions**: 0530 is verbatim this — "STATE BADGES
  (configured / covered / not-configured / linked; enabled / asleep) and
  PER-ROW ACTION BUTTONS (Edit / Clear / Override / Configure; Rotate /
  Disable / Delete; Talk / Manage)" (0530 Context), with the rotate-a-key
  journey as acceptance and 0250-ruling activation semantics encoded.
- **Wizards**: 0520 (stepped container, gates, summary/apply), with
  crash-resume via control-plane 0340 as its accepted first consumer —
  the cross-band edge is already recorded both ends (0520 Metadata;
  overview "0340 ↔ 0520").
- **Forms**: 0510 (field rows, validation, submit gating; `TextInput::
  masked` engine delta already SHIPPED per overview ledger).
- **Chrome**: 0560 header + admin-context banner; 0550 section sidebar;
  0540 chips/counts.

**Named fold-in (not a new item)**: "multimodal route editors" carry one
pattern no item names explicitly — the default-vs-override mode with a
resolved-state line ("Applies now: …"), placeholder-occupied index 0, and
dependent provider→model selects where a saved model may only render under
its own saved provider. The AbstractAssistant settings rewrite
(AGENTS.md 2026-07-17, "combo-fabrication class") is the paid-for lesson:
a route editor that lets the widget's index-0 default masquerade as
configuration ships a footgun. 0510's Context already cites
"provider/model rows, each with validation" and 0590's admin console
names route screens — the integrator should add the
override-mode/resolved-state pattern to 0510's scope (or the admin-console
acceptance journey) so the lesson lands in the kit, not in each app.
Voice/audition ("Test") buttons in route editors are MEDIA's band.

## 2. Workflow visualization / interaction — COVERED

The extensions track was written against exactly this surface. 0430's
Context names the reference UX in the maintainer's own vocabulary: "node
cards with title bars, colored typed-port dots, inline fields; bezier
curves between ports; canvas panning; selection, drag, tooltips" — and its
Current-code-reality section already verified the engine mechanisms
(absolute positioning + solver, pan-without-remount via `style_signal`,
pointer capture for drag, hover/tooltip routing via 0500's tooltip mode).
0420 supplies the stroke substrate (beziers as public API over the
now-private `BrailleGrid`, chart.rs:49-115 cited in-item); 0440 supplies
read-only auto-layout for flows you did not hand-position (layered v1 for
DAG-class — a VisualFlow graph is a DAG — with honest cycle handling), and
is sequenced BEFORE 0430 as risk retirement. The overview's sequencing
edges (0420 before 0430/0440; 0440 before 0430; link seam 0480 before
0430's activation milestone) are already load-bearing.

Checked for gaps and found none item-worthy: live run-state overlay on a
flow graph (the abstractflow run modal shape) is reactive props over
0430/0440's model — a consumer concern, and 0440's determinism promise
("same graph, same picture") is exactly what a status-tinting overlay
needs. Typed-port TYPE registries are app vocabulary, not engine.

## 3. Collaborative agent coordination UI — COVERED

0210 (a2a chat epic) is the umbrella and already encodes the hard domain
semantics (importance from unforgeable signals; acks are triage-seen and
never discharge obligations; body/preview caps). The continuum-style
chrome maps one-to-one onto app-kits items, each of which cites the chat
shell (reference class C) explicitly:

- channel sidebar with unread badges → 0550 NavList ("channels + DMs list
  with unread badges (C)");
- filter tabs with live counts → 0550 FilterTabs ("All / Unread / Asks /
  Needs-vigilance / FYI / Resolved tabs with live counts (C)");
- attention banners → 0560 ("'69 threads need vigilance — Review' with an
  action button (C)");
- member/files/leaderboard rails → 0580 panel rail ("right-edge vertical
  rail of collapsible panels — Assistant / Members / Files / Leaderboard /
  Desk (C)");
- thread view → Feed (shipped 0100) + the consumer-tensions findings
  (notably `FeedBlock::Rich` for multi-ink message headers — see
  field-consumer-tensions.md §4.1, which this class inherits);
- transport → live-data 0040/0050 (0210's stated dependency), with 0060
  (read-only watcher) as the dogfood on-ramp.

No new item. One observation for the integrator: 0210's phase-1 read-only
watcher (= 0060) remains the cheapest way to validate 0040 before either
big port — unchanged from the overview's sequencing, now with extra
urgency from §4 below.

## 4. Entity monitoring / management — COVERED-with-dependency

Widget-side, everything has a home:

- lifecycle states (awake/asleep/paused/visiting) → `Badge` (shipped) +
  0540's chip/count vocabulary + 0530 rich-cell tables for entity rosters
  with per-row actions (Talk / Manage — 0530's Context literally lists
  these);
- cognition/drive bars → `Progress` with thresholds + ramp
  (engine:src/widgets/progress.rs:27-65) covers the "both counts visible,
  amber saturation cue" bar shape today;
- memory-graph views → 0440's v1.5 force stage, which was explicitly
  upgraded to "designed, not research" BECAUSE knowledge-graph-class
  consumers (routinely cyclic, dense) are a named motivator (0440
  Context);
- replay timelines / event streams folded to state-at-T → app logic over
  Feed + signals (the observer's fold-up-to-T is a pure client fold; the
  engine owes it nothing new).

The dependency: **SSE-fed dashboards stand on live-data 0040, which is
still proposed.** The first shipped consumer has now hand-rolled SSE
parsing (abstractcode-tui gateway/sse.rs, 125 lines) and
reconnect/backoff/poll-fallback without jitter (runner.rs:923-1012) — the
exact machinery 0040 describes, including its named thundering-herd risk.
An entity monitor multiplies streams (per-entity replay + roster
polling), making the hand-roll N× worse. Verdict: no new item; **0040's
promotion trigger has fired** — this study's strongest cross-track
recommendation. 0050 (transport ADR) correctly stays gated on watcher
evidence; ureq-class blocking HTTP in a worker thread is what the
consumer proved workable meanwhile.

## 5. Realtime monitoring generally — PARTIAL

What exists and suffices: Sparkline / LineChart (multi-series, optional
axes + range labels) / BarChart / Progress-with-thresholds (all shipped,
src/widgets/chart.rs, progress.rs), `reactive::interval` with the
coalescing contract a suspended monitor needs (missed ticks collapse, no
catch-up storm — interval.rs module doc), Badge tones, 0550 FilterTabs
for log-level filtering, 0560 banners for standing alert states, planned
0150 for notify/bell reachability from components, and the dashboard
example as the seed it was named to be.

Named gaps:

- **GAP → NEEDS-ITEM (app-widgets band, integrator to file): time-axis
  charts + history windows.** LineChart labels VALUE range only; nothing
  renders a time axis, and every monitor hand-rolls the history ring —
  the flagship example included (`const WINDOW: usize = 72` +
  hand-rolled data walks, examples/dashboard/main.rs:42-43). A
  production monitor needs: a bounded time-series buffer (push (t, v),
  drop-by-age or by-count), time-axis tick labels ("now", −30s, −1m),
  and gap honesty when samples pause (the sparkline's finite-gap
  handling, chart.rs, already models the cell-level half). This is one
  cohesive item — `TimeSeries` model + chart time-axis support — and it
  belongs in app-widgets (0100–0190 band; 0190 is free). It should share
  `solve_columns`-style discipline with 0142's table work only if
  natural; do not gate on it.
- **GAP (already-named home): severity-tinted log lines.** A log tail
  wants per-line ink by level inside one feed block;
  `FeedBlock::Text` is single-ink, which is the same `FeedBlock::Rich`
  gap the consumer-tensions report ranks #1
  (field-consumer-tensions.md §4.1). Home: the first-app band next to
  0280 (Feed block capabilities) — one design pass should cover both
  block variants.
- **Minor (note only, no item)**: no radial/arc gauge widget. Linear
  Progress + thresholds covers the class need; braille arc gauges become
  an afternoon of app code once 0420's dot canvas is public. Filing a
  gauge item before 0420 lands would invert the dependency.
- **Alert-state recipe (note only)**: threshold → banner + `notify`/bell
  is a composition of Progress thresholds + 0560 + 0150; worth one
  paragraph in 0560's validator journey or docs/live-data.md, not an
  item.

## Integrator hand-off summary

1. Fold the route-editor default-vs-override + resolved-state pattern into
   0510 (or 0590's admin-console journey) — class 1.
2. Promote live-data 0040: its trigger has fired with shipped-consumer
   evidence (classes 3 and 4; field-consumer-tensions.md §3.5).
3. File the time-axis/history-window chart item in app-widgets (0190
   suggested) — class 5.
4. Take `FeedBlock::Rich` into the 0280 design pass (classes 3 and 5;
   consumer tension #1).
5. Games band findings are separate: reviews/study2/field-games.md +
   docs/backlog/proposed/games/ (0700-0790), filed by this study.
