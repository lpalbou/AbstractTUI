//! The frame-loop driver: one `turn` = one pass of the damage contract's
//! phase sequence (docs/design/01-damage-contract.md §1):
//!
//! ```text
//! U. drain posted jobs -> dispatch input (each event batch-wrapped)
//!    -> effects flush (Dyn remounts happen here, marking damage)
//! L. re-solve dirty layout; geometry damage folds into the ui damage set
//! D. clear + redraw ONLY damaged regions into the root layer (draw-phase
//!    guard active: tracked reads panic in debug)
//! C. Compositor::flatten(layers) -> frame + damage union
//! P. diff(prev, next, damage) -> presenter bytes -> ONE flush
//! S. prev <- next; damage bookkeeping cleared
//! ```
//!
//! The frame's damage set is sealed at L (epoch rule §2): user code runs
//! only in U, cross-thread writes arrive only as posted jobs, and posted
//! jobs run only in U — a write landing mid-frame wakes the loop and is
//! drained by the NEXT frame's U. This is structural, not disciplinary.
//!
//! `Driver` is deliberately separable from the blocking outer loop:
//! `turn` never blocks (tests drive it frame by frame against a scripted
//! terminal and inspect bytes between turns); `wait_for_activity` is the
//! blocking edge only the real `App::run` uses.

use std::rc::Rc;

use crate::base::{Point, Rect, Result, Size};
use crate::gfx::ImageSession;
use crate::input::{Event, EventReader};
use crate::reactive::{
    self, drain_posted, flush_effects, take_frame_request, take_worker_failures,
};
use crate::render::{Cell, Compositor, FrameDiff, Glyph, PresentCaps, Presenter, Surface};
use crate::term::{ActiveProbe, Capabilities, EnterOptions, KittyFlags, Terminal, TerminalWaker};
use crate::theme::TokenId;
use crate::ui::SurfaceCanvas;

use super::events::{convert_event, is_default_quit};
use super::overlays::Overlays;
use super::theme::current_theme;
use super::App;

/// How a `run` session is configured. `Default` is the interactive
/// posture: env-detected capabilities, capability-derived enter options,
/// active probe on.
pub struct RunConfig {
    /// Capabilities to assume. `None` = passive env detection at start
    /// (tests inject a fixed set so host env never leaks into assertions).
    pub caps: Option<Capabilities>,
    /// Session options. `None` = derived from capabilities (kitty
    /// keyboard flags requested only when the terminal speaks them).
    pub enter: Option<EnterOptions>,
    /// Write the active capability probe at startup and fold replies as
    /// they arrive (RT1-6: first paint NEVER waits for this — env-pass
    /// caps draw frame 1, probe results upgrade later frames).
    pub probe: bool,
}

impl Default for RunConfig {
    fn default() -> Self {
        RunConfig {
            caps: None,
            enter: None,
            probe: true,
        }
    }
}

/// What one non-blocking `turn` did — the outer loop's steering data.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct Turn {
    /// Input events dispatched during phase U.
    pub events: usize,
    /// A frame was rendered (phases L..S ran).
    pub rendered: bool,
    /// The rendered frame actually emitted bytes (idle frames do not).
    pub emitted: bool,
    /// The app asked to quit (explicit `Quitter` or default Ctrl+C).
    pub quit: bool,
    /// Nothing happened and nothing is pending: the loop may block.
    pub idle: bool,
}

/// `base::FrameRequester` that interrupts the terminal's blocking read,
/// so a frame requested from a posted job (timer thread) wakes the loop.
struct WakeOnFrame(Option<TerminalWaker>);

impl crate::base::FrameRequester for WakeOnFrame {
    fn request_frame(&self) {
        if let Some(w) = &self.0 {
            w.wake();
        }
    }
}

