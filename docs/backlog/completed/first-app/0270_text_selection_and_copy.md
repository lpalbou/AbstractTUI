# Proposed: Text selection + copy from rendered screens (mouse capture blocks native selection)

## Metadata
- Created: 2026-07-22
- Status: Completed (all three tiers; screen-text scope — see the report)
- Completed: 2026-07-22

## ADR status
- Governing ADRs: damage contract (01) for any render-side highlight; presenter
  byte custody for OSC 52 writes. ADR impact: additive engine feature.

## Context
The maintainer, using `abstractcode-tui` live, tried to select text on the
screen and could not: "i can't select any text of the UI". Root cause is
structural, not an app bug: the engine enables SGR mouse reporting
(DEC 1006/1000-class capture) for wheel scrolling and click routing, and a
terminal in mouse-capture mode routes drag events to the APPLICATION instead
of performing native text selection. Every mouse-capturing TUI has this
problem; the app cannot fix it without engine support (and per the project
rule, apps must not hand-roll engine-grade features).

## What terminals already offer (works today, undocumented)
Most terminal emulators bypass mouse capture with a modifier: Shift+drag
(iTerm2, GNOME/VTE, kitty, WezTerm) or Fn/Option+drag (macOS Terminal.app,
iTerm2 alt-mode) selects natively even while the TUI captures the mouse.
This is the zero-code answer and should at least be DOCUMENTED by the engine
(and surfaced by apps in their help), but it selects RAW SCREEN CELLS
(padding, borders, gutters included) — poor for copying an answer or a code
block.

## Feature request (three tiers, smallest first)
1. **Document the modifier bypass** in the engine's docs (terminal matrix:
   which modifier per emulator) so every app's /help can point at it.
2. **Runtime mouse-capture toggle**: a public `App`/term verb to suspend and
   resume mouse reporting (`set_mouse_reporting(false)`) so apps can offer a
   "selection mode" keybinding: suspend capture → user selects natively →
   any keypress resumes. Cheap (the term layer already tracks moused state
   for teardown), zero rendering work, and honest (native selection quality).
3. **Engine selection + OSC 52 copy** (the real feature): a `Selectable`
   region attribute; mouse-drag paints a selection overlay (theme
   selection_bg per the damage contract), releases copy the UNDERLYING TEXT
   (logical run content, not screen cells — widgets know their text) to the
   system clipboard via OSC 52 (with the documented base64 size caps and a
   `#FALLBACK` when the terminal refuses OSC 52). TextInput already has
   cluster-correct internal selection to model the semantics on.

## Current code reality
- `term/caps.rs` claims `sgr_mouse` for interactive terminals; `term/unix.rs`
  arms/desarms reporting at enter/leave; there is no public suspend verb.
- No selection model outside TextInput (`widgets/input.rs` has
  cluster-indexed selection internally).
- No OSC 52 emitter in the verb set.

## Acceptance sketch
- Tier 2: an app binds a key to `app.set_mouse_reporting(false)`; native
  drag-select works; next key restores; CaptureTerm asserts the arm/disarm
  byte sequences.
- Tier 3: drag over a Markdown answer selects logical lines; OSC 52 payload
  contains the answer text, not border glyphs; wheel scroll still works
  outside selection mode.

## First-app context
Filed from abstractcode-tui (the first AbstractTUI application) after live
maintainer use. The app will document the Shift/Option native bypass in its
help as the interim answer; it deliberately does NOT hand-roll selection.

## Completion report

Completed 2026-07-22 — all three tiers shipped in one wave.

- **Tier 1 (docs)**: modifier-bypass matrix in
  `docs/troubleshooting.md` ("I can't select text with the mouse":
  iTerm2 Option, Terminal.app Fn/Option-rectangle, kitty/WezTerm/VTE/
  Alacritty/Windows Terminal Shift, tmux per the outer terminal), plus a
  second entry on OSC 52 delivery honesty (tmux `set-clipboard`, terminal
  clipboard permissions). Cross-linked from the new `docs/api.md`
  "app::selection" section.
- **Tier 2 (suspend verb)**: `Terminal::set_mouse_reporting(bool)` —
  default is an honest refusal; `UnixTerminal`, `WindowsTerminal`
  (VT-mode console, compile-checked logic only per the platform claims),
  and `testing::CaptureTerm` implement it from their entered options via
  the new `MouseMode::{arm,disarm}_bytes` (options.rs owns both pairs, so
  enter/leave and the verb can never drift). App-code path:
  `app::selection::mouse_capture()` handle (suspend/resume/set_reporting),
  drained by the driver each turn with a labeled notice on refusal;
  `Driver::set_mouse_reporting` is the immediate form for embedders.
- **Tier 3 (engine selection + OSC 52 copy), v1 SCREEN-TEXT scope**: the
  opt-in selection layer (`src/app/selection.rs`): left-drag paints
  `selection_fg`/`selection_bg` over the composed frame as a
  post-flatten patch (old∪new region rects damaged on the root layer
  pre-flatten, so the compositor recomposes truth and the diff emits
  only real changes — damage-contract §1/§3 honest, wide pairs repaired);
  release or Enter/`c`/Ctrl+C copies via OSC 52 through
  `Presenter::external_write` custody (§6); Esc/click clears; wheel and
  all other input route normally. Selection semantics: linear row-flow
  over the rendered screen, wide glyphs never split, per-row trailing
  trim, `\n` joins, both endpoints clamped to the pane under the drag
  anchor (content box of the nearest clipping-or-padded ancestor via
  `UiTree::pane_rect_at`, topmost overlay tree first, else the whole
  tree). Also shipped: `app::selection::copy_to_clipboard` (the
  0150 clipboard leg) and the `examples/feed.rs` demo wiring.
- **DELIBERATE DEFERRAL — logical content selection**: item 3's
  "underlying text (logical run content, not screen cells)" is NOT in
  this slice. What shipped extracts the RENDERED screen (what-you-see);
  widgets' logical text↔cells mapping (copy markdown source, unwrap
  soft-wrapped lines) remains backlog 0160's scope, noted there.
- **Acceptance evidence**: `tests/adv_selection.rs` (10 tests through the
  real `Driver`+`CaptureTerm`+`VtScreen` pipeline: highlight cells carry
  the selection ink, OSC 52 payload matches an independent base64 oracle,
  release/copy-key/Ctrl+C-vs-quit, click clears, pane clamp excludes the
  sibling pane, wheel still scrolls, one-row drag emits <1/4 of a full
  repaint, suspend verb byte pairs + VtScreen mode set, custody copy from
  app code, unadvertised-OSC 52 one-time notice, idle-zero with select
  mode on and with a parked selection) and 14 unit tests in
  `src/app/selection_tests.rs` (row-flow spans, wide-edge extraction,
  trim/blank rows, event-claim rules, paint/damage bookkeeping, pane
  walk). Whole tree green incl. the alloc pins.
