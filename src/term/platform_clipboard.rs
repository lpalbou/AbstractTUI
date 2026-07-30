//! Best-effort host clipboard when the terminal does not advertise OSC 52 copy.
//!
//! Used by selection release-copy and explicit `copy_to_clipboard` calls so
//! macOS Terminal.app / Cursor-style hosts still receive text without OSC 52.

use std::io::Write;
use std::process::{Command, Stdio};

/// Copy `text` to the OS clipboard via a platform helper. Returns false when
/// no helper exists, spawn fails, or the helper exits non-zero.
pub(crate) fn try_copy(text: &str) -> bool {
    if text.trim().is_empty() {
        return false;
    }
    #[cfg(target_os = "macos")]
    {
        copy_via_stdin("pbcopy", text)
    }
    #[cfg(target_os = "linux")]
    {
        if copy_via_stdin("wl-copy", text) {
            return true;
        }
        copy_via_stdin_args(&["xclip", "-selection", "clipboard"], text)
    }
    #[cfg(windows)]
    {
        copy_windows(text)
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
    {
        let _ = text;
        false
    }
}

fn copy_via_stdin(program: &str, text: &str) -> bool {
    let mut child = match Command::new(program)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(c) => c,
        Err(_) => return false,
    };
    if let Some(mut stdin) = child.stdin.take() {
        if stdin.write_all(text.as_bytes()).is_err() {
            let _ = child.kill();
            return false;
        }
    }
    child.wait().map(|s| s.success()).unwrap_or(false)
}

#[cfg(target_os = "linux")]
fn copy_via_stdin_args(program_and_args: &[&str], text: &str) -> bool {
    let (program, args) = match program_and_args.split_first() {
        Some(pair) => pair,
        None => return false,
    };
    let mut child = match Command::new(program)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(c) => c,
        Err(_) => return false,
    };
    if let Some(mut stdin) = child.stdin.take() {
        if stdin.write_all(text.as_bytes()).is_err() {
            let _ = child.kill();
            return false;
        }
    }
    child.wait().map(|s| s.success()).unwrap_or(false)
}

#[cfg(windows)]
fn copy_windows(text: &str) -> bool {
    // clip.exe expects UTF-16 LE on stdin without a BOM.
    let mut wide: Vec<u8> = text.encode_utf16().flat_map(|u| u.to_le_bytes()).collect();
    wide.extend_from_slice(&[0, 0]);
    let mut child = match Command::new("cmd")
        .args(["/C", "clip"])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(c) => c,
        Err(_) => return false,
    };
    if let Some(mut stdin) = child.stdin.take() {
        if stdin.write_all(&wide).is_err() {
            let _ = child.kill();
            return false;
        }
    }
    child.wait().map(|s| s.success()).unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_text_is_rejected() {
        assert!(!try_copy(""));
        assert!(!try_copy("   \n\t"));
    }
}
