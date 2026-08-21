//! `AnimatedImage`: play a decoded frame sequence in the cell grid.
//!
//! ```no_run
//! use abstracttui::prelude::*;
//! use abstracttui::widgets::{AnimatedImage, ImageFit};
//!
//! # fn build(cx: Scope) -> View {
//! AnimatedImage::from_path("loading.gif")
//!     .fit(ImageFit::Contain)
//!     .view(cx)
//! # }
//! ```
//!
//! ## What playback costs (the zero-idle law, stated honestly)
//!
//! A moving picture is the one thing in this engine that cannot be
//! idle. Each frame arms ONE one-shot timer for that frame's own delay
//! ([`reactive::after`](crate::reactive::after)), so a paused or
//! finished animation costs exactly nothing, and a playing one costs
//! one wakeup per frame — not a poll loop, and never a wakeup between
//! frames. A single-frame source (any still image) arms no timer at
//! all.
//!
//! Bytes per frame are the channel's business: mosaic repaints are a
//! CELL DIFF (only what changed is emitted), which is why a moving
//! picture is cheapest on the universal path and most expensive on the
//! pixel protocols, where every frame is a full payload. See
//! `docs/graphics-and-3d.md`.
//!
//! Tokens: only `text_faint` (the broken-source label). OWNER: GFX3D.

use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;

use crate::base::{Point, Rgba};
use crate::gfx::anim::{decode_animation, Animation};
use crate::gfx::mosaic::{MosaicMode, MosaicRenderer};
use crate::layout::{Dimension, Style as LayoutStyle};
use crate::reactive::{after, Scope, Signal};
use crate::theme::TokenSet;
use crate::ui::{dyn_view, Element};
use crate::widgets::image::{draw_fitted, ImageAlign, ImageFit};

/// A frame sequence playing in a cell rect. Build it from a file, from
/// bytes, or from an [`Animation`] you decoded yourself.
pub struct AnimatedImage {
    source: Result<Arc<Animation>, String>,
    fit: ImageFit,
    mode: Option<MosaicMode>,
    align_h: ImageAlign,
    align_v: ImageAlign,
    layout: LayoutStyle,
    playing: Option<Signal<bool>>,
    repeat: Option<bool>,
}

impl AnimatedImage {
    /// Decode an animated file at view-build time: an animated GIF or
    /// an APNG. A still image decodes to a one-frame animation, so any
    /// picture works here. Undecodable sources — a video file among
    /// them, since the engine decodes no video codecs — produce the
    /// labeled broken state carrying the decoder's own message,
    /// including its conversion command.
    pub fn from_path(path: impl AsRef<std::path::Path>) -> AnimatedImage {
        let path = path.as_ref();
        let source = match std::fs::read(path) {
            Err(e) => Err(format!("unreadable: {e}")),
            Ok(bytes) => decode_animation(&bytes)
                .map(Arc::new)
                .map_err(|e| format!("undecodable: {e}")),
        };
        AnimatedImage::new(source)
    }

    /// Decode from bytes already in hand (an attachment, a download).
    pub fn from_bytes(bytes: &[u8]) -> AnimatedImage {
        AnimatedImage::new(
            decode_animation(bytes)
                .map(Arc::new)
                .map_err(|e| format!("undecodable: {e}")),
        )
    }

    /// Play an [`Animation`] you already hold (decoded once, shared
    /// across rebuilds — the `Arc` makes rebuilds free).
    pub fn from_animation(animation: Arc<Animation>) -> AnimatedImage {
        AnimatedImage::new(if animation.is_empty() {
            Err("empty animation".to_string())
        } else {
            Ok(animation)
        })
    }

    fn new(source: Result<Arc<Animation>, String>) -> AnimatedImage {
        AnimatedImage {
            source,
            fit: ImageFit::Contain,
            mode: None,
            align_h: ImageAlign::Center,
            align_v: ImageAlign::Center,
            layout: LayoutStyle::default(),
            playing: None,
            repeat: None,
        }
    }

    pub fn fit(mut self, fit: ImageFit) -> AnimatedImage {
        self.fit = fit;
        self
    }

    /// Pin the mosaic glyph family (default: follow the terminal —
    /// see [`Image::mode`](crate::widgets::Image::mode)).
    pub fn mode(mut self, mode: MosaicMode) -> AnimatedImage {
        self.mode = Some(mode);
        self
    }

