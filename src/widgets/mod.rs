//! Widget library built on `ui` + `layout` + `theme`: visual widgets are
//! DESIGN-owned (block, separator, badge, progress, spinner, logo);
//! behavior-heavy widgets (input, list, table, scroll) are REACT-owned and
//! land in later cycles. Every widget is themable via design tokens and
//! composable like any user component (widgets have no private engine
//! privileges — the proof the component model is enough).
//!
//! ## The token rule (RT1-9b, executable)
//!
//! Widgets consume [`crate::theme::TokenSet`] values resolved at view-build
//! time into plain `Rgba` captured by draw closures (damage contract §5).
//! NO hex literals, NO color arithmetic in widget code — derived shades
//! come from `theme::derive` at theme-build time or not at all. The
//! `no_color_arithmetic_in_widgets` test below enforces both rules over
//! this directory's sources.
//!
//! File ownership header convention: each widget file names its owner.

pub mod audio_scope;
pub mod badge;
pub mod block;
pub mod button;
pub mod chart;
pub mod checkbox;
pub mod code;
pub mod feed;
pub mod grid;
pub mod image;
pub mod input;
pub mod list;
pub mod logo;
pub mod markdown;
pub mod meter;
pub mod progress;
pub mod radio;
pub mod richtext;
pub mod scroll;
pub mod separator;
pub mod spinner;
pub mod table;
pub mod tabs;
pub mod textarea;
pub mod viewport3d;

/// Re-exported beside [`Image`] (RT8-4): the pixel type
/// `Image::from_bitmap` wants (wrapped in `Arc`) without hunting
/// through `gfx`.
pub use crate::gfx::Bitmap;
pub use badge::{Badge, Tone};
pub use block::{Block, BorderKind, TitleAlign};
pub use button::{Button, ButtonStyle};
pub use chart::{BarChart, LineChart, Sparkline};
pub use checkbox::Checkbox;
pub use code::{code_token_color, CodeView};
pub use feed::{CustomBlock, Feed, FeedBlock, FeedItem, FeedState};
pub use grid::Grid;
pub use image::{Image, ImageAlign, ImageFit};
pub use input::TextInput;
pub use list::List;
pub use logo::Logo;
pub use markdown::MarkdownView;
pub use progress::Progress;
pub use radio::RadioGroup;
pub use richtext::RichTextView;
pub use scroll::Scroll;
pub use separator::Separator;
pub use spinner::{Spinner, SpinnerKind};
pub use table::{ColWidth, Column, Table};
pub use tabs::Tabs;
pub use textarea::{SubmitPolicy, TextArea, TextAreaState};
pub use viewport3d::Viewport3D;
// Appended (0140 diff slice): the diff twin of `code_token_color`.
pub use code::diff_token_color;
// Appended (0104): the Signal<Vec<T>> -> keyed-feed diffing bridge.
pub use feed::SyncSpec;
// Appended (0190): the chart history ring + its reactive handle.
pub use chart::{TimeSeries, TimeSeriesState};
// Appended (wave 3, media-av/0620): live level rendering.
pub use audio_scope::AudioScope;
pub use meter::Meter;
// Appended (wave 3, reader enablers 0146/0148): outline entries with
// typeset rows + search matches for the markdown reader surface.
pub use markdown::{MdSearchMatch, OutlineEntry};

/// Shared callback-slot shape for interactive widgets: the builder's
/// `Option<Box<dyn FnMut(..)>>` moved behind `Rc<RefCell<..>>` so
/// multiple handlers (key + mouse) can fire the same app callback.
/// One alias, one clippy-visible name (RT4-2 hygiene).
pub(crate) type SharedCallback<Arg> = std::rc::Rc<std::cell::RefCell<Option<Box<dyn FnMut(Arg)>>>>;

/// Resolve the ACTIVE theme's tokens for `Widget::view(cx)` — reads the
/// theme signal the app provided as reactive CONTEXT (`App::mount`), so
/// no upward `app` import exists here (layer map holds). The read is
/// TRACKED: a widget built inside a `dyn_view` re-renders on theme
/// switch for free. Outside an app (bare `UiTree` tests), the default
/// theme applies.
pub(crate) fn theme_tokens(cx: crate::reactive::Scope) -> crate::theme::TokenSet {
    match cx.use_context::<crate::reactive::Signal<&'static crate::theme::Theme>>() {
        Some(sig) => sig.get().tokens,
        None => crate::theme::default_theme().tokens,
    }
}

/// Interactive-widget test scaffolding (REACT): real tree + real dispatch.
#[cfg(test)]
pub(crate) mod itest_util;

#[cfg(test)]
pub(crate) mod test_util {
    use crate::base::{Rect, Size};
    use crate::ui::{BufferCanvas, Element};

