//! JSON/YAML lexer goldens (wave 13): kind-per-token assertions on
//! realistic lines, the trait-shape contract (ascending non-overlap,
//! char-boundary ranges), and hostile-input totality.

use super::*;

fn json_kinds(line: &str) -> Vec<(String, DataKind)> {
    JsonLexer::new()
        .spans(line)
        .into_iter()
        .map(|(r, k)| (line[r].to_string(), k))
        .collect()
}

fn yaml_kinds(line: &str) -> Vec<(String, DataKind)> {
    YamlLexer::new()
        .spans(line)
        .into_iter()
        .map(|(r, k)| (line[r].to_string(), k))
        .collect()
}

#[test]
fn json_pretty_line_keys_values_and_literals() {
    let toks = json_kinds(r#"  "name": "Ada", "age": 36, "admin": true, "note": null,"#);
    use DataKind::*;
    let expect = |t: &str, k: DataKind| {
        assert!(
            toks.iter().any(|(s, kk)| s == t && *kk == k),
            "expected {t:?} as {k:?} in {toks:?}"
        );
    };
    expect("\"name\"", Key);
    expect("\"Ada\"", String);
    expect("\"age\"", Key);
    expect("36", Number);
    expect("\"admin\"", Key);
    expect("true", Literal);
    expect("null", Literal);
    expect(":", Punct);
    expect(",", Punct);
}

#[test]
fn json_minified_and_negative_numbers() {
    let toks = json_kinds(r#"{"a":-1.5e3,"b":[false,"x"]}"#);
    use DataKind::*;
    assert!(toks.contains(&("\"a\"".into(), Key)), "{toks:?}");
    assert!(toks.contains(&("-1.5e3".into(), Number)), "{toks:?}");
    assert!(toks.contains(&("\"b\"".into(), Key)), "{toks:?}");
    assert!(toks.contains(&("false".into(), Literal)), "{toks:?}");
    assert!(toks.contains(&("\"x\"".into(), String)), "{toks:?}");
    assert!(toks.contains(&("{".into(), Punct)), "{toks:?}");
    assert!(toks.contains(&("[".into(), Punct)), "{toks:?}");
}

#[test]
fn json_escapes_comments_and_unterminated_strings() {
    // Escaped quote stays inside the string; the colon after keeps the
    // key reading.
    let toks = json_kinds(r#""a\"b": 1 // trailing"#);
    assert!(
        toks.contains(&(r#""a\"b""#.into(), DataKind::Key)),
        "{toks:?}"
    );
    assert!(
        toks.contains(&("// trailing".into(), DataKind::Comment)),
        "{toks:?}"
    );
    // Unterminated string runs to EOL as a VALUE (no colon follows),
    // span exactly as written — no invented closing quote.
    let toks = json_kinds(r#""open"#);
    assert_eq!(toks, vec![("\"open".to_string(), DataKind::String)]);
    // Block comment mid-line.
    let toks = json_kinds(r#"1 /* mid */ 2"#);
    assert!(
        toks.contains(&("/* mid */".into(), DataKind::Comment)),
        "{toks:?}"
    );
}

#[test]
fn yaml_block_mapping_list_and_comment() {
    let toks = yaml_kinds("  region: eu-west-1   # primary");
    use DataKind::*;
    assert!(toks.contains(&("region".into(), Key)), "{toks:?}");
    assert!(toks.contains(&(":".into(), Punct)), "{toks:?}");
    assert!(toks.contains(&("# primary".into(), Comment)), "{toks:?}");
    // The bare value scalar stays UNTINTED (base ink): prose is prose.
    assert!(
        !toks.iter().any(|(s, _)| s.contains("eu-west")),
        "bare scalars stay untinted: {toks:?}"
    );

    let toks = yaml_kinds("  - name: web");
    assert!(toks.contains(&("-".into(), Punct)), "{toks:?}");
    assert!(toks.contains(&("name".into(), Key)), "{toks:?}");
}

#[test]
fn yaml_literals_numbers_and_time_shapes() {
    let toks = yaml_kinds("enabled: TRUE");
    assert!(
        toks.contains(&("TRUE".into(), DataKind::Literal)),
        "1.1 bools, case-insensitive: {toks:?}"
    );
    let toks = yaml_kinds("count: 42");
    assert!(toks.contains(&("42".into(), DataKind::Number)), "{toks:?}");
    let toks = yaml_kinds("retries: ~");
    assert!(toks.contains(&("~".into(), DataKind::Literal)), "{toks:?}");
    // `10:30:00` — no space after the colon: NOT a key, NOT a number
    // (continues into scalar text); the whole thing stays untinted.
    let toks = yaml_kinds("start: 10:30:00");
    assert!(
        toks.iter()
            .all(|(s, k)| !(s == "10" && *k == DataKind::Key)),
        "time shapes are not keys: {toks:?}"
    );
    // kebab-case keys read whole.
    let toks = yaml_kinds("read-only: yes");
    assert!(
        toks.contains(&("read-only".into(), DataKind::Key)),
        "{toks:?}"
    );
    assert!(
        toks.contains(&("yes".into(), DataKind::Literal)),
        "{toks:?}"
    );
}

#[test]
fn yaml_tags_anchors_aliases_and_doc_markers() {
    let toks = yaml_kinds("base: &defaults !!map");
    assert!(
        toks.contains(&("&defaults".into(), DataKind::Tag)),
        "{toks:?}"
    );
    assert!(toks.contains(&("!!map".into(), DataKind::Tag)), "{toks:?}");
    let toks = yaml_kinds("prod: *defaults");
    assert!(
        toks.contains(&("*defaults".into(), DataKind::Tag)),
        "{toks:?}"
    );
    assert_eq!(
        yaml_kinds("---"),
        vec![("---".to_string(), DataKind::Tag)],
        "document start marker"
    );
    // Flow collections tint inner keys after `{` and `,`.
    let toks = yaml_kinds("point: {x: 1, y: 2}");
    assert!(toks.contains(&("x".into(), DataKind::Key)), "{toks:?}");
    assert!(toks.contains(&("y".into(), DataKind::Key)), "{toks:?}");
    // Quoted scalars: single-quote escape stays inside.
    let toks = yaml_kinds("msg: 'it''s fine'");
    assert!(
        toks.contains(&("'it''s fine'".into(), DataKind::String)),
        "{toks:?}"
    );
    // A mid-scalar `#` is content, not a comment.
    let toks = yaml_kinds("color: a#b");
    assert!(
        !toks.iter().any(|(_, k)| *k == DataKind::Comment),
        "{toks:?}"
    );
}

#[test]
fn lang_label_routing() {
    for l in [
        "json",
        "JSONC",
        "json5",
        "jsonl",
        "ndjson",
        "json filename=x",
    ] {
        assert!(JsonLexer::matches_lang(l), "{l}");
    }
    for l in ["yaml", "YML", "yaml k=v"] {
        assert!(YamlLexer::matches_lang(l), "{l}");
    }
    for l in ["rust", "diff", "toml", ""] {
        assert!(!JsonLexer::matches_lang(l), "{l}");
        assert!(!YamlLexer::matches_lang(l), "{l}");
    }
}

/// The Highlighter-shape contract both lexers promise: ascending,
/// non-overlapping ranges on char boundaries — and totality over
/// hostile bytes (mixed UTF-8, lone quotes, controls).
#[test]
fn ranges_are_ordered_boundary_safe_and_total() {
    let corpus = [
        r#"{"名前": "héllo", "n": [1, -2.5, true]}"#,
        "клавиша: значение # коммент",
        "x: 'unterminated",
        "\"\\",
        "-",
        "- - nested: {a: 1, b: *x}",
        "e: !!binary |",
        "\u{1}\u{2}: \u{3}",
        "🦀: 🚀",
    ];
    for line in corpus {
        for spans in [JsonLexer::new().spans(line), YamlLexer::new().spans(line)] {
            let mut last = 0;
            for (r, _) in &spans {
                assert!(r.start >= last, "disorder at {r:?} in {line:?}");
                assert!(r.end >= r.start, "inverted {r:?} in {line:?}");
                let _ = &line[r.clone()]; // char-boundary or panic
                last = r.end;
            }
        }
    }
}
