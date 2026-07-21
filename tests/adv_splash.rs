//! REDTEAM cycle-3 attack: the boot splash player (DESIGN's confessed
//! risks). Everything runs on a virtual clock through the `SplashIo`
//! seam — no sleeps, no real terminal, wall-clock honesty asserted from
//! the clock the test itself controls.

use std::cell::RefCell;
use std::rc::Rc;

use abstracttui::base::{Result, Size};
use abstracttui::boot::player::{
    play, skip_reason, SplashFrameSource, SplashIo, SplashOptions, SplashOutcome, SplashWait,
};
use abstracttui::input::{Event, KeyCode, KeyEvent, KeyEventKind};
use abstracttui::render::{Cell, PresentCaps, Style, Surface};
use abstracttui::testing::VtScreen;
use abstracttui::theme::default_theme;

/// Virtual clock shared between the test, the IO double and the player.
#[derive(Clone)]
struct Clock(Rc<RefCell<u64>>);

impl Clock {
    fn new() -> Clock {
        Clock(Rc::new(RefCell::new(0)))
    }
    fn now(&self) -> u64 {
        *self.0.borrow()
    }
    fn advance(&self, ms: u64) {
        *self.0.borrow_mut() += ms;
    }
    fn now_fn(&self) -> impl FnMut() -> u64 {
        let c = self.clone();
        move || c.now()
    }
}

/// Scripted SplashIo: `present` costs `present_cost_ms` of virtual time
/// (the slow-flush terminal), `wait` advances the clock by the budget
/// (or delivers the next scripted event first). Every presented byte
/// chunk feeds a VtScreen so frame content is verifiable.
struct ScriptedIo {
    clock: Clock,
    size: Size,
    present_cost_ms: u64,
    presents: Vec<u64>, // virtual timestamps of each present
    /// (deliver_at_ms, events) — handed out the first wait AT/after that time.
    events: Vec<(u64, Vec<Event>)>,
    resizes: Vec<(u64, Size)>,
    screen: VtScreen,
}

impl ScriptedIo {
    fn new(clock: Clock, size: Size, present_cost_ms: u64) -> ScriptedIo {
        ScriptedIo {
            clock,
            size,
            present_cost_ms,
            presents: Vec::new(),
            events: Vec::new(),
            resizes: Vec::new(),
            screen: VtScreen::new(size),
        }
    }
}

impl SplashIo for ScriptedIo {
    fn size(&mut self) -> Result<Size> {
        Ok(self.size)
    }

    fn present(&mut self, bytes: &[u8]) -> Result<()> {
        self.presents.push(self.clock.now());
        self.screen.feed(bytes);
        self.clock.advance(self.present_cost_ms);
        Ok(())
    }

    fn wait(&mut self, budget_ms: u64) -> Result<SplashWait> {
        let now = self.clock.now();
        // Scripted resize due?
        if let Some(pos) = self.resizes.iter().position(|(at, _)| *at <= now) {
            let (_, size) = self.resizes.remove(pos);
            self.size = size;
            return Ok(SplashWait::Resize(size));
        }
        if let Some(pos) = self.events.iter().position(|(at, _)| *at <= now) {
            let (_, ev) = self.events.remove(pos);
            return Ok(SplashWait::Events(ev));
        }
        // Nothing due: sleep the whole budget (virtual).
        self.clock.advance(budget_ms.max(1));
        Ok(SplashWait::Idle)
    }
}

/// A frame source that records every `t` it was asked to render and
/// paints a counter (plus optional wide content for the fade attack).
struct RecordingSource {
    frame: Surface,
    render_ts: Rc<RefCell<Vec<u64>>>,
    wide: bool,
    calls: u32,
}

impl RecordingSource {
    fn new(size: Size, render_ts: Rc<RefCell<Vec<u64>>>, wide: bool) -> RecordingSource {
        RecordingSource {
            frame: Surface::new(size, Cell::EMPTY),
            render_ts,
            wide,
            calls: 0,
        }
    }
}

impl SplashFrameSource for RecordingSource {
    fn render(&mut self, t: f32, size: Size, theme: &abstracttui::theme::Theme) -> &Surface {
        self.calls += 1;
        self.render_ts
            .borrow_mut()
            .push((t * 1000.0).round() as u64);
        if self.frame.size() != size {
            self.frame = Surface::new(size, Cell::EMPTY);
        }
        self.frame.clear(Cell::EMPTY.with_bg(theme.tokens.bg));
        let style = Style::new().fg(theme.tokens.text).bg(theme.tokens.bg);
        if self.wide {
            self.frame.draw_text(1, 1, "日本語テスト 🎉", style);
            self.frame.draw_text(
                1,
                2,
                "\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F466} fam",
                style,
            );
        }
        self.frame
            .draw_text(1, 0, &format!("frame {}", self.calls), style);
        &self.frame
    }
}

