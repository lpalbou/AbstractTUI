# VERIFY (redteam series) cycle-8 findings

API usability review + platform verification, two cycles from ship. The
deliverables: a newcomer's first-use friction list, the doctest status,
the Windows deep audit, the two-full-stacks integration matrix, the clean
perf table for the docs cycle, and the honest Known Limits list.

## 1. API first-use review (the friction list)

Three small apps in `tests/api_first_use.rs`, written reaching for
`abstracttui::prelude::*` first and reading only doc comments. They now
pass and stay as documentation-accuracy guards. Friction hit, in order:

### RT8-1 (P2, REACT/integrator): interactive widgets are not in the prelude

The prelude re-exports `Badge/Block/Logo/Progress/Separator/Spinner` but
NOT the widgets any real app needs — `List, TextInput, Checkbox, Button,
Image, Table, Tabs, Scroll`. Every one of the three apps' first `use`
line is `use abstracttui::widgets::{...}`. A newcomer following "just
`use prelude::*`" cannot build a list, a form, or an image app without
discovering the `widgets` module. Demand: add the interactive widgets to
the prelude (or a `prelude::widgets`), OR the docs must state plainly
that the prelude is core-only and interactive widgets live in `widgets`.

### RT8-2 (P2, integrator): the headless/test drive path is undiscoverable from the prelude

`App::run` (real tty) and `App::simple` (sugar) are documented, but
TESTING your app headlessly needs `app::Driver` + `RunConfig` +
`testing::CaptureTerm` — none in the prelude, and the "how do I test a
component" story isn't in the `App` docs. A newcomer must reverse-engineer
the `Driver::new(&mut app, &mut term, cfg)` + `turn()` loop. Demand: a
`testing::headless` helper (or a doc example on `App`/`Driver`) showing
the mount→drive→assert loop; it is the single most common thing a user
writes after their first app.

### RT8-3 (P3, GFX3D/REACT): `Image::element(t)` / `Block::element(t)` signature differs from the cell widgets

`List/TextInput/Checkbox/Button::element(cx, t)` take `(Scope, &TokenSet)`;
`Image::element(t)` and `Block::element(t)` take only `&TokenSet`. A
newcomer types `.element(cx, t)` uniformly and hits a compile error on
the first image/block. Not wrong (those widgets need no scope), but the
inconsistency is a papercut. Demand: either accept an ignored `_cx` for
uniformity, or document the split prominently.

### RT8-4 (P3, GFX3D): `Bitmap` for `Image::from_bitmap` isn't re-exported near `Image`

An image app needs `gfx::bitmap::Bitmap` wrapped in `Arc`. Neither
`Bitmap` nor the `Arc` requirement is visible from the `Image` widget's
own module surface a newcomer reads first. Demand: re-export `Bitmap`
from `widgets` (beside `Image`) or the prelude, and show the `Arc` in the
`Image::from_bitmap` doc example.

### RT8-7 (P2, REACT): sizing a `Block` with `.style()` on its Element clobbers the border inset

