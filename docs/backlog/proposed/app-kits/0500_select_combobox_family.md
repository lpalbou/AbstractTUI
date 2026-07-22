# 0500 — Anchored-popup substrate + the choice-control family (Select / Combobox / MultiSelect)

## Metadata
- Created: 2026-07-22
- Status: Proposed (substrate partially SHIPPED — see the 2026-07-22
  status note below; the select family itself remains unbuilt)
- Track: app-kits
- Completed: N/A
- Depends on: nothing in-band (this item is the band's trunk). Engine
  delta: `Overlays::top_z()` (one method, additive, merges under
  0170's budget window). Designed jointly with 0120 (app-widgets),
  whose completion dropdown is the substrate's passive-panel consumer.
- Validator (0590): `examples/admin_console` (edit-panel selects incl.
  MultiSelect) + `examples/setup_wizard` (provider/model pickers).
- Promotion trigger: any dogfood app reaching a settings/config surface
  (the 0200 console's `/model`/`/theme` pickers already hand-roll the
  modal-list workaround); or the 0510 form kit starting (field rows
  embed this control).

## ADR status
- Governing ADRs: None — no ADR system in this repo yet (see 0170).
  ADR impact: none expected (new widget family; existing widgets
  unchanged; the one engine delta — `Overlays::top_z()` — is additive
  public API that merges under 0170's budget window). The anchored-
  popup SUBSTRATE is specified in this item (spec v1 below), designed
  jointly with 0120's completion dropdown (its passive-panel consumer)
  and consumed cross-band (0530 menus; extensions 0430 tooltips via
  the TOOLTIP mode — their item cites this spec).

## Context
Every reference class in the study brief needs a choice control on its
first screen: the admin console's per-row provider/model pickers and
per-panel filters (reference UI A), the wizard's dropdowns/selects for
provider, model, language (B), the chat shell's channel-scope and
notification-level pickers (C), file managers' sort-by/view-as
switchers, and the smart-note app's tag/kind pickers (D). The first
shipped app already paid the price of the gap: `abstractcode-tui`
built its pickers as List-in-a-Modal and hit the 0250 crash + silent
preference corruption on first contact. A one-of-N control that opens
in place, filters as you type, and (in its third form) accumulates
chips is the single most reused control in application software; the
engine ships nothing shaped like it.

## Current code reality
- **No select/dropdown/combobox/picker widget exists.** Verified two
  ways: the full module list (`src/widgets/mod.rs:19-40` — badge,
  block, button, chart, checkbox, code, feed, grid, image, input,
  list, logo, markdown, progress, radio, richtext, scroll, separator,
  spinner, table, tabs, viewport3d) and a source grep for
  dropdown/combo/picker (only hits: theme-registry doc comments calling
  `theme::list()` "the picker surface", `src/theme/registry.rs:69,99`).
- **One-of-N exists only fully expanded**: `RadioGroup`
  (`src/widgets/radio.rs:23-28`) renders all N rows inline — right for
  2–5 options, wrong for 26 themes or 40 models. `List`
  (`src/widgets/list.rs:54-64`) has the selection/virtualization
  machinery (variable heights, sticky `selection_key`, `scroll_to`)
  but no popup form, and its `on_select` fires on arrow movement
  (0250) — the activation semantic a select needs does not exist yet.
- **The popup substrate is ready**: `Overlays::layer_tree(z, bounds,
  modal, scope, view)` mounts a focusable overlay tree and
  `on_outside_press` is the dismiss hook (`src/app/overlays.rs:194,
  222`; content mode `Tree { modal, on_outside }` at overlays.rs:56-66).
  `Modal`/`Toast` prove the pattern end to end (`src/app/popups.rs`).
- **Anchoring is solvable in-handler only**: nothing exposes an
  element's solved rect outside event dispatch (0120 records the same
  gap for the caret cell), but `EventCtx::current_rect()` is available
  inside handlers — `Table` and `Tabs` both use it for hit mapping
  (`src/widgets/table.rs:178`, `src/widgets/tabs.rs:130`). A select
  opens FROM its own activation handler, so capturing the trigger rect
  there suffices for v1; a general rect-query API is a design question
  to record, not a blocker.
- **A11y roles are pre-provisioned but unused**: `Role::Menu` /
  `Role::MenuItem` exist in the vocabulary (`src/ui/access.rs:31-51`)
  and no widget emits them (grep: only the `as_str` arms at
  access.rs:72-73).
- **Styling vocabulary is ready**: the selection pair for the active
  option, `surface_raised` for the popup ground (the token docs name
  "popovers, menus" explicitly — `docs/theming.md:23-24`), `text_faint`
  placeholders, `border_focus` for the focused closed control
  (docs/theming.md:266-288 state table).

## Problem
There is no way to offer "pick one of N" in bounded space, no
type-to-filter, and no multi-pick. Every config surface must fork a
modal + List + Enter-shortcut + deferred-close workaround (the exact
four-step dance 0250 documents), and every fork re-decides keyboard
semantics, dismiss rules, and option rendering.

## What we want
One widget family over one shared core, three faces:
1. **Popup core — the cross-track anchored-popup SUBSTRATE, owned
   here, in core** (cycle-2 resolution; cycle-3 spec below). The three
   faces consume it; so do 0120's completion dropdown, 0530's action
   menus, and the extensions band's 0430 (whose item now cites the
   panned-card placement case back at this spec). Everything
   load-bearing — placement, stacking, dismiss, key routing, the one
   engine delta — is specified in "Anchored-popup substrate (spec
   v1)" below, and lands FIRST (see the resequenced checklist).