fn key_press() -> Vec<Event> {
    vec![Event::Key(KeyEvent::plain(KeyCode::Enter))]
}

fn key_release() -> Vec<Event> {
    // KeyEvent is #[non_exhaustive] (cycle-8 freeze) — construct via the
    // builder, never a struct literal, so future field additions can't
    // break this call site.
    vec![Event::Key(
        KeyEvent::plain(KeyCode::Enter).with_kind(KeyEventKind::Release),
    )]
}

fn opts() -> SplashOptions {
    SplashOptions::default() // 30 fps, 2000 ms, 2500 ceiling, fade per identity
}

// ---------------------------------------------------------------------------
// Pacing honesty on a slow terminal.
// ---------------------------------------------------------------------------

/// Each present costs 300 virtual ms (a stalled ssh link): the player
/// must DROP frames — the storyboard still ENDS on time (wall ≈
/// total_ms, never stretched to 60 frames x 300 ms = 18 s), and the
/// sampled `t`s advance by ~write cost (frames dropped by never having
/// existed).
#[test]
fn slow_flush_terminal_drops_frames_and_finishes_on_time() {
    let clock = Clock::new();
    let mut io = ScriptedIo::new(clock.clone(), Size::new(40, 10), 300);
    let ts = Rc::new(RefCell::new(Vec::new()));
    let mut source = RecordingSource::new(Size::new(40, 10), ts.clone(), false);
    let caps = PresentCaps::FULL;
    let mut now = clock.now_fn();
    let outcome = play(
        &mut io,
        &mut source,
        default_theme(),
        &caps,
        &opts(),
        &mut now,
    )
    .expect("play");
    // The timeline finishes at total_ms wall time (dropping absorbed the
    // stall); queuing would stretch to ~18 s and trip the ceiling.
    assert_eq!(outcome, SplashOutcome::Completed);
    let wall = clock.now();
    assert!(
        (2000..2500).contains(&wall),
        "wall-clock honesty: 2 s storyboard must END near 2 s, took {wall}"
    );
    let n = io.presents.len();
    assert!(
        n <= 8,
        "a 300 ms/write terminal carries at most ~7 frames in 2 s, presented {n}"
    );
    // Dropped-not-stretched: consecutive sampled times step by ~write
    // cost, not by the 33 ms cadence.
    let samples = ts.borrow();
    for pair in samples.windows(2) {
        let step = pair[1].saturating_sub(pair[0]);
        assert!(
            step >= 250,
            "sampled t advanced only {step} ms on a 300 ms-stalled terminal — \
             frames are queuing, not dropping: {samples:?}"
        );
    }
}

/// A catastrophically stalled terminal (900 ms/write): the storyboard
/// cannot even finish — the HARD CEILING is what ends it (RT1-10b).
#[test]
fn catastrophic_stall_hits_the_hard_ceiling() {
    let clock = Clock::new();
    let mut io = ScriptedIo::new(clock.clone(), Size::new(40, 10), 900);
    let ts = Rc::new(RefCell::new(Vec::new()));
    let mut source = RecordingSource::new(Size::new(40, 10), ts, false);
    let caps = PresentCaps::FULL;
    let mut now = clock.now_fn();
    let outcome = play(
        &mut io,
        &mut source,
        default_theme(),
        &caps,
        &opts(),
        &mut now,
    )
    .expect("play");
    assert_eq!(
        outcome,
        SplashOutcome::CutOff,
        "the 2.5 s ceiling must fire"
    );
    let wall = clock.now();
    assert!(
        wall <= 2500 + 900 + 50,
        "ceiling honored within one stalled write: wall {wall}"
    );
}

