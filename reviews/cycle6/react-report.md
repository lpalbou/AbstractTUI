# REACT cycle-6 report

Shipped: the grid layout (solver + widget), the shareable-component
pattern with `Callback<T>` and the tested Card example, the semantic
(a11y) model with text serialization + focus announcements + the
focus-visible hook, flex wrap + `Overflow` semantics, keymap help,
KERNEL's KeyEvent constructors adopted, plus two new form widgets
(Checkbox, RadioGroup) that exercise the a11y defaults. Verified at
close: `cargo test --no-run` clean, lib **856 passed / 0 failed**,
examples compile, ZERO clippy warnings in owned files.

## 1. Grid (layout/grid.rs + widgets::Grid)

Solver-level `Display::Grid { cols, rows }` with the full track
vocabulary: `Cells` (fixed), `Percent` (of content extent), `Auto`
(fits the largest intrinsic size of children starting in the track),
`Fr` (leftover shares, largest-remainder — fr tracks tile EXACTLY,
property-tested over 300 random extent/gap/track mixes). Row-major
auto-placement with `col_span`/`row_span` via a first-fit occupancy
scan; explicit rows first, implicit rows are Auto; `gap` = column gap,
`cross_gap` = row gap. Cell alignment: Stretch fills the cell; explicit
sizes / non-Stretch `align_self` size to content and align inside the
cell (one alignment for both axes — the `justify_self` split is a
recorded later decision, doc §17). `widgets::Grid` is the thin
container builder (`Grid::new(cols, rows).gap(1).child(..).element()`),
in the lint SOURCES list. Tests: fr tiling property, spans + gaps
placement, fr rows, row-span displacement, Auto tracks via the
accessibility snapshot's solved bounds.

Also landed on the layout side (same wave): **flex wrap**
(`Style::wrap()`, lines break on flex BASES, each line distributes
grow/shrink independently, Stretch fills the LINE; property-pinned
no-overlap/left-aligned/per-line tiling) and **Overflow semantics**
(`Visible/Clip/Scroll` replacing the clip bool — `Scroll` is the
clip-plus-hint the scroll widget now sets).

## 2. Shareable components (ui::compose) — the React-shaped contract

The pattern is a CONVENTION over plain Rust, formalized and documented
in `ui::compose`: a component is `fn(Scope, Props) -> View`; props are
a caller-built struct of data fields + `Callback<T>` event fields +
`View` slot fields (children). State = signals on the passed scope;
updates = `dyn_view` INSIDE the component (no VDOM, no parent
re-render). `Callback<T>`: clone-cheap (`Rc<RefCell<FnMut>>`, clones
share ONE callback), `Default = noop` for optional events, debug-panic
on self-reentrancy (release: skip).

The Card example from the docs is TESTED
(`card_component_composes_twice_with_props_events_and_slots`): a
`CardProps { title, on_close: Callback<()>, children: View }` component
defined once, instantiated twice with different props/slots; clicks
route to the RIGHT instance's callback; the semantic tree shows both
regions by label. This is the "define in one module, compose in
another" proof.

## 3. Accessibility infrastructure (honestly labeled)

NOT a screen-reader bridge — the in-engine model one would read
(doc §19 says so in the first sentence):

- `Element::role/access_label/access_value` + `Role` vocabulary
  (button/checkbox/radiogroup/input/textarea/list/listitem/table/cell/
  tabs/tab/dialog/menu/menuitem/scrollarea/heading/region/text).
- Widget defaults shipped: Button (role+label, "disabled" value),
  TextInput (role, placeholder label, LIVE value), List/Table
  (count + selection values), Tabs (active title), Checkbox (on/off),
  RadioGroup (selected item), Scroll (scrollarea), Modal (dialog).
- `UiTree::accessibility_tree()` / `a11y_tree()`: preorder snapshot of
  annotated nodes + text leaves with role/label/value/focus/bounds;
  unannotated containers flatten out; the focus mark lands on the
  focused node's nearest ANNOTATED self-or-ancestor.
  `accessibility_tree_text()` is the `--a11y`-style dump REDTEAM can
  assert against; `focus_announcement()` derives the "what got focus"
  line (`input "query" = "teapots"`).
- FOCUS-VISIBLE guarantee: `ui::focus_affordance_visible(&mut tree)`
  draws with/without focus and demands a visible difference (glyph,
  colors, or ATTRS) inside the focused rect. Pinned for Button +
  TextInput; the hook is public for every widget suite and REDTEAM.
- Keyboard audit outcome: one real gap found and FIXED — Table sorting
  was click-only; `s` now requests sort round-robin from the sorted
  column (same `on_sort_requested` contract as a header click; test
  pinned). Everything else in the interactive set was already
  keyboard-complete (Tab/arrows/Enter/Space/Escape), including the two
  new widgets (RadioGroup is ONE tab stop with arrow selection, per
  HIG).

## 4. Keymap help

`Element::shortcut_labeled(chord, label, f)` (labels on the existing
shortcut table), `UiTree::keymap_of_focus_path()` (focused node +
ancestors, the §12a resolution order), `KeyChord::display()`
("Ctrl+Shift+S"), and `app::KeymapHelp::open(..)` — a Modal listing
path shortcuts + all `app::actions` chords, Esc closes (via the new
`Modal::share()` handle; `Modal::close` is `&self` + idempotent now).
App-side placement per the R4-1 layer rule (rides Modal/overlays).

## 5. KERNEL constructors + misc

- `KeyEvent::char(..)` / `.with_mods(..)` adopted at every
  `input::KeyEvent` construction site I own (app/events.rs tests) —
  future field additions stop breaking them.
- Checkbox + RadioGroup shipped (builder props, tokens-only — both in
  the lint SOURCES — doc snippets, itest_util tests: click/Space
  toggle, disabled inertness, one-tab-stop arrow selection).
- Prelude: `Callback` + `Role` added (hello.rs remains 54 lines).

## 6. Perf + risks

- The a11y snapshot is PULL-ONLY (built on demand, no per-frame cost);
  `access_value` closures run untracked at snapshot time.
- New REDTEAM surface pre-named in doc §20 risks 9–12: grid first-fit
  scan complexity, the Auto+span start-track approximation,
  `access_value` closures over foreign disposed signals, and the
  focus-visible hook's batch-flush reliance (a widget deferring focus
  visuals through timers would false-negative — and should be caught).
- Foreign at close: GFX3D's `three` wave broke/unbroke the crate
  several times mid-cycle (their files); at my close everything
  compiles and 856/0 stands.