pub struct Driver {
    reader: EventReader,
    pub(super) caps: Capabilities,
    present_caps: PresentCaps,
    probe: Option<ActiveProbe>,
    comp: Compositor,
    diff: FrameDiff,
    presenter: Presenter,
    /// All compositor layers (root at id 0 + app overlays) live in the
    /// shared overlay store; the driver borrows them per phase.
    pub(super) overlays: Overlays,
    /// Terminal-held image state (RT4-1): one session per terminal;
    /// slot keys are `ImageEntry` ids.
    pub(super) image_session: ImageSession,
    /// Byte-channel image payloads rendered pre-flatten, emitted through
    /// presenter custody AFTER the cell runs (§6: cells first, protocol
    /// payloads second, ONE flush).
    pub(super) pending_image_bytes: Vec<(Vec<u8>, Point)>,
    frame: Surface,
    prev: Surface,
    size: Size,
    /// Event captured by a blocking wait, dispatched by the next turn so
    /// ALL routing stays inside turn's phase U.
    pending: Vec<Event>,
    /// Scratch for `poll_many` bursts (reused, no per-turn alloc).
    burst: Vec<Event>,
    /// tmux passthrough grace: after the DA1 sentinel, wrapped replies
    /// get [`crate::term::probe::TMUX_GRACE`] to arrive; at the deadline
    /// the probe finalizes with whatever answered (KERNEL's reference
    /// loop, driver edition).
    probe_grace: Option<std::time::Instant>,
    now_fn: Option<Rc<dyn Fn() -> std::time::Instant>>,
    out: Vec<u8>,
    scratch_damage: Vec<Rect>,
}

impl Driver {
    /// Enter the terminal session and prepare the pipeline. Emits the
    /// enter bytes and (optionally) the probe queries; does NOT render —
    /// the first `turn` does, from the mount-time damage.
    pub fn new(app: &mut App, term: &mut dyn Terminal, cfg: RunConfig) -> Result<Driver> {
        let caps = cfg.caps.unwrap_or_else(Capabilities::detect_env);
        let enter = cfg.enter.unwrap_or_else(|| EnterOptions {
            kitty_keyboard: if caps.kitty_keyboard {
                KittyFlags::standard()
            } else {
                KittyFlags(0)
            },
            ..EnterOptions::default()
        });
        term.enter(&enter)?;
        let size = term.size()?;
        // Through App::set_viewport, never tree-direct: App::viewport()
        // must stay truthful (RT2-9).
        app.set_viewport(size);

        // Cross-thread wakeups: posted jobs and frame requests interrupt
        // the blocking read. A terminal without a waker (scripted tests)
        // still works — turns discover work on their own cadence.
        let waker = term.waker();
        if let Some(w) = waker.clone() {
            reactive::set_wake_callback(move || w.wake());
        }
        reactive::set_frame_requester(Rc::new(WakeOnFrame(waker)));

        // First paint uses env-pass caps IMMEDIATELY; the probe upgrades
        // later frames (RT1-6). Never probe a dumb terminal (RT1-6b).
        // `for_caps` + `full_query_bytes` (KERNEL cycle 4): under tmux
        // the batch adds WRAPPED queries so passthrough graphics get
        // verified instead of conservatively zeroed.
        let probe = if cfg.probe && !caps.dumb {
            let probe = ActiveProbe::for_caps(&caps);
            term.write(&probe.full_query_bytes())?;
            term.flush()?;
            Some(probe)
        } else {
            None
        };

        let blank = Cell::EMPTY;
        let overlays = app.overlays();
        overlays.ensure_root(size);
        Ok(Driver {
            reader: EventReader::new(),
            present_caps: present_caps_from(&caps),
            caps,
            probe,
            comp: Compositor::new(),
            diff: FrameDiff::new(),
            presenter: Presenter::new(),
            overlays,
            image_session: ImageSession::new(),
            pending_image_bytes: Vec::new(),
            frame: Surface::new(size, blank),
            prev: Surface::new(size, blank),
            size,
            pending: Vec::new(),
            burst: Vec::new(),
            probe_grace: None,
            now_fn: None,
            out: Vec::new(),
            scratch_damage: Vec::new(),
        })
    }

    /// Inject the frame-loop clock (animations, one-shot timers, probe
    /// grace all read it). Tests drive turns on synthetic time instead
    /// of real sleeps; production never calls this (`Instant::now`).
    pub fn set_clock(&mut self, f: impl Fn() -> std::time::Instant + 'static) {
        self.now_fn = Some(Rc::new(f));
    }

    /// The loop clock: injected in tests, `Instant::now` in production.
    fn now(&self) -> std::time::Instant {
        match &self.now_fn {
            Some(f) => f(),
            None => std::time::Instant::now(),
        }
    }

    pub fn caps(&self) -> &Capabilities {
        &self.caps
    }