    pub fn align(mut self, horizontal: ImageAlign, vertical: ImageAlign) -> AnimatedImage {
        self.align_h = horizontal;
        self.align_v = vertical;
        self
    }

    pub fn layout(mut self, style: LayoutStyle) -> AnimatedImage {
        self.layout = style;
        self
    }

    /// Bind playback to a signal: `false` pauses on the current frame
    /// and disarms the clock, `true` resumes. Unbound animations play.
    pub fn playing(mut self, playing: Signal<bool>) -> AnimatedImage {
        self.playing = Some(playing);
        self
    }

    /// Override the file's own loop declaration: `true` loops forever,
    /// `false` stops on the last frame. A GIF usually declares
    /// "forever"; a movie declares "once".
    pub fn repeat(mut self, repeat: bool) -> AnimatedImage {
        self.repeat = Some(repeat);
        self
    }

    /// The decode error, if the source is broken.
    pub fn error(&self) -> Option<&str> {
        self.source.as_ref().err().map(String::as_str)
    }

    /// Frame count (1 for a still, 0 for a broken source).
    pub fn frame_count(&self) -> usize {
        self.source.as_ref().map(|a| a.len()).unwrap_or(0)
    }

    /// Canonical build: tokens resolve from the app's theme context.
    pub fn view(self, cx: Scope) -> crate::ui::View {
        let t = crate::widgets::theme_tokens(cx);
        self.element(cx, &t).build()
    }

    /// Explicit-theming door. Takes a `Scope` (unlike `Image`) because
    /// playback owns reactive state: the frame index and its clock.
    pub fn element(self, cx: Scope, t: &TokenSet) -> Element {
        let faint = t.text_faint;
        let (fit, pinned) = (self.fit, self.mode);
        let (ah, av) = (self.align_h, self.align_v);
        let frame = cx.signal(0usize);

        if let Ok(animation) = &self.source {
            // A still needs no clock at all — the zero-idle law holds
            // for every non-animated picture that comes through here.
            if !animation.is_still() {
                arm(cx, animation.clone(), frame, self.playing, self.repeat);
            }
        }

        let source = self.source;
        let natural = match &source {
            Ok(a) => {
                let mode = crate::widgets::Image::resolved_mode(pinned);
                let (subw, subh) = mode.cell_pixels();
                crate::base::Size::new(
                    (a.width.div_ceil(subw) as i32).max(1),
                    (a.height.div_ceil(subh) as i32).max(1),
                )
            }
            Err(_) => crate::base::Size::new(7, 2), // "⌧ image" + message
        };
        // The frame index is read in a `dyn_view`, NEVER in the draw
        // closure: a tracked read inside paint is the stale-region bug
        // (RT1-2) and a debug panic in a real app. The picture regions
        // it damages are exactly this child's, so the clock's advance
        // repaints the picture and nothing else.
        let fill = LayoutStyle::default()
            .width(Dimension::Percent(1.0))
            .height(Dimension::Percent(1.0));
        let frames = dyn_view(fill.clone(), move || {
            let i = frame.get();
            let source = source.clone();
            let mut renderer = MosaicRenderer::new();
            Element::new()
                .style(fill.clone())
                .draw(move |canvas, rect| {
                    if rect.w <= 0 || rect.h <= 0 {
                        return;
                    }
                    let animation = match &source {
                        Ok(a) => a,
                        Err(label) => {
                            canvas.print(rect.origin(), "⌧ image", faint, Rgba::TRANSPARENT);
                            let msg: String = label.chars().take(rect.w.max(0) as usize).collect();
                            if rect.h > 1 {
                                canvas.print(
                                    Point::new(rect.x, rect.y + 1),
                                    &msg,
                                    faint,
                                    Rgba::TRANSPARENT,
                                );
                            }
                            return;
                        }
                    };
                    let Some(f) = animation
                        .frames
                        .get(i.min(animation.len().saturating_sub(1)))
                    else {
                        return;
                    };
                    let mode = crate::widgets::Image::resolved_mode(pinned);
                    draw_fitted(canvas, rect, &f.image, fit, mode, ah, av, &mut renderer);
                })
                .build()
        });
        Element::new()
            .style(self.layout)
            .measure(move |_avail| natural)
            .child(frames)
    }
}

