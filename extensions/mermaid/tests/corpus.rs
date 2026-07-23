//! Corpus walk: the fixture naming convention IS the assertion —
//! `accept_*` must parse, `fallback_*` must return a NAMED verdict.
//! Counts are pinned as minimums so the corpus can only grow.

use std::fs;
use std::path::PathBuf;

use abstracttui_mermaid::parse;

#[test]
fn corpus_prefixes_are_the_assertion() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures");
    let mut paths: Vec<PathBuf> = fs::read_dir(&dir)
        .expect("fixtures dir")
        .map(|e| e.expect("entry").path())
        .filter(|p| p.extension().is_some_and(|e| e == "mmd"))
        .collect();
    paths.sort();
    assert!(!paths.is_empty(), "corpus present");

    let (mut accepted, mut fell_back) = (0usize, 0usize);
    for path in paths {
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .expect("utf8 name")
            .to_string();
        let src = fs::read_to_string(&path).expect("readable fixture");
        let result = parse(&src);
        if name.starts_with("accept_") {
            assert!(
                result.is_ok(),
                "{name} must parse, got: {}",
                result.err().map(|e| e.to_string()).unwrap_or_default()
            );
            accepted += 1;
        } else if name.starts_with("fallback_") {
            let err = match result {
                Err(e) => e,
                Ok(d) => panic!("{name} must fall back, parsed {d:?}"),
            };
            assert!(err.line_no >= 1, "{name}: verdict names a line");
            assert!(!err.reason.is_empty(), "{name}: verdict names a reason");
            fell_back += 1;
        } else {
            panic!("fixture {name} matches no naming convention (accept_/fallback_)");
        }
    }
    println!("corpus: {accepted} accepted, {fell_back} fell back");
    assert!(accepted >= 11, "accepting corpus shrank: {accepted}");
    assert!(fell_back >= 19, "fallback corpus shrank: {fell_back}");
}
