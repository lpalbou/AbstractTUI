//! Corpus tests for the paste/file-drop classifier — one test per real
//! terminal spelling (module docs carry the provenance table), one per
//! documented refusal. `#[path]`-included as `input::paste::tests`.

use super::classify;

fn paths(v: &[&str]) -> Option<Vec<String>> {
    Some(v.iter().map(|s| s.to_string()).collect())
}

// ---------------------------------------------------------------------
// The corpus: every researched terminal's real drop spelling accepts.
// ---------------------------------------------------------------------

#[test]
fn terminal_app_backslash_escaped_specials() {
    // macOS Terminal.app: specials escaped, trailing space after drop.
    assert_eq!(
        classify(r"/Users/San/abc\(ev50\)_xyz.tif "),
        paths(&["/Users/San/abc(ev50)_xyz.tif"])
    );
    assert_eq!(classify(r"/Lorem\ Ipsum.txt"), paths(&["/Lorem Ipsum.txt"]));
}

#[test]
fn terminal_app_multi_drop_space_joined_escaped() {
    assert_eq!(
        classify(r"/a/one\ file.txt /b/two.txt "),
        paths(&["/a/one file.txt", "/b/two.txt"])
    );
}

#[test]
fn iterm2_default_backslash_and_optional_single_quotes() {
    // Default (>= 3.4): backslash escaping.
    assert_eq!(
        classify(r"/Users/anon/text\ file\ \(1\).txt"),
        paths(&["/Users/anon/text file (1).txt"])
    );
    // Advanced pref: single-quoted per file.
    assert_eq!(
        classify("'/Users/anon/text file (1).txt'"),
        paths(&["/Users/anon/text file (1).txt"])
    );
    // Multi-drop with the pref on: each file quoted, space-joined.
    assert_eq!(
        classify("'/a/one file.txt' '/b/two file.txt'"),
        paths(&["/a/one file.txt", "/b/two file.txt"])
    );
}

#[test]
fn ghostty_backslash_escaped_and_gtk_newline_joined_multi_drop() {
    // macOS/Linux: iTerm2-parity backslash escaping (PR #5036).
    assert_eq!(
        classify(r"/home/u/my\ document.jpg"),
        paths(&["/home/u/my document.jpg"])
    );
    // GTK apprt: multi-drop pastes shell-escaped paths joined by
    // NEWLINES (PR #4211) — per-line classification carries it.
    assert_eq!(
        classify("/home/u/a\\ b.txt\n/home/u/c.txt\n"),
        paths(&["/home/u/a b.txt", "/home/u/c.txt"])
    );
}