    /// One non-blocking pass: phase U always; phases L..S only when a
    /// frame is wanted. Never blocks — the caller decides how to wait.
    pub fn turn(&mut self, app: &mut App, term: &mut dyn Terminal) -> Result<Turn> {
        // ---- phase U: posted jobs, timers, animation ticks, then input --
        drain_posted();
        let now = self.now();
        // One-shot timers (toast dismissal, debounce) fire here; the
        // outer loop sleeps until the earliest deadline, so a pending
        // timer costs zero wakeups until due.
        reactive::run_due_timers(now);
        // Signal transitions (reactive::animate) advance here — one tick
        // per frame, billed as frame requests per the cursor/animation
        // policy (§4). An empty task list costs nothing.
        reactive::run_frame_tasks(now);
        flush_effects();
        // tmux probe grace expired with wrapped replies still missing:
        // finalize on the evidence in hand (passthrough-off sessions
        // never answer — spending the grace once is the design).
        if let Some(deadline) = self.probe_grace {
            if now >= deadline {
                self.probe_grace = None;
                if self.probe.take().is_some() {
                    self.apply_caps_upgrade(app);
                }
            }
        }
        // A worker that died surfaces as an app error (RT1-15b). Checked
        // AFTER the drain: the failure report itself arrives as a posted
        // job, so draining first catches a death in the same turn.
        let failures = take_worker_failures();
        if !failures.is_empty() {
            return Err(crate::base::Error::App(failures.join("; ")));
        }

        let mut events = 0usize;
        let mut quit = false;
        let pending: Vec<Event> = self.pending.drain(..).collect();
        for ev in pending {
            events += 1;
            self.handle_event(app, ev, &mut quit);
        }
        // Drain whatever is immediately available in ONE burst
        // (`poll_many` with an elapsed deadline = non-blocking drain;
        // KERNEL cycle 4 — one syscall shape instead of one zero-timeout
        // confirmation per event). Dispatch stays per-event: each event
        // is its own reactive batch (inside UiTree::dispatch), so
        // effects flush between events — event N+1 routes over the tree
        // event N produced.
        let drain_deadline = std::time::Instant::now();
        let mut burst = std::mem::take(&mut self.burst);
        burst.clear();
        self.reader
            .poll_many(term, &mut burst, Some(drain_deadline))?;
        // THE COALESCING RULE (mouse-move storms, cycle 7): within one
        // phase-U batch, only the LAST of each consecutive run of plain
        // Move events dispatches — intermediate hover positions were
        // never visible (no frame rendered between them) so nothing is
        // lost. Drag/Down/Up/Wheel are NEVER coalesced (capture and
        // click handlers see every one), and a non-mouse event between
        // moves breaks the run (ordering with keys is preserved).
        // Widgets needing raw motion trails will need an opt-out; none
        // exists in-tree, so the rule is global until one does.
        coalesce_moves(&mut burst);
        for ev in burst.drain(..) {
            events += 1;
            self.handle_event(app, ev, &mut quit);
        }
        self.burst = burst;
        if app.quit_requested() {
            quit = true;
        }
        if quit {
            return Ok(Turn {
                events,
                quit: true,
                ..Turn::default()
            });
        }

        // ---- frame decision: damage set seals HERE (epoch rule §2) -----
        let frame_requested = take_frame_request();
        let wants_frame = frame_requested
            || app.tree().has_pending_work()
            || self.overlays.has_pending_work()
            || {
                let store = self.overlays.store().borrow();
                Compositor::any_dirty(&store.layers)
            };
        if !wants_frame {
            return Ok(Turn {
                events,
                idle: events == 0,
                ..Turn::default()
            });
        }
        let emitted = self.render_frame(app, term)?;
        Ok(Turn {
            events,
            rendered: true,
            emitted,
            ..Turn::default()
        })
    }