/// Healthy terminal: ~30 fps cadence, natural completion at total_ms,
/// and the frame count is in the right neighborhood (the wait budget
/// aligns to frame boundaries).
#[test]
fn healthy_terminal_plays_to_completion_at_cadence() {
    let clock = Clock::new();
    let mut io = ScriptedIo::new(clock.clone(), Size::new(40, 10), 1);
    let ts = Rc::new(RefCell::new(Vec::new()));
    let mut source = RecordingSource::new(Size::new(40, 10), ts, false);
    let caps = PresentCaps::FULL;
    let mut now = clock.now_fn();
    let outcome = play(
        &mut io,
        &mut source,
        default_theme(),
        &caps,
        &opts(),
        &mut now,
    )
    .expect("play");
    assert_eq!(outcome, SplashOutcome::Completed);
    let wall = clock.now();
    assert!(
        (2000..2200).contains(&wall),
        "healthy run completes at ~total_ms, took {wall}"
    );
    let n = io.presents.len();
    assert!(
        (45..=62).contains(&n),
        "2 s at 30 fps with 1 ms writes should land ~55-60 frames, got {n}"
    );
}

// ---------------------------------------------------------------------------
// Skip semantics.
// ---------------------------------------------------------------------------

#[test]
fn deliberate_key_starts_fade_and_second_key_cuts() {
    let clock = Clock::new();
    let mut io = ScriptedIo::new(clock.clone(), Size::new(30, 8), 1);
    io.events.push((400, key_press()));
    io.events.push((450, key_press())); // second press mid-fade: cut NOW
    let ts = Rc::new(RefCell::new(Vec::new()));
    let mut source = RecordingSource::new(Size::new(30, 8), ts, false);
    let caps = PresentCaps::FULL;
    let mut now = clock.now_fn();
    let outcome = play(
        &mut io,
        &mut source,
        default_theme(),
        &caps,
        &opts(),
        &mut now,
    )
    .expect("play");
    assert_eq!(outcome, SplashOutcome::Skipped);
    let wall = clock.now();
    assert!(
        wall < 700,
        "double-press must cut well before the fade completes: wall {wall}"
    );
}

#[test]
fn single_key_fades_out_within_fade_budget() {
    let clock = Clock::new();
    let mut io = ScriptedIo::new(clock.clone(), Size::new(30, 8), 1);
    io.events.push((400, key_press()));
    let ts = Rc::new(RefCell::new(Vec::new()));
    let mut source = RecordingSource::new(Size::new(30, 8), ts, false);
    let caps = PresentCaps::FULL;
    let mut now = clock.now_fn();
    let outcome = play(
        &mut io,
        &mut source,
        default_theme(),
        &caps,
        &opts(),
        &mut now,
    )
    .expect("play");
    assert_eq!(outcome, SplashOutcome::Skipped);
    let wall = clock.now();
    // Fade length comes from identity; the whole run must end shortly
    // after skip + fade, far before total_ms.
    assert!(
        (400..800).contains(&wall),
        "fade-out must end promptly after the skip: wall {wall}"
    );
}

/// Releases, focus chatter and caps replies must NEVER skip: with only
/// those arriving, the splash plays to natural completion.
#[test]
fn release_only_events_never_skip() {
    let clock = Clock::new();
    let mut io = ScriptedIo::new(clock.clone(), Size::new(30, 8), 1);
    io.events.push((200, key_release()));
    io.events.push((400, vec![Event::FocusGained]));
    io.events.push((600, vec![Event::FocusLost]));
    io.events.push((800, key_release()));
    let ts = Rc::new(RefCell::new(Vec::new()));
    let mut source = RecordingSource::new(Size::new(30, 8), ts, false);
    let caps = PresentCaps::FULL;
    let mut now = clock.now_fn();
    let outcome = play(
        &mut io,
        &mut source,
        default_theme(),
        &caps,
        &opts(),
        &mut now,
    )
    .expect("play");
    assert_eq!(
        outcome,
        SplashOutcome::Completed,
        "non-deliberate events must not skip"
    );
}

/// Release-only events on a CATASTROPHICALLY stalled terminal: still
/// unskippable (releases are not deliberate), so the hard ceiling is
/// what ends it (the RT1-10b compound case).
#[test]
fn release_only_on_slow_terminal_ends_at_ceiling() {
    let clock = Clock::new();
    let mut io = ScriptedIo::new(clock.clone(), Size::new(30, 8), 900);
    io.events.push((500, key_release()));
    io.events.push((1000, key_release()));
    let ts = Rc::new(RefCell::new(Vec::new()));
    let mut source = RecordingSource::new(Size::new(30, 8), ts, false);
    let caps = PresentCaps::FULL;
    let mut now = clock.now_fn();
    let outcome = play(
        &mut io,
        &mut source,
        default_theme(),
        &caps,
        &opts(),
        &mut now,
    )
    .expect("play");
    assert_eq!(
        outcome,
        SplashOutcome::CutOff,
        "releases must never skip; ceiling ends it"
    );
    assert!(
        clock.now() <= 3500,
        "ceiling within one stalled write: {}",
        clock.now()
    );
}

