# agora-tui — engine-seat recommendation letter (2026-07-25)

Surveyed: `~/projects/gh/agora-tui` at its current working tree (read-only;
no files touched). Engine reference: abstracttui **0.2.16** (crates.io),
`docs/api.md`, CHANGELOG 0.2.12–0.2.16, backlog state as of today. This
letter supersedes `reviews/agora-tui-v2-upgrade-prompt.md`, which is fully
discharged (§1 below).

## Executive summary

1. agora-tui fully discharged the v2 prompt on its own judgment: 0.2.12 pinned (Cargo.toml:15, lock exact), the reader landed as a **passive** right Drawer generalized into a four-mode panel (`src/ui/reader.rs`), the sidebar drawer and PageHost were declined for the reasons the prompt itself sanctioned, and a `push_resize` test pins the drawer across a narrow resize (tests/watch_ui.rs:920).
2. Since then the app outgrew the prompt's premises entirely: it is now a full chat client (write plane, composer, command grammar, ~126 tests), and its message cards are a **Scroll column of real Disclosure widgets — not Feed** — so the 0.2.16 double-click recipe below is written against Disclosure plus a capture-phase wrapper handler, and it fixes a real regression on the way: mouse click-to-select died in the Disclosure migration.
3. P1: bump to 0.2.16 (additive, MSRV 1.87 unchanged); one ~15-line capture-phase handler restoring click-select and adding double-click-opens-reader; mint SVG captures (0.2.14) to replace the README's hand-typed screen mock — which already teaches retired keys.
4. Graph verdict: an honest **on-demand** fit exists (a `/net` force-layout view over agents/reply/address pairs already sitting in the local rings, presence-inked, reputation-badged) but it is P3 — the presence board answers daily triage; mermaid-in-message-bodies is recommended **against** (their hostile-input posture is correct).
5. Robustness: exemplary — every 0.2.15 recipe was already in place before 0.2.15 named them (shrink pins on all chrome, `use_startup_notices` rendered twice, the one hand-rolled draw closure carries its rect guard); the honest debts run the **other** way: all five of their filed findings (0885/0890/0895/0900/0905) are still open engine-side, so their workarounds must stay after the bump.

---

## 1. Adoption state (the v2 prompt is discharged — do not re-send it)

- **Pin**: `abstracttui = "0.2.12"` (Cargo.toml:15), Cargo.lock resolves
  0.2.12 exactly. The MSRV comment (Cargo.toml:5) cites "0.2.12 declares
  1.87" — 0.2.16 still declares 1.87 (engine Cargo.toml:11), so only the
  version string needs refreshing, same as last time.
- **§1 reader drawer: LANDED, with two justified divergences.** It is
  `DrawerFocus::Passive` at `Percent(0.55)` with `motion(ZERO)`
  (src/ui/mod.rs:157–167), not the Modal the prompt sketched — their
  rationale (a keyboard reader wants zero latency; keys stay with the
  panes) is sound and their KnowledgeBase item 27 records that the default
  slide never completes under fast headless turns anyway. And it is
  generalized beyond one message: `PanelContent::{Message, Doc, Help,
  Files}` (src/ui/reader.rs:23–39) makes the drawer the app's one
  right-panel surface (file listings, `/help`, invite tokens, `/diag`).
- **The Scroll-in-drawer part of the prompt was WRONG in practice** and
  they proved it: a bound `Scroll::offset_y(Signal)` is ignored inside
  Drawer pages, filed as **0895** with a minimal repro. Their reader
  self-windows its lines at an app-owned offset with an honest "▲ N
  above" marker (reader.rs:87–90, `windowed()` at reader.rs:329–350).
  **0895 is still open** (backlog/proposed/field-agora/0895) — the
  workaround stays after any bump.
