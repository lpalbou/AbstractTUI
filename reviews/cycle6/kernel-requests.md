# KERNEL cycle 6 — requests + hardening notes

## The field-add hazard, structurally closed (integrator note + all owners)

Root cause of the `tests/adv_splash.rs` breakage acknowledged: I added
`KeyEvent::keypad` without grepping downstream constructors — and
`tests/`, `benches/`, `examples/` are DOWNSTREAM CRATES of the lib, where
full struct literals break on every field addition. The fix is layered:

1. **Constructors landed** (use these everywhere):
   `KeyEvent::char('x')`, `KeyEvent::key(code)`, chainable
   `.with_mods(..) .with_kind(..) .with_text(..) .on_keypad()`;
   `MouseEvent::new(kind, button, pos, mods)` + `.with_pixel(..)`.
   In-crate construction sites already converted (mouse.rs).
2. **`#[non_exhaustive]` on KeyEvent/MouseEvent: cycle 7, announced now.**
   Tradeoff, stated honestly: it does NOTHING in-crate (src/ literals
   keep compiling), but for downstream crates — which include our own
   tests/benches/examples — it hard-blocks full literals AND
   functional-update syntax and forces `..` in destructuring patterns.
   For input events that is the right posture (fields will keep coming:
   kitty grows). Flipping it THIS cycle would break `tests/adv_splash.rs`
   again the very cycle REDTEAM fixed it — so the flip is deferred one
   cycle with this migration note in advance.
3. **Capabilities/GraphicsCaps stay exhaustive, deliberately**: their
   downstream idiom is `Capabilities { field: true, ..Default::default() }`
   (fixture caps in adv_app/adv_overlay/adv_input), and non_exhaustive
   would BAN downstream FRU — the cure would break the good pattern to
   prevent the bad one. Convention instead: config structs are built by
   FRU over `Default`/`detect_env`, never full literals. (GraphicsCaps in
   `tests/adv_image.rs:41` is a FULL literal today — see REDTEAM item 2.)

## To REDTEAM

1. **adv_splash.rs migration** (whenever convenient this cycle, before
   the cycle-7 non_exhaustive flip): the `key_release()` literal becomes
   `KeyEvent::key(KeyCode::Enter).with_kind(KeyEventKind::Release)` —
   one line, never breaks on field adds again.
2. **adv_image.rs `GraphicsCaps` full literal** (line ~41): switch to
   `GraphicsCaps { kitty_graphics: true, ..Default::default() }` — FRU
   survives field adds (it broke once already in cycle 4 when `wrap`
   landed).
3. `tests/adv_input.rs` and `adv_app.rs` already use `..` patterns and
   FRU respectively — nothing owed there.

## To REACT

1. **poll_many adoption verified** — driver.rs:263 with a reused burst
   buffer is exactly the intended shape. Standing ping closed.
2. **a11y sources audit (your app-level signal)**: the environment offers
   exactly two honest inputs, both already surfaced —
   `Capabilities::no_color` and `Capabilities::dumb`. There is NO
   terminal/env standard for reduced motion or high contrast (OS
   preferences don't cross the pty boundary, and no terminal advertises
   them); details in term-input.md §3.4. Recommendation: your signal
   defaults from `no_color || dumb` for "minimal effects" and is
   otherwise a user setting; the kernel will surface any future real
   convention in one line.
3. `src/boot/player_tests.rs:227` uses FRU over `KeyEvent::new` — that
   pattern survives everything (it is also in-crate); no action, noted
   for completeness. If you prefer the new constructors:
   `KeyEvent::char('x').with_kind(KeyEventKind::Release)`.
4. Cycle-3/4/5 reminder, LAST one from me (it is genuinely two lines):
   `app/driver.rs::present_caps_from` still hardcodes `undercurl: false`
   — `caps.present_caps()` fills undercurl + underline_color from real
   detection. After three cycles the folklore-y bit is that editors on
   kitty-class terminals silently lose curly error underlines.

## To DESIGN

Two `--caps` surfaces shipped, both on `Capabilities` (deliberate home:
a `Terminal` instance holds no capabilities — detection is env + probe
OUTSIDE the terminal object, so a trait method could only lie):

- `caps.summary_line()` — the one-liner: `truecolor, kitty-kbd,
  kitty-gfx, sixel(256), sync, mouse-sgr(+pixels), paste, focus,
  undercurl, osc52, tmux(passthrough)`. Tokens appear only when TRUE;
  degradations show as their own tokens (`dumb`, `no-color`, bare
  `tmux`).
- `caps.summary()` — the multi-line human report with negatives spelled
  out, cell pixel size, notify dialect, versions.

Both test-pinned; format is stable-ish prose — scripts read fields.

## To the integrator

- New public surface: `KeyEvent::{char, key, with_mods, with_kind,
  with_text, on_keypad}`, `MouseEvent::{new, with_pixel}`. No base/deps
  changes.
- Cycle-7 heads-up: `#[non_exhaustive]` lands on `KeyEvent`/`MouseEvent`
  after the two downstream literal migrations above; if the crate ever
  splits, config structs (`Capabilities`) stay exhaustive by design (FRU
  is their contract).
