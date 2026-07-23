//! Paste-text classification: does this paste LOOK like a terminal
//! file drop? (backlog first-app/0273)
//!
//! Terminals have no drop protocol — dropping a file onto every major
//! terminal PASTES its path (through bracketed paste when the app
//! enabled it, so it arrives as one `Event::Paste`). The spelling of
//! that paste varies per terminal, and that cross-terminal knowledge
//! is exactly what this module owns so applications never re-derive
//! it. [`classify`] is PURE string parsing — zero I/O by contract:
//! existence checks (and `~` expansion) are the application's side of
//! the split, where a wrong guess can be corrected against the real
//! filesystem.
//!
//! # The drop-spelling corpus (researched 2026-07-25)
//!
//! | Terminal | Drop spelling | Source |
//! |---|---|---|
//! | Terminal.app (macOS) | backslash-escaped specials (`/a/b\ c\(1\).txt`), space-joined multi-drop, trailing space | apple.SE 401039; SO 31926420 |
//! | iTerm2 ≥ 3.4 | backslash-escaped by default; advanced pref switches to single-quoted (`'/a/b c.txt'`) | apple.SE 401039 (accepted answer) |
//! | Ghostty | backslash-escaped on macOS AND Linux (`Shell.escape`, PR #5036 "same behaviour as iTerm2"); GTK multi-drop joins with **newlines** (PR #4211) | ghostty-org/ghostty #5036, #4211, discussion #10029 |
//! | WezTerm | `quote_dropped_files`: default `SpacesOnly` (backslash-escapes spaces only); `Posix` = double-quoted with `\$`-class escapes (`"hello (\$world)"`); `Windows`/`WindowsAlwaysQuoted` = double-quoted; `None` = raw | wezterm.org/config/lua/config/quote_dropped_files |
//! | kitty | raw path AS-IS — no escaping, no quoting, by maintainer policy; multi-drop separation has varied | kovidgoyal/kitty #613, #4734 |
//! | Windows Terminal | double-quoted when the path has spaces (`"C:\a b\c.txt"`), raw otherwise; WSL tabs single-quote (PR #16214) but do NOT escape embedded `'` (bug #18006 — malformed quoting) | microsoft/terminal #8109, #15646, #18006 |
//! | GNOME Terminal (VTE class) | dropped `file://` URIs are converted to LOCAL paths and `g_shell_quote`d (single quotes), space-joined | gnome-terminal r2613/r2620 commits |
//! | MATE Terminal (bug class) | raw `file://` URI reaches the app (the `text/uri-list` pick-over-plain bug); uri-lists are CRLF-joined | mate-desktop/mate-terminal #448 |
//!
//! Every spelling above is corpus-tested in `paste_tests.rs`.
//!
//! # The acceptance rule
//!
//! The whole paste (ends trimmed) must decompose into shell-style
//! tokens where EVERY token is one path-shaped string: POSIX absolute
//! (`/…`), home-relative (`~/…`, returned as-is — expansion is the
//! app's), Windows drive (`C:\…` / `C:/…`), UNC (`\\server\…`), or a
//! `file://` URL (percent-decoded; empty or `localhost` host only).
//! Multi-file drops are space-joined (every terminal above except
//! Ghostty GTK) or newline-joined (Ghostty GTK, raw uri-lists) — a
//! multi-LINE paste classifies only when every line classifies on its
//! own. One token that is not path-shaped refuses the WHOLE paste.
//!
//! # The ambiguity policy (asymmetry rules)
//!
//! When unsure, return `None`: a false positive EATS user text (the
//! app consumes the paste), a false negative just pastes — so every
//! ambiguous case refuses. The deliberate refusals:
//!
//! - **Raw unescaped spaces** (`/a/My File.txt` from kitty, or WezTerm
//!   with `quote_dropped_files = "None"`): after tokenizing, `File.txt`
//!   is not path-shaped, so the paste refuses. Accepting "one path
//!   with spaces" here would also capture every prose sentence that
//!   starts with an absolute path. The escaping/quoting signal is what
//!   licenses multi-token acceptance; raw-with-specials terminals are
//!   the documented residual (the app may still fs-check and offer).
//! - **Raw unescaped quotes** (`/a/John's.txt` from kitty): an
//!   unterminated quote refuses — same residual class.
//! - **Interior blank lines**, control characters (tabs included),
//!   relative paths (`./x`, `a/b`), bare `~`, non-`file:` URLs
//!   (`http://…`), pastes over 64 KiB, tokens over 4 KiB, more than
//!   512 paths: all refuse.
//!
//! Compiler-diagnostic spellings (`/src/main.rs:12:5`) DO classify as
//! a single path — colons are legal in POSIX filenames; the app's
//! existence check is the honest arbiter for those (the ruled split).
//!
//! OWNER: KERNEL (input pipeline knowledge, pure string half).