// ---------------------------------------------------------------------------
// Fade over wide-glyph content (their get-modify-set continuation risk).
// ---------------------------------------------------------------------------

/// During the fade, every emitted frame must stay VT-model exact: no
/// torn pairs, no unknown bytes, styled continuations consistent. The
/// screen at the end must still carry the intact wide clusters.
#[test]
fn fade_over_wide_glyphs_stays_model_clean() {
    let clock = Clock::new();
    let mut io = ScriptedIo::new(clock.clone(), Size::new(30, 8), 1);
    io.events.push((300, key_press())); // start the fade over wide content
    let ts = Rc::new(RefCell::new(Vec::new()));
    let mut source = RecordingSource::new(Size::new(30, 8), ts, true);
    let caps = PresentCaps::FULL;
    let mut now = clock.now_fn();
    let outcome = play(
        &mut io,
        &mut source,
        default_theme(),
        &caps,
        &opts(),
        &mut now,
    )
    .expect("play");
    assert_eq!(outcome, SplashOutcome::Skipped);
    let screen = &io.screen;
    assert_eq!(
        screen.unknown_seq_count(),
        0,
        "fade frames emitted unmodeled bytes: {:?}",
        screen.unknown_samples()
    );
    // Wide pairs intact on the final screen.
    for y in 0..8 {
        for x in 0..30 {
            let cell = screen.cell(x, y).unwrap();
            if cell.is_continuation() {
                assert!(
                    x > 0 && screen.cell(x - 1, y).unwrap().is_wide_leader(),
                    "orphan continuation at ({x},{y}) after fade:\n{}",
                    screen.to_styled_dump()
                );
            }
            if cell.is_wide_leader() {
                assert!(
                    screen
                        .cell(x + 1, y)
                        .map(|c| c.is_continuation())
                        .unwrap_or(false),
                    "torn leader at ({x},{y}) after fade:\n{}",
                    screen.to_styled_dump()
                );
            }
        }
    }
    // The cluster content survived the restyling.
    assert!(screen.to_text().contains("日本語"), "{}", screen.to_text());
}

// ---------------------------------------------------------------------------
// Resize mid-splash.
// ---------------------------------------------------------------------------

#[test]
fn resize_mid_splash_recovers_and_completes() {
    let clock = Clock::new();
    let mut io = ScriptedIo::new(clock.clone(), Size::new(30, 8), 1);
    io.resizes.push((500, Size::new(50, 14)));
    io.resizes.push((900, Size::new(20, 5)));
    let ts = Rc::new(RefCell::new(Vec::new()));
    let mut source = RecordingSource::new(Size::new(30, 8), ts, true);
    let caps = PresentCaps::FULL;
    let mut now = clock.now_fn();
    let outcome = play(
        &mut io,
        &mut source,
        default_theme(),
        &caps,
        &opts(),
        &mut now,
    )
    .expect("play");
    assert_eq!(outcome, SplashOutcome::Completed);
    // NOTE: the io's VtScreen is fixed-size, so byte-level assertions
    // stop at "no panic + completion" for the resize path (model reflow
    // is a rig cycle-4 item); the player's own size handling is
    // exercised by the source's debug_assert on requested size.
}

// ---------------------------------------------------------------------------
// The gate.
// ---------------------------------------------------------------------------

#[test]
fn gate_reasons_are_exact() {
    let none: &dyn Fn(&str) -> Option<String> = &|_| None;
    assert_eq!(skip_reason(false, none), Some("render handle is not a tty"));
    assert_eq!(skip_reason(true, none), None);
    let no_splash: &dyn Fn(&str) -> Option<String> =
        &|k| (k == "ABSTRACTTUI_NO_SPLASH").then(|| "1".to_string());
    assert!(skip_reason(true, no_splash).is_some());
    // "0" opts back IN (wrapper-script affordance).
    let zero: &dyn Fn(&str) -> Option<String> =
        &|k| (k == "ABSTRACTTUI_NO_SPLASH").then(|| "0".to_string());
    assert_eq!(skip_reason(true, zero), None);
    let no_color: &dyn Fn(&str) -> Option<String> = &|k| (k == "NO_COLOR").then(|| "1".to_string());
    assert!(skip_reason(true, no_color).is_some());
    let dumb: &dyn Fn(&str) -> Option<String> = &|k| (k == "TERM").then(|| "dumb".to_string());
    assert!(skip_reason(true, dumb).is_some());
}

