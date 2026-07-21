//! StreamSession tests: the equivalence contract (any chunking ==
//! batch parse), mid-fence honesty, sealing behavior, and the
//! O(open block) cost pin.

use super::*;
use crate::render::md::{parse, Block, MdStyles};
use crate::testing::Rng;

fn styles() -> MdStyles {
    MdStyles::default()
}

/// Documents chosen to cross every block boundary rule: paragraphs
/// joining across lines, fences (closed, unclosed, immediately closed,
/// with lang, fence-closing-fence), rules and the `---x` join hazard,
/// quotes, lists (nested, numbered, marker fragments), headings,
/// escapes, links, CRLF, unicode, blank storms, empty input.
fn corpus() -> Vec<&'static str> {
    vec![
        "",
        "\n",
        "plain paragraph",
        "one\ntwo\nthree joined lines",
        "para one\n\npara two\n\n\n\npara three",
        "# Title\n\nIntro with **bold**, *italic*, `code` and [a link](https://x.example).\nContinues.\n\n## Section\n\n- first\n- second **strong**\n  - nested\n1. numbered\n\n> quoted wisdom\n\n```rust\nlet x = 1; // verbatim, **not** parsed\n```\n\n---\ntail",
        "```\nunclosed fence\nstill code",
        "```rust\nfn main() {}\n```",
        "```\n```",
        "text\n```lang info\nbody\n```\nafter",
        "a fence closing fence\n```\ncode\n```more\nplain",
        "text\n---x",
        "text\n---\nafter rule",
        "text\n***",
        "para\n# heading flushes\npara again",
        "para\n> quote flushes",
        "para\n- list flushes",
        "- item\ncontinuation is a NEW paragraph",
        "-\nnot a list, joins para",
        "1.\nnot numbered either",
        "12345678901. too many digits",
        "> > folded quote\n>> also folded",
        "###### deep heading\n####### too deep is text",
        "#nospace is a paragraph",
        "escaped \\*not bold\\* and \\`not code\\`",
        "unclosed **bold and *italic and `code",
        "[text](url) [broken](  [also broken] ()",
        "line with trailing spaces   \nnext",
        "crlf line\r\nsecond\r\n\r\nthird",
        "héllo wörld — 世界 👍🏽 emoji **bläck** text\n\n```\nünïcode fence 世界\n```",
        "   indented text\n  - indented list\n    - deeper",
        "```\nfence at eof\n",
        "# heading at eof",
        "- list at eof",
        "> quote at eof",
        "---",
        "--",
        "****",
        "text\n\n```\nfence after blank",
    ]
}

/// Char-boundary chunkings of `src` (append takes &str, so chunks must
/// respect UTF-8 boundaries).
fn chunkings(src: &str, rng: &mut Rng) -> Vec<Vec<String>> {
    let chars: Vec<usize> = {
        let mut idx: Vec<usize> = src.char_indices().map(|(i, _)| i).collect();
        idx.push(src.len());
        idx
    };
    let n = chars.len() - 1; // char count
    let mut out = Vec::new();
    // Whole string at once.
    out.push(vec![src.to_string()]);
    // Char by char (the token-stream worst case).
    out.push(
        (0..n)
            .map(|i| src[chars[i]..chars[i + 1]].to_string())
            .collect(),
    );
    // Line-ish: split right after every newline.
    {
        let mut chunks = Vec::new();
        let mut start = 0;
        for (i, b) in src.bytes().enumerate() {
            if b == b'\n' {
                chunks.push(src[start..=i].to_string());
                start = i + 1;
            }
        }
        if start < src.len() {
            chunks.push(src[start..].to_string());
        }
        out.push(chunks);
    }
    // Random-sized chunks, several seeds.
    for _ in 0..4 {
        let mut chunks = Vec::new();
        let mut i = 0;
        while i < n {
            let take = 1 + rng.below(6).min(n - i - 1);
            chunks.push(src[chars[i]..chars[i + take]].to_string());
            i += take;
        }
        out.push(chunks);
    }
    out
}

