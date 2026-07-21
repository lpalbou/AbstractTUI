//! The splash player: wall-clock paced, frame-dropping, always skippable.
//!
//! Self-contained by design — it runs BEFORE the app loop exists, so it
//! depends on `render` (diff/present), `input` (event classification) and
//! `term` (the byte pipe), never on `app::App`. The 3D mark (GFX3D, cycle
//! 6) and the 2D fallback (`fallback2d`, this cycle) are both just
//! [`SplashFrameSource`]s; the player is the only timeline authority.
//!
//! ## Liveness contract (RT1-10)
//!
//! - **Wall-clock pacing with frame DROP.** The animation position `t` is
//!   computed from the clock, never from a frame counter; after each
//!   present, the player sleeps to the next frame boundary strictly after
//!   *now*. A slow terminal therefore skips ahead — frames are never
//!   queued, and 2 s of splash is 2 s of wall time.
//! - **Skip check between every frame.** The wait between frames drains
//!   input; any deliberate event (key press/repeat, mouse press/wheel,
//!   paste) starts the exit. Capability-probe replies, focus chatter and
//!   unknown sequences are NOT deliberate — the splash starts on env-pass
//!   caps (RT1-6) and probe replies legitimately arrive mid-splash; they
//!   are retained for the app (see [`TerminalIo::finish`]), never treated
//!   as a skip, never dropped.
//! - **Hard wall cutoff.** At `hard_cutoff_ms` (default 2.5 s) the player
//!   returns unconditionally — checked before every render, so even a
//!   fade cannot stretch past it.
//! - **Skip exit = fast fade, second input = cut.** A deliberate event
//!   starts a `fade_ms` (120 ms) opacity ramp toward the theme ground —
//!   cells lerp to `bg`, a cheap post-process at cell scale. A second
//!   deliberate event during the fade cuts immediately. Rationale: the
//!   ramp keeps the exit composed at zero perceptible latency cost, and
//!   the impatient double-tap is instant.
//! - **The gate tests the REAL render handle.** [`skip_reason`] takes the
//!   ttyness of the handle the engine renders to (KERNEL opens `/dev/tty`
//!   even when stdout is a pipe — `isatty(stdout)` is the wrong question);
//!   plus `ABSTRACTTUI_NO_SPLASH`, `NO_COLOR`, `TERM=dumb`.
//!
//! OWNER: DESIGN (GFX3D plugs its frame source in via the same trait).

use std::time::{Duration, Instant};

use crate::base::{Point, Result, Size};
use crate::input::{Event, KeyEventKind, MouseKind, Parser};
use crate::render::{Cell, FrameDiff, PresentCaps, Presenter, Surface};
use crate::term::{Capabilities, TermRead, Terminal};
use crate::theme::Theme;

use super::identity;

/// A source of splash frames. `t` is SECONDS since splash start (the
/// storyboard domain; `identity::SPLASH_TOTAL_MS / 1000.0` is the natural
/// end — sources clamp internally, the player may sample slightly past
/// the end during the exit fade). The returned surface must be exactly
/// `size` cells; the player re-diffs from scratch when the size changes.
pub trait SplashFrameSource {
    fn render(&mut self, t: f32, size: Size, theme: &Theme) -> &Surface;
}

/// What one inter-frame wait observed.
pub enum SplashWait {
    /// Parsed input events (any kind — the player classifies).
    Events(Vec<Event>),
    Resize(Size),
    Idle,
}

/// The player's narrow I/O seam: real terminals adapt via [`TerminalIo`];
/// tests script it. Keeping the seam this small is what makes the pacing
/// logic testable with a virtual clock (RT1-10 test demand).
pub trait SplashIo {
    fn size(&mut self) -> Result<Size>;
    /// Write one frame's bytes and flush (exactly one flush per frame).
    fn present(&mut self, bytes: &[u8]) -> Result<()>;
    /// Wait up to `budget_ms` for input/resize.
    fn wait(&mut self, budget_ms: u64) -> Result<SplashWait>;
}

