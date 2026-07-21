//! Convenience re-exports: `use abstracttui::prelude::*;` is all an
//! application needs for the common path (REACT's cycle-8 proposal,
//! approved + executed cycle 9 for RT8-1).
//!
//! Curation rules: app-code surface only — engine/test types
//! (`UiTree`, `Driver`, `create_root`, canvases) stay behind explicit
//! imports; `render::Style` is deliberately ABSENT (two `Style` types
//! one glob apart was the top newcomer trap — layout's is here as
//! [`LayoutStyle`], paint styles use the full `render::Style` path
//! inside draw closures).

pub use crate::base::{Point, Rect, Rgba, Size};

pub use crate::reactive::{batch, untrack, Memo, Scope, Signal};

pub use crate::layout::{
    Align, Dimension, Direction, Display, Edges, Inset, Justify, LayoutStyle, Overflow, Track,
};

pub use crate::ui::{
    dyn_view, dyn_view_scoped, text, Callback, Element, Key, KeyChord, Mods, Role, View,
};

pub use crate::render::Surface;

pub use crate::theme::{Theme, TokenId, TokenSet};

// Interactive widgets (RT8-1: every real app's first import line) +
// the display set DESIGN ships.
pub use crate::widgets::{
    Badge, Bitmap, Block, BorderKind, Button, Checkbox, Grid, Image, List, Logo, Progress,
    RadioGroup, Scroll, Separator, Spinner, Table, Tabs, TextInput, Viewport3D,
};

pub use crate::app::{
    current_theme, set_theme, set_theme_by_id, use_startup_notices, use_theme, use_viewport, App,
    KeymapHelp, Modal, Quitter, RunConfig, Toast,
};

pub use crate::anim::{Easing, Timeline, Transition, Tween};
