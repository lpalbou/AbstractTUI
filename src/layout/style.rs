//! Layout style types: a flexbox subset chosen for terminal reality —
//! integer cells, no fractional pixels, no wrapping (v1), deterministic
//! rounding. Percent resolves against the parent's CONTENT box (padding
//! excluded), matching CSS `box-sizing: border-box` intuition.

/// Main axis of a container.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub enum Direction {
    #[default]
    Row,
    Column,
}

/// Main-axis distribution of leftover space (applies only when no child
/// grows — growth consumes all free space first).
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub enum Justify {
    #[default]
    Start,
    Center,
    End,
    SpaceBetween,
}

/// Cross-axis placement.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub enum Align {
    Start,
    Center,
    End,
    #[default]
    Stretch,
}

/// One dimension of a box.
#[derive(Copy, Clone, Debug, Default, PartialEq)]
pub enum Dimension {
    /// Content-driven (measure callback or children).
    #[default]
    Auto,
    /// Fixed terminal cells.
    Cells(i32),
    /// Fraction of the parent's content box on that axis, `0.0..=1.0`.
    /// (Stored as a fraction, not 0–100: no divide-by-100 surprises.)
    Percent(f32),
}

/// Per-side spacing. All values are cells and non-negative by convention;
/// negative values are clamped at use sites.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct Edges {
    pub left: i32,
    pub right: i32,
    pub top: i32,
    pub bottom: i32,
}

impl Edges {
    pub const ZERO: Edges = Edges {
        left: 0,
        right: 0,
        top: 0,
        bottom: 0,
    };

    pub const fn all(n: i32) -> Edges {
        Edges {
            left: n,
            right: n,
            top: n,
            bottom: n,
        }
    }

    pub const fn hv(horizontal: i32, vertical: i32) -> Edges {
        Edges {
            left: horizontal,
            right: horizontal,
            top: vertical,
            bottom: vertical,
        }
    }

    pub const fn horizontal(self) -> i32 {
        self.left + self.right
    }

    pub const fn vertical(self) -> i32 {
        self.top + self.bottom
    }
}

/// In-flow vs out-of-flow placement.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub enum Position {
    /// Participates in the parent's flex flow.
    #[default]
    Flow,
    /// Removed from flow; placed against the parent's content box using
    /// `inset` + size (CSS `position: absolute` against the padding box).
    Absolute,
}

/// What happens to children outside this node's content box. Layout
/// itself NEVER clips (solved rects stay truthful); this is metadata for
/// the ui draw/hit paths and the wheel-routing heuristic.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub enum Overflow {
    /// Children paint and hit wherever they land (CSS `visible`).
    #[default]
    Visible,
    /// Draw clips children to the content box; hit testing refuses to
    /// descend outside it.
    Clip,
    /// `Clip` + "this node scrolls": the hint wheel routing and
    /// ensure-visible helpers use to find the nearest scroll container.
    Scroll,
}

/// One grid track (column or row extent).
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum Track {
    /// Fixed terminal cells.
    Cells(i32),
    /// Fraction of the parent's content extent on the track's axis
    /// (same semantics as `Dimension::Percent`, `0.0..=1.0`).
    Percent(f32),
    /// Content-sized: the track fits the largest intrinsic size of the
    /// children placed in it (via their measure callbacks).
    Auto,
    /// Fraction of the leftover after fixed/percent/auto tracks and
    /// gaps, weighted (CSS `fr`). Rounding distributes
    /// largest-remainder, so fr tracks tile the container exactly.
    Fr(f32),
}

/// Container layout algorithm.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum Display {
    /// The flexbox subset (direction/justify/align/grow/shrink/wrap).
    #[default]
    Flex,
    /// Track grid: children auto-place row-major into the column
    /// tracks; explicit rows first, then implicit rows sized by their
    /// tallest child. Spans via `Style::col_span`/`row_span`; children
    /// fill their cell area (per-cell alignment is a later decision).
    Grid { cols: Vec<Track>, rows: Vec<Track> },
}

/// Absolute-position offsets. `None` = unconstrained on that side. When
/// both sides of an axis are set and the size is `Auto`, the size is
/// derived from the two insets.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct Inset {
    pub left: Option<i32>,
    pub right: Option<i32>,
    pub top: Option<i32>,
    pub bottom: Option<i32>,
}

