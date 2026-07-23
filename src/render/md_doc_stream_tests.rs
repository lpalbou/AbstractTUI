//! DocStreamSession tests: the streamed-vs-batch equivalence contract
//! over table/image/task boundaries, table open/close semantics, the
//! cost pin, and hostile-corpus fuzz.

use super::*;
use crate::render::md::{parse_doc, DocBlock, MdStyles};
use crate::testing::{hostile_corpus, Rng};

fn styles() -> MdStyles {
    MdStyles::default()
}

/// Documents crossing every DOC boundary rule: tables (aligned, padded,
/// escaped pipes, interrupted paragraphs, closed by blank/prose/block,
/// EOF-open), header candidates that never resolve, delimiter look-
/// alikes, images (block, inline, malformed), tasks, fences hiding
/// pipes, plus the core corpus hazards.
fn corpus() -> Vec<&'static str> {
    vec![
        "",
        "| a | b |\n|---|---|\n| 1 | 2 |",
        "| a | b |\n|:--|--:|\n| 1 | 2 |\n| 3 | 4 |\n\nafter",
        "intro\n| a | b |\n|---|---|\n| 1 | 2 |\ntail prose",
        "| a | b |\n|---|\nno table, counts differ",
        "| a \\| b |\n|---|\n| \\|x |",
        "a | b without delimiter\njust text",
        "para\na | b\n---|---\nrows | here\n\nafter",
        "| open table |\n|---|\n| row one |",
        "| h |\n|---|",
        "```\n| a | b |\n|---|---|\n```\nafter",
        "![logo](x.png)\n\npara ![inline](y.png) text\n\n![](z.png)",
        "![broken](\n![alt]() empty src",
        "- [ ] open\n- [x] done\n  - [X] nested\n- [ ]\n- plain",
        "1. [ ] numbered stays list",
        "text\n---x",
        "text\n---\nafter",
        "# H\n\n| t |\n|---|\n| r |\n\n```rust\nlet a = b | c;\n```\n\n- [x] ship",
        "|-|\n|-|\n|-|",
        "||\n||\n||",
        "| a |\n|---|\n| r1 |\n| r2 |\n| r3 |\n# heading closes",
        "héllo | wörld\n---|---\n世 | 界",
        "crlf | table\r\n---|---\r\nr1 | r2\r\n",
        "| a |",
        "|---|",
        "\\| escaped start\nplain",
    ]
}

