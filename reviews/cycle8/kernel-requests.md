# KERNEL cycle 8 — API freeze notes + requests

## non_exhaustive DECISION: option (a), executed

`KeyEvent` is now `#[non_exhaustive]` (joining `MouseEvent`, flipped in
cycle 7). Rationale for (a) over (b): a documented-but-unenforced
convention already failed once (the `keypad` field breakage happened
DESPITE the construction contract existing) — the docs cycle should
document a compiler-enforced contract, not a courtesy. Constructors have
existed since cycle 6; the enforcement now matches the documentation.

## To REDTEAM — one line owed, tree red at your file until applied

`tests/adv_splash.rs:138` uses downstream FRU
(`KeyEvent { kind: …, ..KeyEvent::plain(…) }`), which `#[non_exhaustive]`
forbids from downstream crates (tests/ compile as one). The swap:

```rust
vec![Event::Key(KeyEvent::plain(KeyCode::Enter).with_kind(KeyEventKind::Release))]
```

Everything else of yours already conforms (`..` patterns in adv_input,
constructors elsewhere). Filed per the cycle-8 order sheet's option (a)
"file it, they apply"; my modules and the lib itself are green.

## API freeze summary (what the docs cycle documents)

- **Rustdoc coverage**: kernel missing-docs went 122 → 0 (crate-wide
  count was 1195; my share is now zero) and is LOCKED by
  `#![warn(missing_docs)]` at the top of `term/mod.rs` and
  `input/mod.rs` — regressions warn at every build. One deliberate
  `#[allow(missing_docs)]`: `CapsReply`'s variant FIELDS (the variant
  docs carry the wire format; field names are the escape sequences' own
  parameter terms). `cargo doc --no-deps`: zero warnings from kernel
  files.
- **Doctests** (compiled examples): term module (minimal session,
  `no_run`), input module (bytes→events, runs + asserts),
  `EventReader::poll_many` (the app-loop shape, `no_run`),
  `Capabilities::detect_env` (+ `present_caps`/`graphics`/`summary_line`
  in one view), `TerminalWaker` (cross-thread wake, runs + asserts).
- **Error ergonomics pass**: every terminal-acquisition/loop error now
  says what to DO — "run inside a terminal emulator, or use
  testing::CaptureTerm for headless/CI runs" (no-tty), the POLLNVAL
  refusal names it as a reportable emulator quirk + workaround, suspend's
  Unsupported tells the app to hide the Ctrl+Z binding off-unix, EOF says
  the emulator hung up.
- **Trait growth**: `Terminal::degraded()` promoted (defaulted `None`;
  unix overrides) — REACT's dyn-consumer ask; the concrete
  `UnixTerminal::degraded` remains.

## To REACT

`term.degraded()` now works through `Box<dyn Terminal>` — the one
diagnostics line at startup needs no downcast anymore.

## To the integrator / docs cycle — what the docs must warn about

1. **Windows is compile-checked + clippy-clean, never executed** (no host
   in ten cycles): evidence matrix §3.5 is the honest source; docs should
   say "Windows support is best-effort pending first live run" rather
   than implying parity.
2. **Legacy-undecidable key combos** (editor matrix pins them):
   Ctrl+Enter/Shift+Enter/Ctrl+Backspace-vs-Ctrl+H collapse on legacy
   wires; kitty or xterm modifyOtherKeys required — apps degrade
   features, the parser never guesses.
3. **OSC 52 is write-only by design** (clipboard-exfiltration stance) —
   paste is bracketed paste, period.
4. **tmux**: graphics require the user's `allow-passthrough on`; the
   probe verifies per session; images don't reflow across tmux
   scroll/split (unicode-placeholder transport is future work).
5. **summary()/summary_line() are prose for humans** — scripts read
   fields.
6. `suspend()` preconditions (default SIGTSTP disposition, foreground
   group) and its aftermath (damage-all, re-query size, re-apply verbs).
