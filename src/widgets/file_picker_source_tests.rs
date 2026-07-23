//! StdFileSource integration: the one tempdir test for the seam's
//! `std::fs` half (sorting, sizes, hidden toggle, honest errors).
//! `#[path]`-included as `file_picker::source::tests`.

use super::{FileSource, StdFileSource};

#[test]
fn std_source_lists_sorts_sizes_and_toggles_hidden() {
    let dir = std::env::temp_dir().join(format!("abstracttui_file_picker_{}", std::process::id()));
    // Fresh fixture; tolerate a leftover from a killed prior run.
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("Beta")).unwrap();
    std::fs::create_dir_all(dir.join("alpha")).unwrap();
    std::fs::write(dir.join("zzz.txt"), b"12345").unwrap();
    std::fs::write(dir.join("Aaa.txt"), b"1").unwrap();
    std::fs::write(dir.join(".hidden"), b"x").unwrap();
    let path = dir.to_string_lossy().into_owned();

    // Dirs first, case-insensitive names inside each group, dot
    // entries skipped by default, sizes on files only.
    let listing = StdFileSource::default().read_dir(&path).unwrap();
    let names: Vec<&str> = listing.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(names, ["alpha", "Beta", "Aaa.txt", "zzz.txt"]);
    assert!(listing[0].is_dir && listing[0].size.is_none());
    let zzz = listing.iter().find(|e| e.name == "zzz.txt").unwrap();
    assert!(!zzz.is_dir);
    assert_eq!(zzz.size, Some(5));

    let with_hidden = StdFileSource::new()
        .show_hidden(true)
        .read_dir(&path)
        .unwrap();
    assert!(with_hidden.iter().any(|e| e.name == ".hidden"));

    // An unreadable/missing directory is an Err(String), never a panic.
    let missing = dir.join("missing").to_string_lossy().into_owned();
    assert!(StdFileSource::default().read_dir(&missing).is_err());

    let _ = std::fs::remove_dir_all(&dir);
}