- **§2 sidebar drawer: deliberately declined** (their CHANGELOG, "as the
  prompt itself sanctions") — instead the docked sidebar clamps 26→21
  cols below 100-col viewports (src/ui/sidebar.rs:16–20, 104–115). A
  better answer than the drawer for a watcher; no complaint.
- **§3 PageHost: assessed-and-declined** per the prompt's own honest
  assessment. Still correct — nothing in the chat era is page-shaped.
- **§4 push_resize: LANDED** — the reader-drawer test resizes 120×34 →
  80×24 mid-open and asserts re-clamp-not-dismiss (tests/watch_ui.rs:920).
- **§5 filings: over-delivered.** Since the prompt they filed 0885, 0890,
  0895, 0900, 0905 (all with repros/context; see their
  docs/experience-report.md and untracked/KnowledgeBase.md items 23–25).

**What moved beyond the prompt's premises** (this changes the advice):
the app is no longer a read-only watcher. A full write plane
(`hub/api_write.rs`, one command worker in `hub/writer.rs`), a modal
composer (`ui/composer.rs`), a slash-command grammar (`ui/commands.rs`),
and the **chat-first key model** (ui/keys.rs:271–279: every printable
opens the composer seeded with itself; actions live on Alt-chords —
⌥o read, ⌥r reply, ⌥q quit, ⌥1-9 panes). And the cards migrated from the
Feed message-card recipe to a **Scroll column of real `Disclosure`
widgets** (src/ui/panes.rs:292–369, src/ui/cards.rs:518–535) under an
operator directive. Both facts drive §2 below.

## 2. P1 — adopt now

### P1-a. Bump to 0.2.16 (one line)

`abstracttui = "0.2.16"` (Cargo.toml:15) + refresh the MSRV comment
(Cargo.toml:5). 0.2.12→0.2.16 is additive (engine CHANGELOG 0.2.13–0.2.16
lists only Added/Fixed; the CI semver gate held; MSRV 1.87 unchanged).
Behavior deltas that could touch existing frames, checked against their
tree:

- 0.2.15 zero-area fusion fix: their chrome never crushes (shrink pins
  everywhere, §5) and their one draw closure already guards
  `rect.is_empty()` (cards.rs:549) — no visible change expected.
- 0.2.16 click-chain synthesis: no handler of theirs reads
  `click_count()` yet; sidebar `List::on_select` (sidebar.rs:196) and the
  Disclosure title toggle are unchanged by it.
- **What the bump does NOT fix**: 0885 (rich title slot), 0890 (capped
  body under-measures rich items), 0895 (bound offset in drawer pages),
  0900 (completion panel reserved row), 0905 (drawer vertical insets) are
  all still `proposed` in the engine backlog. Keep the status gutter
  (cards.rs:539–566), the block-level `max_rows(24)` + `max_body_rows(0)`
  pair (cards.rs:16–20, 502–524), and the self-windowed reader exactly as
  they are. The "switch back when it lands" comments (cards.rs:19) stay
  accurate.

### P1-b. Double-click (0.2.16): one capture-phase handler on the card wrapper — and it repairs a real regression

**Premise correction to the engine's own briefing**: the cards are not
Feed+`on_item_press` anymore. There is **no mouse handler anywhere in
their src** (verified by grep); today's click surfaces are the
Disclosure's native title-row toggle and the sidebar List. Which exposes
the gap: **mouse click-to-select died in the Disclosure migration.** The
Feed era had "any other row click moves the selection band there" (their
cycle-3 changelog); now the band moves only by ↑/↓ — yet every message
verb demands a selection (`selected_ref`, ui/mod.rs:172–193; the grammar
teaches "select a message first (↑/↓)" — commands.rs:103, 174, 179). A
mouse-first user cannot arm reply/rate/retract at all. Bonus defect the
recipe fixes: a double-click on a title row today toggles the fold
**twice** (visible flicker, net nothing) — the widget toggles per press.

The honest recipe — not `ClickChain` (that is for input paths outside
tree dispatch) and not a Feed lane (there is none): a **capture-phase
mouse handler on each card's row wrapper** (cards.rs:539, the
`[gutter][card]` row), reading `ctx.click_count()` (engine:
ui/event.rs:362; routing law api.md "Double-click", api.md:136–176;
`Element::on` at ui/view.rs:205; `stop_propagation` at ui/event.rs:299).
Capture runs ancestor-first, so the wrapper sees every press inside the
card before the Disclosure's title handler does:

```rust
// cards.rs message_card(): thread in `selected`/`follow` (PaneWires) and
// an open_reader(channel, seq) closure; `key` is "channel:seq".
.on(Phase::Capture, move |ctx, ev| {
    if let UiEvent::Mouse(m) = ev {
        if matches!(m.kind, MouseKind::Down(MouseButton::Left)) {
            let already =
                selected.with_untracked(|s| s.as_deref() == Some(key.as_str()));
            if ctx.click_count() >= 2 && already {
                open_reader(&key);        // press 2: open the full reader
                ctx.stop_propagation();   // …and don't re-toggle the fold
            } else {
                follow.set(false);        // press 1: select (Feed-era
                selected.set(Some(key.clone())); // semantics restored)
            }
        }
    }
})
```

