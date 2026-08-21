# AbstractTUI backlog — overview

Planning memory for AbstractTUI (the Rust terminal-UI engine, published as
`abstracttui` — 0.4.0 as of 2026-08-21). The engine itself is complete and
shipped — content widgets (Feed/TextArea/MarkdownView + the doc vocabulary),
app-shell chrome (PageHost, Drawer, ChoicePrompt, ThemeSwitcher), the
live-data lane with connection lifecycle, key-state/PTT, attachments, the
canvas layer, and the sibling extension family (`abstracttui-graph`,
`abstracttui-mermaid`) are all in `completed/`. Four validator applications
have now been built on it (abstractcode-tui, agora-tui, the gateway
console, and the abstractcore console launched 2026-07-25), and the
pending work has shifted shape accordingly: it is dominated by FIELD
FINDINGS (four `field-*`/`first-app` tracks of reproduced engine defects
with app-side workarounds to delete), the wave-12 pixel-review engine
items (filed 2026-07-25, this pass), the remaining roadmap tracks
(control-plane, the app-kits remainder, the ports epics, the 0050
transport ADR), and the maintenance ledger (wave11's file-size splits).
Still true from the original observation: the engine ships no
HTTP/WebSocket transport — that decision (0050) deliberately waits on the
watcher evidence. The original evidence base lives in `reviews/cycle11/`;
the current evidence base is the field tracks and the per-wave handoffs
under `reviews/wave*/`.

## Design principles

General-needs-first (every capability justified by an app class, never one
app), apps-as-validators, standalone dependency posture, honest degradation,
zero idle cost — codified with the milestone bands and validation vehicles in
`planned/0001_roadmap.md`, the canonical roadmap.

## Counts

| State | Count |
| --- | --- |
| Planned | 5 |
| Proposed | 84 |
| Completed | 74 |
| Deprecated | 0 |
| Recurrent | 0 |

(2026-07-26: +1 completed — app-kits/1250 reasoning controls, filed
directly in completed/ per the same-wave precedent; the app-kits band
continues at 1250, see the bands note below.)

(Count reconciliation 2026-08-20, during the layout-honesty wave: the
table had drifted from the directory across several waves and now
matches a file count again. The directory is the truth.)

Counting rule: `NNNN_*.md` files on disk under each lifecycle directory
(topic subfolders included). The completed ledger below additionally
lists one unnumbered integrator row (the Feed/md-vocabulary adoption,
recorded in a handoff, no item file) — it is not in the count.

(Counted from the filesystem 2026-07-25, wave-13 hygiene pass — the
count reconciles with `find <state> -name '[0-9]*.md' | wc -l`. This
pass: +8 wave-12 pixel-review filings (0135/0175/0185/0380/0445/0455/
0555/0615), +1 renumber (proposed first-app 0290 → 0274, collision with
completed 0290), and 6 completed items that were on disk but missing
from the ledger below (0273, 0370, 0420, 0440, 0450, 0605) added to it.
Prior notes preserved: the 2026-07-24 double-click fold filed app-kits
0535 directly in completed/; the 2026-07-23 0.2.6 field-wave fold moved
first-app 0291 + 0299 to completed/ and recorded the 0299/0291 renumber
precedent.)

## Known number collisions (flagged, not renumbered)

Global `NNNN` uniqueness is broken in five places today. Renumbering is
the item owners' call (the wave11 precedent: record visibly, don't
silently rename ids that external app repos may cite); one same-track
collision WAS renumbered this pass (proposed first-app 0290 → 0274 —
zero inbound references existed).

- `field-agora/0900` vs `field-gateway/0900` (different tracks — the
  field-agora band overflowed into field-gateway's 0900–0990 range
  before the overflow rule existed)
- `field-agora/0905` vs `field-gateway/0905` (same cause)
- `field-agora/0910` vs `field-gateway/0910` (same cause)
- `wave11/0990` vs `field-gateway/0990` (recorded in wave11/README.md
  at filing)

Rule going forward (stated in `proposed/field-core/README.md`): a full
band continues at the NEXT FREE FIFTY and says so in its README —
field-gateway continues at 1000–1050, field-core owns 1100–1190,
1200+ is taken by the wave-13 architecture/md-lane filings.

## Topic tracks

