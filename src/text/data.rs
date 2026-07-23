//! Structured-data lexers: JSON and YAML line tinting (wave 13, the
//! "top-tier json/yaml" half of the content-rendering ruling,
//! ADR-0005).
//!
//! WHY A THIRD VOCABULARY: the distinction a data document lives on is
//! KEY vs VALUE — and "key" is not a token kind the C-like vocabulary
//! can express (`TokenKind` is a public exhaustive enum frozen until
//! the 0.3 window, the same constraint that shaped [`super::diff`]).
//! [`DataKind`] is the dedicated, additive vocabulary; consumers map it
//! to theme inks in ONE place (`widgets::code::data_token_color`),
//! exactly like `code_token_color` and `diff_token_color`.
//!
//! Honest limits (documented, not hidden — "approximate by design,
//! never a language authority", the module rule from `highlight.rs`):
//!
//! - Both lexers are STATELESS per line, deliberately: `CodeView`
//!   renders from a scroll offset, so cross-line state would tint the
//!   same line differently depending on scroll position. The cost:
//!   JSON strings never span lines (the grammar agrees), YAML
//!   multi-line quoted scalars mis-tint from the second line on, and
//!   block-scalar bodies (`|`/`>` continuations) render untinted —
//!   which for prose bodies is the right look anyway.
//! - Key detection is the space-after-colon rule: a string/word whose
//!   next non-space byte is `:` followed by a space (or end of line,
//!   or a closing/flow byte) reads as a key. `10:30:00` therefore
//!   stays a plain scalar (no space after the colon) — the YAML
//!   block-mapping rule, applied uniformly to both languages and to
//!   flow collections (`{a: 1}` tints `a` as a key).
//! - Totality: any `&str` line lexes without panicking; every range
//!   sits on char boundaries (token starts/continuations are ASCII
//!   tests; non-ASCII text falls through to the untinted gap whole).
//!
//! JSON accepts the JSONC/JSON5 comment forms (`//`, `/* */`) so
//! config-dialect fences tint instead of degrading; strict-JSON input
//! never contains them, so the widening costs nothing.

use std::ops::Range;

/// Theme-agnostic structured-data token classes. Coarse on purpose,
/// like [`super::TokenKind`]: what a reader scans a JSON/YAML document
/// by, without the lexer knowing any theme.
///
/// `#[non_exhaustive]`: this vocabulary may grow (multi-document
/// markers, merge keys) — per ADR-0003 §3, enums the engine may grow
/// are born non-exhaustive. Downstream `match`es carry a `_` arm and
/// should render unknown kinds as body text (never invisible).
#[non_exhaustive]
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum DataKind {
    /// A mapping key (`"name":` / `name:`), quoted or bare.
    Key,
    /// A quoted string VALUE (keys are [`DataKind::Key`]).
    String,
    /// A numeric literal (decimal, hex/octal via the alnum rule, `_`
    /// separators, exponents).
    Number,
    /// A word literal of the grammar: `true`/`false`/`null` (JSON);
    /// plus `yes`/`no`/`on`/`off`/`~` and case variants (YAML).
    Literal,
    /// A comment (`#` in YAML; `//` and `/* */` in JSONC/JSON5).
    Comment,
    /// Structural punctuation: braces, brackets, commas, colons, list
    /// dashes, block-scalar indicators.
    Punct,
    /// YAML machinery that ANNOTATES structure: anchors (`&a`),
    /// aliases (`*a`), tags (`!!str`), and document markers
    /// (`---`/`...`).
    Tag,
}

/// The JSON line lexer (strict JSON plus JSONC/JSON5 comments).
/// Stateless and `Copy`-cheap; a struct so per-instance configuration
/// can arrive additively later (the [`super::DiffLexer`] shape).
#[derive(Copy, Clone, Debug, Default)]
pub struct JsonLexer;

impl JsonLexer {
    pub fn new() -> JsonLexer {
        JsonLexer
    }

    /// True when a markdown fence / code-view language label names a
    /// JSON dialect (`json`, `jsonc`, `json5`, `jsonl`, `ndjson` —
    /// first word, case-insensitive).
    pub fn matches_lang(label: &str) -> bool {
        let first = label.split_whitespace().next().unwrap_or("");
        ["json", "jsonc", "json5", "jsonl", "ndjson"]
            .iter()
            .any(|l| first.eq_ignore_ascii_case(l))
    }