Semantics this yields, all deliberate:

- **Click 1 selects** (and a title-row click still toggles — the press is
  not consumed, so the Disclosure behaves as today, now with the band
  following the pointer). If the pane is unfocused, also set `focused` to
  the card's channel (the key carries it) — `selected_ref` reads the
  focused pane only (ui/mod.rs:177–179).
- **Click 2 on the already-selected card opens the reader** and the
  `stop_propagation()` kills the double-toggle flicker for free.
- **The already-selected gate IS the same-logical-row guard** the engine
  docs demand (api.md:158–162): folded cards are 1 row tall and the
  chain's tolerance is ≤1 cell Chebyshev, so a fast click-walk down
  adjacent folded titles chains — but press 2 lands on a card that is not
  yet selected, so it only selects. Same gate `Table::on_activate` uses.
- A slow second click on the selected card re-selects, never opens —
  matching Table's deliberate browsing semantics (0.2.16 CHANGELOG).

Testability: their harness drives the real `Driver`, which publishes the
ambient event time each turn (0.2.16 CHANGELOG; `ui::set_event_time` /
the driver's `set_clock`-injectable clock), so two scripted SGR presses
in adjacent turns count 2 headlessly — pin it next to the existing
fold/click tests.

Optional sibling, zero new machinery: the sidebar List already ships
activation vocabulary (api.md:232 — `on_activate` = Enter/Space/
click-on-selected). Binding it to "unhide + focus + follow tail" would
give clicking the already-focused channel a meaning; P3, skip freely.

### P1-c. Screenshot (0.2.14): mint the README's screen from the production tree

They ship no screenshot usage today, and the README's hand-typed ASCII
mock is **already stale** — its footer row teaches `o read i compose R
reply F files q quit` (README.md:21) and the status row says `i to
compose` (README.md:20), all retired by the cycle-6 chat-first model
(bare letters type; the real footer says `⌥o read · ⌥r reply`,
ui/mod.rs:292–300). A minted capture cannot drift silently. The recipe,
against their exact harness (tests/watch_ui.rs:41–119):

```rust
#[test]
#[ignore = "artifact mint, not a pin — run explicitly to refresh docs/captures"]
fn mint_readme_captures() {
    let mut h = harness(&["commons", "entity-society"], Size::new(120, 36));
    let mut driver = driver(&mut h);
    turn(&mut h, &mut driver);
    // …seed Baseline + rows (one open ask, one ▲-rated, one reply),
    // select + expand a card, settle(&mut h, &mut driver)…
    driver.screenshot().write_svg("docs/captures/watcher.svg").unwrap();
}
```

`Driver::screenshot()` is a pure read of the composed frame (0.2.14
CHANGELOG; `write_svg` per engine examples/screenshot.rs:117,126–128;
SVGs render on GitHub). Scenes worth minting, in value order:

1. the flagship 120×36 multi-pane frame — unread badges, presence board
   with ✦ delegate marks and rep scores, the `◦ owed:` chip, one expanded
   card with its rich meta line (the showcase the engine would also like
   to point at);
2. the reader drawer open over a reply chain (their 0.2.12 adoption,
   pane band still visible beside it);
3. the composer armed — inverted `✎ #channel` chip + the `@` completion
   dropdown above it (their AbovePreferred adoption, composer.rs:181);
4. 80×24 — the narrow sidebar clamp plus the `▸ +N more:` overflow strip
   (panes.rs:391–428): the small-terminal honesty story in one image;
5. the structured `/help` panel.

## 3. P2 — worth planning

- **P2-a. A resize-ladder test at 60×16 / 40×12.** The suite pins 80×24
  (watch_ui.rs:920) but nothing smaller; their own degradation machinery
  (MIN_PANE_ROWS window + strip, panes.rs:244–262; sidebar clamp;
  two-line `/help` below 64 cols, reader.rs:127–128) is exactly what the
  engine's wave-10 sweep (0.2.15, `tests/wave_size_sweep.rs`,
  reviews/wave10/size-ratio-sweep.md) found apps get wrong unpinned. One
  `push_resize` ladder asserting the strip appears, chrome rows survive,
  and the composer still mounts would pin their best work.