/// The full per-node style. `Default` is a sane flow child: auto-sized,
/// no growth, stretch cross-axis, zero spacing.
#[derive(Clone, Debug, PartialEq)]
pub struct Style {
    /// Layout algorithm for THIS container's children.
    pub display: Display,
    pub direction: Direction,
    pub justify: Justify,
    /// Cross-axis alignment this container imposes on its children.
    pub align_items: Align,
    /// Per-child override of the parent's `align_items`.
    pub align_self: Option<Align>,
    /// Flex: wrap children onto new lines when the main axis overflows
    /// (each line distributes grow/shrink independently; lines stack
    /// along the cross axis separated by `cross_gap`).
    pub wrap: bool,
    /// Cells between adjacent flow children (not before the first or
    /// after the last — use padding for that). In grid: the column gap.
    pub gap: i32,
    /// Cells between wrapped lines (flex) or between rows (grid).
    pub cross_gap: i32,
    /// Grid child: how many column tracks this child covers (min 1).
    pub col_span: i32,
    /// Grid child: how many row tracks this child covers (min 1).
    pub row_span: i32,
    pub padding: Edges,
    pub margin: Edges,
    pub width: Dimension,
    pub height: Dimension,
    pub min_width: Option<i32>,
    pub max_width: Option<i32>,
    pub min_height: Option<i32>,
    pub max_height: Option<i32>,
    /// Share of free space taken when the container has room to spare.
    pub grow: f32,
    /// Share of overflow absorbed when children exceed the container
    /// (weighted by basis, like CSS flex-shrink).
    pub shrink: f32,
    /// Starting main-axis size before grow/shrink; `Auto` falls back to
    /// the explicit main-axis size, then to intrinsic content size.
    pub basis: Dimension,
    pub position: Position,
    pub inset: Inset,
    /// Overflow metadata consumed by the ui draw/hit paths (and the
    /// wheel-routing hint for `Scroll`): children clip to this node's
    /// CONTENT box (padding excluded) and are not hit-testable outside
    /// it. Layout itself never clips — solved rects stay truthful so
    /// scroll offsets and ensure-visible math work on real geometry.
    pub overflow: Overflow,
}

impl Default for Style {
    fn default() -> Self {
        Style {
            display: Display::Flex,
            direction: Direction::Row,
            justify: Justify::Start,
            align_items: Align::Stretch,
            align_self: None,
            wrap: false,
            gap: 0,
            cross_gap: 0,
            col_span: 1,
            row_span: 1,
            padding: Edges::ZERO,
            margin: Edges::ZERO,
            width: Dimension::Auto,
            height: Dimension::Auto,
            min_width: None,
            max_width: None,
            min_height: None,
            max_height: None,
            grow: 0.0,
            // CSS default shrink is 1: children yield before overflowing.
            shrink: 1.0,
            basis: Dimension::Auto,
            position: Position::Flow,
            inset: Inset::default(),
            overflow: Overflow::Visible,
        }
    }
}

impl Style {
    pub fn row() -> Style {
        Style {
            direction: Direction::Row,
            ..Style::default()
        }
    }

    /// Fill the parent on both axes (the "pane" default apps reach for).
    pub fn fill() -> Style {
        Style {
            width: Dimension::Percent(1.0),
            height: Dimension::Percent(1.0),
            ..Style::default()
        }
    }

    /// Fixed height in cells, full width — the "one bar/row of UI"
    /// shape (`Style::line(1)` = a status line).
    pub fn line(rows: i32) -> Style {
        Style {
            width: Dimension::Percent(1.0),
            height: Dimension::Cells(rows.max(0)),
            ..Style::default()
        }
    }

    pub fn column() -> Style {
        Style {
            direction: Direction::Column,
            ..Style::default()
        }
    }

    // Fluent helpers: terminal UIs build styles inline; a builder object
    // would only add noise.
    pub fn justify(mut self, j: Justify) -> Style {
        self.justify = j;
        self
    }

    pub fn align_items(mut self, a: Align) -> Style {
        self.align_items = a;
        self
    }

