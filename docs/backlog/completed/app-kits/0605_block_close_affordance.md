# 0605 — Block close affordance: the panel ✕ (`on_close`)

## Metadata
- Created: 2026-07-25
- Status: Completed (same wave; filed directly in completed/ — the
  0535/0595 precedent)
- Track: app-kits (panel chrome, beside 0585 drawer + 0595 switcher;
  first-app was considered and rejected — that band records field
  findings from driving the engine, this is a commissioned chrome
  feature)
- Completed: 2026-07-25
- Trigger: operator order (with an agora-tui multi-pane screenshot) —
  "in terms of components/panels, sometimes we need to close them to
  free up space. I would like a cross (probably upper right?) on each
  discussion panel so I can close them and the other panels get more
  space. Whether or not a panel can be closed is the app's decision
  (here yes)."
- Adoption target: agora-tui's watcher panes already hide via a `v`
  key; the ✕ is that mechanism's mouse twin — the API had to make the
  adoption a one-liner (`.on_close(move || hidden.set(true))`).

## ADR status
- Governing ADRs: ADR-0001 (API stability — everything ADDITIVE vs
  0.2.21, `cargo semver-checks` clean). `Block` gains one builder and
  one private field; no signature changes, no new dependencies.

## Design rulings

- **API fork settled: (a) `Block::on_close(impl FnMut() + 'static)`**,
  callback-presence = the opt-in (the `Button::on_click` /
  `Drawer::on_close` house convention — no separate `closable()` flag
  to desync from the handler). The generic title-ACTIONS slot (b) was
  rejected FOR TODAY: a close affordance has non-generic semantics —
  danger hover tint, right-corner anchoring, the title-yields-first
  truncation priority, the mouse-only/never-focusable rule — that a
  generic slot would either hardcode anyway or push onto every caller
  as glyph/tint/ordering decisions (inconsistency machinery), and the
  charter forbids speculative machinery. No corner painted: `on_close`
  names ONE action; a later `title_action(glyph, f)` slot can join
  additively (and absorb `on_close` as sugar), and the internal
  `close_run` geometry already right-aligns an N-cell run.
- **Block stays draw-only at rest; geometry has ONE owner.** The ROOT
  draw paints the muted ✕ and owns every layout decision (border,
  title truncation, close reservation) — so the title and the ✕ can
  never disagree about the same cells. It publishes the PANEL rect
  (post-shadow) through a probe cell each paint; the root element is
  `probe_when_culled` when closable, so zero-area paints still publish
  and a crushed block retracts its affordance the same frame.
- **Interactivity = one absolute out-of-flow child spanning the title
  row** (inset left 0 / right 0 / top −1 against the content box —
  the border row), so a closable block's CHILDREN lay out
  byte-identically to a plain one. The child's width derives from the
  interior: a w ≤ 2 bordered block solves it to ZERO cells —
  unhittable and unpaintable by geometry, not by guards (the fusion
  law falls out of the layout math). Hit/hover/press all do their rect
  math against the probe cell, never the child's own rect: draw and
  hit can never disagree with the frame.
- **Scope without a `Scope` parameter**: `Block::element(&TokenSet)`
  has no scope for hover/pressed signals; the child is a
  `dyn_view_scoped` whose closure has NO tracked reads — it runs once
  and its generation scope owns the signals for the block's life. The
  inner restyle `dyn_view` is the only reactive region (1 row), and it
  builds an EMPTY element when neither hot nor pressed — at rest the
  affordance subtree carries no draw closure at all.
- **Mouse-only, never focusable** (the 0.2.12 P1: a focusable ✕ was a
  drawer panel's first focusable and stole content focus; keyboard
  close stays app-side — Esc/`v`/whatever the app binds). Activation
  is the Button 0.2.20 convention: press + release with the release
  inside the run (pointer capture routes the release; the rect check
  decides) — deliberately NOT the drawer ✕'s fire-on-Down, which
  predates that lesson (follow-up 1).
