//! Streaming session for the DOC vocabulary (0142): append deltas,
//! re-parse only the open tail — [`StreamSession`](super::StreamSession)
//! extended to tables, block images and task items.
//!
//! ## The correctness contract (extends the core session's)
//!
//! `parse_doc(prefix) ++ parse_doc(suffix) == parse_doc(whole)` whenever
//! the cut sits at a line start that is (a) not inside an open fence,
//! (b) not splitting a paragraph joint, and (c) not inside a TABLE
//! REGION: from the header line of an open table through its last body
//! row, and — one line earlier — from any complete pipe line whose
//! successor has not arrived yet (it may be a table header awaiting its
//! delimiter; cutting between them would parse the header as a
//! paragraph in the prefix and the delimiter as prose in the suffix).
//! The seal uses the SAME classifiers as `parse_doc`
//! (`doc_line_class` / `table_opens`, children of `md` sharing one
//! implementation), so batch and stream cannot drift. Test-pinned: for
//! any chunking of any input, `finish()` equals `parse_doc` of the
//! whole source.
//!
//! ## Open/close semantics, mid-stream honesty
//!
//! A table OPENS in `open_blocks()` once its header and delimiter lines
//! are both complete, grows a row per complete pipe line, and CLOSES
//! (seals) at the first non-pipe line; EOF (`finish`) closes an open
//! table with the rows received. While the DELIMITER line itself is
//! still incomplete the open tail may honestly re-parse as paragraph
//! text — open blocks are a live view and may flap; CLOSED blocks never
//! do. Fences keep the core session's no-flap guarantee; images and
//! task items are single-line blocks (complete line = closed block).

use super::doc::{doc_line_class, parse_doc, table_opens, DocLineClass};
use super::stream::fragment_committed_non_para;
use super::{DocBlock, MdStyles};

/// Incremental doc-vocabulary parser: closed blocks freeze, the open
/// tail re-parses per delta. See the module docs for the contract.
///
/// ```
/// use abstracttui::render::md::{self, DocStreamSession, MdStyles};
///
/// let styles = MdStyles::default();
/// let mut session = DocStreamSession::new(styles.clone());
/// session.append("# Title\n\n| a | b |\n|---|---|\n| 1 ");
/// session.append("| 2 |\n\ntail");
/// assert_eq!(
///     session.finish(),
///     md::parse_doc("# Title\n\n| a | b |\n|---|---|\n| 1 | 2 |\n\ntail", &styles),
///     "streamed == batch"
/// );
/// ```
pub struct DocStreamSession {
    styles: MdStyles,
    closed: Vec<DocBlock>,
    closed_revision: u64,
    tail: String,
    open_cache: Vec<DocBlock>,
    finished: bool,
    bytes_reparsed: u64,
}

impl DocStreamSession {
    pub fn new(styles: MdStyles) -> DocStreamSession {
        DocStreamSession {
            styles,
            closed: Vec::new(),
            closed_revision: 0,
            tail: String::new(),
            open_cache: Vec::new(),
            finished: false,
            bytes_reparsed: 0,
        }
    }

    /// Append a delta (any chunking). Appending to a finished session
    /// is a caller bug: debug builds assert, release builds ignore the
    /// delta (same contract as the core session).
    pub fn append(&mut self, delta: &str) {
        debug_assert!(!self.finished, "DocStreamSession::append after finish()");
        if self.finished || delta.is_empty() {
            return;
        }
        self.tail.push_str(delta);
        self.seal();
        self.reparse_tail();
    }

