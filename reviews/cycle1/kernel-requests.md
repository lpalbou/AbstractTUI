# KERNEL cycle 1 — requests to other owners

## Build-state note (at KERNEL handoff)

Mid-cycle the shared tree transiently failed to compile because
`src/gfx/mod.rs` declared modules whose files were still being written
(parallel wave timing, not a defect); kernel modules were validated in an
isolated copy with gfx stubbed during that window. By handoff the shared
tree was whole again and re-validated directly: `cargo check` clean both
native and `--target x86_64-pc-windows-msvc`, `cargo test --lib -- term::
input::` = 71/71 green, zero warnings in `src/term/**` or `src/input/**`
on either target (the two crate warnings live in `src/reactive`).

## To REACT (app runtime)

1. **Install the panic hook.** The terminal restore guarantee has three
   layers; the third needs the app layer: call `term::emergency_restore()`
   from a panic hook installed at app startup (both platforms export it).
   Without it, a panic on a stack that does not unwind through the
   `Terminal` leaves raw mode + altscreen active. Suggested shape:

   ```rust
   let prev = std::panic::take_hook();
   std::panic::set_hook(Box::new(move |info| {
       abstracttui::term::emergency_restore();
       prev(info);
   }));
   ```

2. **Use `input::EventReader::poll_event` as the single input entry.** It
   owns the ESC-disambiguation deadlines (bare ESC ~30 ms, torn sequence
   ~500 ms), resize passthrough, and internal queueing. Calling
   `Terminal::read` directly bypasses deadline handling and will make Esc
   feel dead.

3. **Ctrl+C policy is yours.** Raw mode clears ISIG: Ctrl+C arrives as
   `Key(Char('c'), CTRL)`, never a signal. Decide the default quit binding
   at the app layer.

## To RENDER (presenter)

1. **OPOST is off in raw mode**: emit `\r\n`, never bare `\n`; the terminal
   does not translate for you.
2. **Do not read capabilities from env yourself** — take a
   `term::Capabilities` (env pass + optional `input::probe_active`).
   `sync_output_2026`, `truecolor`, `colors_256` are the fields you care
   about. Note the active probe can *lower* an env guess (DECRPM says mode
   2026 is permanently reset on some terminals that otherwise look modern).
3. **`Terminal::write` is byte-opaque and buffered** (flush at 64 KiB or on
   `flush()`); one `write` per frame region is fine, end frames with
   `flush()`.

## To GFX3D

1. Gate protocol choice on `Capabilities`: `kitty_graphics`,
   `iterm2_images`, `sixel` — all three are env-conservative and the active
   probe confirms kitty graphics with a real round-trip (unique-id query).
   Under tmux all three are forced false in cycle 1 (no passthrough yet).
2. If you need the terminal's cell size in pixels (`TIOCGWINSZ`
   ws_xpixel/ws_ypixel) for image scaling, say so in your cycle-1 report: I
   deferred exposing it because `base::Size` is cells-only and a pixel-size
   type belongs in `base` (integrator call, below).

## To the integrator

1. **Proposal**: a `base::PixelSize` (or a `cell_pixel: Option<(u16,u16)>`
   on some terminal-info struct) so gfx can scale images without a second
   ioctl path. Not urgent; kitty/iTerm2 can work in cells.
2. File-size budget: all kernel files are under 600 lines (largest:
   `input/parser.rs` at 595 — the state machine proper; its framing tests
   and the CSI parameter grammar live in sibling files, and `unix.rs`'s
   pty tests live in `unix_tests.rs`).

## To REDTEAM

1. The parser's fuzz surface is `input::Parser::feed` + `finish`; the
   invariants it defends are listed at the top of `src/input/parser.rs`
   (no panic, bounded memory, no garbage-to-text leakage). In-module tests
   already do 0..=255 singles and xorshift chunk fuzzing — your job is to
   beat them.
2. Honest weak spots to aim at, from my own review: (a) `CSI 1;5R` is
   decoded as Ctrl+F3, which is wrong if a cursor-position report for row 1
   ever arrives (we never send DSR 6 in cycle 1, so it takes foreign
   traffic to trigger); (b) ESC-at-chunk-boundary timing: a bare ESC
   followed >30 ms later by `[A` emits Esc then Alt+[... wait, no — Esc
   then `[` re-enters Csi and `A` completes it as Up; the misfire window is
   real but narrower: it requires the reader deadline to fire between the
   bytes; (c) the SIGWINCH claim is process-global — two `UnixTerminal`s
   in one process degrade the second to 250 ms poll slices: resize latency,
   not correctness.
3. The windows backend compiles under `x86_64-pc-windows-msvc` but has
   never executed (no Windows host here). Correct-by-inspection claims that
   deserve hostility: surrogate pairing across record batches, the
   `WriteConsoleA`-with-UTF-8-codepage write path, and quick-edit
   interaction with mouse VT sequences.
