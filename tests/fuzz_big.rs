//! VERIFY cycle-7 FULL FUZZ RE-RUN — enlarged campaigns across every
//! parser/decoder surface. `#[ignore]`d (doctrine: big/slow tests run
//! explicitly), so the default suite stays fast while this provides the
//! deep coverage on demand:
//!
//! ```sh
//! cargo test --test fuzz_big -- --ignored --nocapture
//! ```
//!
//! Every generator is SEEDED (reproducible). The invariant on all of
//! them is identical: never panic, never absurd-allocate, always return
//! a Result (decode or reject). Split-invariance is additionally checked
//! for the input parser (any chunking yields the same events).

use abstracttui::gfx::{jpeg, png};
use abstracttui::input::{Event, Parser};
use abstracttui::render::md::{self, MdStyles};
use abstracttui::testing::jpeg_build::FlatJpeg;
use abstracttui::testing::{glb_mutants, hostile_corpus, random_splits, GlbExpect, Rng};
use abstracttui::text::{CLikeLexer, Highlighter};
use abstracttui::three::Model;

/// Input parser: 20,000 hostile chunks, no panic; plus split-invariance
/// (feeding the same bytes in arbitrary chunk boundaries yields the same
/// event stream) over a sample.
#[test]
#[ignore = "big fuzz"]
fn parser_20k_hostile_chunks_and_split_invariance() {
    let corpus = hostile_corpus(0x0B16_FADE, 20_000);
    for chunk in &corpus {
        let mut p = Parser::new();
        let mut out = Vec::new();
        p.feed(chunk, &mut out); // must not panic (a panic aborts the test)
    }

    // Split-invariance on a 2,000-case sample: one-shot feed == chunked
    // feed for ANY split.
    let mut rng = Rng::new(0x0051_9117);
    let mut mismatches = 0usize;
    for (i, chunk) in corpus.iter().take(2_000).enumerate() {
        let mut whole = Parser::new();
        let mut a = Vec::new();
        whole.feed(chunk, &mut a);

        let mut piece = Parser::new();
        let mut b = Vec::new();
        for slice in random_splits(&mut rng, chunk, 5) {
            piece.feed(&slice, &mut b);
        }
        if events_eq(&a, &b) {
            continue;
        }
        mismatches += 1;
        eprintln!(
            "split-invariance mismatch at case {i}: {} vs {} events",
            a.len(),
            b.len()
        );
    }
    assert_eq!(
        mismatches, 0,
        "parser split-invariance broke on {mismatches} cases"
    );
    eprintln!("parser: 20000 hostile chunks, 0 panics; 2000 split-invariance cases clean");
}

fn events_eq(a: &[Event], b: &[Event]) -> bool {
    // Event derives PartialEq in the engine; compare directly.
    a == b
}

/// GLB mutator: 5,000 mutants through the full `Model::load`. MustReject
/// mutants that LOAD are recorded (the hard ratchet lives in adv_gfx);
/// the property here is totality — every mutant returns, none panics.
#[test]
#[ignore = "big fuzz"]
fn glb_5k_mutants_through_full_loader() {
    let corpus = glb_mutants(0x06B1_0ADE, 5_000);
    let total = corpus.len();
    let (mut loaded, mut rejected, mut surprises) = (0usize, 0usize, 0usize);
    for m in corpus {
        match Model::load(&m.bytes) {
            Ok(_) => {
                loaded += 1;
                if matches!(m.expect, GlbExpect::MustReject) {
                    surprises += 1;
                }
            }
            Err(_) => rejected += 1,
        }
    }
    eprintln!(
        "glb: {loaded} loaded / {rejected} rejected / {total} total, 0 panics ({surprises} MustReject loaded — ratchet in adv_gfx)"
    );
    assert_eq!(loaded + rejected, total);
}

/// PNG: 5,000 hostile vectors (random soup + mutated real headers), no
/// panic, no absurd dimensions on success.
#[test]
#[ignore = "big fuzz"]
fn png_5k_hostile_vectors() {
    let mut rng = Rng::new(0x00F1_6DEA ^ 0x1);
    let mut ok = 0usize;
    for _ in 0..5_000 {
        // A plausible-looking PNG signature + random chunk soup.
        let mut bytes = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
        let n = rng.below(300);
        for _ in 0..n {
            bytes.push(rng.byte());
        }
        if let Ok(img) = png::decode(&bytes) {
            ok += 1;
            assert!(
                img.width() <= 1 << 16 && img.height() <= 1 << 16,
                "absurd PNG dims"
            );
        }
    }
    eprintln!("png: 5000 hostile vectors, {ok} decoded, 0 panics");
}