/// Char-boundary chunkings (mirrors the core session's test rig).
fn chunkings(src: &str, rng: &mut Rng) -> Vec<Vec<String>> {
    let chars: Vec<usize> = {
        let mut idx: Vec<usize> = src.char_indices().map(|(i, _)| i).collect();
        idx.push(src.len());
        idx
    };
    let n = chars.len() - 1;
    let mut out = Vec::new();
    out.push(vec![src.to_string()]);
    out.push(
        (0..n)
            .map(|i| src[chars[i]..chars[i + 1]].to_string())
            .collect(),
    );
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

/// THE contract (0142): any chunking through `append` + `finish`
/// yields blocks identical to `parse_doc` of the whole source.
#[test]
fn any_chunking_equals_batch_doc_parse() {
    let styles = styles();
    let mut rng = Rng::new(0x0142);
    for (di, doc) in corpus().into_iter().enumerate() {
        let expected = parse_doc(doc, &styles);
        for (ci, chunks) in chunkings(doc, &mut rng).into_iter().enumerate() {
            let mut session = DocStreamSession::new(styles.clone());
            for chunk in &chunks {
                session.append(chunk);
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

/// Randomized deep pass biased toward table fragments.
#[test]
fn randomized_documents_hold_the_equivalence() {
    let styles = styles();
    let fragments = [
        "| a | b |\n",
        "|---|---|\n",
        "|---|\n",
        "| r |\n",
        "a|b\n",
        "-|-\n",
        "text ",
        "\n",
        "\n\n",
        "# h\n",
        "- [ ] t\n",
        "- [x] t\n",
        "![i](p.png)\n",
        "![i](p.png)",
        "```\n",
        "---\n",
        "---",
        "\\|",
        "|",
        "世|界\n",
        "x",
    ];
    let mut rng = Rng::new(0xD0C_5EED);
    for round in 0..250 {
        let mut doc = String::new();
        for _ in 0..rng.below(20) {
            doc.push_str(fragments[rng.below(fragments.len())]);
        }
        let expected = parse_doc(&doc, &styles);
        let mut session = DocStreamSession::new(styles.clone());
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

/// Hostile corpus (lossy-decoded) streamed in random chunks equals the
/// batch parse — and never panics anywhere on the way.
#[test]
fn hostile_corpus_streams_equal_batch() {
    let styles = styles();
    let mut rng = Rng::new(0xBAD_F00D);
    for chunk in hostile_corpus(0x0142_0148, 200) {
        let doc = String::from_utf8_lossy(&chunk).into_owned();
        let expected = parse_doc(&doc, &styles);
        let mut session = DocStreamSession::new(styles.clone());
        let chars: Vec<usize> = {
            let mut idx: Vec<usize> = doc.char_indices().map(|(i, _)| i).collect();
            idx.push(doc.len());
            idx
        };
        let n = chars.len() - 1;
        let mut i = 0;
        while i < n {
            let take = 1 + rng.below(7).min(n - i - 1);
            session.append(&doc[chars[i]..chars[i + take]]);
            i += take;
        }
        assert_eq!(session.finish(), expected, "source: {doc:?}");
    }
}

/// Open/close semantics (the 0142 contract): the table OPENS in
/// `open_blocks` once header + delimiter lines are complete, grows per
/// row, seals on the first non-pipe line.
#[test]
fn table_opens_after_delimiter_grows_and_seals_on_close() {
    let mut session = DocStreamSession::new(styles());
    session.append("| a | b |\n");
    assert_eq!(
        session.closed_blocks().len(),
        0,
        "header candidate must not seal (delimiter may follow)"
    );
    session.append("|---|---|\n");
    let open = session.open_blocks();
    assert!(
        matches!(open, [DocBlock::Table(t)] if t.rows.is_empty()),
        "table open from header+delimiter: {open:?}"
    );
    session.append("| 1 | 2 |\n| 3 | 4 |\n");
    let open = session.open_blocks();
    assert!(
        matches!(open, [DocBlock::Table(t)] if t.rows.len() == 2),
        "rows accumulate while open: {open:?}"
    );
    assert_eq!(session.closed_blocks().len(), 0, "still open");
    session.append("\n");
    assert!(
        matches!(session.closed_blocks(), [DocBlock::Table(t)] if t.rows.len() == 2),
        "blank line closes and seals the table: {:?}",
        session.closed_blocks()
    );
}

/// A header candidate whose next line disproves the table joins the
/// paragraph flow instead — and nothing seals early on the way.
#[test]
fn unresolved_candidate_resolves_to_paragraph() {
    let styles = styles();
    let mut session = DocStreamSession::new(styles.clone());
    session.append("plain para\na | b\n");
    assert_eq!(
        session.closed_blocks().len(),
        0,
        "pipe line may still become a table header"
    );
    session.append("just prose\n\n");
    assert_eq!(
        session.closed_blocks(),
        &parse_doc("plain para\na | b\njust prose\n\n", &styles)[..]
    );
}

/// Images and tasks are single-line blocks: complete line = sealed.
#[test]
fn image_and_task_lines_seal_immediately() {
    let mut session = DocStreamSession::new(styles());
    session.append("![logo](x.png)\n");
    assert!(
        matches!(session.closed_blocks(), [DocBlock::Image(_)]),
        "{:?}",
        session.closed_blocks()
    );
    session.append("- [x] done\n");
    assert!(matches!(
        session.closed_blocks().last(),
        Some(DocBlock::Task(_))
    ));
}

/// Cost pin: appends behind sealed content re-parse only the open
/// region (the doc session keeps the core session's O(open) law).
#[test]
fn appends_behind_closed_content_cost_only_the_open_region() {
    let mut session = DocStreamSession::new(styles());
    for i in 0..500 {
        session.append(&format!("| h{i} |\n|---|\n| r |\n\n"));
    }
    assert!(session.closed_blocks().len() >= 500);
    assert_eq!(session.open_len(), 0);
    let before = session.bytes_reparsed_total();
    let token = "streaming ";
    for _ in 0..50 {
        session.append(token);
    }
    let cost = session.bytes_reparsed_total() - before;
    let open_only_ceiling = (0..=50u64).sum::<u64>() * token.len() as u64 + 64;
    assert!(
        cost <= open_only_ceiling,
        "append cost {cost} B exceeds the open-region ceiling {open_only_ceiling} B"
    );
}

#[test]
fn finish_is_idempotent_and_eof_closes_open_tables() {
    let styles = styles();
    let mut session = DocStreamSession::new(styles.clone());
    session.append("| a |\n|---|\n| r1 |");
    let first = session.finish();
    assert_eq!(first, parse_doc("| a |\n|---|\n| r1 |", &styles));
    assert!(matches!(&first[..], [DocBlock::Table(t)] if t.rows.len() == 1));
    assert_eq!(session.finish(), first, "finish must be idempotent");
    assert!(session.is_finished());
}