    /// Close the tail (EOF-closes open fences AND open tables, exactly
    /// like `parse_doc`) and mark the session complete. Idempotent.
    pub fn finish(&mut self) -> Vec<DocBlock> {
        if !self.finished {
            if !self.tail.is_empty() {
                self.bytes_reparsed += self.tail.len() as u64;
                let mut rest = parse_doc(&self.tail, &self.styles);
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
    pub fn closed_blocks(&self) -> &[DocBlock] {
        &self.closed
    }

    /// Bumped whenever `closed_blocks` grows.
    pub fn closed_revision(&self) -> u64 {
        self.closed_revision
    }

    /// Parse of the open tail region; empty after `finish`.
    pub fn open_blocks(&self) -> &[DocBlock] {
        &self.open_cache
    }

    /// Closed + open, in document order (a convenience clone).
    pub fn blocks(&self) -> Vec<DocBlock> {
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

    /// Cumulative bytes fed to `parse_doc` over the session's lifetime
    /// — the honest cost meter (closed content must never re-parse).
    pub fn bytes_reparsed_total(&self) -> u64 {
        self.bytes_reparsed
    }

    /// Move the longest provably-final prefix of `tail` into `closed`.
    /// See the module docs for the cut-safety rules; the walk mirrors
    /// `parse_doc`'s dispatch (same classifiers) with ONE line of
    /// lookahead for the header→delimiter pair.
    fn seal(&mut self) {
        let mut in_fence = false;
        let mut para_open = false;
        let mut table_open = false;
        // A complete pipe line whose successor has not arrived: it may
        // be a table header awaiting its delimiter — nothing at or
        // beyond it can seal until the next line settles it.
        let mut candidate_pending = false;
        let mut best_cut = 0usize;
        let mut pos = 0usize;

        while let Some(rel) = self.tail[pos..].find('\n') {
            let line_start = pos;
            let line = &self.tail[line_start..line_start + rel];
            pos = line_start + rel + 1;
            // The next COMPLETE line, if it has fully arrived (needs a
            // terminating newline of its own to be trusted).
            let next_complete: Option<&str> = self.tail[pos..]
                .find('\n')
                .map(|nrel| &self.tail[pos..pos + nrel]);

            if in_fence {
                if line.trim_end().trim_start().starts_with("```") {
                    in_fence = false;
                }
                para_open = false;
                continue;
            }

            let cls = doc_line_class(line);
            if table_open {
                if cls == DocLineClass::PipeText {
                    continue; // body row (or the delimiter): stays open
                }
                table_open = false; // first non-pipe line closes it
            }

            // Would this line open a table with its successor? Unknown
            // successors are worst-cased below.
            let opens_table = cls == DocLineClass::PipeText
                && next_complete.is_some_and(|n| table_opens(line.trim_end().trim_start(), n));
            candidate_pending = cls == DocLineClass::PipeText && next_complete.is_none();

            // Cut BEFORE this line: safe unless it would split a
            // paragraph joint. A pipe line that provably opens a table
            // interrupts the paragraph in the batch parse too, so the
            // cut is clean; an UNRESOLVED pipe line worst-cases to
            // "joins".
            let joins_para = para_open
                && (cls == DocLineClass::ParaText
                    || (cls == DocLineClass::PipeText && !opens_table));
            if !joins_para {
                best_cut = line_start;
            }

            match cls {
                DocLineClass::FenceOpen => {
                    in_fence = true;
                    para_open = false;
                }
                DocLineClass::Image | DocLineClass::Task | DocLineClass::Boundary => {
                    para_open = false;
                }
                DocLineClass::PipeText => {
                    if opens_table {
                        table_open = true;
                        para_open = false;
                    } else {
                        // Plain prose with a pipe (or an unresolved
                        // candidate): paragraph text either way for
                        // join purposes.
                        para_open = true;
                    }
                }
                DocLineClass::ParaText => para_open = true,
            }
        }

        // The end position (input ends at a line boundary) and the cut
        // before the incomplete fragment.
        let fragment = &self.tail[pos..];
        if !in_fence && !table_open && !candidate_pending {
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
            let mut sealed = parse_doc(&self.tail[..best_cut], &self.styles);
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
            self.open_cache = parse_doc(&self.tail, &self.styles);
        }
    }
}

impl std::fmt::Debug for DocStreamSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DocStreamSession")
            .field("closed", &self.closed.len())
            .field("open_len", &self.tail.len())
            .field("finished", &self.finished)
            .finish()
    }
}

#[cfg(test)]
#[path = "md_doc_stream_tests.rs"]
mod tests;