    /// Phases L..S for one frame.
    fn render_frame(&mut self, app: &mut App, term: &mut dyn Terminal) -> Result<bool> {
        let theme = current_theme();
        let text_fg = theme.tokens.get(TokenId::Text);
        let bg = theme.tokens.get(TokenId::Bg);
        app.tree().set_text_fg(text_fg);
        // Compositing ground = theme bg (RENDER cycle 5): additive light
        // and translucent veils blend against the theme instead of
        // black. A theme switch already damage_alls (contract §5), so
        // re-reading per frame keeps the ground in lockstep for free.
        self.comp.set_ground(Some(bg));

        // ---- phase L: layout (folds geometry damage into the ui set) ---
        app.tree().layout();
        self.overlays.layout_all();

        // ---- phase D: clear + redraw damaged regions (root layer) ------
        // The root surface is STOLEN from the store while user draw code
        // runs (the overlay borrow rule); overlay content paints next.
        let viewport = Rect::from_size(self.size);
        let mut damage = app.tree().take_damage();
        coalesce_damage(&mut damage, viewport);
        let mut root_surface = self.steal_root_surface();
        {
            let clear = Cell::new(Glyph::SPACE).with_fg(text_fg).with_bg(bg);
            for &rect in &damage {
                // The clear erases stale glyphs where content shrank or
                // moved away; surface writes record their own damage for
                // the compositor.
                root_surface.fill_rect(rect, clear);
            }
            let mut canvas = SurfaceCanvas::new(&mut root_surface);
            app.tree().draw_damaged(&mut canvas, &damage);
        }
        // ---- phase D2: image overlays (gfx ladder). Mosaic falls back
        // to CELLS blitted into the root surface (pre-flatten); byte
        // channels stash payloads for post-present custody emission.
        self.render_images(&mut root_surface);
        self.restore_root_surface(root_surface);
        self.overlays.draw_all();

        // ---- phase C: flatten (root + overlays, z-sorted) ---------------
        self.scratch_damage.clear();
        {
            let mut store = self.overlays.store().borrow_mut();
            let flat = self.comp.flatten(&mut self.frame, &mut store.layers);
            self.scratch_damage.extend_from_slice(flat);
        }

        // ---- phase P: diff -> present -> image payloads -> ONE flush ----
        // Scroll-aware diff (RENDER cycle 5): when the damage reads as
        // one vertical band shift, the terminal scrolls (DECSTBM+SU/SD,
        // ~8-9x fewer bytes on list/log workloads) and only residuals
        // repaint; detection declining yields plain-compute bytes.
        self.out.clear();
        let runs = self
            .diff
            .compute_scrolled(&self.prev, &self.frame, &self.scratch_damage);
        self.presenter
            .emit_scrolled(runs, &self.frame, &self.present_caps, &mut self.out);
        // Protocol payloads AFTER cell runs, through presenter custody
        // (close SGR/link, absolute CUP, invalidate) — same buffer, so
        // the frame still reaches the terminal in one write + one flush.
        for (bytes, at) in self.pending_image_bytes.drain(..) {
            self.presenter.external_write(&mut self.out, &bytes, at);
        }
        let emitted = !self.out.is_empty();
        if emitted {
            term.write(&self.out)?;
            term.flush()?; // exactly one flush per emitting frame (RT1-16a)
        }

        // ---- phase S: swap ----------------------------------------------
        self.prev
            .blit(&self.frame, Rect::from_size(self.size), Point::ZERO);
        Ok(emitted)
    }

    /// Block until input, a wake, or a resize; capture at most one event
    /// for the NEXT turn's phase U. Used only by the real `App::run` —
    /// never by tests (a scripted terminal has no blocking read).
    pub fn wait_for_activity(&mut self, term: &mut dyn Terminal) -> Result<()> {
        if let Some(ev) = self.reader.poll_event(term, None)? {
            self.pending.push(ev);
        }
        // None = waker fired (posted work / frame request): the next turn
        // drains it. Deliberately no re-loop here: EVERY consequence of a
        // wake is turn's business.
        Ok(())
    }

    /// Frame-paced wait: block until `deadline` (the next animation frame)
    /// or earlier activity. Same event capture as `wait_for_activity`.
    pub fn wait_until(
        &mut self,
        term: &mut dyn Terminal,
        deadline: std::time::Instant,
    ) -> Result<()> {
        if let Some(ev) = self.reader.poll_event(term, Some(deadline))? {
            self.pending.push(ev);
        }
        Ok(())
    }

    /// Leave the terminal session (idempotent; also runs on drop of the
    /// platform terminal — this explicit call just makes teardown bytes
    /// deterministic for tests). Releases every live image slot first:
    /// leaving the alt screen erases CELLS but kitty uploads live in
    /// terminal memory until deleted — exiting without the deletes is
    /// the RT4-1 leak in its most durable form.
    pub fn finish(&mut self, term: &mut dyn Terminal) -> Result<()> {
        if self.image_session.live_slots() > 0 {
            let mut bytes: Vec<(Vec<u8>, Point)> = Vec::new();
            let mut sink = super::driver_images::BufSink(&mut bytes);
            self.image_session
                .release_all(&mut sink, &self.caps.graphics());
            self.out.clear();
            for (payload, at) in bytes {
                self.presenter.external_write(&mut self.out, &payload, at);
            }
            if !self.out.is_empty() {
                term.write(&self.out)?;
                term.flush()?;
            }
        }
        term.leave()
    }