- **P2-b. File the ensure-visible gap instead of living with the
  approximation.** `offset_of_card` (cards.rs:582–592) models every
  expanded card as `2 + 24` rows because a Scroll of widgets offers no
  child-offset/ensure-visible verb — with several expanded cards above,
  scroll-to-selection drifts (they note "Scroll clamps overshoot"
  themselves). Nothing in the engine backlog covers it (checked; List has
  `scroll_to`, Feed has `row_of`, a widget column has nothing). This is a
  legitimate 0910-class field-agora ask — the engine seat pre-endorses
  the filing; their 0850 shipped same-day, the band works (their
  KnowledgeBase item 18).
- **P2-c. De-duplicate the drawer width constant.** reader.rs:127
  hardcodes `viewport.w * 55 / 100 - 2` to mirror
  `DrawerSize::Percent(0.55)` (ui/mod.rs:158) — a silent-drift pair if
  either changes. App-side: one named const used by both. Engine-side
  honesty: a drawer page has no "my width" hook today; if the const feels
  wrong, that observation belongs in the 0905 thread (drawer insets ↔
  page geometry) rather than a new workaround.
- **P2-d. Teaching-surface truth sweep after the cycle-6 key model.**
  Their /help and live footer are correct; four surfaces still teach
  retired keys, and one teaches a false safety claim:
  - reader acts line `"acts: R reply · /rate ±1 · /retract (yours)"`
    (reader.rs:292–296) — bare `R` now types "R" into the composer; it is
    `⌥r`;
  - boot note `"1-9/tab jump, v hides, z zooms"` (main.rs:141) — all
    three are Alt-chords now (keys.rs:461, 466, 510–514);
  - panes empty state `"(i composes, …)"` (panes.rs:213) — works but
    seeds a stray `i` into the draft; "type to chat" is their own better
    wording (composer.rs:108);
  - README command table `"(or `R` to arm, then type)"` (README.md:39) —
    same retirement as the mock rows P1-c replaces;
  - **main.rs:1–7 module doc and the `--help` text (main.rs:38–46) still
    say "read-only … never registers, joins, posts, acks, or writes
    anything to the hub" — false since the chat era**, as is the
    Cargo.toml description (line 8: "read-only multi-channel watcher")
    and the 0.1.0 version against a CHANGELOG already at 0.2.0-dev.
    For an app whose brand is honest labeling, the `--help` claim is the
    one to fix first. (Their lane, not engine surface — flagged because
    the survey found it.)

## 4. Graph family (0.2.13) — verdict: honest fit exists, P3, on-demand only

The data is genuinely a network and it is **already in memory**: presence
roster, reputation scores and delegate marks feed the sidebar today
(sidebar.rs:260–266), and every ring row carries `sender`, `to[]`,
`reply_to` (300 rows/channel). An honest shape:

- a `/net` command (their grammar's natural home, commands.rs) setting a
  new `PanelContent::Network` arm — the drawer already hosts full pages;
- nodes = agents seen in the watched rings + presence roster; edges =
  reply pairs and address pairs folded from the rings, weight = count;
- `force(&desc, &ForceOpts)` — the engine's own table names "knowledge
  graphs, networks" for it (docs/graphs-and-diagrams.md:60), it is a
  bounded act that freezes on settle (never an idle animation — matching
  their zero-idle discipline), and `GraphView` gives selection, pan,
  tooltips, `on_node_press`, kind-tinted accents (presence ink) and a
  reactive badge slot (rep score) out of the box
  (graphs-and-diagrams.md:70–115). `abstracttui-graph = "0.1"` is a
  separate dep (extension family).

Why P3 and not higher: the watcher's core loop is triage + chat, and the
presence board already answers "who is here, who is vested, who is
trusted" in 26 columns. The graph answers a different, occasional
question — "who is coordinating with whom lately" (isolated seats,
DM-heavy pairs, delegation clusters). On demand behind `/net` that is
substance; in the boot path it would be decoration. Two honest caveats if
they build it: the passive drawer keeps keys with the root
(GraphView wants arrows for pan/selection — click-to-focus per the
passive-drawer rule, or add a Network arm to their panel-keys routing in
keys.rs:298–319, which already has a Files arm); and rebuild the view
over their `gens` signal so layout re-runs only on real change.

**Mermaid: recommend against** for message bodies. Their rule —
"bodies are hostile input … render as text, never re-parsed"
(panes.rs:24–26) — is the right posture for an agent hub. `MermaidView`
parses-only with atomic fallback, but rendering agent-controlled diagrams
inline still contradicts the stance for zero triage value. If ever: an
explicit per-message act in the reader, nothing inline. No action.

## 5. Robustness (0.2.15) — audit result: nothing to adopt, keep what you have

Checked against the engine's guarantees-vs-recipes ruling (api.md
"Small terminals & content pressure", api.md:204–218):

