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
use super::overlays::{Overlays, ROOT_LAYER_ID};
use super::selection::{selection_pane, MouseCapture, Selection, SelectionAct};
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
    /// Kitty keyboard flags the session currently has pushed (enter-time
    /// value, updated when the probe upgrade pushes them — 0293). Only
    /// meaningful with `kitty_auto`.
    kitty_flags: KittyFlags,
    /// The enter options were DERIVED from capabilities (`cfg.enter` was
    /// `None`), so the driver owns the kitty keyboard posture and may
    /// upgrade it when the probe proves the protocol. An explicit
    /// `RunConfig::enter` is the embedder's exact posture: never touched.
    kitty_auto: bool,
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
    /// Image-ladder degradation labels awaiting the notices lane
    /// (phase U owns signal writes; D2 only queues). Deduped via
    /// `image_notice_seen` — one line per DISTINCT warning per run.
    pub(super) pending_image_notices: Vec<String>,
    pub(super) image_notice_seen: std::collections::HashSet<String>,
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
    /// Screen-text selection layer (0270 tier 3) — the same app-thread
    /// state `app::selection::selection()` hands components.
    selection: Selection,
    /// Tier-2 mouse-reporting suspend requests, drained per turn.
    mouse_capture: MouseCapture,
    /// OSC 52 payloads awaiting presenter-custody emission (§6): cell
    /// runs first, then protocol payloads, one flush.
    pending_clipboard: Vec<Vec<u8>>,
    /// One-time labeled notice latch: OSC 52 copy on an unadvertising
    /// terminal (fire-and-forget — "may be ignored" is the honest claim).
    osc52_noticed: bool,
    /// Zero-collapse diagnostics drained from the trees during phase L/D
    /// of the PREVIOUS frame, forwarded into the notices lane at the next
    /// phase U (signal writes belong to phase U, never the draw phases).
    collapse_pending: Vec<String>,
    /// Everything forwarded this run (bounded like the tree buffer) —
    /// flushed to stderr by `App::run` AFTER the terminal is restored,
    /// so headless/exit visibility survives without corrupting a live
    /// alternate screen.
    collapse_log: Vec<String>,
}

impl Driver {
    /// Enter the terminal session and prepare the pipeline. Emits the
    /// enter bytes and (optionally) the probe queries; does NOT render —
    /// the first `turn` does, from the mount-time damage.
    pub fn new(app: &mut App, term: &mut dyn Terminal, cfg: RunConfig) -> Result<Driver> {
        let caps = cfg.caps.unwrap_or_else(Capabilities::detect_env);
        let kitty_auto = cfg.enter.is_none();
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
        // Publish the env-pass capabilities into the reactive view
        // (`app::use_caps`, 0295/0685); probe folds upgrade it later.
        super::caps::publish_caps(&caps);
        // Key-state fidelity (games/0700): Full only when kitty release
        // events are actually live on THIS session (protocol spoken +
        // event-type flags pushed). Republished at the 0293 upgrade.
        super::keys::publish_fidelity(super::keys::release_events_live(
            &caps,
            enter.kitty_keyboard,
        ));

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
        // A fresh session starts with no visible selection (the previous
        // driver's screen-space region is meaningless on a new frame);
        // the app's select-mode choice survives.
        let selection = super::selection::selection();
        selection.reset_session();
        Ok(Driver {
            reader: EventReader::new(),
            present_caps: present_caps_from(&caps),
            kitty_flags: enter.kitty_keyboard,
            kitty_auto,
            caps,
            probe,
            comp: Compositor::new(),
            diff: FrameDiff::new(),
            presenter: Presenter::new(),
            overlays,
            image_session: ImageSession::new(),
            pending_image_bytes: Vec::new(),
            pending_image_notices: Vec::new(),
            image_notice_seen: std::collections::HashSet::new(),
            frame: Surface::new(size, blank),
            prev: Surface::new(size, blank),
            size,
            pending: Vec::new(),
            burst: Vec::new(),
            probe_grace: None,
            now_fn: None,
            out: Vec::new(),
            scratch_damage: Vec::new(),
            selection,
            mouse_capture: super::selection::mouse_capture(),
            pending_clipboard: Vec::new(),
            osc52_noticed: false,
            collapse_pending: Vec::new(),
            collapse_log: Vec::new(),
        })
    }

