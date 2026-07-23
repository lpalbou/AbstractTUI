//! Statement normalization, shared by every diagram parser.
//!
//! Mermaid is line-oriented with optional `;` statement terminators
//! and `%%` end-of-line comments. Normalization is LEXICAL only (it
//! recognizes no constructs): strip comments (quote-aware — a `%%`
//! inside `"…"` is text), split on newlines and `;`, trim, drop
//! empties, keep 1-based source line numbers for honest fallback
//! reporting. `%%{…}` init/theme directives are recognized-and-dropped
//! WITH a notice (the subset table's IGNORED row); plain comments drop
//! silently.

/// One trimmed, non-empty statement with its source line number.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Stmt {
    pub line_no: usize,
    pub text: String,
}

/// Normalize a source into statements + directive notices.
pub(crate) fn statements(source: &str) -> (Vec<Stmt>, Vec<String>) {
    let mut stmts = Vec::new();
    let mut notices = Vec::new();
    for (i, raw) in source.lines().enumerate() {
        let line_no = i + 1;
        let code = strip_comment(raw, line_no, &mut notices);
        for piece in code.split(';') {
            let text = piece.trim();
            if !text.is_empty() {
                stmts.push(Stmt {
                    line_no,
                    text: text.to_string(),
                });
            }
        }
    }
    (stmts, notices)
}

/// Remove a trailing `%%…` comment (quote-aware). An init/theme
/// directive (`%%{…`) records a notice; a plain comment is silent.
fn strip_comment<'a>(line: &'a str, line_no: usize, notices: &mut Vec<String>) -> &'a str {
    let bytes = line.as_bytes();
    let mut in_quotes = false;
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'"' => in_quotes = !in_quotes,
            b'%' if !in_quotes && i + 1 < bytes.len() && bytes[i + 1] == b'%' => {
                if bytes.get(i + 2) == Some(&b'{') {
                    notices.push(format!("init/theme directive ignored (line {line_no})"));
                }
                return &line[..i];
            }
            _ => {}
        }
        i += 1;
    }
    line
}

/// Leading identifier (`[A-Za-z0-9_]+`): `(id, rest)` or `None` when
/// the text does not start with one.
pub(crate) fn take_id(s: &str) -> Option<(&str, &str)> {
    let end = s
        .bytes()
        .position(|b| !(b.is_ascii_alphanumeric() || b == b'_'))
        .unwrap_or(s.len());
    if end == 0 {
        None
    } else {
        Some((&s[..end], &s[end..]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn comments_split_and_numbering() {
        let src = "graph TD %% header comment\n\n  A --> B; B --> C ;\n%%{init: {\"theme\":\"dark\"}}%%\nC --> D";
        let (stmts, notices) = statements(src);
        let texts: Vec<&str> = stmts.iter().map(|s| s.text.as_str()).collect();
        assert_eq!(texts, vec!["graph TD", "A --> B", "B --> C", "C --> D"]);
        assert_eq!(stmts[0].line_no, 1);
        assert_eq!(stmts[1].line_no, 3);
        assert_eq!(stmts[3].line_no, 5);
        assert_eq!(notices.len(), 1, "init directive noticed: {notices:?}");
    }

    #[test]
    fn quoted_percent_is_text_not_comment() {
        let (stmts, notices) = statements("graph TD\nA[\"100%% done\"] --> B");
        assert_eq!(stmts[1].text, "A[\"100%% done\"] --> B");
        assert!(notices.is_empty());
    }
}