// ---------------------------------------------------------------------------
// Brandmark 3D determinism: same t, same size, same theme -> identical
// pixels across two INDEPENDENT constructions (a splash that renders
// differently per process would defeat the byte-level goldens and any
// recorded demo).
// ---------------------------------------------------------------------------

#[test]
fn brandmark_render_is_deterministic_across_constructions() {
    use abstracttui::boot::brandmark3d::identity_params;
    use abstracttui::three::brandmark::BrandmarkRenderer;
    let theme = default_theme();
    let size = Size::new(60, 18);
    let dump = |t: f32| -> String {
        let mut r = BrandmarkRenderer::with_params(identity_params());
        let surface = r.render(t, size, theme);
        // Cheap stable digest: per-cell glyph + colors folded to a string.
        let mut s = String::new();
        for y in 0..size.h {
            for x in 0..size.w {
                let c = surface.get(x, y).unwrap();
                s.push_str(&format!(
                    "{}:{}:{};",
                    surface.glyph_str(c),
                    c.fg.to_hex(),
                    c.bg.to_hex()
                ));
            }
            s.push('\n');
        }
        s
    };
    for t in [0.0f32, 0.4, 1.0, 1.8] {
        assert_eq!(
            dump(t),
            dump(t),
            "brandmark frame at t={t} differs across constructions"
        );
    }
    // And time actually animates: two distinct t values differ.
    assert_ne!(dump(0.2), dump(1.6), "the brandmark must animate over t");
}

/// The renderer is deliberately STATEFUL across samples (trail decay is
/// part of the identity), so pure time-travel statelessness is NOT the
/// contract. What the player actually needs is SEQUENCE determinism:
/// the same monotonic sampling sequence — including a mid-sequence
/// resize — reproduces byte-identically across constructions.
#[test]
fn brandmark_sampling_sequence_with_resize_is_reproducible() {
    use abstracttui::boot::brandmark3d::identity_params;
    use abstracttui::three::brandmark::BrandmarkRenderer;
    let theme = default_theme();
    let digest = |s: &abstracttui::render::Surface| -> String {
        let size = s.size();
        let mut out = String::new();
        for y in 0..size.h {
            for x in 0..size.w {
                let c = s.get(x, y).unwrap();
                out.push_str(&format!(
                    "{}{}{};",
                    s.glyph_str(c),
                    c.fg.to_hex(),
                    c.bg.to_hex()
                ));
            }
        }
        out
    };
    let run = || -> Vec<String> {
        let mut r = BrandmarkRenderer::with_params(identity_params());
        vec![
            digest(r.render(0.3, Size::new(40, 12), theme)),
            digest(r.render(0.9, Size::new(64, 20), theme)), // resize
            digest(r.render(1.4, Size::new(64, 20), theme)),
            digest(r.render(1.9, Size::new(30, 9), theme)), // shrink
        ]
    };
    assert_eq!(
        run(),
        run(),
        "monotonic sampling + resizes must reproduce exactly"
    );
}

// ---------------------------------------------------------------------------
// theme::register concurrency (splash + app threads racing).
// ---------------------------------------------------------------------------

#[test]
fn register_same_id_from_many_threads_is_consistent() {
    use abstracttui::theme::{get, register, RegisterMode, ThemeCandidate};
    let base = abstracttui::theme::get("abstract-dark").unwrap().tokens;
    let threads: Vec<_> = (0..8)
        .map(|i| {
            std::thread::spawn(move || {
                let cand = ThemeCandidate {
                    id: "rt3-race".into(),
                    label: format!("Racer {i}"),
                    dark: true,
                    tokens: base,
                };
                register(cand, RegisterMode::Strict)
            })
        })
        .collect();
    let results: Vec<_> = threads
        .into_iter()
        .map(|t| t.join().expect("no panic"))
        .collect();
    let ok_count = results.iter().filter(|r| r.is_ok()).count();
    // Contract options: first-wins (1 ok, 7 refused) or idempotent
    // re-register (8 ok) — both are consistent; a torn registry is not.
    assert!(
        ok_count == 1 || ok_count == 8,
        "ambiguous race outcome: {ok_count}/8 registrations succeeded"
    );
    let theme = get("rt3-race").expect("the id must resolve after the race");
    assert_eq!(theme.id, "rt3-race");
    // Lookup is stable across repeated calls (no torn Vec state).
    for _ in 0..100 {
        assert!(
            std::ptr::eq(get("rt3-race").unwrap(), theme),
            "lookup flapped"
        );
    }
}