/// How a splash ended. All variants mean "hand the screen to the app now".
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum SplashOutcome {
    /// Timeline played to the end.
    Completed,
    /// User input ended it early (fade or cut already performed).
    Skipped,
    /// The hard wall-clock ceiling fired (slow terminal / stalled writes).
    CutOff,
    /// Not played at all; the reason is the labeled gate decision.
    SkippedByGate(&'static str),
}

#[derive(Copy, Clone, Debug)]
pub struct SplashOptions {
    /// Target frame cadence. 30 fps is the identity storyboard's design
    /// rate; the pacing degrades by DROPPING, never by stretching.
    pub fps: u32,
    /// Timeline length (identity constant by default).
    pub total_ms: u32,
    /// Unconditional wall ceiling (RT1-10b).
    pub hard_cutoff_ms: u32,
    /// Skip fade length (identity constant by default).
    pub fade_ms: u32,
}

impl Default for SplashOptions {
    fn default() -> Self {
        SplashOptions {
            fps: 30,
            total_ms: identity::SPLASH_TOTAL_MS,
            hard_cutoff_ms: 2500,
            fade_ms: identity::SKIP_FADE_MS,
        }
    }
}

/// Environment gate. Returns the reason to skip, or `None` to play.
/// `render_handle_is_tty` MUST describe the handle frames are written to
/// (RT1-10c) — the caller owns that fact; this function refuses to guess
/// from stdout. `env` is injectable for tests; production callers pass
/// `&|k| std::env::var(k).ok()`.
pub fn skip_reason(
    render_handle_is_tty: bool,
    env: &dyn Fn(&str) -> Option<String>,
) -> Option<&'static str> {
    if !render_handle_is_tty {
        return Some("render handle is not a tty");
    }
    // Set-to-anything-but-"0" disables ("0" opts back in so wrapper
    // scripts can force-enable without unsetting).
    if env(identity::SPLASH_DISABLE_ENV).is_some_and(|v| !v.is_empty() && v != "0") {
        return Some("ABSTRACTTUI_NO_SPLASH is set");
    }
    if env("NO_COLOR").is_some_and(|v| !v.is_empty()) {
        return Some("NO_COLOR is set (a colorless splash is a delay, not an identity)");
    }
    if env("TERM").as_deref() == Some("dumb") {
        return Some("TERM=dumb");
    }
    None
}

/// The production boot gate, one callable: real-tty check
/// (`term::have_tty` — the handle the engine renders to), the disable
/// env (`ABSTRACTTUI_NO_SPLASH`), `NO_COLOR`, `TERM=dumb`, plus the
/// capability report's own dumb verdict (covers `RunConfig`-injected
/// caps that never saw the env). `Ok(())` = play; `Err(reason)` = go
/// straight to the app, reason ready for a log line.
///
/// Ambient by design (process env + tty) — the injectable pieces stay
/// [`skip_reason`] for tests; this is their composition.
pub fn should_splash(caps: &Capabilities) -> std::result::Result<(), &'static str> {
    if let Some(reason) = skip_reason(crate::term::have_tty(), &|k| std::env::var(k).ok()) {
        return Err(reason);
    }
    if caps.dumb {
        return Err("capabilities report a dumb terminal");
    }
    Ok(())
}

/// Deliberate = "the user did something": key press/repeat, mouse
/// press/wheel, paste. Probe replies, focus chatter, releases, motion and
/// unrecognized sequences never skip (RT1-6 composition: replies arrive
/// mid-splash by design).
fn is_deliberate(event: &Event) -> bool {
    match event {
        Event::Key(k) => k.kind != KeyEventKind::Release,
        Event::Mouse(m) => matches!(
            m.kind,
            MouseKind::Down
                | MouseKind::WheelUp
                | MouseKind::WheelDown
                | MouseKind::WheelLeft
                | MouseKind::WheelRight
        ),
        Event::Paste(_) => true,
        _ => false,
    }
}

