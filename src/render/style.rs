//! Style: a composable patch applied to cells.
//!
//! A `Style` is deliberately *not* "the appearance of a cell" — it is a
//! delta. `fg`/`bg` at `None` keep whatever the target cell already has
//! (text drawn over a filled panel keeps the panel's background); attribute
//! changes are add/remove sets, so BOLD can be layered onto existing
//! content without knowing what is already there. Widgets wanting absolute
//! styling start from [`Style::absolute`], which removes everything first.
//!
//! The hyperlink id is the one non-patch field: text either carries a link
//! or it does not. Inheriting a stale link from the cell underneath a fresh
//! label would be a correctness hazard, so `apply` always overwrites it.

use crate::base::Rgba;

use super::cell::{Attrs, Cell};

/// A composable styling PATCH (not an absolute appearance): unset fields
/// keep whatever the target cell already has.
///
/// ```
/// use abstracttui::base::Rgba;
/// use abstracttui::render::{Attrs, Style};
///
/// // The common one-liner: ink + emphasis.
/// let err = Style::new().fg(Rgba::rgb(255, 80, 80)).bold();
/// assert_eq!(err.add, Attrs::BOLD);
/// assert_eq!(err.bg, None); // bg unset: keeps the panel underneath
///
/// // Patches compose; the later opinion wins where both have one.
/// let quoted = err.merge(Style::new().dim().fg(Rgba::rgb(150, 150, 150)));
/// assert_eq!(quoted.fg, Some(Rgba::rgb(150, 150, 150)));
/// assert_eq!(quoted.add, Attrs::BOLD | Attrs::DIM);
/// ```
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct Style {
    /// Ink color; `None` keeps the cell's.
    pub fg: Option<Rgba>,
    /// Ground color; `None` keeps the cell's.
    pub bg: Option<Rgba>,
    /// Underline color (SGR 58); `Some(TRANSPARENT)` explicitly resets to
    /// the default (follow fg), `None` keeps the cell's.
    pub ul: Option<Rgba>,
    /// Attributes this patch turns ON.
    pub add: Attrs,
    /// Attributes this patch turns OFF (applied before `add`).
    pub remove: Attrs,
    /// Hyperlink id in the target surface's link table (0 = no link).
    pub link: u16,
}

impl Style {
    /// The identity patch (changes nothing except clearing a stale link).
    pub const EMPTY: Style = Style {
        fg: None,
        bg: None,
        ul: None,
        add: Attrs::NONE,
        remove: Attrs::NONE,
        link: 0,
    };

    /// Alias of [`Style::EMPTY`] as the builder entry point.
    pub fn new() -> Style {
        Style::EMPTY
    }

    /// A patch that fully replaces attributes: everything not explicitly
    /// added is removed. Colors still default to "keep" until set.
    pub fn absolute() -> Style {
        Style {
            remove: Attrs::ALL,
            ..Style::EMPTY
        }
    }

    /// Sets the ink color.
    pub fn fg(mut self, color: Rgba) -> Style {
        self.fg = Some(color);
        self
    }

    /// Sets the ground color.
    pub fn bg(mut self, color: Rgba) -> Style {
        self.bg = Some(color);
        self
    }

    /// Sets the underline color. Usually paired with
    /// `.attrs(Attrs::UNDERLINE)` — the color alone draws nothing.
    pub fn underline_color(mut self, color: Rgba) -> Style {
        self.ul = Some(color);
        self
    }

    /// Adds attributes (and cancels any earlier removal of them).
    pub fn attrs(mut self, attrs: Attrs) -> Style {
        self.add = self.add.with(attrs);
        self.remove = self.remove.without(attrs);
        self
    }

    // Terse attribute builders [C8]: the six attributes users reach for
    // constantly get one-word spellings, so the common line reads
    // `Style::new().fg(ink).bold()`. Rarer attributes (BLINK, HIDDEN,
    // UNDERCURL) stay behind `.attrs(...)` — an API teaches by what it
    // makes short.

    /// Adds BOLD (shorthand for `.attrs(Attrs::BOLD)`).
    pub fn bold(self) -> Style {
        self.attrs(Attrs::BOLD)
    }

    /// Adds DIM.
    pub fn dim(self) -> Style {
        self.attrs(Attrs::DIM)
    }

    /// Adds ITALIC.
    pub fn italic(self) -> Style {
        self.attrs(Attrs::ITALIC)
    }

    /// Adds UNDERLINE (pair with [`Style::underline_color`] for colored
    /// underlines).
    pub fn underline(self) -> Style {
        self.attrs(Attrs::UNDERLINE)
    }