/// THE contract (backlog 0110): any chunking of the same bytes through
/// `append` + `finish` yields blocks identical to `md::parse` of the
/// whole source.
#[test]
fn any_chunking_equals_batch_parse() {
    let styles = styles();
    let mut rng = Rng::new(0x0110);
    for (di, doc) in corpus().into_iter().enumerate() {
        let expected = parse(doc, &styles);
        for (ci, chunks) in chunkings(doc, &mut rng).into_iter().enumerate() {
            let mut session = StreamSession::new(styles.clone());
            for chunk in &chunks {
                session.append(chunk);
                // Mid-stream invariant: closed + open always parses to
                // a PREFIX-consistent view — closed blocks must equal
                // the batch parse of everything, truncated to their
                // count, once the stream completes. (Checked cheaply
                // here: closed blocks never exceed the final total.)
                assert!(
                    session.closed_blocks().len() <= expected.len(),
                    "doc {di} chunking {ci}: sealed more blocks than the batch total"
                );
            }
            let got = session.finish();
            assert_eq!(
                got, expected,
                "doc {di} chunking {ci}: streamed != batch\nsource: {doc:?}"
            );
        }
    }
}

/// Randomized deep pass: hostile little documents assembled from
/// boundary-heavy fragments, random chunkings — same equivalence.
#[test]
fn randomized_documents_hold_the_equivalence() {
    let styles = styles();
    let fragments = [
        "text ",
        "**b** ",
        "`c` ",
        "\n",
        "\n\n",
        "# h\n",
        "- li\n",
        "> q\n",
        "```\n",
        "```rust\n",
        "---\n",
        "---",
        "*",
        "\\*",
        "1. n\n",
        "  ",
        "世界",
        "x",
    ];
    let mut rng = Rng::new(0xC0FFEE);
    for round in 0..200 {
        let mut doc = String::new();
        for _ in 0..rng.below(24) {
            doc.push_str(fragments[rng.below(fragments.len())]);
        }
        let expected = parse(&doc, &styles);
        let mut session = StreamSession::new(styles.clone());
        let chars: Vec<usize> = {
            let mut idx: Vec<usize> = doc.char_indices().map(|(i, _)| i).collect();
            idx.push(doc.len());
            idx
        };
        let n = chars.len() - 1;
        let mut i = 0;
        while i < n {
            let take = 1 + rng.below(5).min(n - i - 1);
            session.append(&doc[chars[i]..chars[i + take]]);
            i += take;
        }
        assert_eq!(
            session.finish(),
            expected,
            "round {round}: streamed != batch\nsource: {doc:?}"
        );
    }
}

/// Mid-fence honesty (0110 §3): an unclosed fence reports as an OPEN
/// CodeFence with its lang from the moment the opening line arrives —
/// never flapping to literal text while the close is in flight.
#[test]
fn open_fence_reports_as_code_before_the_close() {
    let mut session = StreamSession::new(styles());
    session.append("```rust\nlet x");
    let open = session.open_blocks();
    assert_eq!(open.len(), 1, "one open block: {open:?}");
    match &open[0] {
        Block::CodeFence { lang, lines } => {
            assert_eq!(lang, "rust");
            assert_eq!(lines, &vec!["let x".to_string()]);
        }
        other => panic!("open fence must be a CodeFence, got {other:?}"),
    }
    // More body arrives: still code, still open, nothing sealed.
    session.append(" = 1;\nlet y = 2;");
    assert!(matches!(session.open_blocks()[0], Block::CodeFence { .. }));
    assert_eq!(session.closed_blocks().len(), 0, "fence must stay open");
    // The close seals it.
    session.append("\n```\n");
    assert_eq!(session.closed_blocks().len(), 1);
    assert!(matches!(
        session.closed_blocks()[0],
        Block::CodeFence { .. }
    ));
    assert_eq!(session.open_blocks().len(), 0);
}

/// The `---x` hazard: a rule-shaped incomplete line can still grow into
/// paragraph text that soft-joins backwards, so it must NOT seal the
/// paragraph before it.
#[test]
fn rule_shaped_fragment_does_not_seal_the_paragraph() {
    let styles = styles();
    let mut session = StreamSession::new(styles.clone());
    session.append("text\n---");
    assert_eq!(
        session.closed_blocks().len(),
        0,
        "'---' can still become '---x' and join the paragraph"
    );
    session.append("x");
    assert_eq!(session.finish(), parse("text\n---x", &styles));

    // And the counterpart: once the newline lands it IS a rule.
    let mut session = StreamSession::new(styles.clone());
    session.append("text\n---");
    session.append("\n");
    assert_eq!(
        session.closed_blocks(),
        &parse("text\n---\n", &styles)[..],
        "complete rule line seals paragraph + rule"
    );
}

