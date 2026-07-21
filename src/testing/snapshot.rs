//! Golden snapshot assertions, dependency-free.
//!
//! OWNER: REDTEAM.
//!
//! Goldens live in `tests/goldens/<name>.txt`. Behavior:
//! - `UPDATE_GOLDENS=1` -> (re)write the golden and pass.
//! - Golden missing     -> fail with instructions (never silently create:
//!   a first run on CI must not mint truth nobody reviewed).
//! - Mismatch           -> fail with a line-level diff.
//!
//! Names are restricted to `[a-z0-9_-]` so a snapshot name can never
//! escape the goldens directory or collide across platforms.

use std::fs;
use std::path::PathBuf;

fn goldens_dir() -> PathBuf {
    // CARGO_MANIFEST_DIR is compiled in, so this works from any test cwd.
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("goldens")
}

fn validate_name(name: &str) {
    assert!(
        !name.is_empty()
            && name
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'-'),
        "snapshot name {name:?} must be non-empty [a-z0-9_-] (it becomes a filename)"
    );
}

/// Compare `actual` against the stored golden `name`, per the policy above.
///
/// # Panics
/// On mismatch or missing golden (that is the point).
pub fn assert_snapshot(name: &str, actual: &str) {
    validate_name(name);
    let path = goldens_dir().join(format!("{name}.txt"));
    let update = std::env::var("UPDATE_GOLDENS")
        .map(|v| v == "1")
        .unwrap_or(false);

    if update {
        fs::create_dir_all(path.parent().expect("goldens dir has a parent"))
            .expect("create goldens dir");
        fs::write(&path, actual).expect("write golden");
        return;
    }

    let expected = match fs::read_to_string(&path) {
        Ok(s) => s,
        Err(_) => panic!(
            "golden {} does not exist.\n\
             Run with UPDATE_GOLDENS=1 to create it, then REVIEW the file:\n  {}\n\
             --- actual ---\n{}",
            name,
            path.display(),
            actual
        ),
    };

    if expected != actual {
        panic!(
            "snapshot mismatch for {name} ({}):\n{}\n\
             Run with UPDATE_GOLDENS=1 if the change is intended.",
            path.display(),
            diff(&expected, actual)
        );
    }
}

/// Line diff, minimal but readable: common prefix/suffix trimmed, the
/// differing middle shown side by side with markers.
fn diff(expected: &str, actual: &str) -> String {
    let e: Vec<&str> = expected.lines().collect();
    let a: Vec<&str> = actual.lines().collect();

    let mut start = 0;
    while start < e.len() && start < a.len() && e[start] == a[start] {
        start += 1;
    }
    let mut e_end = e.len();
    let mut a_end = a.len();
    while e_end > start && a_end > start && e[e_end - 1] == a[a_end - 1] {
        e_end -= 1;
        a_end -= 1;
    }

    // No differing middle at all: the strings differ only in trailing
    // whitespace / final-newline shape (lines() cannot see that).
    if start >= e_end && start >= a_end {
        return format!(
            "  (line content equal; byte lengths differ: expected {} vs actual {} — \
             trailing whitespace or final newline)\n",
            expected.len(),
            actual.len()
        );
    }

    let mut out = String::new();
    if start > 0 {
        out.push_str(&format!("  ... {start} matching line(s) ...\n"));
    }
    for line in &e[start..e_end] {
        out.push_str(&format!("- {line}\n"));
    }
    for line in &a[start..a_end] {
        out.push_str(&format!("+ {line}\n"));
    }
    let tail = e.len() - e_end;
    if tail > 0 {
        out.push_str(&format!("  ... {tail} matching line(s) ...\n"));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diff_trims_common_context() {
        let d = diff("a\nb\nc\nd\n", "a\nX\nc\nd\n");
        assert!(d.contains("- b"));
        assert!(d.contains("+ X"));
        assert!(d.contains("1 matching line(s)"));
        assert!(d.contains("2 matching line(s)"));
    }

    #[test]
    fn diff_detects_whitespace_only_change() {
        let d = diff("a\n", "a");
        assert!(d.contains("byte lengths differ"));
    }

    #[test]
    #[should_panic(expected = "snapshot name")]
    fn rejects_path_escaping_names() {
        assert_snapshot("../evil", "x");
    }
}