THE most damaging first-use trap found. A newcomer makes a panel fill
its parent by calling `.style(LayoutStyle::default().grow(1.0))` on the
Element returned by `Block::element(t)`. That REPLACES the padding
`Block::element` installs to make room for the border (`Edges::all(1)`),
so the child renders on TOP of the border and title — the panel's title
collapses to a couple of leading characters and the frame corners vanish.
This looked at first like an "Image overdraws the panel" bug (I filed it
as RT8-8), but the BASELINE text-in-a-panel reproduced it identically —
it is `.style()`-clobbers-inset, not image-specific. The correct API is
`Block::layout(style)` (the widget's own builder, which merges with the
inset). Demand: either make `Block` remember its inset when a later
`.style()` sets grow/size (merge, don't replace), or document loudly that
Block sizing goes through `.layout()`, never `.style()` on the result.
This WILL bite every newcomer who wants a full-screen panel.

### RT8-6 (P3, REACT/docs): unsized flex children collapse; side-by-side panes need `grow`

`Element::row()` with two children and no sizes collapses/contends
because nothing shares the main axis; a newcomer's first two-panel layout
renders one pane at full content width and the other at ~0. The fix
(`grow(1.0)` on each) is correct flexbox, but the failure mode (silent
collapse, not an error) is opaque. Recorded so the docs' first
multi-pane example leads with `grow`.

### RT8-5 (P3, expected): no built-in form-validation helper

The themed form composes validation by hand (a signal + a dyn_view). No
`Form`/validation abstraction exists. Expected for a v1 toolkit —
recorded so the docs set the expectation rather than a user hunting for a
`Form` type.

Verdict: the CORE reactive/layout API reads cleanly; the friction is all
at the "assemble real widgets into a screen" seam — prelude coverage
(RT8-1/2/4), the `.style()`-vs-`.layout()` inset trap (RT8-7, the one
real bug-shaped item), and signature consistency (RT8-3). None blocks
shipping; RT8-7 and RT8-1 most deserve a fix or a prominent doc note
before the docs cycle.

## 2. Doctest sweep

`cargo test --doc`: **27 passed / 0 failed / 24 ignored**. Green — nothing
to file to owners. BUT the 24 ignored are ```ignore-fenced (NOT `no_run`),
so they are never even COMPILE-checked and can rot silently — mostly
widget doc examples (`radio, richtext, separator, spinner, viewport3d,
table, tabs, scroll, markdown, code`, …). RT8-8 (P3, all widget owners):
convert `ignore` doc fences to `no_run` wherever the example is real code
(keeps them compiling against the frozen API); reserve `ignore` for
genuinely non-code snippets. Otherwise the docs cycle ships examples that
were never checked against the final API.

## 3. Windows deep audit

- **clippy `--target x86_64-pc-windows-msvc --all-targets`**: 36 warnings,
  ZERO in `src/term/windows*.rs`, ZERO in VERIFY files after this cycle's
  fixes. The remainder are the same cross-platform builder warnings the
  default target carries (widgets/list, boot/identity, ui/compose,
  type_complexity). Not a clean-zero gate crate-wide, but the WINDOWS
  code itself is clippy-clean.
- **One windows-target compile break FIXED (mine)**: KERNEL made
  `input::KeyEvent` `#[non_exhaustive]` this cycle (the freeze). My
  `adv_splash.rs` built a release-kind event via a struct literal with
  `..base` — forbidden on a non_exhaustive struct from outside the crate
  (E0639), so it broke on BOTH targets. Rebuilt via the
  `KeyEvent::plain(..).with_kind(KeyEventKind::Release)` builder — exactly
  the field-addition-survival path the constructors exist for. (This also
  validates KERNEL's non_exhaustive decision: it immediately caught a
  struct-literal construction site.)
- **Static review of `windows.rs` (556 lines) against the documented
  Win32 console semantics** — verdict SOUND on all three load-bearing
  mechanisms:
  - *wait/latch* (`read`, lines 407-471): the `wake_pending` latch is
    correct. The auto-reset event resets when a wait consumes it, so the
    field is the durable memory of a wake that lost the same wakeup to
    input; the `WAIT_WAKE if self.wake_event.is_some()` guard is right
    (with no event, only `hin` is waited and index 1 cannot occur). Input
    is drained before the wake is latched, preserving input-before-wake
    ordering.
  - *resize re-query* (`check_resize_query` + the `WINDOW_BUFFER_SIZE`
    arm): geometry is re-queried on EVERY wait pass and on the record,
    deduped via `seen_size`, using the visible window (not `dwSize`
    scrollback) — mirrors the unix ioctl posture; RT1-12a (missed/
    coalesced conhost records) is handled.
  - *surrogate pairing* (`push_utf16_unit`): high+low paired across
    record boundaries via `pending_high`; a lone high emits U+FFFD then
    processes the current unit; a lone low emits U+FFFD. No panic, no
    silent drop. Correct.
- **RT8-9 (P2, KERNEL): the surrogate-pairing and wake-latch logic have
  ZERO unit tests reachable off-Windows.** Both are methods on the
  `#[cfg(windows)]` `WindowsTerminal`, so on this (and CI's) host they are
  compile-checked only, never executed. The logic is platform-INDEPENDENT
  (u16→UTF-8 pairing; a boolean latch decision). Demand: extract
  `push_utf16_unit`'s surrogate state machine and the wake/resize/input
  precedence decision into free functions (or a tiny `#[cfg(all())]`
  helper struct) that unit tests run on ANY host. A latch/pairing bug is
  exactly the class that a compile-only path hides until a real Windows
  user hits it.

## 4. Integration matrix

`tests/integration_matrix.rs` (4 tests, all green):
- **Two full App+Driver+CaptureTerm stacks sequentially** in one process
  render their OWN content with no bleed; a THIRD run with the first's
  seed is byte-identical to the first — proving no reactive thread-local,
  static pool, or id-counter state leaks across teardown.
- **Ten stacks back to back** do not drift from the baseline frame or
  panic (a slow leak would surface by the 10th).
- **ImageSession reuse-after-drop**: a dropped session leaves no live
  image ids; a fresh session in the same process starts clean (no id
  pollution, no protocol violations).
- **Overlay world reuse across three Apps**: each App gets a fresh,
  working overlay world; teardown of one doesn't corrupt the next.

No teardown-leak findings — the stack is re-instantiable in-process.

## 5. THE perf table (for the docs cycle)

Methodology: release build, `cargo test --release --test perf_budgets --
--ignored`; median of N runs × M iters per line (the harness prints
best/worst too). Taken at host load average ~17 (a shared box; see the
cycle-7 note that a load ~99 spike inflates medians 3-10×). These are the
numbers to cite; budgets carry deliberate slack for scheduler jitter.

| Metric | Median | Budget | Notes |
| --- | --- | --- | --- |
| diff+present 200×60 full-change | 464 µs | 2 ms | worst realistic animated redraw |
| keystroke→frame via Driver::turn | 45 µs | 3 ms | input→painted-frame latency |
| flatten+diff+present 200×60 + cell shader | 1.96 ms | 3 ms | one active Shimmer shader |
| brandmark 3D frame 100×30 | 452 µs | 8 ms | boot splash mark |
| splash 2D fallback frame 100×30 | 102 µs | 2 ms | |
| grid solve, 12 cols × 480 children | 112 µs | 3 ms | layout hot path |
| markdown parse + rich, 1000-line doc | 2.25 ms | 20 ms | |
| richtext wrap, 800-para doc @ 60 cols | 12.5 ms | 20 ms | per-resize cost |
| parser, 1 MB hostile soup | 21.9 ms | 50 ms | input robustness |
| VT model referee, 200×60 styled frame | 1.92 ms | 3 ms | rig overhead (RT6-4) |
| pool churn, 100k unique clusters | 515 ms wall | (cap contract) | interner stress |

Scroll-opt byte wins (referee-verified): log-append **7.8×**
(69,546→8,948 B), list up+down **9.0×** (72,316→8,031 B), banded
fixed-chrome **8.1×** (49,442→6,073 B).

Soak: 10,000 frames, allocation plateau FLAT across ten 1,000-frame
windows (`[163k,169k,170k,168k,170k,177k,176k,173k,176k,173k]` — last
within 2% of the back-half median). No leak, no drift, terminal restored.

## 6. RT ledger — final status (all findings)

| Finding | Sev | Owner | Status |
| --- | --- | --- | --- |
| RT3-1..3-4 | P1/P2 | GFX3D/REACT | CLOSED |
| RT4-1 image lifecycle | P3 | GFX3D+REACT | CLOSED |
| RT4-2 clippy | P3 | all | CARRIED — default `--all-targets` ~30-43 (churns with new surface); ZERO in VERIFY/windows files |
| RT4-3 windows target | P2 | KERNEL | GREEN (compiles; windows code clippy-clean) |
| RT5-1 keyboard-dead | P0 | KERNEL | CLOSED (c7) |
| RT5-2 JPEG SOS selector unvalidated | P3 | GFX3D | OPEN → **Known Limit** |
| RT6-1 animation NaN panic | P2 | GFX3D | CLOSED (c7) |
| RT6-2 no animated GLB in test set | P3 | GFX3D/DESIGN | OPEN → **Known Limit** |
| RT6-3 shader perf | P2 | RENDER | CLOSED (c7) |
| RT6-4 VT referee self-budget | P3 | VERIFY | RESOLVED (c7) |
| RT7-1 tree-un-buildable stretches | P2 | all | PROCESS (cycle-7); recurred this cycle — see Known Limits |
| RT7-2 VtScreen interner | P3 | VERIFY | DEFERRED (not needed) |
| RT8-1 interactive widgets not in prelude | P2 | REACT/integrator | OPEN |
| RT8-2 headless drive path undiscoverable | P2 | integrator | OPEN |
| RT8-3 element() signature inconsistency | P3 | GFX3D/REACT | OPEN |
| RT8-4 Bitmap not re-exported near Image | P3 | GFX3D | OPEN |
| RT8-5 no form-validation helper | P3 | — | Known Limit (by design) |
| RT8-6 unsized flex children collapse | P3 | REACT/docs | OPEN (docs) |
| RT8-7 Block `.style()` clobbers border inset | P2 | REACT | OPEN (fix or doc loudly) |
| RT8-8 ignored doctests not compile-checked | P3 | widget owners | OPEN |
| RT8-9 windows surrogate/latch logic untested off-Windows | P2 | KERNEL | OPEN |

## Known Limits list (for the docs cycle — the honest list)

These are OPEN and unlikely to be fully fixed by ship; the docs must
state them so users aren't surprised:

1. **Windows is compile-verified, not run-verified.** The console backend
   is clippy-clean and statically audited, but has never executed on a
   live Windows host in this project; its surrogate-pairing and wake-latch
   logic have no off-Windows unit tests (RT8-9). Treat Windows as
   best-effort until first-run verification.
2. **The prelude is core-only.** Interactive widgets (`List`, `TextInput`,
   `Checkbox`, `Button`, `Image`, `Table`, `Tabs`, `Scroll`) and the
   headless drive path (`Driver`, `CaptureTerm`) are NOT in the prelude
   (RT8-1/2). Document the `use abstracttui::widgets::*` requirement.
3. **Panel sizing goes through `Block::layout()`, not `.style()`**
   (RT8-7): `.style()` on a Block's Element drops content onto the
   border. Until fixed, this must be a prominent doc warning.
4. **Side-by-side/unsized flex children need explicit `grow`** (RT8-6):
   they collapse silently otherwise.
5. **No form/validation abstraction** (RT8-5): compose by hand.
6. **JPEG scan component selectors are not validated against SOF ids**
   (RT5-2): harmless for single-scan baseline files; a malformed selector
   is silently accepted.
7. **No animated-GLB asset in the test set** (RT6-2): the glTF animation
   sampler is unit-tested directly, but the full load→rig→pose path has
   no live subject in-repo.
8. **Sixel is single-shared-palette** (RT1-11, standing): prefer one live
   sixel image per screen.
9. **Perf numbers are load-sensitive** (methodology note above): cite the
   quiet-box table; medians inflate several-fold under host contention.

## State
- Full default suite: **1,227 passed / 0 failed / 72 ignored**.
- `cargo test --no-run`: clean across ALL binaries (both targets).
- `cargo test --doc`: 27 passed / 0 failed.
- `cargo check` + windows clippy: green; windows code clippy-clean.
- Zero clippy warnings in VERIFY-owned or windows files.