/// Refuse pastes larger than this outright — no real drop is 64 KiB,
/// and the cap keeps `classify` O(small) on huge code pastes.
const MAX_TOTAL_BYTES: usize = 64 * 1024;
/// Per-path byte cap (PATH_MAX class: 1 KiB on macOS, 4 KiB on Linux).
const MAX_TOKEN_BYTES: usize = 4096;
/// Sanity cap on the number of dropped paths.
const MAX_PATHS: usize = 512;

/// Classify pasted text as a file drop: `Some(paths)` when the WHOLE
/// paste is one or more path-shaped tokens in a real terminal's drop
/// spelling (decoded: escapes removed, quotes stripped, `file://` URLs
/// percent-decoded to plain paths), `None` for everything else —
/// prose, code, URLs, and every ambiguous case (module docs: the
/// asymmetry policy). Pure string parsing; no I/O, no allocation
/// beyond the returned paths.
///
/// ```
/// use abstracttui::input::paste::classify;
///
/// // Terminal.app / iTerm2 / Ghostty: backslash-escaped drop.
/// assert_eq!(
///     classify(r"/Users/a/My\ File.txt "),
///     Some(vec!["/Users/a/My File.txt".into()])
/// );
/// // Two files, mixed spellings (escaped + single-quoted).
/// assert_eq!(
///     classify(r"/tmp/a\ b.txt '/tmp/c d.txt'"),
///     Some(vec!["/tmp/a b.txt".into(), "/tmp/c d.txt".into()])
/// );
/// // file:// URL (VTE class), %20-decoded.
/// assert_eq!(
///     classify("file:///home/u/a%20b.txt\n"),
///     Some(vec!["/home/u/a b.txt".into()])
/// );
/// // Prose is never a drop.
/// assert_eq!(classify("see /usr/bin for details"), None);
/// ```
pub fn classify(text: &str) -> Option<Vec<String>> {
    if text.is_empty() || text.len() > MAX_TOTAL_BYTES {
        return None;
    }
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }
    let mut out = Vec::new();
    // Ghostty GTK joins multi-drops with newlines; uri-lists are
    // CRLF-joined — a multi-line paste classifies only when EVERY
    // line classifies on its own (an interior blank line is prose).
    for line in trimmed.split('\n') {
        let line = line.strip_suffix('\r').unwrap_or(line).trim_matches(' ');
        if line.is_empty() {
            return None;
        }
        classify_line(line, &mut out)?;
    }
    if out.is_empty() || out.len() > MAX_PATHS {
        return None;
    }
    Some(out)
}

/// One line = one or more space-separated tokens, every one a path.
fn classify_line(line: &str, out: &mut Vec<String>) -> Option<()> {
    let mut rest = line;
    loop {
        rest = rest.trim_start_matches(' ');
        if rest.is_empty() {
            return Some(());
        }
        let (token, remainder) = take_token(rest)?;
        out.push(accept_token(token)?);
        rest = remainder;
    }
}

/// Does `s` start with a Windows path shape — drive letter (`C:\`,
/// `C:/`) or UNC (`\\server`)? Selects backslash-LITERAL consumption:
/// POSIX escape decoding would eat the separators of a raw Windows
/// path (`C:\Users` -> `C:Users`).
fn windows_path_ahead(s: &str) -> bool {
    let b = s.as_bytes();
    (b.len() >= 3 && b[0].is_ascii_alphabetic() && b[1] == b':' && (b[2] == b'\\' || b[2] == b'/'))
        || (b.len() >= 3 && b[0] == b'\\' && b[1] == b'\\' && b[2] != b'\\')
}

/// Consume ONE token from the front of `s` (already space-trimmed),
/// returning the DECODED token text and the unconsumed remainder.
/// `None` on anything malformed: unterminated quotes, dangling
/// backslash, control characters, empty token.
fn take_token(s: &str) -> Option<(String, &str)> {
    // Raw Windows shape: backslashes are separators, not escapes.
    // Unquoted Windows drops never contain spaces (Windows Terminal
    // quotes those), so the token ends at the first space.
    if windows_path_ahead(s) {
        let end = s.find(' ').unwrap_or(s.len());
        let tok = &s[..end];
        if tok.chars().any(char::is_control) {
            return None;
        }
        return Some((tok.to_string(), &s[end..]));
    }
    // Double-quoted Windows shape (`"C:\a b\c.txt"`, `"\\srv\share"`):
    // POSIX double-quote decoding would halve a UNC prefix (`\\` ->
    // `\`), so backslashes stay literal up to the closing quote. `"`
    // is illegal in Windows file names, so the next `"` IS the close;
    // the token must end there (Windows Terminal never concatenates).
    if let Some(inner) = s.strip_prefix('"') {
        if windows_path_ahead(inner) {
            let close = inner.find('"')?;
            let tok = &inner[..close];
            if tok.chars().any(char::is_control) {
                return None;
            }
            let rest = &inner[close + 1..];
            if !rest.is_empty() && !rest.starts_with(' ') {
                return None;
            }
            return Some((tok.to_string(), rest));
        }
    }
    take_posix_token(s)
}