| Track | Dir | State | Purpose |
| --- | --- | --- | --- |
| live-data | `planned/live-data/`, `proposed/live-data/` | Mixed | Network-driven reactivity: async-source→signal binding, bounded ingestion, reconnect, the transport decision, and the read-only watcher milestone. |
| app-widgets | `planned/app-widgets/`, `proposed/app-widgets/` | Mixed | The content-widget layer real apps need (feed/transcript, streaming markdown, multiline composer, follow-tail scroll, lexers) + the API-stability and platform-accuracy passes — now also home to the wave-12 measure/crush findings (0135/0175/0185). |
| ports | `planned/ports/`, `proposed/ports/` | Mixed | The application epics that consume both tracks: a coding-agent console, an a2a chat TUI, and the gateway configuration wizard (0215, planned — validator app #2). |
| first-app | `proposed/first-app/`, `completed/first-app/` | Mixed | Bug/footgun reports from the first shipped application (`abstractcode-tui`, 2026-07-21): reproduced engine defects with field workarounds to delete. 30 completed, 4 open. Band 0220–0299 full; continues at 1300–1340. |
| control-plane | `proposed/control-plane/`, `completed/control-plane/` | Mixed | Making running apps observable and drivable from outside their own keyboard: lifecycle events, an automation bus + opt-in JSONL control server (MCP-bridgeable), declared-keys persistence with crash-resume, headless serve with attach/detach — plus the shipped observe primitives (0370 screenshots; 0380 files the damage-visualizer knob). |
| extensions | `proposed/extensions/`, `completed/extensions/` | Mixed | Modularity architecture (ADR-0004, executed: the `abstracttui-*` sibling family is live) and the diagram-class capability lane: canvas layer (0420) + graph view (0440) + mermaid subset (0450) SHIPPED; editor (0430), reader enablement (0460), link seam (0480) and the wave-12 polish items (0445/0455) remain. |
| app-kits | `proposed/app-kits/`, `completed/app-kits/` | Mixed | The application-kit layer over the content widgets: anchored-popup substrate + choice controls (0500/0515 shipped), PageHost (0545), Drawer (0585), double-click (0535), theme switcher (0595), panel ✕ (0605) — form kit, wizard, tables, chips, nav, tree, split panes remain, with the field-gateway track as their live evidence. |
| media-av | `proposed/media-av/`, `completed/media-av/` | Mixed | Voice/AV UI plumbing (PTT contract, meters/scope, speaking highlight, external-process pattern) + image-path follow-ups from the study-2 truth audit. |
| games | `proposed/games/`, `completed/games/` | Mixed | Retro-games feasibility band: key press/release state SHIPPED (0700); tick/sprites/board-grid remain. |
| field-gateway | `proposed/field-gateway/` | Proposed | Bug/footgun reports from the second-wave validator build (`abstractgateway/console-tui`, the 0215 gateway config wizard): 15 open items — the form/wizard/table field evidence for app-kits 0510/0520/0530. Band 0900–0990, overflow 1000–1050. |
| field-agora | `proposed/field-agora/` | Mixed | Bug/footgun reports from the second-wave validator build (`agora-tui`, the 0060 read-only multi-channel hub watcher): 14 open, 1 completed (0850) — the first networked field evidence for live-data 0010/0020/0040 and the 0050 transport ADR. Band 0800–0890, overflow 0895–0910. |
| field-core | `proposed/field-core/` | Proposed | Feedback band for the third-wave validator (`abstractcore-console`, launched 2026-07-25 on 0.2.22). Band 1100–1190. Empty at this count — findings expected as that build proceeds. |
| wave11 | `proposed/wave11/` | Proposed | Maintenance items from the wave-11 adversarial quality audit: one item (0990 file-size budget reconciliation — 12 splits done in wave 12, 15 files still >600 lines re-counted 2026-07-25). |

## Planned ledger

| ID | Title | Track |
| --- | --- | --- |
| 0001 | Roadmap: general capability classes, milestone bands, validation vehicles (canonical) | roadmap |
| 0002 | The 0.3 breaking budget (Role/TokenKind non_exhaustive, content_size deprecation) — Accepted-pending-maintainer; enforced by the CI semver gate | governance |
| 0060 | Milestone: read-only a2a/agora multi-channel watcher — MAINTAINER GREEN-LIT 2026-07-23 (validator app #1); `agora-tui` is that build (field-agora is its findings track); validates 0010/0020/0040 live, feeds 0050's transport ADR | live-data |
| 0215 | EPIC: gateway configuration wizard — MAINTAINER-CHOSEN validator app #2 ("gateway/console but improved"); `abstractgateway/console-tui` is that build (field-gateway is its findings track); promotion trigger for app-kits 0510/0520 | ports |
| 0150 | Terminal verbs (notify/bell/title) reachable from components — clipboard leg SHIPPED with the selection wave; the notify leg now has its first named consumer (first-app 0274 — execute together) | app-widgets |

## Completed ledger

Each file carries a dated completion report with test names and measured
numbers (2026-07-21: the Content + Live-data wave; 2026-07-22: the
composer wave; 2026-07-23/24: the content/reader/extensions/app-shell
waves; 2026-07-25: the attachments + theme-switcher + panel-✕ waves).

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
| 0452 | Mermaid: the subset people actually write — edge chaining, infix labels, `&` cross products, the arrow vocabulary, ten node shapes, `subgraph` flattened with a named notice, and sequence control flow (`alt`/`else`/`opt`/`loop`/`par` frames + activation bars, depth-capped); plus ```mermaid fences rendered INSIDE markdown documents through the new core `widgets::FenceBlock` seam — completed 2026-08-21 | completed/extensions/ |
| 0480 | Core seam `StyledCanvas::register_link` — a draw closure can mint the hyperlink id `Style::link` takes; defaults to `0`, so every existing canvas compiles unchanged — completed 2026-08-21 | completed/extensions/ |
| 0455 | mermaid fallback live-link: an OSC-8 hyperlink where the terminal has one, a wrapped (never mid-URL ellipsis) URL where it does not — completed 2026-08-21 | completed/extensions/ |
| 0490 | PNG screenshots — `Screenshot::to_png`/`to_bitmap`/`write_png`: engine-drawn pixels from an embedded 8x16 bitmap font, with box/block/braille/sextant glyphs drawn geometrically so strokes tile exactly; the faithful capture beside the embeddable SVG — completed 2026-08-21 | completed/control-plane/ |
| 0667 | Animation — in-tree animated GIF + APNG decoding (`gfx::decode_animation`) and the `AnimatedImage` playback lane (one timer per frame, nothing between); video containers are recognized and refused by name with a conversion command, never decoded — completed 2026-08-21 | completed/media-av/ |
| 0151 | Images draw at the terminal's own mosaic density — `widgets::Image` and markdown image blocks follow `MosaicMode::auto` (quadrants on a UTF-8 color terminal) instead of pinning half blocks; measured +3 dB PSNR on a photo pane — completed 2026-08-20 | completed/app-widgets/ |
| 0149 | Progressive JPEG decode (`gfx::jpeg` reads SOF2 beside SOF0/SOF1, plus single-component scans, so multi-scan sequential files decode too); image previews of editor-default JPEGs show the picture instead of a labeled refusal — completed 2026-08-20 | completed/app-widgets/ |
| 1255 | DrawerDock — right-edge rail of always-visible vertical tabs + docked side panel (one open, full collapse, reactive badges); the persistent sibling of app::drawer, layout-docked so content reflows — completed 2026-08-19 | completed/app-kits/ |
| 1320 | A scrollbar you can see (thumb floor, every engine scrollbar) and a copy that confirms itself (`copied N characters to the clipboard`, warning reserved for no working route) — completed 2026-08-19 | completed/first-app/ |
| 1310 | Codex-compatible navigation chords in `TextInput`/`TextArea` — Codex's `EditorKeymap` defaults adopted verbatim (word motion in all three spellings, line start/end, delete-word), pinned by a wire-level parity test — completed 2026-08-16 | completed/first-app/ |
| 1300 | Copy truth fixed: a live selection FREEZES follow-tail (`widgets::scroll::freeze_follow_tail`, published by `app::selection`) — a streaming transcript no longer slides under the highlight, so the clipboard gets the rows the user pointed at — completed 2026-08-16 | completed/first-app/ |
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
| 0700 | Key press/release state (held keys): `app::keys` — `use_key_state`/`key_state` → `KeyState`, `KeyFidelity::{Full,Degraded}`, `hold_gesture_label`; driver pre-conversion tap, per-turn edge sealing, fidelity re-published at the 0293 probe upgrade — completed 2026-07-23 (input/AV wave) | completed/games/ |
| 0610 | Push-to-talk input contract: `app::PushToTalk` (Hold on Full fidelity, labeled Latch on Degraded, FocusLost stops capture in every mode) — completed 2026-07-23 (input/AV wave) | completed/media-av/ |
| 0620 | Meter + AudioScope widgets (ballistics, dB mapping, token zones; THE IDLE LAW pinned) — completed 2026-07-23 (input/AV wave) | completed/media-av/ |
| 0650 | voice mock example (`examples/voice_mock.rs` + `live_voice_mock` smoke) — completed 2026-07-23 (input/AV wave) | completed/media-av/ |
| 0180 | Platform claims + CI gates — CLOSED by the scheduled-gates leg (MSRV 1.87 + semver/msrv/live-pty jobs; `perf.yml` weekly deep gate; byte RATCHETS in perf_app_surfaces) — completed 2026-07-23 (wave 3, REVIEWER) | completed/app-widgets/ |
| — | Feed adopts the md doc vocabulary (handoff-named seam, no backlog id — closes 0142's named follow-up): `FeedItem::markdown` → `parse_doc`; streams → `DocStreamSession`; tables/images/tasks/strike in Feed — completed 2026-07-23 (wave 3, INTEGRATOR) | reviews/wave3/integrator-handoff.md |
| 0297 | Disposal-safety law engine-wide (audit table of every callback site; per-site disposal pins; law stated in api.md) — completed 2026-07-23 (fix wave 3, FIXNET) | completed/first-app/ |
| 0040 | Connection lifecycle + jittered reconnect: `reactive::connection`/`Backoff` (FULL jitter; engine does NO I/O — dial fn is the 0050 seam) — completed 2026-07-23 (fix wave 3, FIXNET) | completed/live-data/ |
| 0291 | Placeholder-while-focused opt-in: `TextArea::placeholder_while_focused` + `TextInput` parity (default OFF) — completed 2026-07-23 (0.2.6 field wave) | completed/first-app/ |
| 0299 | Public full-redraw verb: `app::request_full_redraw()` + opt-in `set_redraw_on_focus_gained` (RunConfig field rejected — literal-constructible struct, semver-major) — completed 2026-07-23 (0.2.6 field wave) | completed/first-app/ |
| 0281 | Scroll offset repair on content shrink (clamp on extent/viewport change; culled-probe exemption) — completed 2026-07-23 | completed/first-app/ |
| 0282 | `FeedState::sync_with` borrow-based source (fold-shaped stores; shared drain core) — completed 2026-07-23 | completed/first-app/ |
| 0283 | Capped preview blocks: `FeedItem::max_rows` + overflow marker (post-wrap, width-aware) — completed 2026-07-23 | completed/first-app/ |
| 0284 | Placeholder clipping fix (both widgets, both branches; right stroke untouchable) — completed 2026-07-23 | completed/first-app/ |
| 0292 | Completion trigger position policy (`trigger_at` + TriggerPosition) — completed 2026-07-23 | completed/first-app/ |
| 0294 | Anchored-panel placement bias (`PanelPlacement`, AbovePreferred for bottom composers) — completed 2026-07-23 | completed/first-app/ |
| 0515 | `ChoicePrompt`/`ChoiceSequence` — the modal decision gate (charter-verified SHIP) — completed 2026-07-23 | completed/app-kits/ |
| 0545 | `PageHost` — the page-level tab host (full pages behind a themed windowed tab bar; capture-reserved chords, opt-in digit jumps, reactive badges) — completed 2026-07-24 | completed/app-kits/ |
| 0285 | Selection click-through (layer claims only once the gesture DRAGS) + pointer-capture heal — completed 2026-07-23 | completed/first-app/ |
| 0286 | KeyChord shifted-letter dual-spelling folded at every chord-match site (`KeyChord::normalized`) — completed 2026-07-23 | completed/first-app/ |
| 0287 | ChoicePrompt `.body(view)` slot — structured/scrollable/reactive display region; options-first height budget — completed 2026-07-23 | completed/first-app/ |
| 0288 | ChoicePrompt `option_key` uppercase dead on kitty — letter matcher folds both wire spellings — completed 2026-07-23 | completed/first-app/ |
| 0271 | ChoicePrompt approval-gate adoption gaps: `body_width`, `dismiss_label`, `handle.retire()` — completed 2026-07-23 | completed/first-app/ |
| 0260 | Disclosure card widget — fold/unfold title row, capped body with scrollbar (`widgets::Disclosure`) — completed 2026-07-24 | completed/first-app/ |
| 0850 | Feed message-card enablers — `Feed::on_item_press` + `item_at_row`, `Scroll::extent_signal`/`scrollbar_auto_hide`, the documented card recipe — completed 2026-07-24 | completed/field-agora/ |
| 0585 | Global drawer system — `app::Drawer` edge-anchored overlay panels hosting full pages + the `animate` mid-flight disposal guard — completed 2026-07-24 | completed/app-kits/ |
| 0535 | Double-click: engine click-chain synthesis (`EventCtx::click_count()`) + `Table::on_activate` — completed 2026-07-24 | completed/app-kits/ |
| 0370 | Screenshot capture + exporters: `render::Screenshot` (deterministic `to_text`, replayable `to_ansi`, GitHub-renderable `to_svg`); three capture surfaces (driver, `app::request_screenshot`, testing rig); labeled protocol-image veils — completed 2026-07-24 (ledger row added 2026-07-25) | completed/control-plane/ |
| 0420 | Canvas/vector layer in core: `crate::canvas` (braille/quadrant dot grids, line/bezier/arc strokes, eighth-block fills); chart refactor goldens byte-identical — completed 2026-07-24, extensions wave (ledger row added 2026-07-25) | completed/extensions/ |
| 0440 | `abstracttui-graph`: auto-layout (`GraphDesc -> Layout`, layered/force/grid) + `GraphView` (cards/strokes/selection/pan/tooltips); first workspace sibling crate — completed 2026-07-24, extensions wave (ledger row added 2026-07-25) | completed/extensions/ |
| 0450 | `abstracttui-mermaid`: spelling-exact subset parser, flowcharts/flat-state compiled onto the graph crate, solverless sequence diagrams, atomic fallback + mermaid.live fragment link; 30-fixture corpus — completed 2026-07-24, extensions wave (ledger row added 2026-07-25) | completed/extensions/ |
| 0595 | Theme modes + `ThemeSwitcher` — `ThemeMode` + `toggle_mode()` (remembered choice per mode) + the drop-in ☾/☼ control; abstractuic survey re-run: strict superset, zero drift — completed 2026-07-25 | completed/app-kits/ |
| 0605 | Block close affordance: `Block::on_close` panel ✕ (mouse-only, never focusable; title-yields-first truncation ladder; disposal-safe) — completed 2026-07-25 (ledger row added 2026-07-25) | completed/app-kits/ |
| 0273 | File-attachment surfaces: `TextInput`/`TextArea` `on_paste` intercept (`PasteAction`), `input::paste::classify` cross-terminal drop classifier, `FilePicker` over the `FileSource` seam + `examples/attachments.rs` — completed 2026-07-25, attachments wave (ledger row added 2026-07-25) | completed/first-app/ |
| 1250 | Reasoning controls (operator-ordered cross-seat plan, tui section): `ReasoningSelect` three-state effort picker (capable/locked/unknown+override, facts AS DATA, screen-anchored popup) + `reasoning_label`/`reasoning_label_glyph` parity grammar (⊘ U+2298 research recorded; plain form canonical) + `ThinkingFold` streaming reasoning fold (folded default, last-wins complete, data-driven indicator, zero idle) + `Disclosure::title_muted`/`detail_signal` + `examples/reasoning.rs` — completed 2026-07-26 | completed/app-kits/ |

## Proposed ledger — general bands

| ID | Title | Track | Promotion trigger |
| --- | --- | --- | --- |
| 0050 | Transport story: HTTP/WebSocket/TLS dependency decision (ADR) | live-data | Decide only after the watcher's evidence (0060 — agora-tui now EXISTS and field-agora holds its findings; the ADR should fold them). 0040 shipped meanwhile — the dial-fn seam is where the transport plugs in. |
| 0135 | Scroll over a measureless PLAIN element tree collapses to a bar-only strip — the REMAINDER after ADR-0005 fixed the content views (wave-12 §2) | app-widgets | The fix-or-document ruling (code seat accepted the lane); or the 0185 solver investigation touching the same seam. |
| 0140 | Stateful cross-line lexers (python/js/toml) — diff lexer SHIPPED 2026-07-22; JSON/YAML lexers SHIPPED with the ADR-0005 wave; stateful seam + language presets remain | app-widgets | A consumer needing real language tinting; the stateful-seam design note in the item gates python. |
| 0160 | Content selection + copy — screen-level v1 SHIPPED via 0270; remaining scope = logical widget-content mapping (copy markdown source, unwrap soft-wraps) shared with 0148 | app-widgets | A consumer needing source-text copy (screen-text copy ships today). |
| 0165 | Hyperlink/reference hit-testing through the event path | app-widgets | A dogfood app reaching its "activate a reference" phase. |
| 0170 | 1.0-track API stability pass — PARTIALLY EXECUTED: ADRs 0001-0003 + `#[non_exhaustive]` on Capabilities/GraphicsCaps; the full 1.0 audit stays open | app-widgets | The remaining audit rides the 0.3 window (budget doc: planned/0002). |
| 0175 | `Block::shadow` consumes a row/col of the block's own slot; under interior crush the FIRST child dies, order-dependently (wave-12 §1b) | app-widgets | The chrome-yields-vs-document ruling in-item; the order-dependence half rides the 0185 investigation. |
| 0185 | Measure inflation around Tabs: fixed-height siblings crushed while grow slack exists (wave-12 §1) | app-widgets | ALREADY ACCEPTED by the code seat as "the top engine investigation for the next wave" — needs the failing-test pin first. |
| 0200 | EPIC: coding-agent console over `abstractcode serve` JSONL | ports | Its widget + live-data dependencies land (widget deps complete). |
| 0210 | EPIC: a2a chat TUI over the agora hub | ports | Its widget + live-data dependencies land (Feed + TextArea DONE; lifecycle 0040 done, 0050 remains). |
| 0272 | ChoicePrompt aux-key vocabulary — non-option key surface, hint row open to callers (split out of 0271) | first-app | The consumer's `f` cards↔JSON toggle ask recurring, or the next ChoicePrompt wave. |
| 0274 | `app::notify()` — presenter-custody emitter for the detected OSC 9/99 notification channels (RENUMBERED from 0290 this pass) | first-app | Execute WITH planned 0150 (the notify leg, same emission path); first consumer named (abstractcode-tui run-conclusion ping). |
| 0280 | Feed custom blocks cannot host widgets; protocol images degrade to mosaic in Feed | first-app | Design with Feed's item model + the 0144 protocol-images-in-flow question. |
| 0289 | Typed uppercase inserts lowercase on kitty-spelling wires (`convert_event` drops the kitty `text` field) | first-app | Next input wave (bug — should not wait long). |
| 1331 | The solver has no min-content sizing mode — the prerequisite under CSS's automatic minimum size AND basis-aware intrinsic sizing (both attempted and measured during 1330) | first-app | A layout-semantics cycle; it is a solver capability, not a bug fix. |
| 0300 | App lifecycle events (boot/ready/resize/caps/focus/suspend/resume/quit + custom) — the band foundation | control-plane | Scheduling any of 0310/0340/0350, or the first app needing suspend/flush hooks. |
| 0310 | Automation bus: inject input, query semantic tree + screen text, invoke named actions, subscribe to events | control-plane | 0300 + a driving consumer (port harness, embedder, or 0320). |
| 0320 | JSONL control protocol + opt-in serve seam (default-OFF `control-server` feature; socket perms = auth) | control-plane | 0310 + the JSON-promotion precondition (with extensions 0410); closes only with the protocol ADR. |
| 0330 | MCP bridge — out-of-crate client of the frozen 0320 protocol | control-plane | 0320's ADR freezing + a kickoff ruling on home/language. |
| 0340 | Persist registry: declared keys, atomic phase-boundary snapshots, crash marker, restore-on-start | control-plane | 0300, or app-kits 0520 starting (its accepted first consumer). |
| 0350 | Background serve + attach/detach design (VirtualTerm, conservative serve caps, attach = caps upgrade) | control-plane | Maintainer security/ownership review; builds only after 0360's report folds back. |
| 0360 | Milestone: attach proof — one headless app, one client, fixed caps (~2-4 days, report-first) | control-plane | 0350 review + 0320 socket seam. |
| 0380 | Debug-damage toggle under `App::run` — app-level knob to `Compositor::set_debug_damage` (wave-12 §5) | control-plane | Ride the driver.rs split (wave11/0990's last entry — the code seat's named landing moment); verb/env shape decision in-item (RunConfig field is semver-major, the 0299 lesson). |
| 0400 | Extension architecture — EXECUTED: ADR-0004 Accepted 2026-07-23; item retained for the decision record | extensions | Done in substance; close formally on the next extensions wave. |
| 0410 | Feature-gate `three`/`jpeg`/`proto` (default-on trim; gltf_json promotion coordinated with 0320) | extensions | ADR-0004 landed; integrator Cargo.toml sign-off; batch with the 0.2/0.3 window (0170). |
| 0430 | `abstracttui-graph`: interactive node-graph editor (cards/ports/edges/pan/drag/tooltips), staged M1-M3, keyboard-first | extensions | 0420 + 0440 landed (both DONE); a named dataflow-editor consumer remains the gate. |
| 0445 | GraphView: force layouts open mostly off-view — first-render bbox centering / `.center()` (wave-12 §4) | extensions | Small; agreed by the code seat ("next wave"); bound offsets keep app ownership. |
| 0460 | mdpad-class reader enablement: parity dashboard + four core-gap seeds (0142-0148 all SHIPPED; ADR-0005 folded the mdpad survey — table wrap + nesting now live in the 1200-band filings) | extensions | Reassess against ADR-0005's decision 4: the remaining scope may be discharged or superseded by the 1200-band items. |
| 0470 | Web/HTML feasibility — verdict: full web NEVER; readable-subset slice gated on four criteria | extensions | All four criteria met — else the verdict stands. |
| 0510 | Form kit: field rows, form state signals, validation, submit gating — `TextInput::masked` SHIPPED | app-kits | 0520 or a second settings form; field-gateway 0930/0935/0990 are its live evidence. |
| 0520 | Wizard flow: multi-step container on the form kit; crash-resume via 0340 (its first consumer) | app-kits | 0510 landing; field-gateway 0920/1010 are its live evidence. |
| 0530 | Table upgrades: rich cells, badges, row actions, activation event, row identity | app-kits | Admin-console validator scheduling; field-gateway 0900/0970/0980 are its live evidence; builds ON 0535's activation. |
| 0540 | Chips, counts, and tag-input vocabulary | app-kits | First consumer among 0500/0550/smart-note-class apps. |
| 0550 | Navigation kit: NavList (sidebar + unread badges) + FilterTabs | app-kits | Validators or 0210's room list. |
| 0555 | PageHost default layout: hug → grow-into-region (wave-12 §6; the Viewport3D default precedent) | app-kits | BEHAVIOR CHANGE — its own wave slot + changelog line (code seat); acceptance = the shell example deletes its explicit `.layout(...)`. |
| 0560 | Header bar + persistent banners (existing tokens only) | app-kits | Admin-console validator. |
| 0570 | Tree view (outline/file-tree; Role variants ride the 0.3 batch) | app-kits | Triage-shell outline or a file-manager consumer. |
| 0580 | Split panes + collapsible panel rail | app-kits | Triage-shell validator. |
| 0590 | Reference validators: admin console, setup wizard, triage shell (in-repo) | app-kits | Grows a slice with each landing app-kits item. |
| 0615 | `gesture_label` composition contract — document the `"{action}: {label}"` template (wave-12 §6) | media-av | Docs-only smallest shape; parts-based labels only on a named localizing consumer. |
| 0630 | Speaking-highlight primitive (Signal<Range> → cells; shares 0148/0160's text↔cells mapping) | media-av | A voice-reader consumer; builds WITH the 0148 substrate. |
| 0640 | External audio-process lifecycle pattern (docs + example; verified no engine code needed) | media-av | Ships with 0650's successor or the first voice app. |
| 0660 | Images inside Feed/Markdown via protocol placement (rect-follow, clip, eviction) | media-av | A feed with image attachments (0144's in-flow mosaic shipped; protocol placement remains). |
| 0665 | Animated image sessions (kitty a=f zero-steady-state-bytes; labeled timer fallback) | media-av | An animated-content consumer; decoder dep needs a ruling. |
| 0670 | Cell-pixel-size refresh on resize (font zoom re-scales sixel/3D) | media-av | First sixel field report or the next driver-images wave. |
| 0675 | Scroll shift × live images: kitty re-place restores the scroll byte win | media-av | A log app keeping a persistent image. |
| 0680 | Sixel bottom-row honesty: last-row clamp + DECSET 8452 probe | media-av | First sixel validation pass of the images-truth recipe. |
| 0688 | Detection/transport robustness: strict kitty-probe reply parse; >1 MiB single-frame payloads under tmux | media-av | Next caps/probe wave or a tmux+iTerm2 field report. |
| 0710 | Game tick: public per-frame tasks + fixed-timestep helper | games | First real-time game example, or the second in-tree consumer hand-rolling an `after`-recursion clock. |
| 0720 | Sprite/tile toolkit: masked blit, sprite sheets, cell-art palette swap | games | First game example reaching its render phase. |
| 0730 | Board-grid math: square + hex coordinates, range, line, aspect-corrected projection | games | First grid-mapped surface in any dogfood app; placement routes through ADR-0004's classification. |

## Proposed ledger — field findings + maintenance

One row per open finding (each file carries the evidence, the app-side
workaround, and what the engine fix deletes). Severities: P1 blocked
the build / P2 cost real time, workaround holds / P3 paper cut.

| Track | ID | Title | Class | Sev |
| --- | --- | --- | --- | --- |
| field-agora | 0800 | use_startup_notices carries unbounded mid-session diagnostics | API gap | P3 |
| field-agora | 0810 | List rows are plain strings — no badge slot | capability gap | P3 |
| field-agora | 0820 | Connection has no app-initiated re-dial verb | API gap | P3 |
| field-agora | 0830 | Reconnect countdown needs app-side deadline bookkeeping | API gap | P3 |
| field-agora | 0860 | RichTextView/MarkdownView no intrinsic measure — invisible in Scroll (MarkdownView half FIXED by the ADR-0005 wave; RichTextView + the general class remain — see 0135) | footgun | P3 |
| field-agora | 0870 | FeedItem headline single-row/nowrap mode | capability gap | P3 |
| field-agora | 0880 | FeedItem body max-measure for wide terminals | capability gap | P3 |
| field-agora | 0885 | Disclosure title needs a rich-span slot (folded cards lose identity color) | capability gap | P2 |
| field-agora | 0890 | Disclosure capped body under-measures rich feed items (rows clip) | bug | P2 |
| field-agora | 0895 | Bound `Scroll::offset_y(Signal)` ignored inside Drawer pages | bug | P1 |
| field-agora | 0900 | Completion panel occludes the row above a bottom-docked composer | API gap | P2 |
| field-agora | 0905 | Drawer needs vertical insets so docked chrome stays visible | API gap | P3 |
| field-agora | 0910 | Scroll of widgets: no ensure-visible / child-offset verb | API gap | P2 |
| field-gateway | 0900 | Table: oversubscribed fixed columns silently starve the Flex column to zero | footgun | P2 |
| field-gateway | 0905 | Select/Combobox same-value re-commit unobservable | API gap | P2 |
| field-gateway | 0910 | Shortcuts on elements outside the focus path silently never fire | footgun | P2 |
| field-gateway | 0920 | Wizard/tab navigation needs an input-immune key lane (0520 evidence) | capability gap | P3 |
| field-gateway | 0930 | Widget `disabled` is build-time only — validation gating forces focus-dropping rebuilds (0510 evidence) | API gap | P2 |
| field-gateway | 0935 | Dirty-form tracking is hand-rolled per form (0510 evidence) | capability gap | P3 |
| field-gateway | 0940 | Modal::open builds content before the Modal exists — self-closing forms need an external-slot dance | API gap | P3 |
| field-gateway | 0945 | ChoicePrompt shares MODAL_Z with app modals — no stacking policy, no introspection | footgun/API gap | P2 |
| field-gateway | 0950 | reactive::connection assumes a persistent transport — probe-shaped clients cannot adopt it | API-fit evidence | P3 |
| field-gateway | 0960 | Element::draw closures paint past their own rect | footgun | P2 |
| field-gateway | 0970 | Table never clamps a bound selection when rows shrink | API gap | P2 |
| field-gateway | 0980 | Table consumes `s` (sort cycling) even without a sort handler | footgun | P3 |
| field-gateway | 0990 | No engine pattern for routing one-shot write completions back to forms (0510 evidence) | capability gap | P3 |
| field-gateway | 1000 | Dead-keys WINDOW when a modal's only focusables mount after an async load (extends 0230) | footgun | P1 |
| field-gateway | 1010 | PageHost: no per-tab locked/disabled affordance for gated (wizard) flows | capability gap | P3 |
| wave11 | 0990 | File-size budget reconciliation — 12 splits DONE (wave 12); 15 files still >600 re-counted 2026-07-25 (driver.rs 1115 last; acceptance.rs test-only non-goal → 14 actionable) | maintenance | P3 |

(field-core 1100–1190: no items yet — the abstractcore-console build
launched 2026-07-25; expect filings.)

## Next recommended work

(Updated 2026-07-25, wave-13 backlog pass. Evidence base: the wave-12
pixel-review handoffs — `reviews/wave12/visual-to-code-handoff.md` +
`code-to-visual-handoff.md` — the four field tracks, ADR-0005, and the
per-item verification done for this pass. The former list is fully
discharged: the 0.2.2 patch, 0102/0104, 0040, 0297, and 0700 are all in
completed/.)

1. **0185 measure inflation (+ 0175, + 0135's remainder)** — the
   measure/crush family the wave-12 pixel review exposed. WHY: 0185 is
   the code seat's OWN named "top engine investigation for the next
   wave" (a crushed fixed-height row while grow slack exists is silent
   content loss in every shell); 0175 (shadow eats a slot + first child
   dies order-dependently) and 0135 (Scroll over plain trees collapses
   to a bar) are the same solver seam. The ADR-0005 wave already took
   the content-view half (MarkdownView/CodeView intrinsic measure) —
   the pattern to extend. Precondition on all three: failing-test pins
   first.
2. **The two field P1s** — field-gateway 1000 (dead-keys window on
   async-mounting modals: silent, looks like a wedge, cost a night
   hour to diagnose; the ask is a structural focus fallback) and
   field-agora 0895 (bound `Scroll::offset_y` dead inside Drawer
   pages: keyboard-first drawer pages — the 0.2.12 headline use case —
   re-derive windowing by hand). Both verified still open 2026-07-25.
3. **The P2 field cluster that feeds app-kits** — field-agora
   0885/0890/0900/0910 (Disclosure rich titles + capped-body measure,
   completion-panel reserved rows, ensure-visible) and field-gateway
   0900/0905/0910/0930/0945/0960/0970 — these are simultaneously bug
   fixes AND the live evidence 0510/0520/0530 build on. Verified still
   open 2026-07-25 (no title_rich/margin_rows/inset/ensure-visible
   surfaces exist in 0.2.22).
4. **The small shipped-crate polish batch** — extensions 0445 + 0455
   (both sized "small, next wave" by the code seat), media-av 0615
   (docs-only), first-app 0289 (a real input bug). One short wave
   clears four items.
5. **0555 PageHost default-grow** — needs its OWN wave slot (behavior
   change + changelog, per the code seat); acceptance is the shell
   example deleting its workaround.
6. **wave11/0990 remaining splits + 0380** — 15 files still >600 lines
   (re-counted 2026-07-25; three grew since filing). driver.rs (1115)
   goes last with the phase structure as the seam — and 0380 (the
   debug-damage knob) lands in that same touch by the code seat's own
   note.
7. **Completion follow-ups already named in completed items** (read
   before scheduling adjacent work): 0595's — Select's short-viewport
   window cap, themes-table.md mode column next regen, upstream theme
   name coordination; 0605's — Drawer ✕ activation parity (still
   fire-on-Down, predates the 0.2.20 press-release lesson),
   `title_action` slot if a second title action is commissioned,
   hover-heal under stationary pointers (tree-side if ever).
8. **Standing gates** — planned/0002 (the 0.3 breaking budget) executes
   when the maintainer signs; 0050 (transport ADR) now HAS its
   evidence source (agora-tui runs live against the hub — fold
   field-agora 0820/0830/0950 into the decision); 0060/0215 close when
   their epics' owners declare the validators done.

## Sequencing (load-bearing)

- **live-data is one-directional**: 0010 before 0020/0030; 0010+0020 before
  the watcher (0060). **0060 before closing 0050**: the transport ADR
  waits on the watcher's experience report — the watcher exists now
  (agora-tui); its transport findings are field-agora 0820/0830/0950.
- **The wave-12 measure family is one investigation**: 0185 (solver
  measure/shrink) is the trunk; 0175's order-dependence half and
  0135's plain-tree half hang off the same seam — pin failing tests
  first, fix once, re-verify all three plus field-agora 0860/0890
  (the same class seen from the field). The ADR-0005 wave's
  content-view fix (intrinsic measure + `basis(Cells(0))`) is the
  established pattern.
- **Ports depend on both tracks**: 0200 (console) ← app-widgets +
  live-data 0010/0020/0030 (subprocess pipe, no network). 0210 (chat)
  ← both + 0040/0050; its read-only phase 1 IS the 0060 milestone.
- **0300 before everything in its band** — 0310/0320/0340/0350 all consume
  the lifecycle surface. **0310 before 0320**; **0320 ↔ 0410** (JSON
  promotion); **0340 ↔ 0520** (the wizard is the persist registry's
  accepted first consumer); **0360 → 0350/0320-ADR** (evidence before
  freeze).
- **0380 rides the driver.rs split** (wave11/0990's deliberately-last
  entry) — the code seat's named landing moment for the knob.
- **0555 is a behavior change**: own wave slot, changelog line,
  acceptance-battery re-run — never batched silently with fixes.
- **0530's remaining scope builds ON 0535's activation event** (the
  0250 ruling as AMENDED 2026-07-24: engine click-chain synthesis is
  in; List's timing-free picker gesture stands).
- **0500's popup substrate before its consumers** — shipped; 0530's
  action menus and extensions 0430's tooltips consume it as public API.
- **0420 before 0430/0440/0450** — all shipped except 0430, which now
  waits only on a named dataflow-editor consumer.
- **Sibling extension crates inherit the dependency posture** (std +
  abstracttui + hand-rolled parsing); the TLS-class exception is not
  granted here — it rides live-data 0050's transport ADR.
- **The key-state chain shipped end-to-end** (0293 → 0700 → 0610);
  0615 is its wording follow-up, docs-only.
- **The Feed-block family**: app-widgets 0102 shipped the crate-private
  `ItemBlock` vocabulary; media-av 0660 and first-app 0280 still extend
  the same enum — one design pass when either schedules; the public
  fold-back is budgeted in planned/0002 entry 5.
- **Same-z Modal stacking hazard** (verified in code 2026-07-22): two
  `Modal::open` calls both mount at `MODAL_Z = 1000`; visually-top and
  key-owner DISAGREE for stacked modals. Whoever ships stacked-dialog
  UX (0510/0520 forms, 0530 row actions, the 0590 validators) must
  give `Modal` a z-or-`top_z` story first — field-gateway 0945 is the
  field sighting of the same hazard; details in
  `completed/app-kits/0500_select_combobox_family.md` "Follow-ups
  revealed".
- **0730's home is an ADR-0004 classification** (core module vs a
  games-domain sibling crate) — promotes only with a recorded ruling.

## ADR state

`docs/adr/` holds five accepted ADRs: **0001** (API stability policy),
**0002** (two-`Style` ruling), **0003** (struct extensibility) — all
2026-07-21; **0004** (extension packaging: features vs sibling crates —
the extension-architecture ADR the 0400 track owed, executed
2026-07-23); **0005** (content rendering responsibilities: markdown/
code/diff/JSON/YAML are CORE; `abstracttui[md]` rejected; extensions own
diagram-class content; mdpad is the quality bar — 2026-07-25). Still
owed: the **0320 control-protocol ADR**, the **0340
persistence-container ADR**, and the **0050 transport ADR** (waits on
0060's evidence — now collectable from field-agora). The
a11y-completeness + redaction-at-source clause (drafted in
`reviews/study/platform-cycle3.md`) still joins the next ADR pass.

## Process

- New item: scan every lifecycle dir + topic folder for the next unused
  global `NNNN` (mind the collision list above — treat a number as
  taken if ANY track uses it), add it under the right state, and update
  this overview's counts, ledgers, and sequencing in the same pass.
  Update the owning track README's table in the same pass too — four
  track tables had drifted from their directories when this pass
  audited them (field-gateway missing 1000/1010; field-agora missing
  0895–0910; first-app missing 0273/0274; this ledger missing six
  completed rows). The directory is the truth; the tables must follow.
- Completion: append a `## Completion report` (final path, date,
  outcome, key validation), move to `completed/`, update the ledgers
  here. Same-wave deliveries may file directly in `completed/` (the
  0535/0595/0605/0273 precedent) — but the ledger row lands in the
  same pass.
- Deprecation: append a `## Deprecation report` with the reason, move
  to `deprecated/`, update this overview.
- Bands: live-data owns 0010–0090, app-widgets 0100–0190, ports
  0200–0290 (0200/0210/0215 = port epics; 0220–0299 = first-app
  findings), control-plane 0300–0390, extensions 0400–0490, app-kits
  0500–0590 (spilled to 0595/0605 — recorded, the numbers are unique),
  media-av 0600–0690 (0605 inside this range is an app-kits spill —
  unique, flagged),
  games 0700–0790, field-agora 0800–0890 (overflowed 0895–0910),
  field-gateway 0900–0990 (overflowed 1000–1050), field-core
  1100–1190, wave-13 architecture/md-lane filings 1200–1249
  (1200/1210/1220 on disk), app-kits CONTINUATION 1250–1290 (the
  0500–0590 band + 0595/0605 spills is full; 1250 = the next free
  fifty, taken 2026-07-26 by the reasoning-controls item), first-app
  CONTINUATION 1300–1340 (0220–0299 full; 1300 = the next free fifty,
  taken 2026-08-16 by the selection/follow-tail freeze). Full bands
  continue at the next free fifty and record it in their README
  (`proposed/field-core/README.md` states the rule). Leave gaps for
  insertion.
