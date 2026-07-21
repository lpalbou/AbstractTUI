//! Pluggable syntax tinting: a line -> token spans seam plus one small
//! built-in lexer.
//!
//! Deliberately THEME-AGNOSTIC and render-agnostic: tokens are byte
//! ranges + [`TokenKind`]; mapping kinds to colors/styles happens at the
//! consumer (`render::rich::RichLine::from_highlighted` bridges into the
//! rich-text model; theme tokens live with DESIGN). Editor/code widgets
//! plug real language servers or tree-sitter-class lexers behind the same
//! trait later — the seam is the contract, the built-in is a demo-grade
//! convenience.
//!
//! Honest limits of the built-in (documented, not hidden): it lexes ONE
//! LINE at a time with no cross-line state, so block comments and string
//! literals spanning lines mis-tint from the second line on; it knows
//! C-family surface syntax only (comments `//` and `/* */`, double/single
//! quoted strings with backslash escapes, numbers incl. `0x`/`_`/`.`,
//! identifiers, punctuation). "Approximate by design" — good enough for
//! demos and logs, never a language authority.

/// Theme-agnostic token classes. Coarse on purpose: six buckets a theme
/// can color without knowing any language.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum TokenKind {
    /// Reserved words (`fn`, `if`, ...).
    Keyword,
    /// Quoted literals, escapes included.
    String,
    /// Numeric literals (decimal, hex, `_` separators).
    Number,
    /// Line and block comments.
    Comment,
    /// Identifiers not matching the keyword list.
    Ident,
    /// Operators, brackets and other punctuation.
    Punct,
}

/// One line -> non-overlapping, ascending byte ranges with kinds. Gaps
/// between ranges (whitespace, anything unclassified) carry no token and
/// render in the consumer's base style.
pub trait Highlighter {
    /// Tokenizes one line (no cross-line state by contract).
    fn spans(&self, line: &str) -> Vec<(std::ops::Range<usize>, TokenKind)>;
}

/// The built-in demo lexer: C-like surface syntax with a configurable
/// keyword list. `Default` is the rust preset (the demo target — widgets
/// constructed it via `default()` the hour this landed, so the alias is
/// contract now).
#[derive(Clone, Debug)]
pub struct CLikeLexer {
    keywords: &'static [&'static str],
}

impl Default for CLikeLexer {
    fn default() -> Self {
        CLikeLexer::rust()
    }
}

impl CLikeLexer {
    /// A lexer with a custom keyword list (sorted or not; matched exactly).
    pub fn new(keywords: &'static [&'static str]) -> CLikeLexer {
        CLikeLexer { keywords }
    }

    /// Rust-ish keyword set (the demo target).
    pub fn rust() -> CLikeLexer {
        const RUST: &[&str] = &[
            "as", "async", "await", "break", "const", "continue", "crate", "dyn", "else", "enum",
            "extern", "false", "fn", "for", "if", "impl", "in", "let", "loop", "match", "mod",
            "move", "mut", "pub", "ref", "return", "self", "Self", "static", "struct", "super",
            "trait", "true", "type", "unsafe", "use", "where", "while",
        ];
        CLikeLexer::new(RUST)
    }

    /// C/JS-ish set, for logs and config-ish text.
    pub fn c() -> CLikeLexer {
        const C: &[&str] = &[
            "break", "case", "char", "const", "continue", "default", "do", "double", "else",
            "enum", "extern", "float", "for", "goto", "if", "int", "long", "return", "short",
            "signed", "sizeof", "static", "struct", "switch", "typedef", "union", "unsigned",
            "void", "volatile", "while",
        ];
        CLikeLexer::new(C)
    }
}

