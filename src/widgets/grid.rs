//! Grid: the container widget over `layout::Display::Grid`.
//!
//! Thin by design — the grid ALGORITHM lives in the layout solver
//! (`layout/grid.rs`: `Cells`/`Percent`/`Auto`/`Fr` tracks, row-major
//! auto-placement, spans, gaps, cell alignment); this builder only
//! assembles the container style and children. Spans/alignment ride
//! each child's OWN style (`Style::col_span/row_span/align_self`).
//!
//! ```ignore
//! Grid::new(vec![Track::Cells(12), Track::Fr(1.0)], vec![])
//!     .gap(1)
//!     .row_gap(1)
//!     .child(text("Name:"))
//!     .child(TextInput::new().element(cx, t).build())
//!     .child(text("Notes:"))
//!     .child(notes_view)
//!     .element()
//! ```
//!
//! OWNER: REACT.

use crate::layout::{Style as LayoutStyle, Track};
use crate::ui::{Element, View};

pub struct Grid {
    cols: Vec<Track>,
    rows: Vec<Track>,
    gap: i32,
    row_gap: i32,
    layout: Option<LayoutStyle>,
    children: Vec<View>,
}

impl Grid {
    /// `cols` define the columns (empty = one full-width column); `rows`
    /// cover leading rows, later rows are implicit `Auto`.
    pub fn new(cols: Vec<Track>, rows: Vec<Track>) -> Grid {
        Grid {
            cols,
            rows,
            gap: 0,
            row_gap: 0,
            layout: None,
            children: Vec::new(),
        }
    }

    /// Column gap (cells between column tracks).
    pub fn gap(mut self, gap: i32) -> Grid {
        self.gap = gap;
        self
    }

    /// Row gap (cells between row tracks).
    pub fn row_gap(mut self, gap: i32) -> Grid {
        self.row_gap = gap;
        self
    }

    /// Outer style (size/grow/padding); the display/gap fields are
    /// overwritten by the grid configuration.
    pub fn layout(mut self, layout: LayoutStyle) -> Grid {
        self.layout = Some(layout);
        self
    }

    pub fn child(mut self, child: View) -> Grid {
        self.children.push(child);
        self
    }

    /// Canonical one-call build: the finished `View` (grids carry no
    /// tokens; the alias exists for call-site consistency with the
    /// interactive widgets).
    pub fn view(self) -> crate::ui::View {
        self.element().build()
    }

    pub fn element(self) -> Element {
        let mut style = self.layout.unwrap_or_default();
        style = style
            .grid(self.cols, self.rows)
            .gap(self.gap)
            .cross_gap(self.row_gap);
        let mut el = Element::new().style(style);
        for child in self.children {
            el = el.child(child);
        }
        el
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::base::{Rect, Size};
    use crate::layout::Dimension;
    use crate::ui::text;
    use crate::widgets::itest_util::mount_widget;

    #[test]
    fn grid_widget_places_children_in_tracks() {
        let (root, mut tree) = mount_widget(Size::new(20, 4), |_cx| {
            crate::ui::Element::new()
                .style(
                    LayoutStyle::default()
                        .width(Dimension::Percent(1.0))
                        .height(Dimension::Percent(1.0)),
                )
                .child(
                    Grid::new(vec![Track::Cells(8), Track::Fr(1.0)], vec![])
                        .gap(1)
                        .layout(
                            LayoutStyle::default()
                                .width(Dimension::Percent(1.0))
                                .height(Dimension::Percent(1.0)),
                        )
                        .child(text("label"))
                        .child(text("value"))
                        .element()
                        .build(),
                )
                .build()
        });
        tree.layout();
        // Column 0 is 8 wide; column 1 starts after the gap at x=9 and
        // takes the leftover 11.
        let snapshot = tree.accessibility_tree();
        let label = snapshot
            .entries
            .iter()
            .find(|e| e.label == "label")
            .expect("label leaf");
        let value = snapshot
            .entries
            .iter()
            .find(|e| e.label == "value")
            .expect("value leaf");
        assert_eq!(
            label.bounds,
            Rect::new(0, 0, 8, 1),
            "{}",
            tree.accessibility_tree_text()
        );
        assert_eq!(value.bounds.x, 9);
        assert_eq!(value.bounds.w, 11);
        root.dispose();
    }
}
