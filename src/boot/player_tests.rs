//! Unit tests for the splash player, split out to keep `player.rs`
//! within the file-size budget; ordinary tests of the public API
//! plus the virtual-clock pacing scripts (RT1-10).

use super::*;
use crate::base::Rect;
use crate::input::{KeyCode, KeyEvent, Mods};
use crate::theme::default_theme;
use std::cell::RefCell;
use std::rc::Rc;

/// Scripted clock: each call returns the next value (repeating the
/// last forever). One call site per loop phase keeps scripts readable.
struct VClock {
    times: Vec<u64>,
    i: usize,
}
impl VClock {
    fn new(times: Vec<u64>) -> Self {
        VClock { times, i: 0 }
    }
    fn tick(&mut self) -> u64 {
        let v = *self.times.get(self.i).or(self.times.last()).unwrap_or(&0);
        self.i += 1;
        v
    }
}

/// Frame source that records every `t` it was asked to render.
struct RecordingSource {
    surface: Surface,
    ts: Rc<RefCell<Vec<f32>>>,
}
impl RecordingSource {
    fn new(size: Size, ts: Rc<RefCell<Vec<f32>>>) -> Self {
        RecordingSource {
            surface: Surface::new(size, Cell::EMPTY),
            ts,
        }
    }
}
impl SplashFrameSource for RecordingSource {
    fn render(&mut self, t: f32, size: Size, _theme: &Theme) -> &Surface {
        if self.surface.size() != size {
            self.surface = Surface::new(size, Cell::EMPTY);
        }
        self.ts.borrow_mut().push(t);
        &self.surface
    }
}

/// Scripted io: fixed size, records presents, pops scripted waits.
struct ScriptIo {
    size: Size,
    presents: usize,
    waits: Vec<SplashWait>, // popped front-first
}
impl ScriptIo {
    fn new(size: Size, waits: Vec<SplashWait>) -> Self {
        ScriptIo {
            size,
            presents: 0,
            waits,
        }
    }
}
impl SplashIo for ScriptIo {
    fn size(&mut self) -> Result<Size> {
        Ok(self.size)
    }
    fn present(&mut self, _bytes: &[u8]) -> Result<()> {
        self.presents += 1;
        Ok(())
    }
    fn wait(&mut self, _budget_ms: u64) -> Result<SplashWait> {
        if self.waits.is_empty() {
            Ok(SplashWait::Idle)
        } else {
            Ok(self.waits.remove(0))
        }
    }
}

fn key(ch: char) -> Event {
    Event::Key(KeyEvent::new(KeyCode::Char(ch), Mods::NONE))
}

fn opts() -> SplashOptions {
    SplashOptions {
        fps: 25,
        total_ms: 2000,
        hard_cutoff_ms: 2500,
        fade_ms: 120,
    }
}

#[test]
fn wall_clock_pacing_drops_frames_when_the_clock_jumps() {
    // The clock leaps 500 ms per loop (a terminal absorbing writes
    // slowly). 2000 ms at 25 fps would be 50 queued frames; dropping
    // means one render per leap: t values track the CLOCK.
    let ts = Rc::new(RefCell::new(Vec::new()));
    let mut source = RecordingSource::new(Size::new(20, 6), ts.clone());
    let mut io = ScriptIo::new(Size::new(20, 6), vec![]);
    // Each loop reads the clock twice (render position + wait budget).
    let mut clock = VClock::new(vec![0, 0, 40, 500, 540, 1000, 1040, 1500, 1540, 2000]);
    let mut now = move || clock.tick();
    let out = play(
        &mut io,
        &mut source,
        default_theme(),
        &PresentCaps::FULL,
        &opts(),
        &mut now,
    )
    .expect("play");
    assert_eq!(out, SplashOutcome::Completed);
    let ts = ts.borrow();
    assert_eq!(io.presents, ts.len(), "one present per rendered frame");
    assert!(
        ts.len() <= 5,
        "dropped pacing must render ~one frame per clock leap, got {ts:?}"
    );
    // Rendered positions follow the wall clock, not a frame counter.
    assert!((ts[0] - 0.0).abs() < 1e-3);
    assert!(
        ts.windows(2).all(|w| w[1] > w[0]),
        "t must be monotonic: {ts:?}"
    );
    assert!(
        (ts.last().unwrap() - 1.5).abs() < 0.06,
        "last frame near 1.5 s: {ts:?}"
    );
}