impl Highlighter for CLikeLexer {
    fn spans(&self, line: &str) -> Vec<(std::ops::Range<usize>, TokenKind)> {
        let mut out = Vec::new();
        let b = line.as_bytes();
        let mut i = 0;
        while i < b.len() {
            let start = i;
            let c = b[i];
            // Line comment: everything to EOL.
            if c == b'/' && b.get(i + 1) == Some(&b'/') {
                out.push((start..b.len(), TokenKind::Comment));
                break;
            }
            // Block comment (within the line; unterminated -> to EOL,
            // the honest single-line approximation).
            if c == b'/' && b.get(i + 1) == Some(&b'*') {
                let end = find_sub(b, i + 2, b"*/").map(|p| p + 2).unwrap_or(b.len());
                out.push((start..end, TokenKind::Comment));
                i = end;
                continue;
            }
            // Strings: double or single quoted, backslash escapes;
            // unterminated -> to EOL.
            if c == b'"' || c == b'\'' {
                let quote = c;
                let mut j = i + 1;
                while j < b.len() {
                    if b[j] == b'\\' {
                        j = (j + 2).min(b.len());
                        continue;
                    }
                    if b[j] == quote {
                        j += 1;
                        break;
                    }
                    j += 1;
                }
                out.push((start..j, TokenKind::String));
                i = j;
                continue;
            }
            // Numbers: digit-led; hex/underscores/dot/exponent letters ride.
            if c.is_ascii_digit() {
                let mut j = i + 1;
                while j < b.len() && (b[j].is_ascii_alphanumeric() || b[j] == b'_' || b[j] == b'.')
                {
                    j += 1;
                }
                out.push((start..j, TokenKind::Number));
                i = j;
                continue;
            }
            // Identifiers / keywords: ASCII ident charset (non-ASCII text
            // falls through to the gap — untinted, never split mid-UTF-8
            // because the byte range grows only over ASCII here).
            if c.is_ascii_alphabetic() || c == b'_' {
                let mut j = i + 1;
                while j < b.len() && (b[j].is_ascii_alphanumeric() || b[j] == b'_') {
                    j += 1;
                }
                let word = &line[start..j];
                let kind = if self.keywords.contains(&word) {
                    TokenKind::Keyword
                } else {
                    TokenKind::Ident
                };
                out.push((start..j, kind));
                i = j;
                continue;
            }
            // Punctuation: single ASCII punct byte (runs kept 1-byte so a
            // theme can tint brackets later without re-lexing).
            if c.is_ascii_punctuation() {
                out.push((start..i + 1, TokenKind::Punct));
                i += 1;
                continue;
            }
            // Whitespace / non-ASCII: skip one UTF-8 scalar (gap).
            i += utf8_len(c);
        }
        out
    }
}

fn find_sub(hay: &[u8], from: usize, needle: &[u8]) -> Option<usize> {
    hay.get(from..)?
        .windows(needle.len())
        .position(|w| w == needle)
        .map(|p| p + from)
}

fn utf8_len(first: u8) -> usize {
    match first {
        0x00..=0x7F => 1,
        0xC0..=0xDF => 2,
        0xE0..=0xEF => 3,
        _ => 4,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(lexer: &CLikeLexer, line: &str) -> Vec<(String, TokenKind)> {
        lexer
            .spans(line)
            .into_iter()
            .map(|(r, k)| (line[r].to_string(), k))
            .collect()
    }

    #[test]
    fn rust_snippet_tokenizes_sanely() {
        let lx = CLikeLexer::rust();
        let toks = kinds(&lx, r#"let n: u32 = font.width("héllo") + 0x1F; // demo"#);
        use TokenKind::*;
        let expect = |t: &str, k: TokenKind| {
            assert!(
                toks.iter().any(|(s, kk)| s == t && *kk == k),
                "expected {t:?} as {k:?} in {toks:?}"
            );
        };
        expect("let", Keyword);
        expect("u32", Ident);
        expect("font", Ident);
        expect("width", Ident);
        expect("\"héllo\"", String);
        expect("0x1F", Number);
        expect("// demo", Comment);
        expect("=", Punct);
        // Ranges are ascending + non-overlapping (the trait contract).
        let spans = lx.spans(r#"let n = "x"; // c"#);
        let mut last = 0;
        for (r, _) in &spans {
            assert!(r.start >= last, "overlap/disorder at {r:?}");
            last = r.end;
        }
    }

    #[test]
    fn strings_and_comments_edge_cases() {
        let lx = CLikeLexer::rust();
        // Escapes stay inside the string; unterminated runs to EOL.
        let toks = kinds(&lx, r#"print("a\"b") "open"#);
        assert!(
            toks.contains(&(r#""a\"b""#.to_string(), TokenKind::String)),
            "escaped quote stays inside: {toks:?}"
        );
        assert_eq!(toks.last().unwrap().0, r#""open"#);
        assert_eq!(toks.last().unwrap().1, TokenKind::String);
        // Block comment inside a line; unterminated to EOL.
        let toks = kinds(&lx, "a /* mid */ b /* open");
        assert!(
            toks.contains(&("/* mid */".to_string(), TokenKind::Comment)),
            "{toks:?}"
        );
        assert_eq!(toks.last().unwrap().0, "/* open");
        // Comment marker inside a string is NOT a comment.
        let toks = kinds(&lx, r#"let u = "http://x";"#);
        assert!(toks.iter().all(|(_, k)| *k != TokenKind::Comment));
    }

    #[test]
    fn non_ascii_text_is_an_untinted_gap_not_a_crash() {
        let lx = CLikeLexer::rust();
        let line = "let 名前 = \"ok\"; // 説明";
        let spans = lx.spans(line);
        // Every range must sit on char boundaries (slicing must not panic).
        for (r, _) in &spans {
            let _ = &line[r.clone()];
        }
        assert!(spans.iter().any(|(_, k)| *k == TokenKind::Keyword));
        assert!(spans.iter().any(|(_, k)| *k == TokenKind::Comment));
    }
}
