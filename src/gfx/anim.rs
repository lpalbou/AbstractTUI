//! Animated pictures: a decoded frame sequence and the magic-routed
//! entry that produces one.
//!
//! Two formats decode here: **animated GIF** (LZW, patents expired
//! 2004) and **APNG** (PNG chunks over the inflate already in-tree).
//! Both are permissively licensed, free of patent pools, need no new
//! dependency, and are small enough to review — the bar an in-tree
//! decoder has to clear.
//!
//! VIDEO IS NOT DECODED HERE. `.mp4`, `.mov`, `.avi`, and `.webm`
//! carry H.264, H.265, VP9 or MPEG-4, which are patent-pooled and
//! whose real-time decoders are each larger than this crate. They are
//! recognized by container and refused by name, with the command that
//! converts them into something this engine does play. To play video
//! as it is, decode it outside the engine and feed frames in — the
//! pattern is in `docs/graphics-and-3d.md`.
//!
//! Guards: a still image is capped at `png::MAX_PIXELS`; an animation
//! is capped again at [`MAX_ANIMATION_PIXELS`] across all frames and
//! [`MAX_FRAMES`] in count, both checked AS frames accumulate — a
//! twelve-byte header must never be able to ask for a gigabyte.

use std::time::Duration;

use crate::base::{Error, Result};
use crate::gfx::bitmap::Bitmap;

/// Total decoded pixels an animation may hold: 64 Mpx ≈ 256 MB of
/// RGBA, four times the still-image budget. Frame sequences are held
/// whole (playback re-reads them every loop); a movie that does not
/// fit belongs on the streaming path, not in a Vec.
pub const MAX_ANIMATION_PIXELS: u64 = 1 << 26;

/// Frame-count ceiling, independent of size: a 1x1 pixel animation
/// with a million frames is still an attack.
pub const MAX_FRAMES: usize = 4096;

/// One fully composited frame and how long it shows.
#[derive(Clone, Debug)]
pub struct Frame {
    /// The COMPLETE picture at this point in time — never a delta.
    /// Formats that encode deltas (GIF disposal, APNG blending) are
    /// composited during decode, so playback is a plain frame swap.
    pub image: Bitmap,
    pub delay: Duration,
}

/// A decoded frame sequence.
#[derive(Clone, Debug)]
pub struct Animation {
    pub frames: Vec<Frame>,
    /// `None` = loop forever (the common case); `Some(n)` = play n
    /// times, as declared by the file.
    pub loop_count: Option<u32>,
    pub width: u32,
    pub height: u32,
}

impl Animation {
    /// A still image as a one-frame animation — the shape that lets a
    /// caller treat every picture the same way.
    pub fn still(image: Bitmap) -> Animation {
        let (width, height) = (image.width(), image.height());
        Animation {
            frames: vec![Frame {
                image,
                delay: Duration::ZERO,
            }],
            loop_count: Some(1),
            width,
            height,
        }
    }

    /// Frame count (always ≥ 1 for a decoded animation).
    pub fn len(&self) -> usize {
        self.frames.len()
    }

    pub fn is_empty(&self) -> bool {
        self.frames.is_empty()
    }

    /// True when there is nothing to animate — one frame, no clock
    /// needed, no idle cost.
    pub fn is_still(&self) -> bool {
        self.frames.len() <= 1
    }

    /// Total run time of one pass.
    pub fn duration(&self) -> Duration {
        self.frames.iter().map(|f| f.delay).sum()
    }

    /// The frame showing at `elapsed` into playback, looping per the
    /// file's declared loop count. `None` once a finite animation has
    /// finished (the caller shows the last frame and stops the clock).
    pub fn frame_at(&self, elapsed: Duration) -> Option<&Frame> {
        if self.frames.is_empty() {
            return None;
        }
        let total = self.duration();
        if total.is_zero() {
            return self.frames.first();
        }
        let passes = elapsed.as_nanos() / total.as_nanos();
        if let Some(limit) = self.loop_count {
            if passes >= limit as u128 {
                return self.frames.last();
            }
        }
        let mut into = Duration::from_nanos((elapsed.as_nanos() % total.as_nanos()) as u64);
        for f in &self.frames {
            if into < f.delay {
                return Some(f);
            }
            into -= f.delay;
        }
        self.frames.last()
    }
}

