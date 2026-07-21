//! `EventReader`: the app-facing glue between a `term::Terminal` and the
//! `Parser`, owning the ESC-disambiguation deadlines and the active
//! capability probe loop.
//!
//! OWNER: KERNEL. Deadline rationale: `docs/design/term-input.md` §3.2.

use super::parser::{Parser, Pending};
use super::Event;
use crate::base::{PixelSize, Point, Result};
use crate::term::caps::Capabilities;
use crate::term::probe::ActiveProbe;
use crate::term::{TermRead, Terminal};
use std::collections::VecDeque;
use std::time::{Duration, Instant};

/// A bare ESC becomes the Esc key after this long. Short: a human tapping
/// Esc feels latency past ~50 ms; sequence bytes from the terminal itself
/// arrive within a millisecond except over the worst links.
pub const DEFAULT_ESC_TIMEOUT: Duration = Duration::from_millis(30);
/// A torn escape sequence flushes as `Unknown`/Alt+intro after this long.
/// Much longer than the ESC timeout: SSH legitimately splits sequences
/// across packets, and destroying a valid arrow key would be worse than a
/// late Unknown.
pub const DEFAULT_SEQ_TIMEOUT: Duration = Duration::from_millis(500);

/// The app-facing input driver: multiplexes terminal reads into events,
/// owns the ESC-disambiguation deadlines, resize/wake passthrough, and
/// the SGR-Pixels conversion. One per terminal.
pub struct EventReader {
    parser: Parser,
    queue: VecDeque<Event>,
    scratch: Vec<Event>,
    /// When the currently-pending partial input times out, if it does.
    pending_since: Option<Instant>,
    /// SGR-Pixels (1016) active: divide mouse coordinates by this cell
    /// geometry into cells, keeping the raw point in `MouseEvent::pixel`.
    pixel_cell: Option<PixelSize>,
    /// Bare-ESC resolution deadline (default 30 ms). Public by contract:
    /// deterministic tests set it directly (REDTEAM's virtual-deadline
    /// technique relies on field access, not builders).
    pub esc_timeout: Duration,
    /// Torn-sequence flush deadline (default 500 ms; SSH splits packets).
    pub seq_timeout: Duration,
}

impl Default for EventReader {
    fn default() -> Self {
        Self::new()
    }
}

impl EventReader {
    /// A reader with the default deadlines.
    pub fn new() -> Self {
        EventReader {
            parser: Parser::new(),
            queue: VecDeque::new(),
            scratch: Vec::new(),
            pending_since: None,
            pixel_cell: None,
            esc_timeout: DEFAULT_ESC_TIMEOUT,
            seq_timeout: DEFAULT_SEQ_TIMEOUT,
        }
    }

    /// Direct access to the underlying parser (probe glue, tests).
    pub fn parser(&mut self) -> &mut Parser {
        &mut self.parser
    }

    /// Turn on SGR-Pixels interpretation. Deliberately REQUIRES the cell
    /// geometry: the 1016 wire grammar is byte-identical to 1006, so a
    /// reader that cannot divide would be forced to emit raw pixels as
    /// cell coordinates — the exact lie this API shape forbids. The app
    /// recipe: enable mode 1016 at the terminal only when
    /// `caps.sgr_pixel_mouse && caps.cell_pixel_size.is_some()`, and call
    /// this with that same cell size (refresh after resizes — font zoom
    /// changes it).
    pub fn enable_pixel_mouse(&mut self, cell: PixelSize) {
        if !cell.is_empty() {
            self.pixel_cell = Some(cell);
        }
    }

    /// Back to cell-unit interpretation (mode 1016 off at the terminal).
    pub fn disable_pixel_mouse(&mut self) {
        self.pixel_cell = None;
    }

    /// Deadline at which pending partial input must be force-resolved.
    fn pending_deadline(&self) -> Option<Instant> {
        let since = self.pending_since?;
        match self.parser.pending() {
            Pending::None => None,
            Pending::BareEsc => Some(since + self.esc_timeout),
            Pending::Sequence => Some(since + self.seq_timeout),
        }
    }