- **R1 `shrink(0.0)` pins: present on every piece of chrome.** Header
  status row (header.rs:82), notice row (header.rs:191, 202), sidebar
  width (sidebar.rs:139–143) and its separators/List wrapper
  (sidebar.rs:145, 180–185), footer (ui/mod.rs:270), composer prompt +
  input region (composer.rs:80, 150, 221), overflow strip
  (panes.rs:415). This predates 0.2.15 — their KnowledgeBase item 4 IS
  the recipe. Nothing to do.
- **R3 `use_startup_notices` rendered: twice.** The footer `⚙N` counter
  (ui/mod.rs:326–343) and the `/diag` panel dumping the full list
  (ops.rs:258–270). Better than the recipe asks. Nothing to do.
- **R4 rect guards in hand-rolled closures: compliant.** The one draw
  closure in the app (the status gutter, cards.rs:548–565) guards
  `rect.is_empty()` and loops within `rect.h` on a 1-cell column — both
  axes bounded. The 0.2.15 fusion fix makes the empty guard redundant at
  zero area, but R4's point is PARTIAL crushes still reach closures:
  keep the guard.
- **Small sizes: already engineered past the sweep's bar.** Pane window
  + honest `▸ +N more:` strip sliding with focus (panes.rs:238–262,
  391–428), sidebar 26→21 clamp with a breakpoint memo (sidebar.rs:104–
  115), width-aware auto-expand (cards.rs:165–187), `/help` two-line
  layout below 64 cols (reader.rs:128), middle-truncating notices
  (header.rs:164–180). The one gap is test coverage below 80×24 — P2-a.

## 6. Anything broken / risky (beyond §3's items)

- **Non-virtualized card columns — sound at ring cap, a boundary to
  know.** A pane is a Scroll over up to 300 built Disclosure elements
  (panes.rs:292–362); folded bodies unmount (zero idle) but layout still
  solves 300 title rows per rebuild per visible pane. At cap 300 with
  `gens`-scoped rebuilds this is fine; if rings ever grow, the
  virtualized shape is the engine's Feed message-card recipe (api.md:551+)
  — which trades away the real-widget cards the operator directed. Not a
  defect; a documented trade with a known exit.
- **Reader offset can over-run content** (keys.rs:452: PgDn does
  `*y += 10` unclamped; `windowed()` clamps at render, reader.rs:331, so
  it self-heals visually, but Home is then the only sane way back).
  One-line clamp against the line count if it ever annoys; cosmetic.
- **Their CHANGELOG is one cycle behind the code** — it documents through
  chat cycle 5; keys.rs:271 cites "cycle-6 P1-1" (the chat-first key
  model). Worth an entry before the next tag, since cycle 6 changed the
  entire key surface.
- **No stale engine idioms found.** Actions-not-shortcuts under overlays
  (main.rs:212–214, their KnowledgeBase item 24), kitty-encoded headless
  Esc (item 26), mount-scope fold registries (item 20), explicit
  `content_size` for RichTextView-in-Scroll (0860 workaround,
  sidebar.rs:314–319) — all current best practice; several of them are
  lessons the engine learned FROM this app.

## Still open in the engine (so their workarounds stay)

| Their filing | Status | Their workaround that must survive the bump |
| --- | --- | --- |
| 0885 rich title slot | proposed | status gutter + glyph vocabulary (cards.rs:537–566) |
| 0890 capped body under-measures rich items | proposed | `max_body_rows(0)` + block `max_rows(24)` (cards.rs:502–524) |
| 0895 bound Scroll offset dead in drawer pages | proposed | self-windowed reader lines (reader.rs:329–350) |
| 0900 completion panel reserved row | proposed | `AbovePreferred` placement (composer.rs:181) covers the worst of it |
| 0905 drawer vertical insets | proposed | none needed today; the P2-c width constant belongs on this thread |

0850 remains the band's shipped exemplar (completed/field-agora/0850,
engine 0.2.11). The invitation stands: P2-b (Scroll ensure-visible) is
the next filing the engine seat would fast-track.