/// The animated container `decode_animation` recognized.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum AnimationFormat {
    Gif,
    /// PNG carrying an `acTL` chunk.
    Apng,
}

/// Sniff an animated container from leading bytes. A still PNG or
/// JPEG answers `None` — route those through
/// [`decode_image`](crate::gfx::decode_image).
pub fn sniff_animation(bytes: &[u8]) -> Option<AnimationFormat> {
    if bytes.starts_with(&crate::gfx::gif::SIGNATURE) {
        return Some(AnimationFormat::Gif);
    }
    if bytes.starts_with(&crate::gfx::png::SIGNATURE) {
        return crate::gfx::apng::is_animated(bytes).then_some(AnimationFormat::Apng);
    }
    None
}

/// Decode an animated picture: **animated GIF** or **APNG**. A still
/// PNG, JPEG, or GIF decodes to a one-frame [`Animation`], so a caller
/// can hand any picture to the same code path.
///
/// Video containers (`.mp4`, `.mov`, `.avi`, `.webm`) reject BY NAME
/// with the command that converts them — see the module docs.
pub fn decode_animation(bytes: &[u8]) -> Result<Animation> {
    match sniff_animation(bytes) {
        Some(AnimationFormat::Gif) => crate::gfx::gif::decode(bytes),
        Some(AnimationFormat::Apng) => crate::gfx::apng::decode(bytes),
        // Not an animated container: a still is a one-frame animation
        // (and a video container refuses inside `decode_image`).
        None => crate::gfx::decode_image(bytes).map(Animation::still),
    }
}