    /// Tokenizes one line: ascending, non-overlapping byte ranges.
    pub fn spans(&self, line: &str) -> Vec<(Range<usize>, DataKind)> {
        let mut out = Vec::new();
        let b = line.as_bytes();
        let mut i = 0;
        while i < b.len() {
            let c = b[i];
            // JSONC/JSON5 comments.
            if c == b'/' && b.get(i + 1) == Some(&b'/') {
                out.push((i..b.len(), DataKind::Comment));
                break;
            }
            if c == b'/' && b.get(i + 1) == Some(&b'*') {
                let end = find_sub(b, i + 2, b"*/").map(|p| p + 2).unwrap_or(b.len());
                out.push((i..end, DataKind::Comment));
                i = end;
                continue;
            }
            if c == b'"' {
                let end = scan_dquote(b, i + 1);
                // JSON colons are unambiguous (they only separate keys
                // from values), so no space-after rule here: any `:`
                // after the string marks a key — minified input
                // (`{"a":1}`) tints like pretty-printed.
                let kind = if json_key_follows(b, end) {
                    DataKind::Key
                } else {
                    DataKind::String
                };
                out.push((i..end, kind));
                i = end;
                continue;
            }
            if c.is_ascii_digit() || (c == b'-' && b.get(i + 1).is_some_and(u8::is_ascii_digit)) {
                let end = scan_number(b, i + 1);
                out.push((i..end, DataKind::Number));
                i = end;
                continue;
            }
            if c.is_ascii_alphabetic() {
                let end = scan_word(b, i + 1);
                if matches!(&line[i..end], "true" | "false" | "null") {
                    out.push((i..end, DataKind::Literal));
                }
                // Other bare words are not JSON: untinted gap, honest.
                i = end;
                continue;
            }
            if matches!(c, b'{' | b'}' | b'[' | b']' | b',' | b':') {
                out.push((i..i + 1, DataKind::Punct));
                i += 1;
                continue;
            }
            i += utf8_len(c);
        }
        out
    }
}

/// The YAML line lexer. Stateless per line (see the module limits);
/// covers block mappings, list items, flow collections, quoted
/// scalars, comments, anchors/aliases/tags and document markers.
#[derive(Copy, Clone, Debug, Default)]
pub struct YamlLexer;

impl YamlLexer {
    pub fn new() -> YamlLexer {
        YamlLexer
    }

    /// True when a language label names YAML (`yaml`, `yml` — first
    /// word, case-insensitive).
    pub fn matches_lang(label: &str) -> bool {
        let first = label.split_whitespace().next().unwrap_or("");
        first.eq_ignore_ascii_case("yaml") || first.eq_ignore_ascii_case("yml")
    }

    /// Tokenizes one line: ascending, non-overlapping byte ranges.
    pub fn spans(&self, line: &str) -> Vec<(Range<usize>, DataKind)> {
        let mut out = Vec::new();
        let b = line.as_bytes();
        // Document markers: the whole trimmed line.
        let trimmed = line.trim();
        if trimmed == "---" || trimmed == "..." {
            let start = line.len() - line.trim_start().len();
            out.push((start..start + 3, DataKind::Tag));
            return out;
        }
        let mut i = 0;
        // True at line start and after `- ` markers / opening flow —
        // the positions where a `#` cannot be mid-scalar. (The real
        // rule is "# after whitespace"; tracked below.)
        let mut prev_space = true;
        while i < b.len() {
            let c = b[i];
            if c == b'#' && prev_space {
                out.push((i..b.len(), DataKind::Comment));
                break;
            }
            if c == b'"' || c == b'\'' {
                let end = if c == b'"' {
                    scan_dquote(b, i + 1)
                } else {
                    scan_squote(b, i + 1)
                };
                out.push((i..end, key_or_string(b, end)));
                prev_space = false;
                i = end;
                continue;
            }
            // List dash: `- ` (or a lone `-` at end of line).
            if c == b'-' && prev_space && matches!(b.get(i + 1), None | Some(b' ')) {
                out.push((i..i + 1, DataKind::Punct));
                prev_space = true; // a nested `- - item` chains
                i += 1;
                if b.get(i) == Some(&b' ') {
                    i += 1;
                }
                continue;
            }
            // Anchors, aliases, tags: `&name` / `*name` / `!tag`.
            if matches!(c, b'&' | b'*' | b'!') && prev_space {
                let mut j = i + 1;
                while j < b.len()
                    && !b[j].is_ascii_whitespace()
                    && !matches!(b[j], b',' | b'}' | b']')
                {
                    j += 1;
                }
                if j > i + 1 || c == b'!' {
                    out.push((i..j, DataKind::Tag));
                    prev_space = false;
                    i = j;
                    continue;
                }
            }
            if c.is_ascii_digit() || (c == b'-' && b.get(i + 1).is_some_and(u8::is_ascii_digit)) {
                let end = scan_number(b, i + if c == b'-' { 2 } else { 1 });
                // A digit-led token that continues into non-delimiter
                // text (`10:30:00`, `1.2.3-beta`) is a plain scalar,
                // not a number — leave the rest untinted too.
                if key_follows(b, end) {
                    out.push((i..end, DataKind::Key));
                } else if matches!(
                    b.get(end),
                    None | Some(b' ') | Some(b',') | Some(b'}') | Some(b']') | Some(b'#')
                ) {
                    out.push((i..end, DataKind::Number));
                }
                prev_space = false;
                i = end.max(i + 1);
                continue;
            }
            if c.is_ascii_alphabetic() || c == b'_' {
                let end = scan_yaml_word(b, i + 1);
                let word = &line[i..end];
                if key_follows(b, end) {
                    out.push((i..end, DataKind::Key));
                } else if is_yaml_literal(word) {
                    out.push((i..end, DataKind::Literal));
                }
                prev_space = false;
                i = end;
                continue;
            }
            if c == b'~' {
                out.push((i..i + 1, DataKind::Literal));
                prev_space = false;
                i += 1;
                continue;
            }
            if matches!(
                c,
                b'{' | b'}' | b'[' | b']' | b',' | b':' | b'|' | b'>' | b'?'
            ) {
                out.push((i..i + 1, DataKind::Punct));
                // `,`/`{`/`[`/`:` open a fresh scalar position (flow
                // keys after commas, values after colons).
                prev_space = matches!(c, b'{' | b'[' | b',' | b':');
                i += 1;
                continue;
            }
            prev_space = c.is_ascii_whitespace();
            i += utf8_len(c);
        }
        out
    }
}