#[test]
fn deliberate_event_skips_with_fade_then_ends() {
    let ts = Rc::new(RefCell::new(Vec::new()));
    let mut source = RecordingSource::new(Size::new(20, 6), ts.clone());
    // First wait delivers a keypress; later waits are idle.
    let mut io = ScriptIo::new(Size::new(20, 6), vec![SplashWait::Events(vec![key(' ')])]);
    // Loop phases: render@0, wait-budget@40 (key arrives, skip_at=80),
    // then fade frames at 100/140, fade deadline passes at 210.
    let mut clock = VClock::new(vec![0, 0, 40, 80, 100, 140, 160, 180, 210]);
    let mut now = move || clock.tick();
    let out = play(
        &mut io,
        &mut source,
        default_theme(),
        &PresentCaps::FULL,
        &opts(),
        &mut now,
    )
    .expect("play");
    assert_eq!(out, SplashOutcome::Skipped);
    assert!(
        io.presents >= 2,
        "the fade must present at least one post-skip frame (got {})",
        io.presents
    );
    // Skip ended the run long before the 2 s timeline.
    assert!(ts.borrow().iter().all(|t| *t < 0.5), "{:?}", ts.borrow());
}

#[test]
fn second_deliberate_event_cuts_the_fade_immediately() {
    let ts = Rc::new(RefCell::new(Vec::new()));
    let mut source = RecordingSource::new(Size::new(20, 6), ts.clone());
    let mut io = ScriptIo::new(
        Size::new(20, 6),
        vec![
            SplashWait::Events(vec![key(' ')]),
            SplashWait::Events(vec![key(' ')]),
        ],
    );
    let mut clock = VClock::new(vec![0, 0, 40, 60, 70, 90]);
    let mut now = move || clock.tick();
    let out = play(
        &mut io,
        &mut source,
        default_theme(),
        &PresentCaps::FULL,
        &opts(),
        &mut now,
    )
    .expect("play");
    assert_eq!(out, SplashOutcome::Skipped);
    // Two renders max: the initial frame and one fade frame — the
    // second keypress exits without waiting out the fade.
    assert!(
        io.presents <= 2,
        "cut must not keep fading (presents={})",
        io.presents
    );
}

#[test]
fn hard_cutoff_fires_unconditionally() {
    let ts = Rc::new(RefCell::new(Vec::new()));
    let mut source = RecordingSource::new(Size::new(20, 6), ts.clone());
    let mut io = ScriptIo::new(Size::new(20, 6), vec![]);
    // A terminal so slow the clock blows straight past everything;
    // cutoff (2500) must win even though the timeline (2000) is also
    // past due — the ceiling is checked first (RT1-10b).
    let mut clock = VClock::new(vec![0, 0, 900, 1800, 1900, 2700]);
    let mut now = move || clock.tick();
    let out = play(
        &mut io,
        &mut source,
        default_theme(),
        &PresentCaps::FULL,
        &opts(),
        &mut now,
    )
    .expect("play");
    assert_eq!(out, SplashOutcome::CutOff);
}

#[test]
fn probe_replies_and_focus_never_skip() {
    let ts = Rc::new(RefCell::new(Vec::new()));
    let mut source = RecordingSource::new(Size::new(20, 6), ts.clone());
    let mut io = ScriptIo::new(
        Size::new(20, 6),
        vec![
            SplashWait::Events(vec![Event::FocusGained]),
            SplashWait::Events(vec![Event::Key(KeyEvent {
                kind: KeyEventKind::Release,
                ..KeyEvent::new(KeyCode::Char('x'), Mods::NONE)
            })]),
        ],
    );
    let mut clock = VClock::new(vec![0, 0, 40, 500, 540, 1000, 1040, 1500, 1540, 2000]);
    let mut now = move || clock.tick();
    let out = play(
        &mut io,
        &mut source,
        default_theme(),
        &PresentCaps::FULL,
        &opts(),
        &mut now,
    )
    .expect("play");
    assert_eq!(
        out,
        SplashOutcome::Completed,
        "non-deliberate events must not skip"
    );
}