/// Shared budget check for the decoders: called as frames accumulate,
/// never trusted from a header.
pub(crate) fn check_budget(frames: usize, w: u32, h: u32) -> Result<()> {
    if frames > MAX_FRAMES {
        return Err(Error::Parse(format!(
            "animation: more than {MAX_FRAMES} frames"
        )));
    }
    if (frames as u64) * (w as u64) * (h as u64) > MAX_ANIMATION_PIXELS {
        return Err(Error::Parse(
            "animation: exceeds the total pixel budget".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gfx::anim_fixtures as fx;

    /// Every animated container the engine decodes, through the one
    /// front door, agreeing on what the clip IS.
    #[test]
    fn every_container_decodes_through_one_entry() {
        for (label, bytes) in [("gif", fx::GIF), ("apng", fx::APNG)] {
            let a = decode_animation(bytes).unwrap_or_else(|e| panic!("{label}: {e}"));
            assert_eq!(a.len(), fx::FRAMES, "{label} frames");
            assert_eq!((a.width, a.height), fx::SIZE, "{label} size");
            assert!(!a.is_still(), "{label} moves");
            assert_eq!(
                a.duration(),
                std::time::Duration::from_millis(300),
                "{label}"
            );
        }
    }

    /// A still image is a one-frame animation: callers get one code
    /// path for "show this picture", moving or not.
    #[test]
    fn stills_decode_as_one_frame_animations() {
        let png = crate::gfx::png_encode::encode(&crate::gfx::Bitmap::new(
            3,
            2,
            crate::base::Rgba::WHITE,
        ));
        let a = decode_animation(&png).unwrap();
        assert_eq!(a.len(), 1);
        assert!(a.is_still());
        assert_eq!((a.width, a.height), (3, 2));
        assert_eq!(a.duration(), Duration::ZERO, "a still has no clock");
    }

    /// The sniffer answers only for containers it can actually decode:
    /// a still PNG is not an animation, an APNG is.
    #[test]
    fn sniffing_separates_animated_from_still() {
        assert_eq!(sniff_animation(fx::GIF), Some(AnimationFormat::Gif));
        assert_eq!(sniff_animation(fx::APNG), Some(AnimationFormat::Apng));
        assert_eq!(
            sniff_animation(fx::H264_MP4),
            None,
            "a video container is not an animation this engine decodes"
        );
        let still = crate::gfx::png_encode::encode(&crate::gfx::Bitmap::new(
            2,
            2,
            crate::base::Rgba::WHITE,
        ));
        assert_eq!(sniff_animation(&still), None, "a plain PNG is a still");
        assert_eq!(sniff_animation(b"nothing at all"), None);
    }

    /// Video is REFUSED, and the refusal is the whole video story:
    /// the container is named and the message carries the command that
    /// turns the file into something the engine plays. Both doors give
    /// the same answer.
    #[test]
    fn video_refuses_by_name_with_the_conversion_line() {
        for msg in [
            decode_animation(fx::H264_MP4).unwrap_err().to_string(),
            crate::gfx::decode_image(fx::H264_MP4)
                .unwrap_err()
                .to_string(),
        ] {
            assert!(msg.contains("mp4/mov"), "names the container: {msg}");
            assert!(msg.contains("video is not decoded"), "{msg}");
            assert!(msg.contains("ffmpeg -i"), "carries the fix: {msg}");
        }
        // Every video container a user is likely to point at gets the
        // same treatment — recognized, named, refused. Never a half
        // decode, never a hex dump of magic bytes.
        let mut avi = b"RIFF\x00\x00\x00\x00AVI LIST".to_vec();
        avi.resize(64, 0);
        assert!(decode_animation(&avi)
            .unwrap_err()
            .to_string()
            .contains("avi:"));
        let mut mkv = vec![0x1A, 0x45, 0xDF, 0xA3];
        mkv.resize(64, 0);
        assert!(decode_animation(&mkv)
            .unwrap_err()
            .to_string()
            .contains("webm/mkv:"));
    }

    /// The playback clock's only question: which frame shows now.
    #[test]
    fn frame_at_walks_the_timeline_and_honors_the_loop_count() {
        let bmp = |v: u8| crate::gfx::Bitmap::new(1, 1, crate::base::Rgba::rgb(v, v, v));
        let frames = (0..3)
            .map(|i| Frame {
                image: bmp(i * 10),
                delay: Duration::from_millis(100),
            })
            .collect::<Vec<_>>();
        let looping = Animation {
            frames: frames.clone(),
            loop_count: None,
            width: 1,
            height: 1,
        };
        let at = |a: &Animation, ms: u64| {
            a.frame_at(Duration::from_millis(ms))
                .unwrap()
                .image
                .get(0, 0)
                .unwrap()
                .r
        };
        assert_eq!(at(&looping, 0), 0);
        assert_eq!(at(&looping, 99), 0, "still inside frame 0");
        assert_eq!(
            at(&looping, 100),
            10,
            "exactly the boundary is the next frame"
        );
        assert_eq!(at(&looping, 250), 20);
        assert_eq!(at(&looping, 300), 0, "wraps forever");
        assert_eq!(at(&looping, 1_000_000), 10, "no drift after many loops");

        let once = Animation {
            frames,
            loop_count: Some(1),
            width: 1,
            height: 1,
        };
        assert_eq!(at(&once, 250), 20);
        assert_eq!(at(&once, 300), 20, "a finished clip holds its last frame");
        assert_eq!(at(&once, 9_999), 20);
    }

    /// The budget is checked as frames ACCUMULATE, never trusted from
    /// a header: a small file must not be able to ask for gigabytes.
    #[test]
    fn budget_rejects_absurd_sequences() {
        assert!(check_budget(1, 16, 16).is_ok());
        assert!(check_budget(MAX_FRAMES + 1, 1, 1).is_err(), "frame count");
        assert!(check_budget(4096, 4096, 4096).is_err(), "total pixels");
    }
}