2. **`Select`** (closed): a one-line trigger showing the current choice
   + a `▾` affordance; Enter/Space/click opens; Up/Down inside move a
   HIGHLIGHT (never the bound value); Enter commits highlight → value,
   Esc abandons — the movement-vs-activation split 0250 demands, built
   in from birth. Bound `Signal<usize>` (or key-signal like
   `List::selection_key`) + `on_change` fired on commit only.
   Type-ahead: printable keys jump the highlight to the next matching
   prefix (the classic closed-select gesture).
3. **`Combobox`** (searchable): the trigger is a `TextInput`; typing
   filters options (case-insensitive substring v1; matcher pluggable
   later — the app can pre-filter its own list reactively); the filter
   text is never the value; a non-matching buffer commits nothing.
   Option count + "no matches" line are part of the popup, not app
   code. Reuses `TextInput` whole (`src/widgets/input.rs` cluster
   editing) — no second editor. **Key routing (resolved per PLATFORM
   cycle-2 F4)**: since the open popup is a modal tree that owns every
   key (overlays.rs:354-356), the EDITOR MOUNTS INSIDE THE POPUP TREE
   while open — and the popup's bounds INCLUDE the anchor row, so the
   editor renders at exactly the trigger's screen position (no visual
   jump; the closed trigger re-renders the committed value on
   dismissal). One popup core is shared for geometry/clamp/dismissal;
   key OWNERSHIP differs by face and is documented per face: Select
   popups own navigation keys only, Combobox popups own the editor
   too.
4. **`MultiSelect`**: same popup, checkbox-marked options, commits
   accumulate into a `Signal<Vec<String>>` (or keys); the closed
   trigger renders the picked set as chips (the 0540 chip vocabulary;
   overflow degrades to `+N` honestly). Space toggles without closing;
   Enter/Esc close.
5. **Option model**: `SelectOption { key, label, hint, disabled }` —
   hint renders muted right-aligned (provider names, shortcut hints);
   disabled options render `text_faint` and are skipped by highlight
   movement (docs/theming.md:284 — disabled is out of the focus order).
6. **Form/table embeddability**: the closed control is a one-row
   element with the standard `focusable()`/`focus_signal` contract
   (`src/ui/view.rs:205,280`) so 0510 field rows and 0530 cell editors
   can host it unchanged; `Role::Menu`/`MenuItem` emitted for the
   popup, `access_value` reports the current choice.

## Anchored-popup substrate (spec v1 — cycle 3; cross-track load-bearing)

Lands in `app::popups` beside `Modal`/`Toast` (same layer, same
no-engine-privileges posture), BEFORE the three faces.

**API sketch** (shapes final at implementation; semantics binding):

