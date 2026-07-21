//! # AbstractTUI
//!
//! A standalone, reactive, compositor-grade terminal UI engine.
//!
//! AbstractTUI treats the terminal as a real display device: a layered
//! compositor with damage tracking sits under a fine-grained reactive
//! component model (signals, not virtual-DOM diffing, not full-frame
//! immediate mode). Pixel graphics (kitty / iTerm2 / sixel / unicode
//! mosaic) and software-rasterized 3D (GLB) are first-class citizens of
//! the same scene, themed by a shared design-token system.
//!
//! Layer map (bottom to top):
//!
//! - [`base`]    — value types: geometry, color, errors
//! - [`term`]    — platform terminal I/O, capability detection
//! - [`input`]   — byte stream -> structured events
//! - [`render`]  — cells, surfaces, compositor, diff, presenter
//! - [`text`]    — measurement, wrapping, shaping helpers
//! - [`anim`]    — clock, tweens, easing, transitions, cell shaders
//! - [`reactive`]— signals, memos, effects, scopes, scheduler
//! - [`layout`]  — flexbox-style layout solver
//! - [`ui`]      — component/view tree, event routing, focus
//! - [`widgets`] — built-in widget library
//! - [`gfx`]     — bitmaps, mosaic renderers, image protocols
//! - [`three`]   — GLB loading and software 3D rasterization
//! - [`theme`]   — design tokens and theme registry
//! - [`app`]     — application runtime and frame loop
//! - [`boot`]    — AbstractTUI visual identity splash
//! - [`testing`] — test terminal, VT interpreter, harness utilities

pub mod anim;
pub mod app;
pub mod base;
pub mod boot;
pub mod gfx;
pub mod input;
pub mod layout;
pub mod prelude;
pub mod reactive;
pub mod render;
pub mod term;
pub mod testing;
pub mod text;
pub mod theme;
pub mod three;
pub mod ui;
pub mod widgets;