/// Play a splash. `now_ms` is the injectable monotonic clock (tests use a
/// virtual one); production callers wrap `Instant` (see [`play_fallback`]).
pub fn play(
    io: &mut dyn SplashIo,
    source: &mut dyn SplashFrameSource,
    theme: &Theme,
    caps: &PresentCaps,
    opts: &SplashOptions,
    now_ms: &mut dyn FnMut() -> u64,
) -> Result<SplashOutcome> {
    let frame_ms = (1000 / opts.fps.max(1)).max(1) as u64;
    let total_ms = opts.total_ms as u64;
    let cutoff_ms = opts.hard_cutoff_ms as u64;
    let fade_ms = opts.fade_ms.max(1) as u64;

    let start = now_ms();
    let mut size = io.size()?;
    let mut prev = Surface::new(size, Cell::EMPTY);
    let mut scratch: Option<Surface> = None; // fade post-process buffer
    let mut diff = FrameDiff::new();
    let mut presenter = Presenter::new();
    let mut bytes: Vec<u8> = Vec::with_capacity(16 * 1024);
    let mut skip_at: Option<u64> = None;

    loop {
        let elapsed = now_ms().saturating_sub(start);

        // Ordered exits: the hard ceiling beats everything (RT1-10b), then
        // fade completion, then natural completion.
        if elapsed >= cutoff_ms {
            return Ok(SplashOutcome::CutOff);
        }
        if let Some(s) = skip_at {
            if elapsed.saturating_sub(s) >= fade_ms {
                return Ok(SplashOutcome::Skipped);
            }
        } else if elapsed >= total_ms {
            return Ok(SplashOutcome::Completed);
        }

        // Render at the WALL position — dropped frames are dropped by
        // never having existed.
        let t = elapsed as f32 / 1000.0;
        let frame = source.render(t, size, theme);
        debug_assert_eq!(
            frame.size(),
            size,
            "frame source must honor the requested size"
        );

        let final_frame: &Surface = if let Some(s) = skip_at {
            let k = (elapsed.saturating_sub(s)) as f32 / fade_ms as f32;
            let k = crate::anim::Easing::CubicBezier(
                identity::EASE_FADE[0],
                identity::EASE_FADE[1],
                identity::EASE_FADE[2],
                identity::EASE_FADE[3],
            )
            .eval(k);
            let buf = scratch.get_or_insert_with(|| Surface::new(size, Cell::EMPTY));
            fade_toward_ground(buf, frame, theme, k);
            buf
        } else {
            frame
        };

        bytes.clear();
        let runs = diff.compute_full(&prev, final_frame);
        presenter.emit(runs, final_frame, caps, &mut bytes);
        io.present(&bytes)?;
        // Keep the presented frame for the next diff.
        prev.blit(final_frame, final_frame.bounds(), Point::new(0, 0));

        // Wait to the next frame boundary strictly after now; a write that
        // blew past several boundaries lands on the NEXT one (drop, never
        // queue). Exit deadlines shrink the budget so skip/cutoff are
        // never overslept.
        let after = now_ms().saturating_sub(start);
        let boundary = (after / frame_ms + 1) * frame_ms;
        let mut budget = boundary - after;
        budget = budget.min(cutoff_ms.saturating_sub(after).max(1));
        if let Some(s) = skip_at {
            budget = budget.min((s + fade_ms).saturating_sub(after).max(1));
        }

        match io.wait(budget)? {
            SplashWait::Events(events) => {
                let deliberate = events.iter().any(is_deliberate);
                if deliberate {
                    if skip_at.is_some() {
                        // Second input during the fade: cut now.
                        return Ok(SplashOutcome::Skipped);
                    }
                    skip_at = Some(now_ms().saturating_sub(start));
                }
            }
            SplashWait::Resize(new_size) => {
                if new_size != size && new_size.w > 0 && new_size.h > 0 {
                    size = new_size;
                    prev = Surface::new(size, Cell::EMPTY);
                    scratch = None;
                    presenter.invalidate();
                }
            }
            SplashWait::Idle => {}
        }
    }
}

/// Copy `frame` into `buf` and lerp every cell's colors toward the theme
/// ground by `k` (0 = untouched, 1 = fully ground). Glyphs stay put — text
/// dims into the ground, which reads as an opacity ramp at cell scale.
/// Wide-glyph safety: continuations are skipped; restyling a leader
/// re-mirrors its continuation through the surface's own pairing rules.
fn fade_toward_ground(buf: &mut Surface, frame: &Surface, theme: &Theme, k: f32) {
    if buf.size() != frame.size() {
        *buf = Surface::new(frame.size(), Cell::EMPTY);
    }
    buf.blit(frame, frame.bounds(), Point::new(0, 0));
    let ground = theme.tokens.bg;
    let (w, h) = (buf.width(), buf.height());
    for y in 0..h {
        for x in 0..w {
            let Some(cell) = buf.get(x, y) else { continue };
            if cell.is_continuation() {
                continue;
            }
            let faded = Cell {
                fg: crate::theme::derive::mix(cell.fg, ground, k),
                bg: crate::theme::derive::mix(cell.bg, ground, k),
                ..*cell
            };
            buf.set(x, y, faded);
        }
    }
}

// ---------------------------------------------------------------------------
// Real-terminal adapter
// ---------------------------------------------------------------------------