/// POSIX-style shell word: backslash escapes outside quotes, literal
/// single quotes, double quotes with the POSIX escape subset
/// (backslash is special only before `$`, `` ` ``, `"`, `\`), adjacent
/// segments concatenating into one word (`'/a/can'\''t'`).
fn take_posix_token(s: &str) -> Option<(String, &str)> {
    #[derive(PartialEq)]
    enum Mode {
        Plain,
        Single,
        Double,
    }
    let mut tok = String::new();
    let mut mode = Mode::Plain;
    let mut chars = s.char_indices();
    while let Some((i, c)) = chars.next() {
        match mode {
            Mode::Plain => match c {
                ' ' => {
                    if tok.is_empty() {
                        return None; // `''` — an empty word refuses
                    }
                    return Some((tok, &s[i..]));
                }
                '\\' => {
                    let (_, next) = chars.next()?; // dangling backslash refuses
                    if next.is_control() {
                        return None;
                    }
                    tok.push(next);
                }
                '\'' => mode = Mode::Single,
                '"' => mode = Mode::Double,
                c if c.is_control() => return None,
                c => tok.push(c),
            },
            Mode::Single => match c {
                '\'' => mode = Mode::Plain,
                c if c.is_control() => return None,
                c => tok.push(c),
            },
            Mode::Double => match c {
                '"' => mode = Mode::Plain,
                '\\' => {
                    let (_, next) = chars.next()?;
                    match next {
                        '$' | '`' | '"' | '\\' => tok.push(next),
                        c if c.is_control() => return None,
                        // POSIX: backslash before anything else stays
                        // a literal backslash, the char follows.
                        c => {
                            tok.push('\\');
                            tok.push(c);
                        }
                    }
                }
                c if c.is_control() => return None,
                c => tok.push(c),
            },
        }
    }
    if mode != Mode::Plain || tok.is_empty() {
        return None; // unterminated quote / empty word
    }
    Some((tok, ""))
}

/// Is this decoded token path-shaped? Returns the final path (plain
/// shapes pass through; `file://` URLs decode).
fn accept_token(tok: String) -> Option<String> {
    if tok.len() > MAX_TOKEN_BYTES {
        return None;
    }
    // URL schemes first: `file` decodes, every other scheme refuses
    // (http/https/ssh/... pastes are links, never drops).
    if let Some(scheme_len) = url_scheme_len(&tok) {
        if tok[..scheme_len].eq_ignore_ascii_case("file") {
            return decode_file_url(&tok[scheme_len + 3..]);
        }
        return None;
    }
    let pathish = tok.starts_with('/')
        || (tok.starts_with("~/") && tok.len() > 2)
        || windows_path_ahead(&tok);
    pathish.then_some(tok)
}

/// Length of the scheme when `s` starts with `scheme://` (RFC 3986
/// shape: ALPHA *( ALPHA / DIGIT / "+" / "-" / "." )), else `None`.
fn url_scheme_len(s: &str) -> Option<usize> {
    let b = s.as_bytes();
    if b.is_empty() || !b[0].is_ascii_alphabetic() {
        return None;
    }
    let mut i = 1;
    while i < b.len()
        && (b[i].is_ascii_alphanumeric() || b[i] == b'+' || b[i] == b'-' || b[i] == b'.')
    {
        i += 1;
    }
    (s[i..].starts_with("://")).then_some(i)
}

/// Decode the part after `file://`: `[host]/path`. Only an empty or
/// `localhost` host is a LOCAL file (RFC 8089); any other host
/// refuses (we cannot hand the app a local path for it).
/// Percent-decodes, requires valid UTF-8, refuses control characters,
/// and strips the leading slash of the Windows drive form
/// (`file:///C:/…` -> `C:/…`).
fn decode_file_url(rest: &str) -> Option<String> {
    let slash = rest.find('/')?;
    let (host, path) = rest.split_at(slash);
    if !(host.is_empty() || host.eq_ignore_ascii_case("localhost")) {
        return None;
    }
    let decoded = percent_decode(path)?;
    if decoded.chars().any(char::is_control) {
        return None;
    }
    // `/C:/…` (and bare `/C:`) is the URL spelling of a drive path.
    let b = decoded.as_bytes();
    let windows_drive = b.len() >= 3
        && b[0] == b'/'
        && b[1].is_ascii_alphabetic()
        && b[2] == b':'
        && (b.len() == 3 || b[3] == b'/' || b[3] == b'\\');
    let out = if windows_drive {
        decoded[1..].to_string()
    } else {
        decoded
    };
    if out.is_empty() || out.len() > MAX_TOKEN_BYTES {
        return None;
    }
    Some(out)
}

/// `%XX` decoding for file-URL paths. Malformed escapes or non-UTF-8
/// results refuse (a real uri-list encodes valid UTF-8 paths).
fn percent_decode(s: &str) -> Option<String> {
    if !s.contains('%') {
        return Some(s.to_string());
    }
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hi = hex_val(*bytes.get(i + 1)?)?;
            let lo = hex_val(*bytes.get(i + 2)?)?;
            out.push(hi * 16 + lo);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

fn hex_val(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
#[path = "paste_tests.rs"]
mod tests;
