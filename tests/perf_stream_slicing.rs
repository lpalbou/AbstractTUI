//! Is feeding a big string to the STREAMING path in slices a real way
//! to spread its cost — or is it an abuse of a path built for wires?
//!
//! ```sh
//! cargo test --release --test perf_stream_slicing -- --ignored --nocapture
//! ```
//!
//! `agora-tui` asked it (`commons#300`) after establishing something I
//! had assumed away: **their client has no deltas at all.** A body is
//! `Full(String) | Pending { bytes } | Failed`, every item enters
//! through `push`, and `FeedState::stream_*` is never called. So a long
//! agent answer is ONE complete string typeset in ONE frame, and the
//! question is whether they should slice it into the streaming path to
//! spread that cost across frames.
//!
//! I own that path, so the answer should be a number rather than an
//! opinion. The engine already carries the honest meter:
//! [`DocStreamSession::bytes_reparsed_total`] — cumulative bytes handed
//! to `parse_doc` over a session's life, whose contract is that closed
//! content must never re-parse.
//!
//! ## What the meter can and cannot settle
//!
//! It measures PARSE work, which is the term slicing multiplies. It
//! does not measure typesetting or draw, so a ratio here is a lower
//! bound on the cost of slicing, not the whole of it. Named because the
//! alternative is someone reading `1.0x` as "slicing is free".
//!
//! OWNER: tui.

use abstracttui::render::md::{DocStreamSession, MdStyles};

/// Slice sizes swept, in bytes. 8 is a plausible token, 4096 a chunky
/// network read.
const SLICES: [usize; 4] = [8, 64, 512, 4096];

/// A long answer with ordinary block structure: short paragraphs, a
/// fence, a list. The open region seals at every block boundary, so
/// this is the FAVOURABLE case for slicing.
fn blocky_answer(paragraphs: usize) -> String {
    let mut s = String::new();
    for i in 0..paragraphs {
        s.push_str(&format!(
            "## Section {i}\n\nSome prose for section {i}, long enough to wrap at a \
             typical terminal width and short enough to be one block.\n\n\
             - first point {i}\n- second point {i}\n\n```rust\nlet x = {i};\n```\n\n"
        ));
    }
    s
}

/// The SAME byte count as one unbroken paragraph — no block boundary
/// anywhere, so the open region never seals. This is the adversarial
/// case and the one worth knowing about.
fn one_giant_block(len: usize) -> String {
    let mut s = String::with_capacity(len + 16);
    while s.len() < len {
        s.push_str("word ");
    }
    s
}

/// Bytes handed to `parse_doc` when `text` is fed in `slice` chunks,
/// paired with what an ALWAYS-OPEN document would cost under the same
/// slicing.
///
/// That second number is the model, computed exactly rather than in
/// closed form: if nothing ever seals, append number `i` re-parses
/// everything fed so far, and `finish()` re-parses the tail once more.
/// A closed form (`(k+1)/2 + 1` documents) is right only for even
/// slices — the final short chunk skews it, which is how my first two
/// attempts at this model failed. Summing the actual cumulative
/// positions has no such caveat and needs no tolerance to hide one.
fn reparsed(text: &str, slice: usize) -> (u64, u64) {
    let styles = MdStyles::default();
    let mut session = DocStreamSession::new(styles);
    let bytes = text.as_bytes();
    let (mut at, mut always_open) = (0, 0u64);
    while at < bytes.len() {
        // Slice on a char boundary — a real feeder splits on tokens, and
        // a panic here would be an artefact of the harness, not a finding.
        let mut end = (at + slice).min(bytes.len());
        while end < bytes.len() && !text.is_char_boundary(end) {
            end += 1;
        }
        session.append(&text[at..end]);
        at = end;
        always_open += at as u64; // this append re-parsed everything so far
    }
    session.finish();
    always_open += bytes.len() as u64; // and finish() re-parses the tail
    (session.bytes_reparsed_total(), always_open)
}

