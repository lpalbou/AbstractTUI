//! Streaming markdown session: append deltas, re-parse ONLY the open
//! tail block (backlog 0110).
//!
//! Text that arrives incrementally (model output, a growing log, a slow
//! pipe) should not pay `md::parse` over the whole accumulated source
//! per delta. [`StreamSession`] keeps CLOSED blocks (frozen — parsed
//! once, never revisited) and one OPEN tail region (re-parsed from its
//! start on each append; a paragraph/fence start is O(that block),
//! never O(document)).
//!
//! ## The correctness contract
//!
//! `md::parse` is line-oriented with exactly two multi-line constructs:
//! fenced code (consumes lines until a closing ``` line) and paragraphs
//! (consecutive plain lines soft-join until a blank/block-start line).
//! Therefore `parse(prefix) ++ parse(suffix) == parse(whole)` whenever
//! the cut sits at a line start that is (a) not inside an open fence and
//! (b) not splitting a paragraph joint. The session seals the longest
//! such prefix after every append, using the SAME line classifiers the
//! batch parser uses (this module is a child of `md`, sharing its
//! private helpers — the boundary rules cannot drift). The contract is
//! test-pinned: for any chunking of any input, `finish()` yields blocks
//! identical to `md::parse` of the full source.
//!
//! ## Mid-stream honesty
//!
//! An unclosed fence is an OPEN block: it reports as a `CodeFence` from
//! the moment the opening fence line arrives (EOF-closes, matching the
//! batch parser's recovery), never flapping to literal text while the
//! close is in flight.
//!
//! The incomplete final line needs WORST-CASE classification: `---` may
//! still grow into paragraph text (`---x` soft-joins a preceding
//! paragraph), so a rule fragment is never a safe boundary; committed
//! prefixes (` ``` `, `>`, `# `, `- `, `1. `) can never become
//! paragraph joiners no matter what arrives, so cutting before them is
//! safe.

use super::{heading_level, list_marker, parse, Block, MdStyles};

/// Incremental markdown parser: closed blocks freeze, the open tail
/// re-parses per delta. See the module docs for the contract.
///
/// ```
/// use abstracttui::render::md::{self, MdStyles, StreamSession};
///
/// let styles = MdStyles::default();
/// let mut session = StreamSession::new(styles.clone());
/// session.append("# Title\n\nStreaming **bo");
/// session.append("ld** text.");
/// assert_eq!(session.closed_blocks().len(), 1, "the heading sealed");
/// assert_eq!(
///     session.finish(),
///     md::parse("# Title\n\nStreaming **bold** text.", &styles),
///     "streamed == batch"
/// );
/// ```
pub struct StreamSession {
    styles: MdStyles,
    /// Frozen blocks: parsed once, never revisited.
    closed: Vec<Block>,
    /// Bumped whenever `closed` grows — consumers (0100's feed tail)
    /// typeset newly closed blocks exactly once by watching this.
    closed_revision: u64,
    /// Source of the open region (suffix of the accumulated input).
    tail: String,
    /// Parse of `tail`, refreshed per mutation. Usually one growing
    /// block; transiently more right after a delta that has not been
    /// sealed yet (never observable as wrong output — only as work).
    open_cache: Vec<Block>,
    finished: bool,
    /// Cumulative bytes handed to `md::parse` — the honest cost meter
    /// (closed content must never re-parse; tests assert on deltas).
    bytes_reparsed: u64,
}

impl StreamSession {
    pub fn new(styles: MdStyles) -> StreamSession {
        StreamSession {
            styles,
            closed: Vec::new(),
            closed_revision: 0,
            tail: String::new(),
            open_cache: Vec::new(),
            finished: false,
            bytes_reparsed: 0,
        }
    }

    /// Append a delta (any chunking — token, line, or arbitrary split).
    /// Appending to a finished session is a caller bug: debug builds
    /// assert, release builds ignore the delta (the sealed result must
    /// stay equal to what `finish()` reported).
    pub fn append(&mut self, delta: &str) {
        debug_assert!(!self.finished, "StreamSession::append after finish()");
        if self.finished || delta.is_empty() {
            return;
        }
        self.tail.push_str(delta);
        self.seal();
        self.reparse_tail();
    }

    /// Close the tail (EOF-closes an open fence, exactly like
    /// `md::parse`) and mark the session complete. Returns the full
    /// block list. Idempotent.
    pub fn finish(&mut self) -> Vec<Block> {
        if !self.finished {
            if !self.tail.is_empty() {
                self.bytes_reparsed += self.tail.len() as u64;
                let mut rest = parse(&self.tail, &self.styles);
                if !rest.is_empty() {
                    self.closed.append(&mut rest);
                    self.closed_revision += 1;
                }
                self.tail.clear();
            }
            self.open_cache.clear();
            self.finished = true;
        }
        self.closed.clone()
    }

    /// Frozen blocks (parsed once; index-stable — new closes only
    /// append).
    pub fn closed_blocks(&self) -> &[Block] {
        &self.closed
    }

    /// Bumped whenever `closed_blocks` grows: consumers typeset the
    /// suffix they have not seen and freeze it.
    pub fn closed_revision(&self) -> u64 {
        self.closed_revision
    }

    /// Parse of the open tail region (usually zero or one block; may be
    /// briefly more). Re-computed per append; empty after `finish`.
    pub fn open_blocks(&self) -> &[Block] {
        &self.open_cache
    }