```rust
// Anchor: solved SCREEN cells, captured inside the opener's event
// handler via EventCtx::current_rect() (the only rect source today —
// table.rs:178 / tabs.rs:130 precedent). Correct by construction for
// panned/absolute subtrees (0430's case): solved coordinates, never
// layout-tree arithmetic. A future out-of-handler rect query (0120's
// caret-cell need) slots in without changing this type.
pub struct PopupAnchor { pub rect: Rect }

pub enum PopupWidth {
    MatchAnchor,                      // selects: popup width == trigger width
    Content { min: i32, max: i32 },   // menus/tooltips: widest option, clamped
}

pub struct Popup;                     // OWNED mode (see routing modes)
impl Popup {
    /// Modal tree at z = overlays.top_z() + 1 (THE engine delta).
    /// Placement: prefer below the anchor; FLIP above when rows below
    /// < needed AND rows above > rows below; height = min(content,
    /// chosen side); x clamped into the viewport (the Toast clamp
    /// math). `include_anchor_row: true` makes the popup's bounds
    /// START at the anchor row (Combobox mounts its editor there —
    /// zero visual jump).
    pub fn open(overlays: &Overlays, cx: Scope, viewport: Size,
                anchor: PopupAnchor, width: PopupWidth,
                build: impl FnOnce(Scope) -> View) -> Popup;
    pub fn close(&self);              // idempotent (Modal::close shape)
    pub fn on_dismiss(&self, f: impl FnMut(DismissReason) + 'static);
}
pub enum DismissReason { OutsidePress, Escape, Closed }

impl Overlays {
    /// ENGINE DELTA — the only one, one method, 0170-gated: highest z
    /// among live layers, so a popup opened over any modal stack
    /// allocates above it (select-inside-modal-inside-modal works;
    /// a static z constant cannot).
    pub fn top_z(&self) -> i32;
}
```

**Three routing modes** (one geometry engine — placement/flip/clamp/
width — three key-ownership contracts; conflating them was cycle-2 F4's
lesson):
1. **OWNED** (`Popup`): a MODAL tree — required for `on_outside_press`
   (fires only for modal trees, overlays.rs:56-60,330-336) and it
   swallows every key while open (overlays.rs:354-356). Dismiss =
   outside-press (never acts below — deliberate overlay semantics) +
   Escape (shortcut inside the tree) + explicit `close()`. Keys inside:
   Up/Down/PageUp/PageDown/Home/End move the HIGHLIGHT, Enter commits,
   printable keys are face policy (type-ahead / popup-mounted editor);
   Tab no-ops v1 (the tree is focus-trapped; multi-region traversal is
   a v2 question). Consumers: the three 0500 faces, 0530 action menus,
   0430 in-card dropdowns.
2. **PASSIVE PANEL**: a NON-modal anchored layer — keys stay with the
   ANCHOR OWNER (a completion dropdown must never steal composer
   typing; the composer routes Up/Down/Enter itself). No outside-press
   available (modal-only, engine fact) — dismissal is OWNER-DRIVEN:
   the anchor's `focus_signal` going false, Esc in the owner, or
   selection commit. Consumer: 0120's completion dropdown (this is
   why the substrate is designed jointly with 0120 — its dropdown is
   NOT an owned popup, and pretending otherwise would break typing).
3. **TOOLTIP**: passive + non-interactive (`layer_draw`, no tree, no
   focus): shown after a hover delay (`after()` one-shot; zero wakeups
   until due), hidden on `MouseLeave`/anchor loss. Consumer: 0430
   hover tooltips.

**Anchor-unmount safety (contract + test, all modes)**: opening from a
`dyn_view`-regenerated subtree must never leak an orphan popup — if
the opener's scope is disposed while the popup lives, the popup closes
(mechanism at implementation: scope-cleanup hook or liveness check;
`Modal` deliberately does NOT do this — its lifetime is the app's
decision — the popup differs precisely because its anchor can die
under it). Pinned by a regression test: regenerate the trigger's
dyn_view with the popup open → layer removed, no stale input owner.