    fn note_pending(&mut self) {
        // Anchor the deadline at the last byte's arrival; refreshed every
        // feed so a slowly-trickling sequence keeps its full allowance.
        self.pending_since = match self.parser.pending() {
            Pending::None => None,
            _ => Some(Instant::now()),
        };
    }

    fn drain_scratch(&mut self) {
        if let Some(cell) = self.pixel_cell {
            for ev in &mut self.scratch {
                convert_pixels(cell, ev);
            }
        }
        self.queue.extend(self.scratch.drain(..));
    }

    /// Wait for the next event until `deadline` (`None` = indefinitely).
    ///
    /// `Ok(None)` means "service your loop": either the deadline expired
    /// or a [`crate::term::TerminalWaker`] fired. Both are deliberately the
    /// same return — a loop that drains posted work and recomputes its
    /// deadline on every `None` handles both correctly, and distinguishing
    /// them would invite callers to skip the drain on timeouts. Exactly one
    /// event per call; buffered events drain before the fd is touched
    /// again.
    pub fn poll_event(
        &mut self,
        term: &mut dyn Terminal,
        deadline: Option<Instant>,
    ) -> Result<Option<Event>> {
        loop {
            if let Some(ev) = self.queue.pop_front() {
                return Ok(Some(ev));
            }
            // Wake at whichever comes first: caller deadline or the pending
            // ESC resolution point.
            let pending = self.pending_deadline();
            let effective = match (deadline, pending) {
                (Some(a), Some(b)) => Some(a.min(b)),
                (a, b) => a.or(b),
            };
            match term.read(effective)? {
                TermRead::Input(bytes) => {
                    self.parser.feed(bytes, &mut self.scratch);
                    self.drain_scratch();
                    self.note_pending();
                }
                TermRead::Resize(sz) => return Ok(Some(Event::Resize(sz))),
                TermRead::Wake => return Ok(None),
                TermRead::Idle => {
                    let now = Instant::now();
                    if let Some(p) = self.pending_deadline() {
                        if now >= p {
                            self.parser.flush_pending(&mut self.scratch);
                            self.drain_scratch();
                            self.pending_since = None;
                            continue; // deliver what the flush produced
                        }
                    }
                    if let Some(d) = deadline {
                        if now >= d {
                            return Ok(None);
                        }
                    }
                    // Spurious idle (pending deadline shortened the read):
                    // loop and re-wait.
                }
            }
        }
    }
}

impl EventReader {
    /// Drain everything available into `out` with at most one blocking
    /// wait: block until the first event (or `deadline`), then keep
    /// appending whatever is already decoded or immediately readable —
    /// never blocking again. Returns the number of events appended.
    ///
    /// `Ok(0)` carries the same meaning as `poll_event`'s `Ok(None)`:
    /// deadline expired or a waker fired — service the loop. Loops built
    /// on this should drain posted work at the top of EVERY iteration
    /// (batch dispatch does not change the wake contract).
    ///
    /// Syscall shape: the internal queue already amortizes reads (one
    /// terminal read can yield many events); this makes the burst
    /// explicit — one blocking wait + one zero-timeout drain pass per
    /// batch instead of one zero-timeout confirmation per event when an
    /// app loop calls `poll_event` until `None`.
    ///
    /// ```no_run
    /// use abstracttui::input::EventReader;
    /// use abstracttui::term::{EnterOptions, Terminal, UnixTerminal};
    /// use std::time::{Duration, Instant};
    ///
    /// # fn main() -> abstracttui::base::Result<()> {
    /// let mut term = UnixTerminal::new()?;
    /// term.enter(&EnterOptions::default())?;
    /// let mut reader = EventReader::new();
    /// let mut batch = Vec::new(); // reused across turns: no per-turn alloc
    /// loop {
    ///     batch.clear();
    ///     // drain posted jobs / recompute animation deadline here…
    ///     let deadline = Some(Instant::now() + Duration::from_millis(250));
    ///     reader.poll_many(&mut term, &mut batch, deadline)?;
    ///     for event in &batch {
    ///         // dispatch per event; Ok(0) meant deadline-or-wake
    ///         # let _ = event;
    ///     }
    ///     # break;
    /// }
    /// # term.leave()
    /// # }
    /// ```
    pub fn poll_many(
        &mut self,
        term: &mut dyn Terminal,
        out: &mut Vec<Event>,
        deadline: Option<Instant>,
    ) -> Result<usize> {
        let mut appended = 0usize;
        // One blocking wait for the first event.
        match self.poll_event(term, deadline)? {
            Some(ev) => {
                out.push(ev);
                appended += 1;
            }
            None => return Ok(0),
        }
        // Opportunistic non-blocking drain: queued events pop free; when
        // the queue empties, one zero-timeout read grabs anything that
        // arrived meanwhile, then `None` ends the burst.
        loop {
            let now = Instant::now();
            match self.poll_event(term, Some(now))? {
                Some(ev) => {
                    out.push(ev);
                    appended += 1;
                }
                None => return Ok(appended),
            }
        }
    }
}