    /// Render a widget's own draw closure into a fresh canvas. Widgets
    /// build Elements whose paint lives in the draw closure; running it
    /// directly keeps widget tests independent of layout/reactivity.
    pub fn draw_into(mut el: Element, size: Size) -> BufferCanvas {
        let mut canvas = BufferCanvas::new(size);
        let rect = Rect::from_size(size);
        let mut draw = el.draw.take().expect("widget must have a draw closure");
        draw(&mut canvas, rect);
        canvas
    }

    /// Row text helper mirroring BufferCanvas::row_text for readability.
    pub fn row(canvas: &BufferCanvas, y: i32) -> String {
        canvas.row_text(y)
    }
}

#[cfg(test)]
mod lint_tests {
    /// RT1-9b made executable: widget sources carry no hex color literals
    /// and no raw color arithmetic. Tokens in, resolved Rgba captured,
    /// nothing invented. (`include_str!` pins the exact shipped sources —
    /// every `pub mod` file must be listed, which the membership check
    /// below enforces against the module declarations above; private
    /// SHIPPED siblings — `feed_typeset.rs`, split for file-size
    /// discipline — join the list by hand, they are widget source too.)
    const SOURCES: [(&str, &str); 34] = [
        ("mod.rs", include_str!("mod.rs")),
        ("audio_scope.rs", include_str!("audio_scope.rs")),
        ("meter.rs", include_str!("meter.rs")),
        ("badge.rs", include_str!("badge.rs")),
        ("block.rs", include_str!("block.rs")),
        ("button.rs", include_str!("button.rs")),
        ("chart.rs", include_str!("chart.rs")),
        ("chart_time.rs", include_str!("chart_time.rs")),
        ("checkbox.rs", include_str!("checkbox.rs")),
        ("code.rs", include_str!("code.rs")),
        ("feed.rs", include_str!("feed.rs")),
        ("feed_item.rs", include_str!("feed_item.rs")),
        ("feed_sync.rs", include_str!("feed_sync.rs")),
        ("feed_typeset.rs", include_str!("feed_typeset.rs")),
        ("grid.rs", include_str!("grid.rs")),
        ("image.rs", include_str!("image.rs")),
        ("input.rs", include_str!("input.rs")),
        ("list.rs", include_str!("list.rs")),
        ("logo.rs", include_str!("logo.rs")),
        ("markdown.rs", include_str!("markdown.rs")),
        // Markdown's private shipped siblings (reader wave 3): widget
        // source too, listed by hand like feed_typeset.rs.
        ("markdown_doc.rs", include_str!("markdown_doc.rs")),
        ("markdown_image.rs", include_str!("markdown_image.rs")),
        ("markdown_search.rs", include_str!("markdown_search.rs")),
        ("progress.rs", include_str!("progress.rs")),
        ("radio.rs", include_str!("radio.rs")),
        ("richtext.rs", include_str!("richtext.rs")),
        ("scroll.rs", include_str!("scroll.rs")),
        ("separator.rs", include_str!("separator.rs")),
        ("spinner.rs", include_str!("spinner.rs")),
        ("table.rs", include_str!("table.rs")),
        ("tabs.rs", include_str!("tabs.rs")),
        ("textarea.rs", include_str!("textarea.rs")),
        ("textarea_model.rs", include_str!("textarea_model.rs")),
        ("viewport3d.rs", include_str!("viewport3d.rs")),
    ];

    #[test]
    fn no_color_literals_or_arithmetic_in_widgets() {
        // The forbidden spellings, assembled so this file's own source
        // (included above) never matches its own patterns.
        let hex = "from_".to_string() + "hex(";
        let rgb = "Rgba::".to_string() + "rgb(";
        let new = "Rgba::".to_string() + "new(";
        let lerp = ".le".to_string() + "rp(";
        let over = ".ov".to_string() + "er(";
        let lum = ".lumin".to_string() + "ance(";
        for (name, src) in SOURCES {
            for needle in [&hex, &rgb, &new, &lerp, &over, &lum] {
                assert!(
                    !src.contains(needle.as_str()),
                    "widget source {name} contains forbidden color spelling {needle:?} \
                     — widgets consume tokens only (RT1-9b)"
                );
            }
        }
    }

    #[test]
    fn every_widget_module_is_linted() {
        // Membership, not arithmetic: every `pub mod x;` above must have
        // "x.rs" in SOURCES (a count proxy broke the moment a private
        // shipped sibling joined the list).
        let this = include_str!("mod.rs");
        for line in this.lines() {
            let Some(name) = line
                .trim()
                .strip_prefix("pub mod ")
                .and_then(|rest| rest.strip_suffix(';'))
            else {
                continue;
            };
            let file = format!("{name}.rs");
            assert!(
                SOURCES.iter().any(|(listed, _)| *listed == file),
                "widget module {file} was added without joining the lint list"
            );
        }
    }
}