#[test]
fn wezterm_all_five_quote_dropped_files_modes() {
    // SpacesOnly (default off-Windows): backslash-escapes spaces only.
    assert_eq!(
        classify(r"/w/hello\ world.txt"),
        paths(&["/w/hello world.txt"])
    );
    // Posix: double quotes + POSIX escapes (their doc example shape).
    assert_eq!(
        classify(r#""/w/hello (\$world)""#),
        paths(&["/w/hello ($world)"])
    );
    // Windows / WindowsAlwaysQuoted: plain double quotes.
    assert_eq!(
        classify(r#""C:\Users\me\hello world.txt""#),
        paths(&[r"C:\Users\me\hello world.txt"])
    );
    // None: raw. Spaceless raw paths accept…
    assert_eq!(classify("/w/plain.txt"), paths(&["/w/plain.txt"]));
    // …raw WITH spaces is the documented ambiguity refusal.
    assert_eq!(classify("/w/hello world.txt"), None);
}

#[test]
fn kitty_raw_single_file() {
    // kitty pastes the path as-is (no escaping, no quoting).
    assert_eq!(classify("/home/u/photo.png"), paths(&["/home/u/photo.png"]));
    // Raw directory drop (trailing slash) too.
    assert_eq!(classify("/home/u/dir/"), paths(&["/home/u/dir/"]));
}

#[test]
fn windows_terminal_quoted_and_raw_drive_paths() {
    // Raw (no spaces): backslashes are separators, never escapes.
    assert_eq!(
        classify(r"C:\Users\me\report.xlsx"),
        paths(&[r"C:\Users\me\report.xlsx"])
    );
    // Quoted (spaces): the researched `"D:\Code\backend\api\main.py"`
    // class with a space in it.
    assert_eq!(
        classify(r#""C:\My Project\config.xml""#),
        paths(&[r"C:\My Project\config.xml"])
    );
    // Forward-slash drive spelling accepts too.
    assert_eq!(classify("C:/Users/me/x.txt"), paths(&["C:/Users/me/x.txt"]));
    // UNC, raw and quoted.
    assert_eq!(
        classify(r"\\server\share\file.txt"),
        paths(&[r"\\server\share\file.txt"])
    );
    assert_eq!(
        classify(r#""\\server\share\my file.txt""#),
        paths(&[r"\\server\share\my file.txt"])
    );
    // WSL tabs: single-quoted POSIX paths (PR #16214).
    assert_eq!(
        classify("'/mnt/c/Users/me/a b.txt'"),
        paths(&["/mnt/c/Users/me/a b.txt"])
    );
}

#[test]
fn windows_terminal_wsl_embedded_quote_bug_refuses() {
    // Bug #18006: embedded `'` is NOT escaped — malformed quoting.
    // The honest answer is refusal, never a mangled path.
    assert_eq!(classify("'/mnt/d/John's Archive'"), None);
}

#[test]
fn gnome_terminal_g_shell_quote_spellings() {
    // VTE class: file:// URIs converted to g_shell_quote'd paths,
    // space-joined.
    assert_eq!(
        classify("'/home/u/my file.txt' '/home/u/two.txt'"),
        paths(&["/home/u/my file.txt", "/home/u/two.txt"])
    );
    // g_shell_quote spells an embedded quote as `'\''`.
    assert_eq!(
        classify(r"'/home/u/can'\''t.txt'"),
        paths(&["/home/u/can't.txt"])
    );
}

#[test]
fn file_urls_decode_with_percent_and_host_handling() {
    assert_eq!(
        classify("file:///home/u/a%20b.txt"),
        paths(&["/home/u/a b.txt"])
    );
    // RFC 8089 localhost authority.
    assert_eq!(
        classify("file://localhost/Users/me/x.txt"),
        paths(&["/Users/me/x.txt"])
    );
    // Case-insensitive scheme + host.
    assert_eq!(classify("FILE://LOCALHOST/tmp/x"), paths(&["/tmp/x"]));
    // Windows drive form drops the URL's leading slash.
    assert_eq!(
        classify("file:///C:/Users/me/x.txt"),
        paths(&["C:/Users/me/x.txt"])
    );
    // uri-list class: CRLF-joined URIs (MATE bug spelling).
    assert_eq!(
        classify("file:///a/one.txt\r\nfile:///b/two%20x.txt\r\n"),
        paths(&["/a/one.txt", "/b/two x.txt"])
    );
    // UTF-8 percent sequences decode whole.
    assert_eq!(
        classify("file:///tmp/caf%C3%A9.txt"),
        paths(&["/tmp/café.txt"])
    );
}

#[test]
fn home_relative_returned_as_is() {
    // Expansion is the app's — the tilde survives verbatim.
    assert_eq!(
        classify("~/Documents/notes.md"),
        paths(&["~/Documents/notes.md"])
    );
    assert_eq!(classify(r"~/My\ Docs/a.txt"), paths(&["~/My Docs/a.txt"]));
}

#[test]
fn mixed_spellings_in_one_drop() {
    assert_eq!(
        classify(r"/a/one.txt '/b/two 2.txt' file:///c/three.txt ~/four.txt"),
        paths(&["/a/one.txt", "/b/two 2.txt", "/c/three.txt", "~/four.txt"])
    );
}

#[test]
fn trailing_newline_variants_accept() {
    assert_eq!(classify("/tmp/x\n"), paths(&["/tmp/x"]));
    assert_eq!(classify("/tmp/x\r\n"), paths(&["/tmp/x"]));
    assert_eq!(classify("/tmp/x \n\n"), paths(&["/tmp/x"]));
}

#[test]
fn root_and_deep_paths_accept() {
    assert_eq!(classify("/"), paths(&["/"]));
    let deep = format!("/{}", "d/".repeat(100));
    assert_eq!(classify(&deep), paths(&[deep.as_str()]));
}

#[test]
fn compiler_diagnostic_single_token_classifies_by_design() {
    // Colons are legal POSIX filename bytes; a single path-shaped
    // token accepts and the APP's existence check arbitrates (the
    // ruled engine/app split — module docs).
    assert_eq!(classify("/src/main.rs:12:5"), paths(&["/src/main.rs:12:5"]));
}

// ---------------------------------------------------------------------
// NOT a drop: prose, code, URLs, and every documented ambiguity.
// ---------------------------------------------------------------------

#[test]
fn prose_and_commands_refuse() {
    assert_eq!(classify("hello world"), None);
    assert_eq!(classify("see /usr/bin for details"), None);
    assert_eq!(classify("ls /a /b"), None);
    assert_eq!(classify("rm -rf /tmp/x"), None);
    // Leading absolute path followed by prose: the prose-protection
    // core case (a false positive here EATS user text).
    assert_eq!(classify("/usr and more prose"), None);
    // Flags shaped like assignments.
    assert_eq!(classify("--path=/x /y"), None);
}

#[test]
fn code_snippets_refuse() {
    assert_eq!(classify("let x = 1;\nlet y = 2;"), None);
    // Two absolute paths split across lines with an interior BLANK
    // line: prose/code shape, not the Ghostty multi-line drop.
    assert_eq!(classify("/a/one.txt\n\n/b/two.txt"), None);
    // Compiler OUTPUT line (path + message) refuses via the prose
    // tokens after the path.
    assert_eq!(classify("/src/main.rs:12:5: error: expected `;`"), None);
}

#[test]
fn non_file_urls_refuse() {
    assert_eq!(classify("http://example.com/a.txt"), None);
    assert_eq!(classify("https://example.com/"), None);
    assert_eq!(classify("ssh://host/path"), None);
    // file URL with a NON-local host cannot become a local path.
    assert_eq!(classify("file://nas.local/share/x.txt"), None);
    // file URL with no path.
    assert_eq!(classify("file://"), None);
    assert_eq!(classify("file://localhost"), None);
}

#[test]
fn relative_and_bare_words_refuse() {
    assert_eq!(classify("foo.txt"), None);
    assert_eq!(classify("./relative/path"), None);
    assert_eq!(classify("../up/one"), None);
    assert_eq!(classify("src/lib.rs"), None);
    assert_eq!(classify("~"), None);
    assert_eq!(classify("~/"), None);
    // ~user form: drops never emit it.
    assert_eq!(classify("~bob/x"), None);
}

#[test]
fn malformed_quoting_and_escapes_refuse() {
    assert_eq!(classify("'/a/unterminated"), None);
    assert_eq!(classify("\"/a/unterminated"), None);
    assert_eq!(classify("/a/dangling\\"), None);
    // kitty-raw path with an unescaped apostrophe: the documented
    // raw-with-specials residual.
    assert_eq!(classify("/a/John's.txt"), None);
    // Empty word.
    assert_eq!(classify("'' /a/x"), None);
}

#[test]
fn control_characters_refuse() {
    assert_eq!(classify("/a/x\t/b/y"), None);
    assert_eq!(classify("/a/x\u{7}"), None);
    // Percent-encoded control in a file URL.
    assert_eq!(classify("file:///a/x%0Ay"), None);
    // Malformed percent escapes.
    assert_eq!(classify("file:///a/x%2"), None);
    assert_eq!(classify("file:///a/x%zz"), None);
    // Non-UTF-8 decode refuses.
    assert_eq!(classify("file:///a/x%FF%FE"), None);
}

#[test]
fn size_and_count_guards_refuse() {
    // Whole-paste cap: 64 KiB+ is code/prose, never a drop.
    let huge = format!("/a/{}", "x".repeat(70 * 1024));
    assert_eq!(classify(&huge), None);
    // Per-token cap (PATH_MAX class).
    let long_tok = format!("/a/{}", "x".repeat(5000));
    assert_eq!(classify(&long_tok), None);
    // Path-count cap.
    let many = (0..600)
        .map(|i| format!("/f/{i}"))
        .collect::<Vec<_>>()
        .join(" ");
    assert_eq!(classify(&many), None);
    // Empty / whitespace-only.
    assert_eq!(classify(""), None);
    assert_eq!(classify("   \n  "), None);
}

#[test]
fn windows_edge_shapes() {
    // Drive letter without a separator is prose ("C:" alone, "note:").
    assert_eq!(classify("C:"), None);
    assert_eq!(classify("note: /a/x"), None);
    // `C://x` reads as a URL scheme, refused (never a real drop
    // spelling; drive paths use one slash or a backslash).
    assert_eq!(classify("C://x"), None);
    // Quoted windows path must END at its closing quote.
    assert_eq!(classify(r#""C:\a b"tail"#), None);
    // POSIX-escaped spelling of a UNC path decodes coherently and
    // accepts: the escaping signal is present and the decoded token is
    // path-shaped (no terminal emits this today; pinned so the decode
    // order — escapes first, shape second — stays deliberate).
    assert_eq!(
        classify(r"\\\\srv\\share\\x.txt"),
        paths(&[r"\\srv\share\x.txt"])
    );
}

#[test]
fn multiline_requires_every_line_to_classify() {
    // One good line + one prose line refuses the WHOLE paste.
    assert_eq!(classify("/a/one.txt\nand some prose"), None);
    // Multi-path lines compose (space-joined per line, newline-joined
    // lines).
    assert_eq!(
        classify("/a/one.txt /b/two.txt\nfile:///c/three.txt"),
        paths(&["/a/one.txt", "/b/two.txt", "/c/three.txt"])
    );
}