#[test]
fn resize_recreates_the_diff_baseline() {
    let ts = Rc::new(RefCell::new(Vec::new()));
    let mut source = RecordingSource::new(Size::new(20, 6), ts.clone());
    let mut io = ScriptIo::new(Size::new(20, 6), vec![SplashWait::Resize(Size::new(30, 8))]);
    let mut clock = VClock::new(vec![0, 0, 40, 500, 540, 1000, 1040, 1500, 1540, 2000]);
    let mut now = move || clock.tick();
    let out = play(
        &mut io,
        &mut source,
        default_theme(),
        &PresentCaps::FULL,
        &opts(),
        &mut now,
    )
    .expect("play");
    assert_eq!(out, SplashOutcome::Completed);
    // The source was asked for the new size after the resize.
    assert_eq!(source.surface.size(), Size::new(30, 8));
}

#[test]
fn gate_reasons() {
    let none = |_: &str| None::<String>;
    assert_eq!(
        skip_reason(false, &none),
        Some("render handle is not a tty")
    );
    assert!(skip_reason(true, &none).is_none());
    let no_splash = |k: &str| (k == identity::SPLASH_DISABLE_ENV).then(|| "1".to_string());
    assert!(skip_reason(true, &no_splash).is_some());
    let no_splash_zero = |k: &str| (k == identity::SPLASH_DISABLE_ENV).then(|| "0".to_string());
    assert!(
        skip_reason(true, &no_splash_zero).is_none(),
        "'0' opts back in"
    );
    let no_color = |k: &str| (k == "NO_COLOR").then(|| "1".to_string());
    assert!(skip_reason(true, &no_color).is_some());
    let dumb = |k: &str| (k == "TERM").then(|| "dumb".to_string());
    assert!(skip_reason(true, &dumb).is_some());
    let smart = |k: &str| (k == "TERM").then(|| "xterm-256color".to_string());
    assert!(skip_reason(true, &smart).is_none());
}

#[test]
fn fade_dims_cells_toward_ground() {
    let theme = default_theme();
    let mut frame = Surface::new(Size::new(4, 1), Cell::EMPTY);
    frame.fill_rect(
        Rect::new(0, 0, 4, 1),
        Cell::new(crate::render::Glyph::SPACE)
            .with_fg(theme.tokens.text)
            .with_bg(theme.tokens.accent),
    );
    let mut buf = Surface::new(Size::new(4, 1), Cell::EMPTY);
    fade_toward_ground(&mut buf, &frame, theme, 1.0);
    let cell = buf.get(0, 0).unwrap();
    assert_eq!(
        cell.fg, theme.tokens.bg,
        "full fade lands exactly on the ground"
    );
    assert_eq!(cell.bg, theme.tokens.bg);
    // Half fade sits strictly between.
    fade_toward_ground(&mut buf, &frame, theme, 0.5);
    let cell = buf.get(0, 0).unwrap();
    assert_ne!(cell.bg, theme.tokens.bg);
    assert_ne!(cell.bg, theme.tokens.accent);
}

// ---------------------------------------------------------------------------
// Click-to-skip on the real-terminal adapter
// ---------------------------------------------------------------------------

use crate::term::EnterOptions;
use crate::testing::CaptureTerm;

/// An entered capture terminal with the given mouse posture.
fn entered_term(mouse: MouseMode) -> CaptureTerm {
    let mut t = CaptureTerm::new(Size::new(20, 6));
    t.enter(&EnterOptions {
        mouse,
        ..EnterOptions::default()
    })
    .expect("enter");
    t
}

/// A left press then its release, SGR-encoded (what a terminal sends for
/// one click once mode 1002 + 1006 is armed).
const CLICK: &[u8] = b"\x1b[<0;5;3M\x1b[<0;5;3m";

