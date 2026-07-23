# AbstractTUI backlog — overview

Planning memory for AbstractTUI (the Rust terminal-UI engine, published as
`abstracttui` 0.1.0). The engine itself is complete and shipped; this backlog
tracks the work that turns it from "a proven engine" into "a foundation people
build long-lived, networked applications on." It is organized around one
honest observation: nobody has yet built a networked, long-lived app on
AbstractTUI, it ships no async/HTTP/WebSocket story, and its text input is
single-line. The two evaluations in `reviews/cycle11/` are the evidence base;
every item cites concrete engine code.

## Design principles

General-needs-first (every capability justified by an app class, never one
app), apps-as-validators, standalone dependency posture, honest degradation,
zero idle cost — codified with the milestone bands and validation vehicles in
`planned/0001_roadmap.md`, the canonical roadmap.

## Counts

| State | Count |
| --- | --- |
| Planned | 3 |
| Proposed | 50 |
| Completed | 34 |
| Deprecated | 0 |
| Recurrent | 0 |

(Counted from the filesystem 2026-07-23, cycle-2 single-writer fold:
`[0-9]*.md` files under planned/, proposed/, completed/. This fold
records the whole cycle: the builder wave's moves (0102/0104/0190 +
0142/0144/0146/0148 to completed/app-widgets/, 0610/0620/0650 to
completed/media-av/, 0700 to completed/games/ — INPUTAV's rows below),
0180 planned→completed (REVIEWER's scheduled-gates leg), 0297 + 0040
proposed→completed (FIXNET), and one NEW filing counted in:
first-app/0299. Renumber note (wave-3 CLOSER, cycle-3 close): that
filing landed as first-app/0300 — colliding with control-plane/0300 —
and was renumbered to 0299 per the cycle-2 review demand; a second
mid-close filing landed as first-app/0310 — colliding with
control-plane/0310 — and was renumbered to 0291 the same way. The
proposed count is now 50 with the 0291 filing counted in.)

## Topic tracks

| Track | Dir | State | Purpose |
| --- | --- | --- | --- |
| live-data | `planned/live-data/`, `proposed/live-data/` | Mixed | Network-driven reactivity: async-source→signal binding, bounded ingestion, reconnect, the transport decision, and the read-only watcher milestone. |
| app-widgets | `planned/app-widgets/`, `proposed/app-widgets/` | Mixed | The content-widget layer real apps need (feed/transcript, streaming markdown, multiline composer, follow-tail scroll, lexers) + the API-stability and platform-accuracy passes. |
| ports | `proposed/ports/` | Proposed | The two application epics that consume both tracks: a coding-agent console and an a2a chat TUI. |
| first-app | `proposed/first-app/`, `completed/first-app/` | Mixed | Bug/footgun reports from the first shipped application (`abstractcode-tui`, 2026-07-21): reproduced engine defects with field workarounds to delete. |
| control-plane | `proposed/control-plane/` | Proposed | Making running apps observable and drivable from outside their own keyboard: lifecycle events, an automation bus + opt-in JSONL control server (MCP-bridgeable), declared-keys persistence with crash-resume, and headless serve with terminal attach/detach. |
| extensions | `proposed/extensions/` | Proposed | Modularity architecture (two feature classes + the `abstracttui-*` sibling family, ADR-ready) and the diagram-class capability lane: core vector canvas + link-registration seam, node-graph widgets, mermaid subset, mdpad-reader enablement, and the standing web-rendering verdict. |
| app-kits | `proposed/app-kits/`, `completed/app-kits/` | Mixed | The application-kit layer over the content widgets: anchored-popup substrate + choice controls, form kit + wizard, rich data tables, chip/count vocabulary, navigation (sidebar + filter tabs), header/banners, tree view, split panes + panel rail — proven by three in-repo reference validators (admin console, setup wizard, triage shell). |
| media-av | `proposed/media-av/`, `completed/media-av/` | Mixed | MEDIA's band (0600–0690): voice/AV UI plumbing (push-to-talk contract, meter/scope widgets, speaking highlight, external-process pattern, the no-audio mock demo) + image-path follow-ups from the study-2 truth audit (`reviews/study2/media-images-truth.md`). |
| games | `proposed/games/`, `completed/games/` | Mixed | Retro-games feasibility band (0700–0790, `reviews/study2/field-games.md`): four general-need gaps a cell game exposes — key press/release state, public frame tasks + fixed-timestep helper, sprite/tile toolkit (masked blit, sheets, palette swap), board-grid math (square + hex). Audio defers to media-av; saves to control-plane 0340; strokes to extensions 0420. |

## Planned ledger

| ID | Title | Track |
| --- | --- | --- |
| 0001 | Roadmap: general capability classes, milestone bands, validation vehicles (canonical) | roadmap |
| 0002 | The 0.3 breaking budget (Role/TokenKind non_exhaustive, content_size deprecation) — Accepted-pending-maintainer; enforced by the CI semver gate | governance |
| 0150 | Terminal verbs (notify/bell/title) reachable from components — clipboard leg SHIPPED with the selection wave | app-widgets |

## Completed ledger

Each file carries a dated completion report with test names and measured
numbers (2026-07-21: the Content + Live-data wave; 2026-07-22: the
composer wave).

| ID | Title | Final path |
| --- | --- | --- |
| 0010 | Async data-source → Signal binding (`channel_source`/`latest_source`) | completed/live-data/ |
| 0020 | Bounded coalescing ingestion (`bounded_source`, stats, fold-panic firewall) | completed/live-data/ |
| 0030 | Live-feed example + `docs/live-data.md` | completed/live-data/ |
| 0070 | `reactive::interval` (cancellable, coalescing) | completed/live-data/ |
| 0100 | `widgets::Feed` (keyed, windowed, streaming items) | completed/app-widgets/ |
| 0110 | `md::StreamSession` (open-block-only re-parse, equivalence-pinned) | completed/app-widgets/ |
| 0270 | Text selection + clipboard copy (all three tiers: bypass docs, mouse-capture suspend verb, screen-text selection + OSC 52) — completed 2026-07-22 | completed/first-app/ |
| 0290 | UX footgun fixed: every selection copy ENDS the gesture (release-copy and mid-drag Enter/`c`/Ctrl+C clear the region with the copy) — post-copy keys reach the app immediately — completed 2026-07-22 | completed/first-app/ |
| 0298 | P0 fixed: stale frame band after resize — `apply_resize` pairs prev-poison with `Presenter::invalidate()` so the post-resize frame re-anchors with absolute CUP; every resize×modal-close interleaving pinned vs a fresh-paint oracle — completed 2026-07-22 | completed/first-app/ |
| 0120 | `widgets::TextArea` + `app::anchored` completion dropdown (0500's passive slice + `Overlays::top_z`) | completed/app-widgets/ |
| 0130 | `Scroll::follow_tail` + measured content extent | completed/app-widgets/ |
| 0220 | BUG fixed: autofocus in dyn_view regeneration panicked | completed/first-app/ |
| 0230 | BUG fixed: modal shortcuts dead until focus entered the modal | completed/first-app/ |
| 0240 | Footgun fixed: modal overflow crushed fixed rows (defaults + debug notice) | completed/first-app/ |
| 0250 | Footgun fixed: `List::on_activate` per the 0250 ruling (selection follows movement; activation = Enter/Space/click-on-selected; bookkeeping-before-callbacks on List AND Table) — completed 2026-07-22 | completed/first-app/ |
| 0500 | Anchored-popup substrate COMPLETE (owned + tooltip modes joined the shipped passive slice) + `Select`/`Combobox`/`MultiSelect` in `app::select` — completed 2026-07-22 | completed/app-kits/ |
| 0293 | BUG fixed: kitty enter-flags now FOLLOW the probe (`Terminal::set_kitty_keyboard`, session-options accounting: leave pops, suspend/resume symmetric) + WezTerm claim evidence-gated — completed 2026-07-22 (fix wave cycle 3) | completed/first-app/ |
| 0295 | `app::use_caps`/`current_caps` — the live post-probe capabilities signal (converged with media-av 0685); TextArea gained the universal Ctrl+J newline chord — completed 2026-07-22 (fix wave cycle 3) | completed/first-app/ |
| 0296 | `SelectHandle` programmatic open on all three select faces (command-summoned pickers; last-painted-rect anchor, disposal-safe wiring) — completed 2026-07-22 (fix wave cycle 3) | completed/first-app/ |
| 0685 | Probed-capabilities signal — discharged by first-app 0295 (one accessor, both consumers); images example's channel label truthful — completed 2026-07-22 (fix wave cycle 3) | completed/media-av/ |
| 0102 | Rich feed lines: `FeedItem::rich`/`rich_block`/`rich_lines` over the crate-private `ItemBlock` vocabulary (semver gate forbade the public variant — fold-back budgeted, planned/0002 entry 5); cell-exact `RichTextView` parity pinned — completed 2026-07-23 (content wave) | completed/app-widgets/ |
| 0104 | `FeedState::sync` + `SyncSpec`: keyed diffing bridge from `Signal<Vec<T>>` (tail push O(1), fingerprint update in place, rebuild on push-order violations; pixel parity vs hand-pushed pinned) — completed 2026-07-23 (content wave) | completed/app-widgets/ |
| 0190 | `TimeSeries`/`TimeSeriesState` history ring (cadence slots, NAN gap padding, by-age/by-count retention) + `LineChart`/`Sparkline::time_axis` relative labels; dashboard traffic panel migrated off its hand-rolled ring — completed 2026-07-23 (content wave) | completed/app-widgets/ |
| 0142 | Markdown tables (GFM subset): `render::md::DocBlock`/`parse_doc` + `DocStreamSession`; tables typeset through the Table widget's `solve_columns` — completed 2026-07-23 (reader wave) | completed/app-widgets/ |
| 0144 | Markdown images: in-flow mosaic rows, header-only sizing (`gfx::probe_dimensions`), lazy decode cached across rebuilds; `Image::from_path` widened to PNG+JPEG — completed 2026-07-23 (reader wave) | completed/app-widgets/ |
| 0146 | Heading anchors + TOC: `render::md::outline`/`slugify` (GitHub-compatible ids) + `MarkdownView::outline_rows`/`resolve_anchor` — completed 2026-07-23 (reader wave) | completed/app-widgets/ |
| 0148 | Search-highlight overlay: `MarkdownView::find` + `.highlights` (case-folded, grapheme-snapped, selection-tone patch; row-local text↔cells mapping shared with 0160) — completed 2026-07-23 (reader wave) | completed/app-widgets/ |
| 0700 | Key press/release state (held keys): `app::keys` — `use_key_state`/`key_state` → `KeyState` (`is_down`/`keys_down`/`pressed`/`pressed_chord`/`released`/`focus_cleared`), `KeyFidelity::{Full,Degraded}`, `hold_gesture_label`; driver pre-conversion tap, per-turn edge sealing, fidelity re-published at the 0293 probe upgrade — completed 2026-07-23 (input/AV wave). Scope notes: legacy repeat-timeout approximation DROPPED by ruling; opt-in release routing for widgets deferred in-item | completed/games/ |
| 0610 | Push-to-talk input contract: `app::PushToTalk` (`bind`/`on_start`/`on_stop(StopReason)`/`state()`/`mode()`/`gesture_label()`/`cancel()`); Hold on Full fidelity, labeled Latch on Degraded, FocusLost stops capture in every mode — completed 2026-07-23 (input/AV wave; closes the 0293→0700→0610 chain: fidelity flips Degraded→Full live at the probe upgrade and the gesture label follows) | completed/media-av/ |
| 0620 | Meter + AudioScope widgets: `widgets::Meter` (ballistics: instant attack, frame-clocked decay 20 dB/s default, peak hold ~1.5 s; dB mapping; mono h/v + band bars; ok/warn/error token zones) + `widgets::AudioScope` (braille strip over a `Signal<Vec<f32>>` window); THE IDLE LAW pinned (fixpoint drops the frame task; zero frames + zero allocs on unchanged input) — completed 2026-07-23 (input/AV wave) | completed/media-av/ |
| 0650 | voice mock example: `examples/voice_mock.rs` (PTT on Space + truthful fidelity footer, fake mic → dB meter + 8-band spectrum + scope, fake transcription into Feed) + `live_voice_mock` smoke case — completed 2026-07-23 (input/AV wave). Does NOT consume 0630 (speaking highlight) or 0640 (`--mock-recorder`); both stay Proposed | completed/media-av/ |
| 0180 | Platform claims + CI gates — CLOSED by the scheduled-gates leg (earlier legs 2026-07-22: MSRV 1.87 + semver/msrv/live-pty jobs): `.github/workflows/perf.yml` (weekly + dispatch: perf suites w/ retry-once load policy, fuzz_big, soak, measurements artifact); byte RATCHETS in perf_app_surfaces (baseline × 1.5, assert in every profile); red-budget dry run executed locally; first hosted green pending push — completed 2026-07-23 (wave 3, REVIEWER) | completed/app-widgets/ |
| — | Feed adopts the md doc vocabulary (handoff-named seam, no backlog id — closes 0142's named follow-up): `FeedItem::markdown` → `parse_doc`; streams → `DocStreamSession`; tables/images/tasks/strike in Feed; streamed table = open region (cost-pinned); captures byte-identical on core sources — completed 2026-07-23 (wave 3, INTEGRATOR) | reviews/wave3/integrator-handoff.md |
| 0297 | Disposal-safety law engine-wide (the 0250 ruling stated as law): Button mouse-Up fixed (`pressed` cleared before `on_click`), TextArea post-callback caret publish fixed (callbacks owed, fired LAST) — audit table of every callback site in-item; disposal test pinned per site (Button/Checkbox/Radio/Tabs/TextInput/TextArea/Table-sort/Select-commit); law stated in api.md; consumer may now delete its one-tick retire deferral — completed 2026-07-23 (fix wave 3, FIXNET) | completed/first-app/ |
| 0040 | Connection lifecycle + jittered reconnect: `reactive::connection`/`ConnState`/`Connection`/`ConnectionEvents` (state as signals, retries on the timer heap, generation-stamped reporters, zero cost when Closed — pinned) + `reactive::Backoff` (FULL jitter, base 500ms ×2 cap 30s, seeded tests); engine does NO I/O — dial fn is the 0050 seam; docs/live-data.md state diagram — completed 2026-07-23 (fix wave 3, FIXNET; graduated directly from proposed/ on the fired trigger, per the in-item evidence section) | completed/live-data/ |

## Proposed ledger

| ID | Title | Track | Promotion trigger |
| --- | --- | --- | --- |
| 0050 | Transport story: HTTP/WebSocket/TLS dependency decision (first ADR) | live-data | Decide only after the watcher's evidence (0060); do not settle from the armchair. 0040 shipped meanwhile — the dial-fn seam is where the transport plugs in. |
| 0060 | Milestone: read-only multi-room watcher over the a2a hub (dogfood) | live-data | Maintainer green-light; validates 0010/0020/0040. Explicitly not-now. |
| 0140 | Stateful cross-line lexers (python/js/toml) — diff lexer SHIPPED 2026-07-22 (`text::DiffLexer`, additive); stateful seam + language presets remain | app-widgets | A consumer needing real language tinting; the stateful-seam design note in the item gates python. |
| 0160 | Content selection + copy — screen-level v1 SHIPPED via 0270; remaining scope = logical widget-content mapping (copy markdown source, unwrap soft-wraps) shared with 0148 | app-widgets | A consumer needing source-text copy (screen-text copy ships today). |
| 0165 | Hyperlink/reference hit-testing through the event path | app-widgets | A dogfood app reaching its "activate a reference" phase. |
| 0170 | 1.0-track API stability pass — PARTIALLY EXECUTED: ADRs 0001-0003 landed + `Capabilities`/`GraphicsCaps` now `#[non_exhaustive]`; the full 1.0 audit (prelude criteria, public-api gate, breaking budget enforcement) stays open | app-widgets | The remaining audit rides the 0.3 window (budget doc: planned/0002). |
| 0200 | EPIC: coding-agent console over `abstractcode serve` JSONL | ports | Its widget + live-data dependencies land (Feed/stream/follow-tail + TextArea 0120 DONE — widget deps complete). |
| 0210 | EPIC: a2a chat TUI over the agora hub | ports | Its widget + live-data dependencies land (Feed + TextArea 0120 DONE; lifecycle 0040/0050 remain). |
| 0260 | Disclosure widget: per-item fold/unfold for transcripts (maintainer ask) | first-app | Fold into Feed's item model (0100 shipped — extend), or standalone on a second consumer. |
| 0280 | Feed custom blocks cannot host widgets; protocol images degrade to mosaic in Feed | first-app | Filed 2026-07-22 (0.2.0 adoption wave); design with Feed's item model + the 0144 protocol-images-in-flow question. |
| 0291 | TextArea placeholder never paints on an autofocused composer — opt-in `placeholder_while_focused` (renumbered from 0310: band collision with control-plane, wave-3 CLOSER) | first-app | Filed 2026-07-23 (field evidence: abstractcode-tui composer teaching never painted one pixel); draw-rule delta in `widgets/textarea.rs`. |
| 0292 | Completion triggers fire on any mid-text token — no position policy (renumbered from 0300) | first-app | Filed 2026-07-22; add trigger-position policy to `Completion` (start-of-line/word options). |
| 0294 | Anchored panel places short lists over the chrome below instead of flipping up (renumbered from 0310) | first-app | Filed 2026-07-22; placement scoring in `place_panel`. |
| 0299 | Public full-redraw verb (poison-prev semantics) + optional focus-regain repaint — external clears (Cmd+K, `\033c`) leave the terminal desynced forever; only resize/caps-upgrade/suspend-resume reach the poison+invalidate pair today (renumbered from 0300: band collision with control-plane, wave-3 CLOSER; `Driver::resync_unknown_screen` — the I-2 suspend work — is the crate-private form of the asked verb) | first-app | Filed 2026-07-23 (field evidence: abstractcode-tui header blanked by external clear + toast). |
| 0300 | App lifecycle events (boot/ready/resize/caps/focus/suspend/resume/quit + custom) — the band foundation | control-plane | Scheduling any of 0310/0340/0350, or the first app needing suspend/flush hooks. |
| 0310 | Automation bus: inject input, query semantic tree + screen text, invoke named actions, subscribe to events | control-plane | 0300 + a driving consumer (port harness, embedder, or 0320). |
| 0320 | JSONL control protocol + opt-in serve seam (default-OFF `control-server` feature; socket perms = auth) | control-plane | 0310 + the JSON-promotion precondition (with extensions 0410); closes only with the protocol ADR. |
| 0330 | MCP bridge — out-of-crate client of the frozen 0320 protocol | control-plane | 0320's ADR freezing + a kickoff ruling on home/language. |
| 0340 | Persist registry: declared keys, atomic phase-boundary snapshots, crash marker, restore-on-start | control-plane | 0300, or app-kits 0520 starting (its accepted first consumer). |
| 0350 | Background serve + attach/detach design (VirtualTerm, conservative serve caps, attach = caps upgrade) | control-plane | Maintainer security/ownership review; builds only after 0360's report folds back. |
| 0360 | Milestone: attach proof — one headless app, one client, fixed caps (~2-4 days, report-first) | control-plane | 0350 review + 0320 socket seam. |
| 0400 | Extension architecture: two feature classes (default-ON trim / default-OFF opt-in) + sibling-crate family; ADR skeleton ready | extensions | Maintainer sign-off; ADR lands before/with the first 04xx packaging execution. |
| 0410 | Feature-gate `three`/`jpeg`/`proto` (default-on trim; gltf_json promotion coordinated with 0320) | extensions | 0400's ADR + integrator Cargo.toml sign-off; batch with the 0.2 window (0170). |
| 0420 | Canvas/vector layer in core: dot canvas, bezier/arc, styled blit; chart refactor gated on byte-identical goldens | extensions | First diagram consumer scheduled (0440/0450) — or standalone on the chart-dedup merit. |
| 0430 | `abstracttui-graph`: interactive node-graph editor (cards/ports/edges/pan/drag/tooltips), staged M1-M3, keyboard-first | extensions | 0420 + 0440 landed; a named dataflow-editor consumer; family launch gate (0170) holds. |
| 0440 | `abstracttui-graph`: read-only auto-layout view — layered v1 (DAG-class), designed force v1.5 (KG-class) | extensions | 0420 + a named DAG-view consumer; v1.5 on the first knowledge-graph consumer. |
| 0450 | `abstracttui-mermaid`: spelling-exact flowchart/sequence subset, atomic per-diagram fallback | extensions | 0420 + 0440 landed; the mdpad rebuild reaching its diagram phase. |
| 0460 | mdpad-class reader enablement: parity dashboard + four core-gap seeds (0142-0148) | extensions | Maintainer green-light on the rebuild; seeds promote individually. |
| 0470 | Web/HTML feasibility — verdict: full web NEVER; readable-subset slice gated on four criteria | extensions | All four criteria met — else the verdict stands. |
| 0480 | Core seam: `StyledCanvas::register_link` (producer half of the link channel; OSC 8 works pre-0165) | extensions | Any canvas-link consumer (0430 M3, 0450) or 0165's scheduling; may merge into 0165. |
| 0510 | Form kit: field rows, form state signals, validation, submit gating, masked input — `TextInput::masked` engine delta SHIPPED 2026-07-22 (draw + access_value redaction) | app-kits | 0520 or a second settings form; remaining engine delta: subtree focus step. |
| 0520 | Wizard flow: multi-step container on the form kit; crash-resume via 0340 (its first consumer) | app-kits | 0510 landing. |
| 0530 | Table upgrades: rich cells, badges, row actions, activation event, row identity | app-kits | Admin-console validator scheduling. |
| 0540 | Chips, counts, and tag-input vocabulary | app-kits | First consumer among 0500/0550/smart-note-class apps. |
| 0550 | Navigation kit: NavList (sidebar + unread badges) + FilterTabs | app-kits | Validators or 0210's room list. |
| 0560 | Header bar + persistent banners (existing tokens only; banner-ground = theme-lane follow-up) | app-kits | Admin-console validator. |
| 0570 | Tree view (outline/file-tree; Role variants ride the 0.2 batch) | app-kits | Triage-shell outline or a file-manager consumer. |
| 0580 | Split panes + collapsible panel rail | app-kits | Triage-shell validator. |
| 0590 | Reference validators: admin console, setup wizard, triage shell (in-repo; no item completes unvalidated) | app-kits | Grows a slice with each landing app-kits item. |
| 0630 | Speaking-highlight primitive (Signal<Range> → cells; shares 0148/0160's text↔cells mapping) | media-av | A voice-reader consumer; builds WITH the 0148 substrate. |
| 0640 | External audio-process lifecycle pattern (docs + example; verified no engine code needed) | media-av | Ships with 0650's successor or the first voice app (0650 itself shipped WITHOUT it — validation stays open). |
| 0660 | Images inside Feed/Markdown via protocol placement (rect-follow, clip, eviction) | media-av | A feed with image attachments, or app-widgets 0144. |
| 0665 | Animated image sessions (kitty a=f zero-steady-state-bytes; labeled timer fallback) | media-av | An animated-content consumer; decoder dep needs a ruling. |
| 0670 | Cell-pixel-size refresh on resize (font zoom re-scales sixel/3D) | media-av | First sixel field report or the next driver-images wave. |
| 0675 | Scroll shift × live images: kitty re-place restores the scroll byte win (plain-diff guard shipped 2026-07-22) | media-av | A log app keeping a persistent image. |
| 0680 | Sixel bottom-row honesty: last-row clamp + DECSET 8452 probe | media-av | First sixel validation pass of the images-truth recipe. |
| 0688 | Detection/transport robustness: strict kitty-probe reply parse; >1 MiB single-frame payloads under tmux (iTerm2 multipart; sixel labeled refusal) | media-av | Next caps/probe wave or a tmux+iTerm2 field report. |
| 0710 | Game tick: public per-frame tasks + fixed-timestep helper | games | First real-time game example, or the second in-tree consumer hand-rolling an `after`-recursion clock (effects example is the first). |
| 0720 | Sprite/tile toolkit: masked blit, sprite sheets, cell-art palette swap | games | First game example reaching its render phase, or a second consumer hand-rolling cell-by-cell sprite copies. |
| 0730 | Board-grid math: square + hex coordinates, range, line, aspect-corrected projection | games | First grid-mapped surface in any dogfood app or game example. Placement (core vs sibling) routes through extensions 0400's classification. |

## Next recommended work

(Updated 2026-07-22, cycle-3 synthesis. Evidence base: the six study-2
reports in `reviews/study2/`; full three-horizon plan with efforts in
`reviews/study2/ACTION-PLAN.md`. The former list is discharged: 0120, the
0.2 budget batch, and 0500 are DONE; 0300 moves to the horizon-3 queue
(control-plane) — still the band foundation, no longer ahead of the
consumer-earned items below.)

1. **The 0.2.2 patch (shipping now)** — the image-lifecycle fixes (five
   bug classes, `reviews/study2/media-images-truth.md`, adversarially
   re-reviewed in `quality-on-media.md`) + first-app
   0290/0293/0295/0296/0298. WHY: 0290 has NO app-side workaround (the
   selection layer eats `c`/Enter before dispatch), and 0293 heads the
   key-state chain while fixing Shift+Enter on the majority macOS
   terminals. (Progress 2026-07-22, fix wave cycle 3: 0293/0295/0296 —
   and media-av 0685 with them — are DONE; 0290/0298 remain this
   patch's open items.) (DISCHARGED: 0.2.2 shipped 2026-07-22 with
   0290/0298 in it — all five items in completed/.)
2. **0102 `FeedBlock::Rich` + 0104 `FeedState::sync`** — WHY: the first
   consumer's #1 tension and its twin (~137-line Card system + ~180-line
   sync machinery, `field-consumer-tensions.md` §4.1/§3.6); additive;
   unblocks the log/chat/entity classes (`field-app-classes.md` classes
   3/5). One block-vocabulary pass with 0280/0660 — the enum grows once.
   (DONE 2026-07-23, content wave — both in completed/app-widgets/ with
   0190; the vocabulary pass landed as the crate-private `ItemBlock`
   with the public fold-back budgeted in planned/0002 entry 5.)
3. **0040 promotion (jittered reconnect)** — WHY: the trigger fired with
   two studies' evidence — the consumer hand-rolled SSE +
   reconnect/backoff WITHOUT jitter (`field-consumer-tensions.md` §3.5)
   and entity monitors multiply it (`field-app-classes.md` class 4);
   dated evidence section in-item. (DONE 2026-07-23, fix wave 3 —
   `reactive::connection` + `Backoff` in completed/live-data/; the
   consumer migration off its hand-roll is the named follow-up.)
4. **0297 disposal law engine-wide** — WHY: tension #2
   (`field-consumer-tensions.md` §3.1) — Button's post-callback write
   forces a one-tick retire deferral in every modal-closing consumer;
   acceptance = the consumer deletes it. (DONE 2026-07-23, fix wave 3 —
   Button + a second offender the audit found (TextArea) fixed, law
   stated in api.md, per-site disposal pins; the consumer's deferral
   deletion is the named follow-up.)
5. **0700 key press/release state** — WHY: the games+voice shared
   primitive (`field-games.md` §2, `media-voice-plumbing.md` §2), now
   unblocked by 0293 in the patch; real-time games stay blocked until it
   lands. (DONE 2026-07-23, input/AV wave — `app::keys` in
   completed/games/ with 0610/0620/0650 behind it.)
6. **The 0.3 budget execution when the maintainer signs**
   (`planned/0002`) — WHY: Role/TokenKind `non_exhaustive` +
   `content_size` deprecation batch in one window; the semver CI gate
   enforces additive-only meanwhile.

## Sequencing (load-bearing)

- **live-data is one-directional**: 0010 before 0020/0030; 0010+0020 before
  the watcher (0060) — hand-rolling their gaps inside the watcher would
  un-validate the track. **0060 before closing 0050**: the transport ADR
  waits on the watcher's experience report as its evidence.
- **0100 is the widget trunk**: 0110 feeds its streaming tail, 0130 is how it
  composes with `Scroll` (design together), 0140 tints its blocks. 0170 gates
  the public shapes of 0100/0130.
- **Ports depend on both tracks**: 0200 (console) ← 0100/0110/0120/0130/0140/0150
  + live-data 0010/0020/0030 (subprocess pipe, no network — not 0040/0050).
  0210 (chat) ← 0100/0120/0130/0150 + live-data 0010/0020/0030/0040/0050; its
  read-only phase 1 IS the 0060 milestone (adopt, don't restart).
- The read-only watcher (0060) needs **nothing** from app-widgets (its scope
  is a hand-windowed read-only view); a full chat client is the first thing
  requiring both tracks.

### Cross-track edges from the 2026-07-21 study (load-bearing)

- **0300 before everything in its band** — 0310/0320/0340/0350 all consume
  the lifecycle surface.
- **0320 ↔ 0410**: whichever ships first must promote `gltf_json` to a
  neutral home (with a `three`-feature re-export) or the second is stranded.
- **0340 ↔ 0520**: the wizard is the persist registry's accepted first
  consumer; 0520's crash-resume journey is 0340's restore-ordering evidence.
- **0360 → 0350/0320-ADR**: the attach proof's experience report folds back
  before the attach design or the protocol ADR freezes (the 0060→0050
  evidence-first pattern).
- **0500's popup substrate before its consumers**: 0120's completion
  dropdown (passive-panel mode), 0530's action menus, extensions 0430's
  tooltips all consume it; the `Overlays::top_z` engine delta rides the
  0.2 window.
- **0420 before 0430/0440/0450**; **0440 before 0430**; the link seam
  (0480, mergeable into 0165) before 0430's activation milestone.
- **The 0250 ruling** (selection follows movement; activation = Enter /
  click-when-selected; commit-on-move per-widget opt-in, default off) is
  recorded in `reviews/study/platform-on-appkits.md` and encoded by
  0530/0550/0570; the List/Table engine fixes cite it.
- **Sibling extension crates inherit the dependency posture** (std +
  abstracttui + hand-rolled parsing); the TLS-class exception is not
  granted here — it rides live-data 0050's transport ADR.
- **The key-state chain (convergence cycle 2)**: first-app 0293 (push
  kitty flags after the probe proves the protocol) → games 0700 (the
  key press/release state service + fidelity honesty) → media-av 0610
  (push-to-talk consumes it). 0700's service lands independently but
  runs repeat-approximated on iTerm2/VS Code/Warp until 0293; 0610
  adds no key-state machinery of its own. **Chain head SHIPPED
  2026-07-22 (fix wave cycle 3)**: 0293 is completed —
  `REPORT_EVENT_TYPES` now reaches probe-proven terminals, so 0700
  starts unblocked at full fidelity.
- **The Feed-block family (convergence cycle 2)**: app-widgets 0102
  (`FeedBlock::Rich`), media-av 0660 (images in Feed), and first-app
  0280 (widget-hosting blocks) all extend the same `FeedBlock` enum —
  one block-vocabulary design pass, owned by whichever executes first,
  reviewed by the other two; the enum grows once.
- **0730's home is a 0400 classification** (core module vs a
  games-domain sibling crate) — the item argues both precedents
  (0420-core vs 0440-sibling-layout) and promotes only with a recorded
  ruling.
- **Same-z Modal stacking hazard (0500 follow-up 2, verified in code
  2026-07-22)**: two `Modal::open` calls both mount at `MODAL_Z =
  1000`; paint order is stable ascending-z (ties keep mount order —
  the SECOND-mounted renders on top) while key dispatch sorts
  `Reverse(z)` stably (ties keep mount order — the FIRST-mounted wins
  keys): visually-top and key-owner DISAGREE for stacked modals. The
  0500 owned popup is immune (`Overlays::top_z() + 1` is strictly
  above). Whoever ships stacked-dialog UX (0510/0520 forms, 0530 row
  actions, the 0590 validators) must give `Modal` a z-or-`top_z` story
  first — details in `completed/app-kits/0500_select_combobox_family.md`
  "Follow-ups revealed".

## ADR state

`docs/adr/` exists: **0001** (API stability policy toward 0.2/1.0),
**0002** (two-`Style` ruling), **0003** (struct extensibility) landed
2026-07-21. Still owed: the **extension-architecture ADR** (skeleton ready
in `reviews/study/extensions-cycle3.md` §1c — lands before/with the first
04xx packaging execution), the **0320 control-protocol ADR**, the **0340
persistence-container ADR**, and the **0050 transport ADR** (waits on
0060's evidence). The a11y-completeness + redaction-at-source clause
(drafted in `reviews/study/platform-cycle3.md`) joins the next ADR pass.

## Process

- New item: scan every lifecycle dir + topic folder for the next unused global
  `NNNN`, add it under the right state, and update this overview's counts,
  ledgers, and sequencing in the same pass.
- Completion: append a `## Completion report` (final path, date, outcome, key
  validation), move to `completed/`, update the ledgers here.
- Deprecation: append a `## Deprecation report` with the reason, move to
  `deprecated/`, update this overview.
- Bands: live-data owns 0010–0090, app-widgets owns 0100–0190, ports own
  0200–0290 (0200/0210 = port epics; 0220–0298 = first-app findings),
  control-plane owns 0300–0390, extensions owns 0400–0490, app-kits owns
  0500–0590, media-av owns 0600–0690, games owns 0700–0790. Leave gaps
  for insertion.