/// JPEG: 3,000 seeded mutations of flat-builder output across several
/// bases (incl. deep-tree tables), no panic, sane dims on success.
#[test]
#[ignore = "big fuzz"]
fn jpeg_3k_pathological_mutations() {
    let bases: Vec<Vec<u8>> = vec![
        FlatJpeg::grayscale(16, 16).build(),
        FlatJpeg::color444(24, 16).build(),
        FlatJpeg::grayscale(16, 16).with_flat_code_len(16).build(),
        FlatJpeg::color444(16, 16).with_flat_code_len(12).build(),
    ];
    let mut rng = Rng::new(0x001E_6600 ^ 0x2);
    let (mut ok, mut rej) = (0usize, 0usize);
    for _ in 0..3_000 {
        let mut b = bases[rng.below(bases.len())].clone();
        for _ in 0..1 + rng.below(8) {
            let off = rng.below(b.len());
            b[off] ^= (rng.byte()) | 1;
        }
        // Occasionally truncate.
        if rng.below(3) == 0 {
            let cut = rng.below(b.len());
            b.truncate(cut);
        }
        match jpeg::decode(&b) {
            Ok(img) => {
                ok += 1;
                assert!(
                    img.width() <= 64 && img.height() <= 64,
                    "absurd JPEG dims from mutation"
                );
            }
            Err(_) => rej += 1,
        }
    }
    eprintln!("jpeg: 3000 pathological mutations, {ok} decoded / {rej} rejected, 0 panics");
}

/// Markdown + highlighter: 5,000 seeded documents each, no panic, bounded
/// output, valid token ranges.
#[test]
#[ignore = "big fuzz"]
fn markup_and_highlighter_5k_each() {
    let styles = MdStyles::default();
    let md_toks = [
        "# ", "## ", "- ", "1. ", "> ", "```", "`", "*", "**", "[", "]", "(", ")", "!", "text ",
        "\n", "\n\n", "    ", "\t", "---", "\u{1b}", "\u{0}", "漢", "🎉",
    ];
    let mut rng = Rng::new(0x_AD_F0_0D);
    for _ in 0..5_000 {
        let n = 1 + rng.below(80);
        let mut src = String::new();
        for _ in 0..n {
            src.push_str(md_toks[rng.below(md_toks.len())]);
        }
        let blocks = md::parse(&src, &styles);
        let rt = md::to_rich_text(&blocks, &styles);
        assert!(rt.height() <= (src.matches('\n').count() as i32 + blocks.len() as i32 + 16));
    }

    let lex = CLikeLexer::rust();
    let code_toks = [
        "fn ", "let ", "// c", "/* b", "*/", "\"s", "\"x\"", "'c'", "0xFF", "3.14", "_i", "foo",
        "+", "->", "{", "}", ";", "\\", "é", "漢", "\u{1b}", "\u{0}",
    ];
    let mut rng = Rng::new(0x_C0DE_FACE);
    for _ in 0..5_000 {
        let n = 1 + rng.below(60);
        let mut line = String::new();
        for _ in 0..n {
            line.push_str(code_toks[rng.below(code_toks.len())]);
        }
        let mut prev_end = 0usize;
        for (r, _) in lex.spans(&line) {
            assert!(r.start >= prev_end && r.end <= line.len() && r.start <= r.end);
            assert!(line.is_char_boundary(r.start) && line.is_char_boundary(r.end));
            prev_end = r.end;
        }
    }
    eprintln!("markup: 5000 md docs + 5000 code lines, 0 panics, all ranges valid");
}

/// Diff lexer (0140's additive slice) joins the campaign at the same
/// bar as the C-like lexer: 5,000 seeded hostile lines, no panic, valid
/// ascending char-boundary ranges, and every byte of every line covered
/// by at most one span (whole-line classification never overlaps).
#[test]
#[ignore = "big fuzz"]
fn diff_lexer_5k() {
    use abstracttui::text::DiffLexer;

    let lexer = DiffLexer::new();
    let atoms = [
        "+",
        "-",
        " ",
        "@@",
        "@@ -1,2 +3,4 @@",
        "@@@",
        "---",
        "--- a/f",
        "+++",
        "+++ b/f\t2026",
        "diff --git a/x b/y",
        "index 83db48f..bf3a1a5 100644",
        "\\",
        "\\ No newline at end of file",
        "Binary files a and b differ",
        "rename from x",
        "text",
        "fn main() {",
        "é",
        "漢",
        "🎉",
        "\u{1b}[31m",
        "\u{0}",
        "\t",
        "  ",
    ];
    let mut rng = Rng::new(0x_D1FF_FACE);
    for _ in 0..5_000 {
        let n = 1 + rng.below(12);
        let mut line = String::new();
        for _ in 0..n {
            line.push_str(atoms[rng.below(atoms.len())]);
        }
        let mut prev_end = 0usize;
        for (r, _) in lexer.spans(&line) {
            assert!(r.start >= prev_end && r.end <= line.len() && r.start <= r.end);
            assert!(line.is_char_boundary(r.start) && line.is_char_boundary(r.end));
            prev_end = r.end;
        }
        // Non-empty lines always classify (no silent byte-0 gap): the
        // first span starts at 0.
        if !line.is_empty() {
            let first = lexer.spans(&line);
            assert_eq!(first.first().map(|(r, _)| r.start), Some(0), "{line:?}");
        }
    }
    eprintln!("diff: 5000 hostile lines, 0 panics, all ranges valid");
}