- **Glyph `✕` U+2715** (the drawer's spelling): East-Asian-NARROW and
  absent from emoji-data — single-width in every convention. `×`
  U+00D7 is East-Asian-AMBIGUOUS (double-width under ambiguous-wide
  terminal settings) and was rejected — the 0595 glyph-research
  method. Width 1 is test-pinned.
- **Truncation order pinned**: the TITLE yields before the ✕ (a close
  affordance you can't see can't free the space the operator wants).
  Ladder by interior width: ≥ 3 → padded ` ✕ ` (3 cells, the
  forgiving click target); 1–2 → the bare glyph at the last interior
  cell; ≤ 0 (bordered w ≤ 2) → the ✕ yields too, corners only —
  nothing ever paints on or outside the frame. Title space ends where
  the run begins; center/right alignment clamps against it.
- **Hover = `error` ink, pressed = `error` + BOLD, rest = `text_muted`**
  (caller-resolved at `element(&t)` like every Block ink — damage
  contract §5). The §3.2 hover-is-accent rule is for NEUTRAL actions;
  a close affordance is consequence-bearing (the diff vocabulary
  precedent: removed = `error`), and the browser-tab convention
  agrees. Hover state lives in one `hot` signal maintained by Move
  (run containment, `set_if_changed` — no per-Move damage) and
  MouseLeave (self-healing when the pointer leaves the row/block; a
  re-layout under a stationary pointer heals at the next mouse event —
  the engine-wide hover posture).
- **Disposal-safe (0297)**: all widget bookkeeping (`pressed` clear,
  `stop_propagation`) lands BEFORE the user callback, so `on_close`
  may synchronously remove the panel (dispose the block's scope) —
  the operator's exact wish is the callback's normal body. The
  SharedCallback held-borrow contract applies (dispatch-only slot).
- **A11y**: Block had no access surface; the affordance brings its own
  honest one — the interactive child reports `Role::Button` with
  label "Close {title}" (or "Close panel"), NOT focusable, exactly
  like the drawer ✕.
- **Borderless blocks** (`BorderKind::None`) float the ✕ over the
  top-right content cells (documented): the affordance needs a row
  and a borderless block reserved none; the app chose both. Inset top
  is 0 there (never −1 — nothing paints above the block).

## Self-adversarial findings (attacked before shipping, all pinned)

1. **Corner-cell click**: the run ends at interior-end; the corner
   glyph cell is outside it — click there = no fire (pinned).
2. **Title fills the whole row**: the root's title math reserves the
   run BEFORE painting, so title chars never render under the ✕
   (single geometry owner; pinned).
3. **Hover stuck after close-under-cursor**: state dies with the
   scope; the sibling that re-flexes under the pointer shows its own
   (un-hovered) ✕ until the next mouse event — engine hover posture.
   A second click at the same cell fires the SIBLING's callback
   (browser-tab close-spam semantics — deliberate, pinned) and the
   dead panel's callback can never re-fire (instances disposed).
4. **Crush classes**: w ≤ 2 → zero-width child + `close_run` None
   (nothing paints, nothing hits); h = 0 → probe publishes the empty
   panel even though the block's own paint is skipped
   (`probe_when_culled`), so a stale `hot` cannot paint a phantom row
   (pinned with a hovered-then-crushed test).
5. **Mid-scroll clicks**: hit-testing already refuses descent outside
   a clip container's content box, and the probe rect is the truthful
   laid-out rect — a half-scrolled block's visible ✕ works, a
   scrolled-out one is unreachable (pinned through a real Scroll).
6. **Click before first draw**: the probe cell is still zero — the
   handler finds no run and refuses honestly (nothing was visible to
   click; pinned).

## What shipped

- `src/widgets/block.rs`: `on_close` builder + the affordance
  (close_run/panel_rect/close_text helpers, probe cell, absolute
  interactive child); existing tests moved to the `#[path]` sibling
  `src/widgets/block_tests.rs` (file-size law) and extended.
- `tests/wave_block_close.rs`: Driver-level composition proofs
  (SGR clicks through CaptureTerm): the operator's 3-panel re-flex,
  press-drag-out, close-spam across re-flexing siblings, PageHost +
  Drawer composition, Scroll mid-scroll, zero idle parked.
- `examples/widgets.rs`: closable-panels row (3 panels + restore) in
  the interactive tab; `examples/README.md` row updated.
- Docs: api.md Block section (the opt-in, mouse-only rule +
  keyboard-close-is-app-side, truncation order, adoption recipe);
  CHANGELOG `[Unreleased]`.

## Validation
- Unit (mounted through the real tree + dispatch): glyph width pin,
  padded run at the row's right end (muted ink), hover restyle +
  MouseLeave restore, truncation ladder at every width 20→1, no paint
  outside the rect at w ∈ {1,2} and h = 0 (phantom-row pin, hovered
  variant included), shadow anchoring to the lifted panel, borderless
  float, release-inside fires once / corner-cell and title-row clicks
  don't / drag-out doesn't, 0297 dispose-in-callback, click before
  first draw inert, a11y label.
- Driver: 3-panel re-flex + exactly-once + idle zero bytes after,
  drag-out, close-spam/double-click after death, Drawer + PageHost
  composition, Scroll mid-scroll, zero idle with closable blocks
  parked.

## Progress checklist
- [x] Ruling written (a-vs-b), geometry model settled
- [x] `on_close` + affordance in block.rs (+ sibling test move)
- [x] Unit tests (render/hover/ladder/fusion/disposal)
- [x] Driver composition tests (re-flex, drawer, page host, scroll)
- [x] Demo (widgets.rs) + examples README
- [x] Docs (api.md) + CHANGELOG
- [x] Gates: workspace tests, clippy, fmt, semver-checks vs 0.2.21,
      demo headless exit-0

## Completion report

- Final path: `docs/backlog/completed/app-kits/0605_block_close_affordance.md`
- Date: 2026-07-25
- Outcome: the operator's panel ✕, additive, mouse-only, fusion-safe
  by geometry; agora-tui adoption is one line per pane.
- Follow-ups revealed (named, not built):
  1. **Drawer ✕ activation parity**: the drawer header ✕ still fires
     on Down (predates the 0.2.20 press-release-inside lesson);
     aligning it to the Button convention is a one-arm change in
     `drawer_view.rs`.
  2. **`title_action(glyph, f)` slot**: if a second title-row action
     is ever commissioned (pin/minimize), `close_run`'s right-aligned
     run generalizes to an actions run; `on_close` becomes sugar.
  3. **Hover heal under stationary pointers**: hover is derived at
     mouse events only (engine-wide); a re-layout under a motionless
     pointer leaves ✕ hover stale until the next event. If this ever
     matters, the fix is tree-side (re-derive hover after layout
     changes), not per-widget.
