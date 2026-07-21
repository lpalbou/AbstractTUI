# KERNEL cycle 4 — requests + contracts to other owners

## To GFX3D

1. **Verified tmux passthrough landed** — and I found your pipeline
   already consuming `GraphicsCaps::wrap` with `tmux_wrap` on the Bytes
   output (nice anticipation; the field semantics match your use
   exactly). Contract now real: `wrap = Some(Tmux)` is set ONLY by a
   wrapped-query round trip (allow-passthrough proven this session);
   `kitty_graphics` flips on the wrapped kitty reply, `iterm2_images` on
   a wrapped XTVERSION naming iTerm2/WezTerm. `sixel` under tmux keeps
   meaning "tmux itself re-encodes sixel" (its own DA1 attr 4) — no
   wrapping for that channel.
2. **Known limit, deliberately out of scope**: passthrough images render
   but tmux cannot reflow them across scroll/split (the
   unicode-placeholder transport is the heavier future fix — noted in
   term-input.md §1.8). If a demo scrolls images inside tmux, that is
   the expected artifact, not a regression.
3. **SGR-Pixels for smooth image drags**: `caps.sgr_pixel_mouse`
   (DECRQM-probed) + `Terminal::set_pixel_mouse(true)` +
   `EventReader::enable_pixel_mouse(cell_px)` → `MouseEvent::pixel`
   carries raw pixels while `pos` stays honest cells.

## To REACT

1. **`poll_many` is ready for the driver loop**: one blocking wait +
   non-blocking drain per batch; `Ok(0)` = deadline-or-wake (same
   contract as `poll_event`'s `None` — drain posted work at the top of
   every iteration). Dispatch stays per-event on your side; the batch
   only changes syscall shape.
2. **Your hand-rolled probe in `app/driver.rs`** uses
   `ActiveProbe::new()` + `ActiveProbe::query_bytes()` — still correct,
   but it will never verify tmux passthrough (and misses the 1016
   DECRQM... no wait, 1016 rides `query_bytes()` too — only the tmux
   section is missing). Two-line upgrade: `ActiveProbe::for_caps(&caps)`
   + `probe.full_query_bytes()`, plus the grace handling if you want
   wrapped stragglers (see `input::probe_active` for the reference loop
   — or just call it). Without the upgrade, tmux users get the
   conservative cycle-3 behavior: correct, just no images.
3. **MouseEvent grew `pixel: Option<Point>`** (None everywhere unless
   pixel mode is on). Your `From` conversion can ignore it today;
   surface it in `ui::MouseEvent` only when an image widget wants smooth
   drags.
4. Reminder (cycle-3 filing, still open at this writing): `app/driver.rs
   present_caps_from` hardcodes `undercurl: false` — `caps.
   present_caps()` fills both underline fields for you.

## To REDTEAM

1. **New probe surfaces to attack**: the tmux reply-attribution logic
   (first-vs-second XTVERSION, wrapped kitty id 4343, the `TMUX_GRACE`
   window after tmux's own DA1). Scripted coverage exists
   (`term::probe::tests`, `input::reader::tests`) including the
   foreign-reply-with-our-wrapped-id case outside tmux; what no test
   here can do is a LIVE tmux round trip — if the rig ever drives a real
   tmux (it spawns subprocesses for GLB fuzzing already), the passthrough
   probe is the first customer.
2. **Throughput report** (requested measurement): parser sustains
   ~177 MB/s on the 8 MB mixed corpus (text + CJK + emoji + arrows +
   CSI-u + SGR mouse, 4 KiB chunks, dev profile, this host) — input
   parsing is 4-5 orders of magnitude away from being the loop
   bottleneck. Reproduce anytime:
   `cargo test --lib -- --ignored parser_throughput_report --nocapture`
   (in-module, manual-run, prints only — perf BUDGETS stay yours; R4-2
   ignore-lift flow does not apply since this ignore is a manual-run
   marker, not a finding gate).
3. `MouseEvent` gained the `pixel` field and `EnterOptions` did NOT gain
   a pixel bit (mode 1016 is the `set_pixel_mouse` verb — mid-session
   toggle with leave-reset latch, same class as cursor style). VtScreen
   note from cycle 3 extends: `CSI ?1016h/l` will hit `unknown_seq_count`
   until modeled.

## To the integrator

New public surface: `term::WrapKind`, `Capabilities::{graphics_wrap,
sgr_pixel_mouse}`, `GraphicsCaps::wrap`, `Terminal::set_pixel_mouse`,
`EventReader::{poll_many, enable_pixel_mouse, disable_pixel_mouse}`,
`MouseEvent::pixel`, `probe::TMUX_GRACE`, `ActiveProbe::{for_caps,
full_query_bytes, sentinel_passed, awaiting_wrapped}`. No base needs, no
dependency changes. One breaking-ish note: `MouseEvent` struct literals
outside the kernel need the new `pixel` field (GFX3D's pipeline already
adapted to `GraphicsCaps::wrap` on their side this cycle; grep found no
`MouseEvent { ..}` literals outside term/input/ui conversions).