    /// The zero-collapse diagnostics forwarded during this run (debug
    /// builds). `App::run` prints them to stderr after teardown.
    pub(crate) fn collapse_log(&self) -> &[String] {
        &self.collapse_log
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
        // Key-state edges seal per turn (games/0700): last turn's
        // press/release pulses clear before this turn's events fold in.
        // One flag read when no consumer ever armed the service.
        super::keys::begin_turn();
        // tmux probe grace expired with wrapped replies still missing:
        // finalize on the evidence in hand (passthrough-off sessions
        // never answer — spending the grace once is the design).
        if let Some(deadline) = self.probe_grace {
            if now >= deadline {
                self.probe_grace = None;
                if self.probe.take().is_some() {
                    self.apply_caps_upgrade(app, term);
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
            self.handle_event(app, term, ev, &mut quit);
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
            self.handle_event(app, term, ev, &mut quit);
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

        // ---- engine verb drains (still phase U) ------------------------
        // App-queued clipboard writes (`app::selection::copy_to_clipboard`)
        // become custody-emitted OSC 52 payloads on this frame.
        for text in self.selection.take_pending_copies() {
            self.queue_clipboard_text(app, &text);
        }
        // Tier-2 mouse-reporting flip (latest request wins). Refusal is a
        // labeled degradation, never a dead loop: scripted terminals
        // without session tracking honestly decline the verb.
        if let Some(on) = self.mouse_capture.take_request() {
            if let Err(e) = term.set_mouse_reporting(on).and_then(|()| term.flush()) {
                app.push_startup_notice(format!("mouse capture: suspend verb unavailable ({e})"));
            }
        }
        // Public full-redraw verb (first-app/0299,
        // `app::request_full_redraw`): the terminal's content can no
        // longer be trusted (external clear — Cmd+K, `\033c`), so
        // resync exactly like suspend-resume does. Drained before the
        // frame decision: a request from this turn's own key handler
        // renders — and re-emits everything — this same turn.
        if super::redraw::take_full_redraw_request() {
            self.resync_unknown_screen();
        }
        // Zero-collapse diagnostics drained from last frame's solve reach
        // the app here (phase U owns signal writes). The notices lane is
        // the in-session surface; stderr waits until teardown.
        for note in self.collapse_pending.drain(..) {
            if self.collapse_log.len() < 64 {
                self.collapse_log.push(note.clone());
            }
            app.push_startup_notice(note);
        }
        // Image-ladder degradation labels (queued by phase D2, deduped
        // there): the charter says degradations are labeled, never
        // silent — the driver used to drop these on the floor.
        for note in self.pending_image_notices.drain(..) {
            if self.collapse_log.len() < 64 {
                self.collapse_log.push(note.clone());
            }
            app.push_startup_notice(note);
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
        // Collect this frame's zero-collapse diagnostics (debug builds;
        // both drains are empty-vec no-ops in release). Forwarded at the
        // NEXT phase U — draw phases never write signals.
        self.collapse_pending
            .extend(app.tree().take_collapse_notices());
        self.collapse_pending
            .extend(self.overlays.take_collapse_notices());

        // ---- phase D: clear + redraw damaged regions (root layer) ------
        // The root surface is STOLEN from the store while user draw code
        // runs (the overlay borrow rule); overlay content paints next.
        let viewport = Rect::from_size(self.size);
        let mut damage = app.tree().take_damage();
        // Image placements vacated since last frame (moved / removed /
        // channel-switched) fold their rects into THIS frame's damage so
        // the tree repaints them from truth, and poison `prev` where the
        // terminal holds pixels the cell model cannot see (details in
        // driver_images.rs).
        self.pre_image_pass(&mut damage);
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

        // ---- selection pre-flatten damage (0270 tier 3) -----------------
        // When the selection region changed, its OLD highlight cells and
        // its NEW row spans must recompose from truth this frame: damage
        // them on the root layer (origin ZERO: screen == layer space) so
        // the compositor rebuilds the full z-stack there — the repair for
        // what the patch below painted last frame, and fresh ground for
        // what it paints now. Zero cost while nothing changed.
        {
            let mut store = self.overlays.store().borrow_mut();
            if let Some(root) = store.index_of(ROOT_LAYER_ID) {
                self.selection
                    .add_flatten_damage(store.layers[root].surface_mut());
            }
        }

        // ---- phase C: flatten (root + overlays, z-sorted) ---------------
        self.scratch_damage.clear();
        {
            let mut store = self.overlays.store().borrow_mut();
            let flat = self.comp.flatten(&mut self.frame, &mut store.layers);
            self.scratch_damage.extend_from_slice(flat);
        }

        // ---- selection patch: recolor selected cells post-flatten -------
        // Glyphs kept, inks replaced (theme selection tokens). Everything
        // this can CHANGE is already inside the flatten damage: region
        // deltas were damaged above, and content changes beneath an
        // unchanged selection arrive damaged by their own layers — cells
        // the compositor left alone get byte-identical rewrites, which
        // the diff never emits.
        self.selection.paint_into(
            &mut self.frame,
            theme.tokens.get(TokenId::SelectionFg),
            theme.tokens.get(TokenId::SelectionBg),
        );

        // ---- phase P: diff -> present -> image payloads -> ONE flush ----
        // Scroll-aware diff (RENDER cycle 5): when the damage reads as
        // one vertical band shift, the terminal scrolls (DECSTBM+SU/SD,
        // ~8-9x fewer bytes on list/log workloads) and only residuals
        // repaint; detection declining yields plain-compute bytes.
        //
        // Byte-channel image guard (MEDIA study 2): terminals scroll
        // protocol images WITH the text (the kitty spec mandates it;
        // sixel pixels scroll on xterm-class emulators), which would
        // move terminal-held placements out from under the session's
        // bookkeeping. While such images are live, take the plain diff —
        // correct pixels over the byte win.
        self.out.clear();
        let runs = if self.image_session.live_byte_slots() > 0 {
            crate::render::ScrolledRuns::plain(self.diff.compute(
                &self.prev,
                &self.frame,
                &self.scratch_damage,
            ))
        } else {
            self.diff
                .compute_scrolled(&self.prev, &self.frame, &self.scratch_damage)
        };
        self.presenter
            .emit_scrolled(runs, &self.frame, &self.present_caps, &mut self.out);
        // Protocol payloads AFTER cell runs, through presenter custody
        // (close SGR/link, absolute CUP, invalidate) — same buffer, so
        // the frame still reaches the terminal in one write + one flush.
        for (bytes, at) in self.pending_image_bytes.drain(..) {
            self.presenter.external_write(&mut self.out, &bytes, at);
        }
        // Clipboard payloads (OSC 52) ride the same custody path: after
        // the cell runs, before the single flush. The park point is a
        // formality — the sequence paints nothing — but custody still
        // closes any open SGR/link state and invalidates the cursor.
        for payload in self.pending_clipboard.drain(..) {
            self.presenter
                .external_write(&mut self.out, &payload, Point::ZERO);
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

    fn handle_event(
        &mut self,
        app: &mut App,
        term: &mut dyn Terminal,
        event: Event,
        quit: &mut bool,
    ) {
        // Key-state tap (games/0700), PRE-conversion and PRE-routing:
        // key state is a physical fact — observed even for events a
        // modal, the selection layer, or the routing drop consumes.
        match &event {
            Event::Key(k) => super::keys::on_key_event(k),
            Event::FocusLost => super::keys::on_focus_lost(),
            _ => {}
        }
        match event {
            Event::Resize(size) => self.apply_resize(app, size),
            // Focus-regain repaint (first-app/0299 ask 2, opt-in via
            // `app::set_redraw_on_focus_gained`): an externally-cleared
            // terminal is nearly always followed by a focus round-trip,
            // so healing on focus-in makes the failure invisible.
            // Routing drops terminal-focus events anyway (documented on
            // `convert_event`), so consuming the event here loses
            // nothing; with the policy off, the event falls through to
            // the ordinary (dropping) path below.
            Event::FocusGained if super::redraw::redraw_on_focus_gained() => {
                self.resync_unknown_screen();
            }
            Event::CapsReply(reply) => {
                if let Some(probe) = &mut self.probe {
                    // Every fold that CHANGED a capability reaches the
                    // reactive view immediately (0295/0685) — partial
                    // probes (a terminal that never answers DA1) still
                    // surface what they proved. Emission-strategy
                    // upgrades stay gated on probe completion below.
                    let before = self.caps.clone();
                    let done = probe.on_reply(&reply, &mut self.caps);
                    if self.caps != before {
                        super::caps::publish_caps(&self.caps);
                    }
                    if done {
                        self.probe = None;
                        self.probe_grace = None;
                        self.apply_caps_upgrade(app, term);
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
                // Screen-text selection intercept (0270 tier 3): while
                // select mode is on, the layer owns left DRAGS (and the
                // gesture-ending Up) — and, while a selection is VISIBLE,
                // the copy/clear keys (Enter / c / Ctrl+C / Esc) plus the
                // dismissal click. Plain clicks pass through to the
                // widgets (click-through, 0285: consuming every Down/Up
                // made every Button dead by mouse); everything else
                // (wheel, motion, other buttons, all other keys) routes
                // normally, so scrolling keeps working mid-selection.
                // Deliberately ahead of overlay routing: select mode is
                // an explicit user mode and may copy from modal content
                // too (the pane clamp resolves overlay tree panes).
                let overlays = &self.overlays;
                let size = self.size;
                match self
                    .selection
                    .on_input(&other, &mut |p| selection_pane(app, overlays, size, p))
                {
                    SelectionAct::Pass => {}
                    SelectionAct::Consumed => return,
                    SelectionAct::Claim => {
                        // The gesture's Down PASSED to the widgets
                        // (click-through, 0285) and just became a
                        // selection drag: resolve that press WITHOUT a
                        // click before the layer owns the gesture. Every
                        // tree with a live pointer capture receives a
                        // release outside every rect — release-inside-
                        // decides widgets (Button) un-press without
                        // firing — and the capture drops, so the NEXT
                        // real click routes fresh instead of into a
                        // stale captured target.
                        self.overlays.cancel_pointer_press();
                        app.tree().cancel_pointer_press();
                        return;
                    }
                    SelectionAct::Copy(region) => {
                        // The act CARRIES the region because a copy ENDS
                        // the gesture (backlog 0290): the selection layer
                        // consumed its own state before answering, so a
                        // region can never linger to swallow the app's
                        // next Enter/`c` keystrokes (the composer
                        // footgun). Extraction reads the last composed
                        // frame — exactly what the highlight showed.
                        let text = super::selection::extract_text(&self.frame, &region);
                        self.queue_clipboard_text(app, &text);
                        return;
                    }
                }
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

    pub(super) fn apply_resize(&mut self, app: &mut App, size: Size) {
        if size == self.size || size.is_empty() {
            return;
        }
        self.size = size;
        // Screen-space selection geometry is meaningless after a resize;
        // the prev-poison below repaints every cell, so clearing state is
        // all the repair needed.
        self.selection.on_resize();
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
        // The CURSOR is as unknowable as the content: emulators move the
        // physical cursor with the reflowed line (macOS Terminal anchored
        // it to the bottom in the 0298 field incident), so the parked
        // virtual cursor is a ghost now. Poisoning `prev` re-emits every
        // CELL, but the first run would still be PLACED by relative
        // motion from that ghost — offsetting the entire frame and
        // leaving a stale band where the old frame peeked out (backlog
        // 0298). Invalidate the presenter so the post-resize frame
        // re-anchors with absolute CUP and a reset-based SGR. Both
        // halves of "the screen is unknown" belong together: cells
        // (poison) and cursor/pen (invalidate).
        self.presenter.invalidate();
        // Through App::set_viewport (RT2-9: tree-direct left
        // App::viewport() reporting the stale size forever).
        app.set_viewport(size);
    }

    /// Capability upgrade (probe completed): emission strategy changed
    /// (color depth, sync brackets, graphics channel), so the next frame
    /// must re-present everything even though the scene is unchanged.
    /// Also the kitty enter-flags moment (0293): a probe that PROVED the
    /// keyboard protocol on a terminal the env pass could not claim
    /// pushes the standard flags now — Shift+Enter-class chords start
    /// working on iTerm2 ≥ 3.5, VS Code/Cursor, and Warp without a
    /// restart. The terminal's session bookkeeping owns the pop (leave
    /// pops the entry; suspend pops and re-pushes symmetrically).
    fn apply_caps_upgrade(&mut self, app: &mut App, term: &mut dyn Terminal) {
        if self.kitty_auto && self.kitty_flags.is_empty() && self.caps.kitty_keyboard {
            let flags = KittyFlags::standard();
            // Flush immediately: a kitty-only upgrade may not render a
            // frame this turn, and unflushed flags on an idle app would
            // arm the protocol arbitrarily late.
            match term.set_kitty_keyboard(flags).and_then(|()| term.flush()) {
                Ok(()) => self.kitty_flags = flags,
                Err(e) => app.push_startup_notice(format!(
                    "kitty keyboard: probe proved support but the flags push \
                     is unavailable ({e})"
                )),
            }
        }
        // The key-state fidelity follows the flags (games/0700): the
        // moment releases become live mid-session, hold semantics do too.
        super::keys::publish_fidelity(super::keys::release_events_live(
            &self.caps,
            self.kitty_flags,
        ));
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

    /// Tier-2 verb, immediate form — for embedders driving their own
    /// turns (component code uses `app::selection::mouse_capture()`,
    /// which the driver applies on its next turn). Emits the entered
    /// mode's disarm/re-arm pair and flushes.
    pub fn set_mouse_reporting(&mut self, term: &mut dyn Terminal, on: bool) -> Result<()> {
        term.set_mouse_reporting(on)?;
        term.flush()
    }

    /// Queue one OSC 52 clipboard write for presenter-custody emission.
    /// Empty/whitespace text is refused (an empty OSC 52 payload CLEARS
    /// the clipboard — a surprise, never a copy). Capability honesty:
    /// OSC 52 is write-only fire-and-forget, so an unadvertising
    /// terminal gets the bytes anyway (harmless — unsupporting terminals
    /// ignore the frame, and the env pass is conservative) plus a
    /// one-time labeled notice that the copy may have been ignored.
    fn queue_clipboard_text(&mut self, app: &mut App, text: &str) {
        if text.trim().is_empty() {
            return;
        }
        if !self.caps.osc52_copy && !self.osc52_noticed {
            self.osc52_noticed = true;
            app.push_startup_notice(
                "clipboard: OSC 52 not advertised by this terminal — copies may be ignored",
            );
        }
        self.pending_clipboard
            .push(crate::term::verbs::clipboard_copy_bytes(text));
        reactive::request_frame();
    }

    /// Make every cell of `prev` unequal to any real content so the next
    /// diff emits the full frame. Glyph stays EMPTY; the impossible color
    /// pair does the work (alpha 7 never occurs: ui colors are opaque or
    /// fully transparent by convention).
    fn poison_prev(&mut self) {
        self.poison_prev_rect(Rect::from_size(self.size));
    }

    /// The terminal's content is UNKNOWN at the current geometry (a
    /// job-control suspend returned: the alt screen came back blank
    /// and the restore reset cursor/pen): poison `prev` AND invalidate
    /// the presenter (both halves of "the screen is unknown" — the
    /// apply_resize rule), damage every layer so the flatten produces
    /// regions for the diff to re-emit, and re-place images. Used by
    /// the suspend orchestration (`driver_suspend.rs`, cycle-2 review
    /// I-2) and, since first-app/0299 shipped, as the drain target of
    /// the public verbs: `app::request_full_redraw` (component-
    /// reachable Ctrl+L class, drained in `turn`'s phase U) and the
    /// opt-in `app::set_redraw_on_focus_gained` (FocusGained handling
    /// in `handle_event`).
    pub(super) fn resync_unknown_screen(&mut self) {
        self.poison_prev();
        self.presenter.invalidate();
        let mut store = self.overlays.store().borrow_mut();
        for layer in store.layers.iter_mut() {
            layer.surface_mut().damage_all();
        }
        let image_keys: Vec<u64> = store.images.iter().map(|e| e.id).collect();
        for img in store.images.iter_mut() {
            img.dirty = true;
        }
        drop(store);
        // The terminal-side IMAGE state (kitty uploads + placements,
        // iTerm2/sixel pixels) is as unknown as the cells — and the
        // dirty flag alone is not enough: `ImageSession::sync` answers
        // `Unchanged` for an unmoved same-version slot. Forget what
        // the session believes the terminal holds so the next sync
        // re-emits in full: kitty through `release` (the delete bytes
        // are harmless where the upload is already gone, and skipping
        // them would leak the upload where it survived — the session's
        // own no-forget rule), cursor-paint channels through
        // `invalidate_slot`. Before this, a resumed/healed screen kept
        // its cells but silently lost every protocol image.
        let gfx_caps = self.caps.graphics();
        for key in image_keys {
            match self.image_session.slot_info(key) {
                Some((crate::gfx::Channel::Kitty, _)) => {
                    let mut sink = super::driver_images::BufSink(&mut self.pending_image_bytes);
                    self.image_session.release(&mut sink, key, &gfx_caps);
                }
                Some(_) => self.image_session.invalidate_slot(key),
                None => {}
            }
        }
        reactive::request_frame();
    }

    /// Poison one region of the previous-frame model: the next diff
    /// re-emits every cell there even when the model believes them
    /// unchanged. Used whole-screen on resize/caps upgrade and per-rect
    /// when a cursor-paint image (iTerm2/sixel) vacates cells the model
    /// never saw painted over (driver_images pass A).
    pub(super) fn poison_prev_rect(&mut self, rect: Rect) {
        let poison = Cell::EMPTY
            .with_fg(crate::base::Rgba::new(1, 2, 3, 7))
            .with_bg(crate::base::Rgba::new(3, 2, 1, 7));
        self.prev
            .fill_rect(rect.intersect(Rect::from_size(self.size)), poison);
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