/// Committed fragments seal what precedes them: a paragraph followed by
/// an arriving `> ` / `- ` / `# ` / ``` prefix is final immediately.
#[test]
fn committed_fragments_seal_the_preceding_paragraph() {
    for prefix in ["> ", "- ", "# ", "```", "1. "] {
        let mut session = StreamSession::new(styles());
        session.append("para text\n");
        assert_eq!(
            session.closed_blocks().len(),
            0,
            "open paragraph must not seal at end-of-line"
        );
        session.append(prefix);
        assert_eq!(
            session.closed_blocks().len(),
            1,
            "prefix {prefix:?} commits to a non-paragraph block, sealing the para"
        );
        assert!(matches!(session.closed_blocks()[0], Block::Paragraph(_)));
    }
}

/// Closed blocks are index-stable: sealing only APPENDS (a consumer may
/// typeset closed[i] once and never revisit).
#[test]
fn closed_blocks_only_append_and_revision_tracks_growth() {
    let styles = styles();
    let doc = "# a\n\npara\n\n- one\n- two\n\n```\ncode\n```\n\ntail";
    let mut session = StreamSession::new(styles);
    let mut seen: Vec<Block> = Vec::new();
    let mut last_rev = session.closed_revision();
    for ch in doc.chars() {
        let mut buf = [0u8; 4];
        session.append(ch.encode_utf8(&mut buf));
        let closed = session.closed_blocks();
        assert!(
            closed.len() >= seen.len() && closed[..seen.len()] == seen[..],
            "sealed prefix changed identity"
        );
        if closed.len() > seen.len() {
            assert!(
                session.closed_revision() > last_rev,
                "growth must bump closed_revision"
            );
            seen = closed.to_vec();
            last_rev = session.closed_revision();
        }
    }
    session.finish();
}

/// THE cost pin (0110 validation): appending into a session with 1,000
/// closed lines re-parses only the open region — measured by the
/// bytes-reparsed meter, which must not scale with closed content.
#[test]
fn appends_behind_closed_content_cost_only_the_open_block() {
    let mut session = StreamSession::new(styles());
    for i in 0..1_000 {
        session.append(&format!("closed paragraph number {i}\n\n"));
    }
    assert!(
        session.closed_blocks().len() >= 1_000,
        "the lines must actually have sealed: {}",
        session.closed_blocks().len()
    );
    assert_eq!(session.open_len(), 0, "blank-terminated tail seals fully");

    let before = session.bytes_reparsed_total();
    let token = "streaming ";
    let mut appended = 0u64;
    for _ in 0..50 {
        session.append(token);
        appended += token.len() as u64;
    }
    let cost = session.bytes_reparsed_total() - before;
    // Each append re-parses the open region (which grows by one token a
    // time): Σ k·|token| for k=1..50 — quadratic in the OPEN length,
    // independent of the 1,000 closed lines (~50 KB of closed source
    // per append would dwarf this ceiling).
    let open_only_ceiling = (0..=50u64).sum::<u64>() * token.len() as u64 + 64;
    assert!(
        cost <= open_only_ceiling,
        "append cost {cost} B exceeds the open-block ceiling {open_only_ceiling} B — \
         closed content is being re-parsed"
    );
    assert_eq!(
        session.open_len() as u64,
        appended,
        "tail holds the open para"
    );
}

/// finish() is idempotent and EOF-closes an open fence exactly like the
/// batch parser.
#[test]
fn finish_is_idempotent_and_eof_closes_fences() {
    let styles = styles();
    let mut session = StreamSession::new(styles.clone());
    session.append("```py\nprint('hi')");
    let first = session.finish();
    assert_eq!(first, parse("```py\nprint('hi')", &styles));
    assert_eq!(session.finish(), first, "finish must be idempotent");
    assert!(session.is_finished());
    assert_eq!(session.open_blocks().len(), 0);
}

/// Empty appends are no-ops; a fresh session is empty everywhere.
#[test]
fn empty_session_and_empty_appends() {
    let mut session = StreamSession::new(styles());
    session.append("");
    assert_eq!(session.closed_blocks().len(), 0);
    assert_eq!(session.open_blocks().len(), 0);
    assert_eq!(session.open_len(), 0);
    assert_eq!(session.bytes_reparsed_total(), 0);
    assert_eq!(session.finish(), Vec::<Block>::new());
}