    fn handle_event(&mut self, app: &mut App, event: Event, quit: &mut bool) {
        match event {
            Event::Resize(size) => self.apply_resize(app, size),
            Event::CapsReply(reply) => {
                if let Some(probe) = &mut self.probe {
                    if probe.on_reply(&reply, &mut self.caps) {
                        self.probe = None;
                        self.probe_grace = None;
                        self.apply_caps_upgrade(app);
                    } else if probe.sentinel_passed()
                        && probe.awaiting_wrapped()
                        && self.probe_grace.is_none()
                    {
                        // Sentinel in, wrapped replies (tmux passthrough)
                        // still possible: grant TMUX_GRACE, then finalize
                        // in phase U. The timer wakes an idle loop.
                        let grace = crate::term::probe::TMUX_GRACE;
                        self.probe_grace = Some(self.now() + grace);
                        reactive::after(grace, || {});
                    }
                }
            }
            other => {
                if let Some(ui_event) = convert_event(&other) {
                    // Overlay trees route first, topmost-z down; a MODAL
                    // overlay owns everything while visible. Unclaimed
                    // events fall to the root tree.
                    let mut consumed = match self.overlays.dispatch(&ui_event) {
                        Some(consumed) => consumed,
                        None => app.tree().dispatch(&ui_event),
                    };
                    // Global actions run LAST: only keys nothing in the
                    // UI consumed reach the keymap (a focused input
                    // typing 's' never fires a bare-'s' binding).
                    if !consumed {
                        if let crate::ui::UiEvent::Key(k) = &ui_event {
                            let chord = crate::ui::KeyChord {
                                key: k.key,
                                mods: k.mods,
                            };
                            consumed = app.actions().dispatch_chord(chord);
                        }
                    }
                    // Default Ctrl+C = quit, unless the app consumed it
                    // (its own handler/shortcut/action overrides it).
                    if !consumed && is_default_quit(&ui_event) {
                        *quit = true;
                    }
                    if app.quit_requested() {
                        *quit = true;
                    }
                }
            }
        }
    }

    fn apply_resize(&mut self, app: &mut App, size: Size) {
        if size == self.size || size.is_empty() {
            return;
        }
        self.size = size;
        let blank = Cell::EMPTY;
        self.overlays.ensure_root(size);
        // Image placements are geometry-relative; re-emit them (the
        // full-repaint pass below rewrites the cells beneath).
        {
            let mut store = self.overlays.store().borrow_mut();
            for img in store.images.iter_mut() {
                img.dirty = true;
            }
        }
        self.frame.resize(size, blank);
        self.prev.resize(size, blank);
        // The terminal's actual content after a resize is unknown (the
        // emulator reflowed or cleared it its own way). Poison `prev` so
        // the diff re-emits every cell of the next frame instead of
        // trusting a model of a screen that no longer exists.
        self.poison_prev();
        // Through App::set_viewport (RT2-9: tree-direct left
        // App::viewport() reporting the stale size forever).
        app.set_viewport(size);
    }

    /// Capability upgrade (probe completed): emission strategy changed
    /// (color depth, sync brackets, graphics channel), so the next frame
    /// must re-present everything even though the scene is unchanged.
    fn apply_caps_upgrade(&mut self, _app: &mut App) {
        let fresh = present_caps_from(&self.caps);
        if fresh != self.present_caps {
            self.present_caps = fresh;
            self.poison_prev();
            let mut store = self.overlays.store().borrow_mut();
            for layer in store.layers.iter_mut() {
                layer.surface_mut().damage_all();
            }
            // The graphics ladder may pick a better channel now.
            for img in store.images.iter_mut() {
                img.dirty = true;
            }
            drop(store);
            reactive::request_frame();
        }
    }

    /// Make every cell of `prev` unequal to any real content so the next
    /// diff emits the full frame. Glyph stays EMPTY; the impossible color
    /// pair does the work (alpha 7 never occurs: ui colors are opaque or
    /// fully transparent by convention).
    fn poison_prev(&mut self) {
        let poison = Cell::EMPTY
            .with_fg(crate::base::Rgba::new(1, 2, 3, 7))
            .with_bg(crate::base::Rgba::new(3, 2, 1, 7));
        self.prev.fill_rect(Rect::from_size(self.size), poison);
    }
}

/// The presenter's view of KERNEL's capabilities — KERNEL's own
/// conversion (`Capabilities::present_caps`, cycle 3) is the one
/// mapping; the hand-assembly this replaced silently zeroed
/// `undercurl`/`underline_color` after the caps fields landed (their
/// cycle-3 reminder).
fn present_caps_from(caps: &Capabilities) -> PresentCaps {
    caps.present_caps()
}

