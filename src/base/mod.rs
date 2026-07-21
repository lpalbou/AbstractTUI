//! Foundational value types shared by every AbstractTUI layer.
//!
//! Owned by the integrator. Changes here ripple everywhere; propose
//! amendments through reviews/ rather than editing casually.

pub mod color;
pub mod error;
pub mod geom;
pub mod palette;

pub use color::Rgba;
pub use error::{Error, Result};
pub use geom::{PixelSize, Point, Rect, Size};

/// Shared vocabulary for "something wants a frame rendered".
///
/// Lives in `base` (not `anim` or `reactive`) because both the animation
/// engine and the reactive scheduler speak it, and neither may depend on
/// the other (REACT request #1, cycle 1).
pub trait FrameRequester {
    fn request_frame(&self);
}
