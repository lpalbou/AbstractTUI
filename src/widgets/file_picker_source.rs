//! The FilePicker's entry seam (backlog first-app/0273): [`FileSource`]
//! and [`FileEntry`] keep the WIDGET pure (hermetic tests, custom
//! backends — archives, remote listings), and [`StdFileSource`] is the
//! `std::fs` implementation beside it so apps get the one-liner.
//!
//! I/O contract: the picker calls [`FileSource::read_dir`] once per
//! NAVIGATION (mount, descend, parent) — never per frame, never from a
//! draw closure. `StdFileSource` reads synchronously on that
//! navigation turn: fine on a local filesystem; a network mount that
//! can stall is the app's risk, and the source seam is the escape
//! hatch (pre-list into a fake source, or debounce app-side).
//!
//! OWNER: REACT.

use std::rc::Rc;

/// One directory entry as the picker renders it. Author-written by
/// sources (ADR-0003 §2 class): plain struct with `Default` — custom
/// sources should construct via the [`FileEntry::dir`] /
/// [`FileEntry::file`] helpers or FRU over `Default`
/// (`FileEntry { name, ..Default::default() }`) so future field
/// additions stay non-breaking.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FileEntry {
    /// Entry name (no path separators). Non-UTF-8 names arrive
    /// lossily converted by `StdFileSource` (the picker speaks
    /// `String` — the terminal can only render text anyway).
    pub name: String,
    /// Directories descend on activation; files pick.
    pub is_dir: bool,
    /// Size in bytes for the optional size column; `None` hides it
    /// for this entry (directories conventionally pass `None`).
    pub size: Option<u64>,
}

impl FileEntry {
    /// A directory entry.
    pub fn dir(name: impl Into<String>) -> FileEntry {
        FileEntry {
            name: name.into(),
            is_dir: true,
            size: None,
        }
    }

    /// A file entry with an optional size.
    pub fn file(name: impl Into<String>, size: Option<u64>) -> FileEntry {
        FileEntry {
            name: name.into(),
            is_dir: false,
            size,
        }
    }
}

/// Where the picker's entries come from. Implementations list ONE
/// directory per call; errors are plain strings the picker renders
/// honestly in the list area (an unreadable directory is a fact, not
/// a panic). Entries render in the order returned — sorting is the
/// source's policy (see [`StdFileSource`] for the standard one).
pub trait FileSource {
    /// List `path` (the picker passes the paths it navigates —
    /// `start_in` joined with picked directory names, `..` never).
    fn read_dir(&self, path: &str) -> Result<Vec<FileEntry>, String>;
}

// Rc<dyn FileSource> is what the picker holds; a blanket impl lets
// helpers take either.
impl FileSource for Rc<dyn FileSource> {
    fn read_dir(&self, path: &str) -> Result<Vec<FileEntry>, String> {
        (**self).read_dir(path)
    }
}

/// The `std::fs` source: dirs first, case-insensitive name order
/// within each group, hidden entries (dot-prefixed) skipped unless
/// [`show_hidden`](StdFileSource::show_hidden). Symlinks resolve
/// through `fs::metadata`, so a symlink to a directory descends (the
/// file-manager convention); broken symlinks list as files without a
/// size.
#[derive(Clone, Debug, Default)]
pub struct StdFileSource {
    show_hidden: bool,
}

impl StdFileSource {
    /// Fresh source with hidden entries skipped.
    pub fn new() -> StdFileSource {
        StdFileSource::default()
    }

    /// Include dot-prefixed entries.
    pub fn show_hidden(mut self, on: bool) -> StdFileSource {
        self.show_hidden = on;
        self
    }
}

impl FileSource for StdFileSource {
    fn read_dir(&self, path: &str) -> Result<Vec<FileEntry>, String> {
        let iter = std::fs::read_dir(path).map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        for entry in iter {
            let entry = entry.map_err(|e| e.to_string())?;
            let name = entry.file_name().to_string_lossy().into_owned();
            if !self.show_hidden && name.starts_with('.') {
                continue;
            }
            // metadata() FOLLOWS symlinks (a link to a dir descends);
            // broken links degrade to a sizeless file entry rather
            // than failing the whole listing.
            match std::fs::metadata(entry.path()) {
                Ok(meta) if meta.is_dir() => out.push(FileEntry::dir(name)),
                Ok(meta) => out.push(FileEntry::file(name, Some(meta.len()))),
                Err(_) => out.push(FileEntry::file(name, None)),
            }
        }
        out.sort_by(|a, b| {
            b.is_dir
                .cmp(&a.is_dir)
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        });
        Ok(out)
    }
}

#[cfg(test)]
#[path = "file_picker_source_tests.rs"]
mod tests;