/// Keep only the LAST of each consecutive run of plain mouse-Move
/// events (order-preserving in-place compaction). See the call site for
/// the full coalescing rule.
fn coalesce_moves(events: &mut Vec<Event>) {
    let is_move =
        |e: &Event| matches!(e, Event::Mouse(m) if m.kind == crate::input::MouseKind::Move);
    let mut keep = 0usize;
    for i in 0..events.len() {
        let dropped = is_move(&events[i]) && events.get(i + 1).map(&is_move).unwrap_or(false);
        if !dropped {
            events.swap(keep, i);
            keep += 1;
        }
    }
    events.truncate(keep);
}

/// Clip to the viewport, drop empties and rects wholly contained in an
/// earlier one. Overlapping-but-not-contained rects stay separate:
/// double-painting a sliver is idempotent and cheaper than a rect union
/// pass that can only grow the area.
fn coalesce_damage(damage: &mut Vec<Rect>, viewport: Rect) {
    for r in damage.iter_mut() {
        *r = r.intersect(viewport);
    }
    damage.retain(|r| !r.is_empty());
    let mut kept: Vec<Rect> = Vec::with_capacity(damage.len());
    for &r in damage.iter() {
        let contained = kept.iter().any(|k| k.intersect(r) == r);
        if !contained {
            kept.retain(|k| r.intersect(*k) != *k); // drop rects r swallows
            kept.push(r);
        }
    }
    *damage = kept;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::ColorDepth;

    #[test]
    fn coalesce_moves_keeps_last_of_runs_and_every_other_kind() {
        use crate::base::Point;
        use crate::input::{MouseButton as B, MouseEvent as M, MouseKind as K};
        let mv = |x: i32| {
            Event::Mouse(M::new(
                K::Move,
                B::Left,
                Point::new(x, 0),
                crate::input::Mods::NONE,
            ))
        };
        let down = Event::Mouse(M::new(
            K::Down,
            B::Left,
            Point::new(9, 0),
            crate::input::Mods::NONE,
        ));
        let mut evs = vec![mv(1), mv(2), mv(3), down.clone(), mv(4), mv(5)];
        coalesce_moves(&mut evs);
        // Runs collapse to their last member; Down survives; order holds.
        assert_eq!(evs.len(), 3);
        assert!(matches!(&evs[0], Event::Mouse(m) if m.pos.x == 3));
        assert!(matches!(&evs[1], Event::Mouse(m) if m.kind == K::Down));
        assert!(matches!(&evs[2], Event::Mouse(m) if m.pos.x == 5));
        // Drags never coalesce (capture handlers see every step).
        let drag = |x: i32| {
            Event::Mouse(M::new(
                K::Drag,
                B::Left,
                Point::new(x, 0),
                crate::input::Mods::NONE,
            ))
        };
        let mut drags = vec![drag(1), drag(2), drag(3)];
        coalesce_moves(&mut drags);
        assert_eq!(drags.len(), 3);
    }

    #[test]
    fn coalesce_drops_contained_and_clips() {
        let vp = Rect::new(0, 0, 20, 10);
        let mut d = vec![
            Rect::new(0, 0, 5, 5),
            Rect::new(1, 1, 2, 2),    // inside the first: dropped
            Rect::new(18, 8, 10, 10), // clipped to viewport
            Rect::new(-5, -5, 3, 3),  // fully outside: dropped
        ];
        coalesce_damage(&mut d, vp);
        assert_eq!(d, vec![Rect::new(0, 0, 5, 5), Rect::new(18, 8, 2, 2)]);
    }

    #[test]
    fn present_caps_mapping_delegates_to_kernel_including_underline() {
        let mut caps = Capabilities::default();
        assert_eq!(present_caps_from(&caps).color, ColorDepth::Ansi16);
        caps.colors_256 = true;
        assert_eq!(present_caps_from(&caps).color, ColorDepth::Xterm256);
        caps.truecolor = true;
        caps.undercurl = true;
        caps.underline_color = true;
        let pc = present_caps_from(&caps);
        assert_eq!(pc.color, ColorDepth::TrueColor);
        // The cycle-3 KERNEL reminder: these two must flow through
        // (the old hand-assembly pinned them false forever).
        assert!(
            pc.undercurl,
            "undercurl capability must reach the presenter"
        );
        assert!(
            pc.underline_color,
            "underline color capability must reach the presenter"
        );
    }
}