/// The bug this guards: an app enters its boot session with
/// `MouseMode::Off`, so the terminal reports no click and only a key could
/// skip. The adapter must arm press reporting itself, the click must skip,
/// its release must not leak to the app, and the session must end as it
/// was entered (mouse off) once the adapter is finished.
#[test]
fn a_click_skips_even_when_the_session_entered_without_mouse() {
    let mut term = entered_term(MouseMode::Off);
    assert!(
        !term.screen().modes().is_set(1002),
        "precondition: no tracking"
    );
    term.push_input(CLICK);

    let mut io = TerminalIo::new(&mut term);
    let ts = Rc::new(RefCell::new(Vec::new()));
    let mut source = RecordingSource::new(Size::new(20, 6), ts.clone());
    let mut clock = VClock::new(vec![0, 0, 40, 80, 100, 140, 160, 180, 210]);
    let mut now = move || clock.tick();
    let out = play(
        &mut io,
        &mut source,
        default_theme(),
        &PresentCaps::FULL,
        &opts(),
        &mut now,
    )
    .expect("play");
    let (carry, _parser) = io.finish();

    assert_eq!(out, SplashOutcome::Skipped, "a click must skip like a key");
    assert!(
        carry.iter().all(|e| !matches!(e, Event::Mouse(_))),
        "the skip click's release must not reach the app: {carry:?}"
    );
    let log = String::from_utf8_lossy(term.bytes()).into_owned();
    let armed = log
        .find("\x1b[?1002h\x1b[?1006h")
        .expect("splash armed press reporting");
    let disarmed = log
        .rfind("\x1b[?1006l\x1b[?1002l")
        .expect("splash disarmed it");
    assert!(armed < disarmed, "arm before disarm: {log:?}");
    assert!(
        !term.screen().modes().is_set(1002) && !term.screen().modes().sgr_mouse(),
        "the session ends the splash with mouse tracking off, as entered"
    );
    assert_eq!(
        term.set_mouse_mode(MouseMode::Off).unwrap(),
        MouseMode::Off,
        "the stored session posture is back to the entered mode"
    );
}

/// A session that already reports presses is left untouched (no bytes),
/// and a richer mode (any-motion) is narrowed for the splash, then put
/// back exactly.
#[test]
fn the_entered_mouse_mode_is_restored_exactly() {
    let mut term = entered_term(MouseMode::ButtonDrag);
    let before = term.bytes().len();
    let io = TerminalIo::new(&mut term);
    let _ = io.finish();
    assert_eq!(term.bytes().len(), before, "ButtonDrag needs no retarget");
    assert!(term.screen().modes().is_set(1002));

    let mut term = entered_term(MouseMode::AnyMotion);
    {
        let _io = TerminalIo::new(&mut term);
    } // dropped without finish (an early return): Drop restores
    assert!(term.screen().modes().is_set(1003), "any-motion is re-armed");
    assert_eq!(
        term.set_mouse_mode(MouseMode::Off).unwrap(),
        MouseMode::AnyMotion
    );
}

/// The terminal verb itself: delta bytes, stored posture, and `leave`
/// disarming a mode armed after enter.
#[test]
fn set_mouse_mode_retargets_and_leave_disarms_it() {
    let mut term = entered_term(MouseMode::Off);
    assert_eq!(
        term.set_mouse_mode(MouseMode::ButtonDrag).unwrap(),
        MouseMode::Off
    );
    assert!(term.screen().modes().is_set(1002));
    assert_eq!(
        term.set_mouse_mode(MouseMode::ButtonDrag).unwrap(),
        MouseMode::ButtonDrag,
        "same mode: no-op"
    );
    term.leave().expect("leave");
    assert!(
        !term.screen().modes().is_set(1002) && !term.screen().modes().sgr_mouse(),
        "leave disarms the retargeted mode"
    );
    assert!(
        term.set_mouse_mode(MouseMode::ButtonDrag).is_err(),
        "outside a session the verb refuses"
    );
}
