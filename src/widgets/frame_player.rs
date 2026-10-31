//! Present live raster frames from a decoder, capture device or procedural source.
//!
//! The producer supplies row-major straight-alpha RGBA frames as [`Bitmap`]s.
//! `FramePlayer` owns no generator, interpreter, timer, network or audio source.
//! It paints only when the frame/visibility/theme changes. Producers should stop
//! their own frame clock while the host is hidden or reduced motion is enabled.
//!
//! ```no_run
//! use std::sync::Arc;
//! use abstracttui::prelude::*;
//!
//! # fn build(cx: Scope) -> View {
//! let frame = cx.signal(Arc::new(Bitmap::new(32, 32, Rgba::TRANSPARENT)));
//! FramePlayer::new(frame).view(cx)
//! # }
//! ```
//!
//! OWNER: GFX3D.

use std::sync::Arc;

use crate::gfx::{Bitmap, MosaicMode};
use crate::layout::{Dimension, Style as LayoutStyle};
use crate::reactive::{Scope, Signal};
use crate::theme::TokenSet;
use crate::ui::{dyn_view, Element, View};
use crate::widgets::{Image, ImageFit};

/// A dependency-free reader of live RGBA frames through the existing image
/// widget's mosaic presentation, fit and error behavior.
pub struct FramePlayer {
    frames: Signal<Arc<Bitmap>>,
    active: Option<Signal<bool>>,
    fit: ImageFit,
    mode: Option<MosaicMode>,
    layout: LayoutStyle,
}

impl FramePlayer {
    pub fn new(frames: Signal<Arc<Bitmap>>) -> Self {
        Self {
            frames,
            active: None,
            fit: ImageFit::Contain,
            mode: None,
            layout: LayoutStyle::default(),
        }
    }

    /// Hidden players neither paint nor subscribe to producer frame changes.
    pub fn active(mut self, active: Signal<bool>) -> Self {
        self.active = Some(active);
        self
    }

    pub fn fit(mut self, fit: ImageFit) -> Self {
        self.fit = fit;
        self
    }

    pub fn mode(mut self, mode: MosaicMode) -> Self {
        self.mode = Some(mode);
        self
    }

    pub fn layout(mut self, layout: LayoutStyle) -> Self {
        self.layout = layout;
        self
    }

    pub fn view(self, cx: Scope) -> View {
        self.build(cx, None).build()
    }

    pub fn element(self, cx: Scope, tokens: &TokenSet) -> Element {
        self.build(cx, Some(*tokens))
    }

    fn build(self, cx: Scope, tokens: Option<TokenSet>) -> Element {
        let fill = LayoutStyle::default()
            .width(Dimension::Percent(1.0))
            .height(Dimension::Percent(1.0));
        let frames = self.frames;
        let active = self.active;
        let fit = self.fit;
        let mode = self.mode;
        let image_layout = fill.clone();
        let content = dyn_view(fill, move || {
            if !active.map(|value| value.get()).unwrap_or(true) {
                return Element::new().build();
            }
            let mut image = Image::from_bitmap(frames.get())
                .fit(fit)
                .layout(image_layout.clone());
            if let Some(mode) = mode {
                image = image.mode(mode);
            }
            image
                .element(&tokens.unwrap_or_else(|| super::theme_tokens(cx)))
                .build()
        });
        Element::new().style(self.layout).child(content)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::base::{Point, Rgba, Size};
    use crate::reactive::next_timer_deadline;
    use crate::widgets::itest_util::{mount_widget, settle};
    use std::cell::RefCell;
    use std::rc::Rc;

    #[test]
    fn live_rgba_frames_update_and_hide_without_an_internal_clock() {
        let controls = Rc::new(RefCell::new(None));
        let out = controls.clone();
        let size = Size::new(8, 4);
        let (root, mut tree) = mount_widget(size, move |cx| {
            let frame = cx.signal(Arc::new(Bitmap::new(4, 4, Rgba::WHITE)));
            let active = cx.signal(true);
            *out.borrow_mut() = Some((frame, active));
            FramePlayer::new(frame)
                .active(active)
                .fit(ImageFit::Fill)
                .mode(MosaicMode::HalfBlock)
                .view(cx)
        });
        let (frames, active) = controls.borrow().unwrap();
        let first = settle(&mut tree, size);
        assert!(first.cell(Point::new(2, 1)).unwrap().2.r > 200);
        assert!(next_timer_deadline().is_none());

        frames.set(Arc::new(Bitmap::new(4, 4, Rgba::BLACK)));
        let next = settle(&mut tree, size);
        assert!(next.cell(Point::new(2, 1)).unwrap().2.r < 20);

        active.set(false);
        let hidden = settle(&mut tree, size);
        frames.set(Arc::new(Bitmap::new(4, 4, Rgba::WHITE)));
        let still_hidden = settle(&mut tree, size);
        assert_eq!(
            hidden.cell(Point::new(2, 1)),
            still_hidden.cell(Point::new(2, 1))
        );
        active.set(true);
        let shown = settle(&mut tree, size);
        assert!(shown.cell(Point::new(2, 1)).unwrap().2.r > 200);
        root.dispose();
        assert!(next_timer_deadline().is_none());
    }
}