    pub fn align_self(mut self, a: Align) -> Style {
        self.align_self = Some(a);
        self
    }

    pub fn gap(mut self, g: i32) -> Style {
        self.gap = g;
        self
    }

    pub fn padding(mut self, p: Edges) -> Style {
        self.padding = p;
        self
    }

    pub fn margin(mut self, m: Edges) -> Style {
        self.margin = m;
        self
    }

    pub fn width(mut self, w: Dimension) -> Style {
        self.width = w;
        self
    }

    pub fn height(mut self, h: Dimension) -> Style {
        self.height = h;
        self
    }

    pub fn w(self, cells: i32) -> Style {
        self.width(Dimension::Cells(cells))
    }

    pub fn h(self, cells: i32) -> Style {
        self.height(Dimension::Cells(cells))
    }

    pub fn min_w(mut self, cells: i32) -> Style {
        self.min_width = Some(cells);
        self
    }

    pub fn max_w(mut self, cells: i32) -> Style {
        self.max_width = Some(cells);
        self
    }

    pub fn min_h(mut self, cells: i32) -> Style {
        self.min_height = Some(cells);
        self
    }

    pub fn max_h(mut self, cells: i32) -> Style {
        self.max_height = Some(cells);
        self
    }

    /// Share of FREE main-axis space this child takes.
    ///
    /// THE multi-pane rule (RT8-6, the first-use collapse trap): an
    /// unsized child contributes only its intrinsic content size — two
    /// side-by-side panes with no sizes do NOT split the row, one
    /// collapses. Give every pane `grow(1.0)` (or explicit sizes):
    ///
    /// ```
    /// # use abstracttui::layout::{LayoutStyle, Dimension};
    /// let pane = LayoutStyle::default().grow(1.0);      // equal split
    /// let sidebar = LayoutStyle::default().width(Dimension::Cells(24)); // fixed
    /// let main = LayoutStyle::default().grow(1.0);      // takes the rest
    /// ```
    ///
    /// This is standard flexbox, kept deliberately: zero-sized children
    /// are legitimate (spacers, collapsed panels), so the engine does
    /// not warn — the docs' every multi-pane example leads with `grow`,
    /// and `LayoutStyle::fill()`/`line()` cover the two common shapes.
    pub fn grow(mut self, g: f32) -> Style {
        self.grow = g;
        self
    }

    pub fn shrink(mut self, s: f32) -> Style {
        self.shrink = s;
        self
    }

    pub fn basis(mut self, b: Dimension) -> Style {
        self.basis = b;
        self
    }

    pub fn absolute(mut self, inset: Inset) -> Style {
        self.position = Position::Absolute;
        self.inset = inset;
        self
    }

    /// Clip children to the content box (scroll containers, marquees).
    pub fn clip(mut self) -> Style {
        self.overflow = Overflow::Clip;
        self
    }

    /// `Overflow::Scroll`: clip + advertise this node as a scroll
    /// container (wheel routing / ensure-visible hint).
    pub fn scroll(mut self) -> Style {
        self.overflow = Overflow::Scroll;
        self
    }

    /// Whether the ui draw/hit paths clip children to the content box
    /// (`Clip` and `Scroll` both clip; `Scroll` additionally hints).
    pub fn clips_children(&self) -> bool {
        matches!(self.overflow, Overflow::Clip | Overflow::Scroll)
    }

    /// Grid container over `cols`/`rows` tracks (see [`Display::Grid`]).
    pub fn grid(mut self, cols: Vec<Track>, rows: Vec<Track>) -> Style {
        self.display = Display::Grid { cols, rows };
        self
    }

    /// Flex wrap: overflowing children start a new line (`cross_gap`
    /// separates lines).
    pub fn wrap(mut self) -> Style {
        self.wrap = true;
        self
    }

    pub fn cross_gap(mut self, gap: i32) -> Style {
        self.cross_gap = gap;
        self
    }

    /// Grid child: span `n` column tracks (clamped to at least 1).
    pub fn col_span(mut self, n: i32) -> Style {
        self.col_span = n.max(1);
        self
    }

    /// Grid child: span `n` row tracks (clamped to at least 1).
    pub fn row_span(mut self, n: i32) -> Style {
        self.row_span = n.max(1);
        self
    }
}