**Consumers (enumerated; sign-off before the spec freezes)**:
| Consumer | Mode | Anchor |
| --- | --- | --- |
| 0500 Select/MultiSelect | owned | trigger rect (in-handler) |
| 0500 Combobox | owned, `include_anchor_row` | trigger rect |
| 0530 row-action overflow menu | owned | action cell rect |
| 0120 completion dropdown (app-widgets) | passive panel | caret cell (0120's exposure item) |
| 0430 tooltips (extensions, public API) | tooltip | hovered node rect |
| 0430 in-card dropdowns | owned | field rect inside panned card |

## Scope / Non-goals
Scope: the substrate (spec above: three routing modes, placement
engine, dismiss contract, anchor-unmount safety, the `top_z` engine
delta), the three faces, option model, keyboard contract, theming,
a11y roles, tests, and a gallery entry.
Non-goals: async/remote option loading (the app owns its option
signal); grouping/section headers inside the popup (v2 — needs List
row-kind support); a general context-menu widget (the substrate is
deliberately shaped to serve it later, but Menu is a separate future
item); editing the option set in place; multi-region Tab traversal
inside owned popups (v2).

## Expected outcomes
The 0250 workaround class is deleted: a settings row becomes
`Select::new(options).value(sig).view(cx)`. The wizard (0520), form kit
(0510), table filters (0530), and every reference UI get their pickers
from the engine. The anchored-popup recipe is written once and shared
with 0120's completion dropdown.

## Validation
- Unit: highlight-vs-value separation (arrows never write the bound
  signal; Enter commits once — the 0250 regression as a birth test);
  disabled-option skipping; type-ahead prefix jumps; combobox filter
  never commits a non-match; multi-select toggle accumulation.
- CaptureTerm acceptance: open/flip-above-when-cramped/clamp; outside
  press dismisses without acting; Esc restores pre-open value; popup
  inside a Modal layers and focus-restores correctly (focus returns to
  the trigger on close) — plus the STACKED case: modal → second modal
  → select popup opens above both, receives keys, and closing it
  returns key ownership to the second modal (the F1 proof); Combobox
  typing lands in the popup-mounted editor with no visual jump; chips
  overflow to `+N` at narrow widths.
- A11y: popup reports Menu/MenuItem; closed control's `access_value`
  is the current label.

## Status note (2026-07-22): passive-panel slice landed via 0120

The completion dropdown's half of this item SHIPPED with backlog 0120
(the composer wave), in `src/app/anchored.rs`:

- **`Overlays::top_z()`** (checklist 1) is in — additive, one method,
  exactly the spec's shape (src/app/overlays.rs; pinned by
  `top_z_tracks_the_live_maximum` in overlay_tests.rs).
- **The placement engine** (`place_panel`: below-preferred, flip when
  below < needed AND above > below, height = min(content, side),
  Toast-style x clamp, `MatchAnchor`/`Content{min,max}` width) is in
  and unit-pinned — geometry is shared code for the future owned mode.
- **PASSIVE routing mode** is in as `AnchoredPanel::open_passive`: a
  non-modal layer that never takes focus (keys stay with the anchor
  owner), with owner-driven dismissal and the anchor-unmount safety
  contract (opener scope death closes the panel; regression-pinned by
  `opener_scope_death_closes_the_panel`). Naming drift from the sketch,
  same semantics: `PopupAnchor`→`PanelAnchor`, `PopupWidth`→
  `PanelWidth`; `DismissReason`/`on_dismiss` deliberately NOT built —
  they belong to the OWNED mode's surface.
- The 0120 consumer (`Completion`) is live on it: the enumerated
  consumer table's passive-panel row is validated in production shape
  (caret anchor, flip over a bottom composer, damage containment —
  tests/wave_composer.rs).

REMAINING here (this item stays Proposed): the OWNED mode (`Popup`,
modal tree, outside-press dismiss, `include_anchor_row`, stacked-modals
proof), the TOOLTIP mode, and checklist 4-9 (the three faces, option
model, type-ahead, chips, gallery). The spec below is unchanged;
implementers extend `app::anchored` rather than starting fresh.

## Progress checklist (substrate-first — nothing else starts before 1-3)
- [x] 1. `Overlays::top_z()` engine delta (one method; 0170-gated,
      rides the 0.2 budget window) — SHIPPED 2026-07-22 via 0120
- [ ] 2. Substrate spec sign-off by the enumerated consumers (0120
      owner for the passive-panel mode — SIGNED by consumption
      2026-07-22; 0530; extensions/0430 for tooltip + panned-anchor
      cases)
- [ ] 3. Substrate implementation: placement engine (below/flip/clamp,
      width policy — SHIPPED), three routing modes (PASSIVE shipped;
      owned + tooltip remain), dismiss contract (passive half shipped),
      anchor-unmount safety test (SHIPPED), stacked-modals test (owned
      mode — remains)
- [ ] 4. Select (closed) with type-ahead + commit semantics
- [ ] 5. Combobox over TextInput (popup-mounted editor,
      include_anchor_row)
- [ ] 6. MultiSelect with chip rendering (0540 vocabulary)
- [ ] 7. Option model incl. hints + disabled
- [ ] 8. A11y roles + access values
- [ ] 9. Acceptance + regression tests; gallery entry