    /// Closed + open, in document order (a convenience clone — hot
    /// consumers use the two slices directly).
    pub fn blocks(&self) -> Vec<Block> {
        let mut out = self.closed.clone();
        out.extend(self.open_cache.iter().cloned());
        out
    }

    pub fn is_finished(&self) -> bool {
        self.finished
    }

    /// Bytes currently in the open (re-parsed-per-delta) region.
    pub fn open_len(&self) -> usize {
        self.tail.len()
    }

    /// Cumulative bytes fed to `md::parse` over the session's lifetime.
    /// Sealed bytes are counted once, open bytes once per append — so a
    /// token appended behind 1,000 closed lines costs O(open block),
    /// and this counter proves it (see the cost test).
    pub fn bytes_reparsed_total(&self) -> u64 {
        self.bytes_reparsed
    }

    /// Move the longest provably-final prefix of `tail` into `closed`.
    ///
    /// A cut at byte offset `off` (a line start) is safe iff:
    /// - no fence opened before `off` is still open, and
    /// - the line starting at `off` cannot soft-join a paragraph left
    ///   open by the line before it (blank lines and block-start lines
    ///   never join; the incomplete final fragment joins unless its
    ///   already-received prefix commits it to a non-paragraph block).
    ///
    /// The end-of-input position (after a trailing newline) is safe iff
    /// no fence is open and no paragraph is open — otherwise a future
    /// delta could still extend the last block.
    fn seal(&mut self) {
        let mut in_fence = false;
        let mut para_open = false;
        let mut best_cut = 0usize;
        let mut pos = 0usize;

        while let Some(rel) = self.tail[pos..].find('\n') {
            let line_start = pos;
            let line = &self.tail[line_start..line_start + rel];
            pos = line_start + rel + 1;

            // Is a cut AT line_start safe, given the state BEFORE this
            // line? (The complete line's own class is exact.)
            let splits_paragraph = para_open && line_class(line) == LineClass::ParaText;
            if !in_fence && !splits_paragraph {
                best_cut = line_start;
            }

            // Advance state over the complete line, mirroring
            // `md::parse`'s dispatch exactly.
            if in_fence {
                if line.trim_end().trim_start().starts_with("```") {
                    in_fence = false; // closing fence line
                }
                para_open = false;
            } else {
                match line_class(line) {
                    LineClass::FenceOpen => {
                        in_fence = true;
                        para_open = false;
                    }
                    LineClass::ParaText => para_open = true,
                    LineClass::Boundary => para_open = false,
                }
            }
        }

        // The end position (only when the input ends at a line
        // boundary) and the cut before the incomplete fragment.
        let fragment = &self.tail[pos..];
        if !in_fence {
            if fragment.is_empty() {
                if !para_open {
                    best_cut = self.tail.len();
                }
            } else if !para_open || fragment_committed_non_para(fragment) {
                best_cut = pos;
            }
        }

        if best_cut > 0 {
            self.bytes_reparsed += best_cut as u64;
            let mut sealed = parse(&self.tail[..best_cut], &self.styles);
            if !sealed.is_empty() {
                self.closed.append(&mut sealed);
                self.closed_revision += 1;
            }
            self.tail.drain(..best_cut);
        }
    }

    fn reparse_tail(&mut self) {
        if self.tail.is_empty() {
            self.open_cache.clear();
        } else {
            self.bytes_reparsed += self.tail.len() as u64;
            self.open_cache = parse(&self.tail, &self.styles);
        }
    }
}

impl std::fmt::Debug for StreamSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StreamSession")
            .field("closed", &self.closed.len())
            .field("open_len", &self.tail.len())
            .field("finished", &self.finished)
            .finish()
    }
}

/// How a COMPLETE line behaves for cut safety.
#[derive(Copy, Clone, PartialEq, Eq)]
enum LineClass {
    /// Opens a fenced code block (state change, never a paragraph).
    FenceOpen,
    /// Plain text: opens or continues a paragraph.
    ParaText,
    /// Blank / heading / rule / quote / list: closes any open
    /// paragraph and never joins one.
    Boundary,
}

/// Classify a complete line with the batch parser's own dispatch order
/// (fence, blank, rule, heading, quote, list, else paragraph).
fn line_class(raw: &str) -> LineClass {
    let line = raw.trim_end();
    let trimmed = line.trim_start();
    if trimmed.starts_with("```") {
        return LineClass::FenceOpen;
    }
    if trimmed.is_empty()
        || trimmed == "---"
        || trimmed == "***"
        || heading_level(trimmed).is_some()
        || trimmed.starts_with('>')
        || list_marker(trimmed).is_some()
    {
        return LineClass::Boundary;
    }
    LineClass::ParaText
}

/// Can the incomplete final line still become a paragraph-joining line
/// as more bytes arrive? Only prefixes that COMMIT the line to a
/// non-paragraph block are safe to cut before:
/// - ``` : stays a fence line whatever follows;
/// - `>` : stays a blockquote line;
/// - `#{1..6} ` / `- ` / `* ` / `+ ` / `N. `: the marker + space is
///   already complete, any suffix extends the content.
///
/// NOT committed: blanks (a next char makes them text), `---`/`***`
/// (one more char makes them paragraph text), bare `#`/`-`/`1.`
/// (marker incomplete).
fn fragment_committed_non_para(fragment: &str) -> bool {
    let trimmed = fragment.trim_start();
    trimmed.starts_with("```")
        || trimmed.starts_with('>')
        || heading_level(trimmed).is_some()
        || list_marker(trimmed).is_some()
}

#[cfg(test)]
#[path = "md_stream_tests.rs"]
mod tests;