    /// Adds STRIKE.
    pub fn strike(self) -> Style {
        self.attrs(Attrs::STRIKE)
    }

    /// Adds REVERSE (swap fg/bg at the terminal).
    pub fn reverse(self) -> Style {
        self.attrs(Attrs::REVERSE)
    }

    /// Removes attributes (and cancels any earlier addition of them).
    pub fn without_attrs(mut self, attrs: Attrs) -> Style {
        self.remove = self.remove.with(attrs);
        self.add = self.add.without(attrs);
        self
    }

    /// Attaches a hyperlink id (from [`Surface::register_link`](super::surface::Surface::register_link)).
    pub fn link(mut self, id: u16) -> Style {
        self.link = id;
        self
    }

    /// Sequential composition: the result of applying `self` then `over` in
    /// one step. `over`'s opinions win where both have one.
    pub fn merge(self, over: Style) -> Style {
        Style {
            fg: over.fg.or(self.fg),
            bg: over.bg.or(self.bg),
            ul: over.ul.or(self.ul),
            // A later remove cancels an earlier add and vice versa.
            add: self.add.without(over.remove).with(over.add),
            remove: self.remove.without(over.add).with(over.remove),
            link: if over.link != 0 { over.link } else { self.link },
        }
    }

    /// Applies the patch to a cell's paint (glyph untouched).
    pub fn apply(self, cell: &Cell) -> Cell {
        Cell {
            glyph: cell.glyph,
            fg: self.fg.unwrap_or(cell.fg),
            bg: self.bg.unwrap_or(cell.bg),
            ul: self.ul.unwrap_or(cell.ul),
            attrs: cell.attrs.without(self.remove).with(self.add),
            link: self.link,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::cell::Glyph;

    #[test]
    fn patch_keeps_unset_fields() {
        let base = Cell::new(Glyph::SPACE)
            .with_fg(Rgba::rgb(1, 1, 1))
            .with_bg(Rgba::rgb(9, 9, 9))
            .with_attrs(Attrs::DIM);
        let styled = Style::new()
            .fg(Rgba::rgb(2, 2, 2))
            .attrs(Attrs::BOLD)
            .apply(&base);
        assert_eq!(styled.fg, Rgba::rgb(2, 2, 2));
        assert_eq!(styled.bg, Rgba::rgb(9, 9, 9), "unset bg keeps the cell's");
        assert_eq!(styled.attrs, Attrs::DIM | Attrs::BOLD);
    }

    #[test]
    fn absolute_replaces_attrs() {
        let base = Cell::new(Glyph::SPACE).with_attrs(Attrs::DIM | Attrs::ITALIC);
        let styled = Style::absolute().attrs(Attrs::BOLD).apply(&base);
        assert_eq!(styled.attrs, Attrs::BOLD);
    }

    #[test]
    fn merge_is_sequential_application() {
        let a = Style::new().fg(Rgba::rgb(1, 0, 0)).attrs(Attrs::BOLD);
        let b = Style::new()
            .fg(Rgba::rgb(0, 1, 0))
            .without_attrs(Attrs::BOLD)
            .attrs(Attrs::DIM);
        let m = a.merge(b);
        let cell = Cell::EMPTY;
        // Applying the merge equals applying a then b.
        assert_eq!(m.apply(&cell), b.apply(&a.apply(&cell)));
        assert_eq!(m.fg, Some(Rgba::rgb(0, 1, 0)));
        assert!(m.remove.contains(Attrs::BOLD));
    }

    #[test]
    fn terse_builders_equal_attrs_form() {
        assert_eq!(Style::new().bold(), Style::new().attrs(Attrs::BOLD));
        assert_eq!(
            Style::new()
                .fg(Rgba::rgb(1, 2, 3))
                .bold()
                .italic()
                .underline(),
            Style::new()
                .fg(Rgba::rgb(1, 2, 3))
                .attrs(Attrs::BOLD | Attrs::ITALIC | Attrs::UNDERLINE)
        );
        assert_eq!(
            Style::new().dim().strike().reverse().add,
            Attrs::DIM | Attrs::STRIKE | Attrs::REVERSE
        );
        // Shorthand cancels a prior remove exactly like .attrs does.
        assert_eq!(
            Style::absolute().bold().remove,
            Attrs::ALL.without(Attrs::BOLD)
        );
    }

    #[test]
    fn link_always_overwrites() {
        let linked = Cell::new(Glyph::SPACE).with_link(4);
        assert_eq!(
            Style::new().apply(&linked).link,
            0,
            "no-link style clears stale link"
        );
        assert_eq!(Style::new().link(2).apply(&linked).link, 2);
    }
}