/// Arm the re-arming one-shot chain that advances `frame`.
///
/// One timer per frame, each for that frame's own delay: variable-delay
/// animations (every GIF that pauses on a beat) play at their real
/// cadence instead of a lowest-common-denominator poll. The chain stops
/// itself when the scope dies (`alive`), when playback is paused, or
/// when a non-repeating animation reaches its last frame.
fn arm(
    cx: Scope,
    animation: Arc<Animation>,
    frame: Signal<usize>,
    playing: Option<Signal<bool>>,
    repeat: Option<bool>,
) {
    let alive = Rc::new(Cell::new(true));
    {
        let alive = alive.clone();
        cx.on_cleanup(move || alive.set(false));
    }
    let forever = repeat.unwrap_or(animation.loop_count.is_none());
    // A paused animation must re-arm when it resumes; the effect
    // re-runs on that flip and starts a fresh chain, so `generation`
    // retires the old one.
    let generation = Rc::new(Cell::new(0u64));
    cx.effect(move || {
        let live = playing.map(|p| p.get()).unwrap_or(true);
        generation.set(generation.get() + 1);
        if !live {
            return; // paused: no timer armed, nothing wakes
        }
        let mine = generation.get();
        step(
            animation.clone(),
            frame,
            alive.clone(),
            generation.clone(),
            mine,
            forever,
        );
    });
}

