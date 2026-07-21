//! Test rig: an in-memory terminal double capturing the byte stream,
//! plus a small VT interpreter that applies emitted bytes to a model
//! screen — so tests can assert "the bytes we emit produce the frame we
//! intended" (the diff/present correctness property), snapshot frames as
//! text, drive scripted input, and fuzz parsers deterministically.
//!
//! OWNER: REDTEAM. Doctrine: `docs/design/testing.md`.
//!
//! Map:
//! - [`vt`] / [`vt_csi`] — `VtScreen`, the VT100/xterm model (ground truth)
//! - [`grid`]     — the model's cell grid + wide-glyph pairing invariant
//! - [`palette`]  — embedded xterm-256 palette (indexed SGR -> `Rgba`)
//! - [`capture`]  — `CaptureTerm`, in-memory terminal double
//! - [`snapshot`] — golden snapshot assertions (UPDATE_GOLDENS=1)
//! - [`fuzzish`]  — seeded PRNG + hostile byte-soup generators
//! - [`bench`]    — std-only timing harness for `#[ignore]`d perf tests
//!
//! This module ships in the library (not `#[cfg(test)]`) so integration
//! tests and other modules' unit tests can use it; it costs nothing
//! unless referenced and depends only on `crate::base` + unicode-width.

pub mod bench;
pub mod capture;
pub mod frames;
pub mod fuzzish;
pub mod glb_mutate;
pub mod grid;
pub mod jpeg_build;
pub mod kitty_model;
pub mod palette;
pub mod pty;
pub mod snapshot;
pub mod vt;
mod vt_csi;
mod vt_dump;
mod vt_state;

pub use bench::{sink, time_median, Measurement};
pub use capture::{CaptureTerm, ScriptedRead};
pub use fuzzish::{hostile_corpus, random_splits, Rng};
pub use glb_mutate::{minimal_glb, mutants as glb_mutants, Expect as GlbExpect, GlbMutant};
pub use grid::{Attrs, CellContent, Paint, VtCell};
pub use kitty_model::{unwrap_tmux, KittyModel};
pub use palette::{xterm_256, SYSTEM_16};
pub use snapshot::assert_snapshot;
pub use vt::{Modes, VtCounters, VtScreen};