/// Pixel->cell conversion for a mouse event decoded while SGR-Pixels is
/// active. Division floors; coordinates clamp at zero (a pixel inside
/// cell (0,0) must never go negative through rounding).
fn convert_pixels(cell: PixelSize, ev: &mut Event) {
    if let Event::Mouse(m) = ev {
        let raw = m.pos;
        m.pixel = Some(raw);
        m.pos = Point::new(
            (raw.x / i32::from(cell.w)).max(0),
            (raw.y / i32::from(cell.h)).max(0),
        );
    }
}

/// Run the active capability probe: write the query batch, then pump events
/// until the DA1 sentinel or `timeout`. User input arriving mid-probe is
/// returned (in order) instead of being dropped; caps replies are folded
/// into `caps`. Safe against terminals that answer nothing: worst case is
/// one quiet `timeout` wait, by construction of the read loop.
///
/// A `dumb`/empty-TERM environment skips the probe entirely — not a single
/// query byte is written (RT1-6b): the same rule that gives a dumb
/// terminal no escapes forbids interrogating it with escapes.
pub fn probe_active(
    term: &mut dyn Terminal,
    reader: &mut EventReader,
    caps: &mut Capabilities,
    timeout: Duration,
) -> Result<Vec<Event>> {
    if caps.dumb {
        return Ok(Vec::new());
    }
    // Seed cell pixel geometry from the platform before asking the wire;
    // the CSI 16 t reply refines/overrides only when it answers sanely.
    let _ = crate::term::probe::refresh_cell_pixel_size(term, caps);
    let mut probe = ActiveProbe::for_caps(caps);
    term.write(&probe.full_query_bytes())?;
    term.flush()?;
    let deadline = Instant::now() + timeout;
    // Under tmux, wrapped replies (outer terminal) can trail tmux's own
    // DA1 sentinel by one extra round trip: grant them a bounded grace.
    let mut grace: Option<Instant> = None;
    let mut passthrough = Vec::new();
    loop {
        let effective = grace.map_or(deadline, |g| g.min(deadline));
        match reader.poll_event(term, Some(effective))? {
            Some(Event::CapsReply(reply)) => {
                if probe.on_reply(&reply, caps) {
                    break; // sentinel + every awaited answer is in
                }
                if probe.sentinel_passed() && probe.awaiting_wrapped() && grace.is_none() {
                    grace = Some(Instant::now() + crate::term::probe::TMUX_GRACE);
                }
            }
            Some(other) => passthrough.push(other),
            None => {
                // Ok(None) is deadline-or-wake; a waker firing mid-probe
                // (scheduler startup posting jobs) must not end the probe
                // early — only a real deadline does.
                let now = Instant::now();
                if now >= deadline {
                    break; // mute terminal or slow replies: passive result stands
                }
                if grace.is_some_and(|g| now >= g) {
                    break; // sentinel passed, wrapped stragglers never came
                }
            }
        }
    }
    Ok(passthrough)
}

// Scripted-terminal tests live beside this file; the path attribute
// keeps them a true child module (private items stay visible).
#[cfg(test)]
#[path = "reader_tests.rs"]
mod tests;