fn step(
    animation: Arc<Animation>,
    frame: Signal<usize>,
    alive: Rc<Cell<bool>>,
    generation: Rc<Cell<u64>>,
    mine: u64,
    forever: bool,
) {
    if !alive.get() || generation.get() != mine {
        return;
    }
    let i = frame.get_untracked().min(animation.len().saturating_sub(1));
    if !forever && i + 1 >= animation.len() {
        // The last frame of a clip that plays once: it stays on screen
        // and NOTHING is armed — a finished movie costs what a still
        // costs.
        return;
    }
    let delay = animation.frames[i].delay;
    if delay.is_zero() {
        return; // a zero-delay sequence is a still: never spin on it
    }
    after(delay, move || {
        if !alive.get() || generation.get() != mine {
            return;
        }
        let next = i + 1;
        if next >= animation.len() {
            if !forever {
                return; // finished: the last frame stays, the clock stops
            }
            frame.set(0);
        } else {
            frame.set(next);
        }
        step(animation, frame, alive, generation, mine, forever);
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::base::Size;
    use crate::gfx::anim_fixtures as fx;
    use crate::reactive::create_root;
    use crate::theme::default_theme;
    use crate::widgets::itest_util::{mount_widget, render};

    /// The widget accepts every animated container the engine decodes,
    /// and a still, through one constructor.
    #[test]
    fn accepts_every_source_the_engine_decodes() {
        for (label, bytes, frames) in [("gif", fx::GIF, fx::FRAMES), ("apng", fx::APNG, fx::FRAMES)]
        {
            let w = AnimatedImage::from_bytes(bytes);
            assert_eq!(w.error(), None, "{label}: {:?}", w.error());
            assert_eq!(w.frame_count(), frames, "{label}");
        }
        let still = crate::gfx::png_encode::encode(&crate::gfx::Bitmap::new(
            4,
            4,
            crate::base::Rgba::WHITE,
        ));
        assert_eq!(AnimatedImage::from_bytes(&still).frame_count(), 1);
    }

    /// A video file reaches the USER as the labeled broken state
    /// carrying the decoder's own message — including the fix.
    #[test]
    fn refused_codec_shows_the_labeled_state_with_the_fix() {
        let w = AnimatedImage::from_bytes(fx::H264_MP4);
        let err = w.error().expect("video must refuse").to_string();
        assert!(err.contains("mp4/mov"), "{err}");
        assert!(err.contains("ffmpeg -i"), "{err}");

        let theme = default_theme();
        let size = Size::new(60, 2);
        let (_root, mut tree) = mount_widget(size, |cx| {
            AnimatedImage::from_bytes(fx::H264_MP4)
                .element(cx, &theme.tokens)
                .build()
        });
        let canvas = render(&mut tree, size);
        assert!(
            canvas.row_text(0).starts_with("⌧ image"),
            "{:?}",
            canvas.row_text(0)
        );
        assert!(
            canvas.row_text(1).contains("video is not decoded"),
            "{:?}",
            canvas.row_text(1)
        );
    }

    /// Frame 0 paints before any clock ticks: a first frame is never
    /// blank while the timer waits.
    #[test]
    fn first_frame_paints_immediately() {
        let theme = default_theme();
        let size = Size::new(12, 4);
        let (_root, mut tree) = mount_widget(size, |cx| {
            AnimatedImage::from_bytes(fx::GIF)
                .fit(ImageFit::Fill)
                .mode(MosaicMode::HalfBlock)
                .element(cx, &theme.tokens)
                .build()
        });
        let canvas = render(&mut tree, size);
        // The fixture's frame 0 has its red bar at column 1; the cell
        // grid is 1:1 horizontally in HalfBlock, so cell 1 carries it.
        let (_, fg, bg) = canvas.cell(crate::base::Point::new(1, 0)).expect("painted");
        assert!(
            bg.r > 150 || fg.r > 150,
            "frame 0's bar must be painted at cell 1: fg {fg:?} bg {bg:?}"
        );
    }

    /// THE playback proof: driving the engine's timer wheel advances
    /// the picture, one frame per armed timer, at the frame's own
    /// delay — and a finished non-repeating clip stops arming.
    #[test]
    fn the_clock_advances_frames_and_then_stops() {
        use crate::reactive::{drain_posted, flush_effects, next_timer_deadline, run_due_timers};

        let theme = default_theme();
        // Mounted and painted through the REAL pipeline (`UiTree::draw`
        // arms the RT1-2 draw-phase guard): a widget that read its frame
        // signal inside paint would panic here exactly as it does in an
        // app, instead of passing against a hand-called closure.
        let size = Size::new(12, 4);
        let (_root, mut tree) = mount_widget(size, |cx| {
            // `.repeat(false)` = one pass, so the chain has an end to
            // reach (the GIF fixture itself declares "loop forever").
            AnimatedImage::from_bytes(fx::GIF)
                .repeat(false)
                .mode(MosaicMode::HalfBlock)
                .element(cx, &theme.tokens)
                .build()
        });
        {
            let painted = |tree: &mut crate::ui::UiTree, i: usize| {
                let canvas = render(tree, size);
                // Which column is reddest tells us WHICH frame painted.
                let reddest = (0..12i32)
                    .max_by_key(|&x| {
                        let (_, fg, bg) = canvas.cell(crate::base::Point::new(x, 1)).unwrap();
                        let p = if fg.r > bg.r { fg } else { bg };
                        p.r as i32 - (p.g as i32 + p.b as i32) / 2
                    })
                    .unwrap() as u32;
                let bar = fx::bar_column(i);
                assert!(
                    reddest == bar || reddest == bar + 1,
                    "expected frame {i} (bar {bar}), reddest column {reddest}"
                );
            };
            flush_effects();
            painted(&mut tree, 0);

            for expected in 1..fx::FRAMES {
                let deadline = next_timer_deadline().expect("a playing clip arms its next frame");
                assert_eq!(
                    run_due_timers(deadline),
                    1,
                    "one timer per frame, never a poll"
                );
                drain_posted();
                flush_effects();
                painted(&mut tree, expected);
            }
        }
        // The fixture declares one pass: after the last frame the
        // chain stops, so nothing is armed and nothing wakes.
        assert!(
            next_timer_deadline().is_none(),
            "a finished clip must leave no timer armed"
        );
    }

    /// A still costs no clock at all — the zero-idle law survives the
    /// animation widget.
    #[test]
    fn a_still_arms_no_timer() {
        use crate::reactive::{flush_effects, next_timer_deadline};
        let theme = default_theme();
        let still = crate::gfx::png_encode::encode(&crate::gfx::Bitmap::new(
            4,
            4,
            crate::base::Rgba::WHITE,
        ));
        create_root(|cx| {
            let _el = AnimatedImage::from_bytes(&still).element(cx, &theme.tokens);
            flush_effects();
            assert!(
                next_timer_deadline().is_none(),
                "a one-frame source must not arm a timer"
            );
        });
    }

    /// The intrinsic size is the animation's own footprint at the
    /// mosaic density — an `Auto` row must not collapse a movie away.
    #[test]
    fn measures_as_its_natural_cell_footprint() {
        let theme = default_theme();
        create_root(|cx| {
            let mut el = AnimatedImage::from_bytes(fx::GIF)
                .mode(MosaicMode::HalfBlock)
                .element(cx, &theme.tokens);
            let measure = el.measure.take().expect("intrinsic size");
            // 12x8 pixels at 1x2 subpixels per cell = 12x4 cells.
            assert_eq!(measure(Size::new(100, 100)), Size::new(12, 4));
        });
    }
}