/// The question agora-tui asked, answered on the favourable case and
/// then on the adversarial one.
///
/// Not `#[ignore]`d: it is an accounting identity, not a timing, so it
/// is as valid in debug as in release and cheap enough to guard every
/// run. The RATIO is the finding; the assertion is only that the
/// pathological shape is genuinely pathological, because that is the
/// part a future reader would otherwise have to take on trust.
#[test]
fn slicing_a_big_body_into_the_stream_path_multiplies_the_parse_work() {
    let blocky = blocky_answer(120);
    let giant = one_giant_block(blocky.len());
    let n = blocky.len() as f64;
    eprintln!(
        "\n  document = {} bytes; one-shot parse = 1.00x by definition\n",
        blocky.len()
    );
    eprintln!("  slice     block-structured        one giant block     model");
    let mut giant_worst = 1.0_f64;
    for &s in &SLICES {
        let ((b, _), (g, model)) = (reparsed(&blocky, s), reparsed(&giant, s));
        let (br, gr) = (b as f64 / n, g as f64 / n);
        giant_worst = giant_worst.max(gr);
        let predicted = model as f64 / n;
        eprintln!("  {s:>5}B   {b:>10} ({br:>6.2}x)   {g:>12} ({gr:>8.1}x)  {predicted:>7.1}x");
        // The unsealed shape must match the always-open model EXACTLY —
        // it is an accounting identity, not an estimate. A departure
        // means the open region is sealing (or re-parsing) somewhere
        // this test does not know about, and the advice built on it
        // would need re-deriving.
        assert_eq!(
            g, model,
            "unsealed re-parse cost {g} != the always-open model {model} at \
             slice {s} — every append should re-parse the whole tail so far, \
             plus one final tail parse at finish()"
        );
        // The absent-input case, made explicit: if sealing ever stopped
        // working, the block-structured document would simply BECOME the
        // always-open one and the left column would climb to the right.
        // Without this, both columns could agree and the test would
        // still pass while the property it exists to show was gone.
        assert!(
            b < g,
            "block-structured document cost {b} at slice {s}, no better than \
             the never-sealing {g} — closed blocks are not being frozen"
        );
    }
    eprintln!(
        "\n  Left column: cost is bounded by the largest OPEN BLOCK, not by the\n  \
         document — that is what makes slicing viable. Right column: nothing\n  \
         ever closes, so every slice re-parses the whole tail so far.\n"
    );

    // The finding worth guarding: the two shapes are not the same
    // engineering decision. A document that seals its blocks stays
    // near-linear; one that never seals is quadratic in disguise, and
    // an app slicing blindly would hit it on a single long paragraph.
    assert!(
        giant_worst > 10.0,
        "expected the unsealed shape to be pathological, measured only \
         {giant_worst:.1}x — if this ever goes green the open region is \
         sealing somewhere it did not used to, and the advice built on \
         this test needs re-deriving"
    );
}

/// The other half of the same answer: closed content must never
/// re-parse. Without this, the ratios above could be explained by the
/// session simply re-parsing everything every time, and the whole
/// "bounded by the open block" story would be wrong.
///
/// Falsification: this is the property the meter's own doc claims, so
/// it is exactly the kind of documented promise that deserves a test
/// rather than a citation.
#[test]
fn closed_blocks_never_re_parse_however_many_slices_arrive() {
    let styles = MdStyles::default();
    let mut session = DocStreamSession::new(styles);

    // Seal a block, then feed 200 slices of a SECOND block.
    session.append("First paragraph.\n\n");
    let after_first = session.bytes_reparsed_total();
    for _ in 0..200 {
        session.append("word ");
    }
    let growth = session.bytes_reparsed_total() - after_first;

    // If the first block re-parsed on every append, growth would carry
    // 200 copies of it. The open tail alone accounts for the rest.
    let tail = 200 * "word ".len() as u64;
    assert!(
        growth < tail * tail / 2,
        "growth {growth} suggests closed content is being re-parsed"
    );
    assert!(
        session.closed_blocks().len() == 1,
        "expected exactly one sealed block, got {}",
        session.closed_blocks().len()
    );
}