/// Adapts a real [`Terminal`] to [`SplashIo`], parsing bytes into events
/// and RETAINING everything non-deliberate (capability replies, focus
/// chatter) for the application. The splash must never eat the active
/// probe's answers (RT1-6): call [`TerminalIo::finish`] after `play` and
/// feed the carryover into the app's event path.
pub struct TerminalIo<'a> {
    term: &'a mut dyn Terminal,
    parser: Parser,
    carryover: Vec<Event>,
}

impl<'a> TerminalIo<'a> {
    pub fn new(term: &'a mut dyn Terminal) -> Self {
        TerminalIo {
            term,
            parser: Parser::new(),
            carryover: Vec::new(),
        }
    }

    /// Hand back what the splash swallowed but the app owns: retained
    /// non-deliberate events (probe replies…) and the parser (it may hold
    /// mid-escape state that the app's reader must continue from).
    pub fn finish(self) -> (Vec<Event>, Parser) {
        (self.carryover, self.parser)
    }
}

impl SplashIo for TerminalIo<'_> {
    fn size(&mut self) -> Result<Size> {
        self.term.size()
    }

    fn present(&mut self, bytes: &[u8]) -> Result<()> {
        self.term.write(bytes)?;
        self.term.flush()
    }

    fn wait(&mut self, budget_ms: u64) -> Result<SplashWait> {
        let deadline = Instant::now() + Duration::from_millis(budget_ms);
        match self.term.read(Some(deadline))? {
            TermRead::Input(bytes) => {
                let mut events = Vec::new();
                self.parser.feed(bytes, &mut events);
                // Retain what the app owns; the player only ever needs to
                // see the deliberate ones (and consumes exactly those).
                let mut deliberate = Vec::new();
                for e in events {
                    if is_deliberate(&e) {
                        deliberate.push(e);
                    } else {
                        self.carryover.push(e);
                    }
                }
                if deliberate.is_empty() {
                    Ok(SplashWait::Idle)
                } else {
                    Ok(SplashWait::Events(deliberate))
                }
            }
            TermRead::Resize(size) => Ok(SplashWait::Resize(size)),
            // A cross-thread waker fired; the app loop that would service
            // it doesn't run yet during boot — surface as an idle wakeup
            // (the poster's work is drained when the real loop starts).
            TermRead::Wake => Ok(SplashWait::Idle),
            TermRead::Idle => Ok(SplashWait::Idle),
        }
    }
}

/// Present caps for the splash. Delegates to KERNEL's official
/// `From<&Capabilities>` conversion (landed cycle 2 — the cycle-1
/// stopgap's field-by-field mapping is retired so caps growth stays a
/// two-owner concern, per RENDER's cycle-2 note).
pub fn splash_present_caps(caps: &Capabilities) -> PresentCaps {
    PresentCaps::from(caps)
}

/// Convenience runner over any frame source: gate (RT1-10c ttyness
/// supplied by the caller), real clock, real terminal. Returns the
/// outcome plus the events the splash retained for the app.
pub fn play_source(
    term: &mut dyn Terminal,
    render_handle_is_tty: bool,
    caps: &Capabilities,
    theme: &Theme,
    source: &mut dyn SplashFrameSource,
) -> Result<(SplashOutcome, Vec<Event>)> {
    if let Some(reason) = skip_reason(render_handle_is_tty, &|k| std::env::var(k).ok()) {
        return Ok((SplashOutcome::SkippedByGate(reason), Vec::new()));
    }
    let mut io = TerminalIo::new(term);
    let present = splash_present_caps(caps);
    let t0 = Instant::now();
    let mut clock = move || t0.elapsed().as_millis() as u64;
    let outcome = play(
        &mut io,
        source,
        theme,
        &present,
        &SplashOptions::default(),
        &mut clock,
    )?;
    let (carryover, _parser) = io.finish();
    Ok((outcome, carryover))
}

/// [`play_source`] with the 2D fallback — the no-graphics-budget default.
pub fn play_fallback(
    term: &mut dyn Terminal,
    render_handle_is_tty: bool,
    caps: &Capabilities,
    theme: &Theme,
) -> Result<(SplashOutcome, Vec<Event>)> {
    let mut source = super::fallback2d::FallbackSplash::new();
    play_source(term, render_handle_is_tty, caps, theme, &mut source)
}

// Tests live in a sibling file to keep this one within the size
// budget; they are ordinary unit tests of the public API plus the
// virtual-clock pacing scripts (RT1-10 demand).
#[cfg(test)]
#[path = "player_tests.rs"]
mod tests;