/// JSON key rule: the next non-space byte after the closed string is a
/// colon (JSON has no other colon position, so no space-after test).
fn json_key_follows(b: &[u8], end: usize) -> bool {
    let mut j = end;
    while j < b.len() && b[j] == b' ' {
        j += 1;
    }
    b.get(j) == Some(&b':')
}

/// After a closed scalar ending at `end`: is the next non-space byte a
/// key-marking colon (`:` followed by space, end of line, or a flow
/// close)? The space-after-colon rule keeps `10:30:00` a plain scalar.
fn key_follows(b: &[u8], end: usize) -> bool {
    let mut j = end;
    while j < b.len() && b[j] == b' ' {
        j += 1;
    }
    if b.get(j) != Some(&b':') {
        return false;
    }
    matches!(
        b.get(j + 1),
        None | Some(b' ') | Some(b'\t') | Some(b',') | Some(b'}') | Some(b']')
    )
}

fn key_or_string(b: &[u8], end: usize) -> DataKind {
    if key_follows(b, end) {
        DataKind::Key
    } else {
        DataKind::String
    }
}

/// Double-quoted scan from just past the opening quote: backslash
/// escapes; unterminated runs to EOL. Returns the byte AFTER the
/// closing quote (or `b.len()`).
fn scan_dquote(b: &[u8], from: usize) -> usize {
    let mut j = from;
    while j < b.len() {
        match b[j] {
            b'\\' => j = (j + 2).min(b.len()),
            b'"' => return j + 1,
            _ => j += 1,
        }
    }
    b.len()
}

/// Single-quoted YAML scan: `''` is an escaped quote; unterminated
/// runs to EOL.
fn scan_squote(b: &[u8], from: usize) -> usize {
    let mut j = from;
    while j < b.len() {
        if b[j] == b'\'' {
            if b.get(j + 1) == Some(&b'\'') {
                j += 2;
                continue;
            }
            return j + 1;
        }
        j += 1;
    }
    b.len()
}

/// Number continuation: the C-like alnum + `_` + `.` rule (hex,
/// exponents and version-ish tails ride; approximate by design).
fn scan_number(b: &[u8], from: usize) -> usize {
    let mut j = from;
    while j < b.len() && (b[j].is_ascii_alphanumeric() || b[j] == b'_' || b[j] == b'.') {
        j += 1;
    }
    j
}

/// ASCII identifier continuation (JSON literals).
fn scan_word(b: &[u8], from: usize) -> usize {
    let mut j = from;
    while j < b.len() && (b[j].is_ascii_alphanumeric() || b[j] == b'_') {
        j += 1;
    }
    j
}

/// YAML bare-scalar word: identifier charset plus `-` (kebab keys are
/// the YAML default: `read-only:`).
fn scan_yaml_word(b: &[u8], from: usize) -> usize {
    let mut j = from;
    while j < b.len() && (b[j].is_ascii_alphanumeric() || b[j] == b'_' || b[j] == b'-') {
        j += 1;
    }
    j
}

/// The YAML word-literal set (1.1 bools included — config files in the
/// wild carry them), case-insensitive for the classic spellings.
fn is_yaml_literal(word: &str) -> bool {
    ["true", "false", "null", "yes", "no", "on", "off"]
        .iter()
        .any(|l| word.eq_ignore_ascii_case(l))
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
#[path = "data_tests.rs"]
mod tests;
