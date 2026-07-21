//! Integer cell geometry. All coordinates are `i32` so compositor math
//! (negative offsets mid-animation, off-screen layers) never underflows;
//! buffer indexing clamps at the edge of a `Rect`.

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Point {
    pub x: i32,
    pub y: i32,
}

impl Point {
    pub const ZERO: Point = Point { x: 0, y: 0 };

    pub const fn new(x: i32, y: i32) -> Self {
        Point { x, y }
    }

    pub const fn translate(self, dx: i32, dy: i32) -> Self {
        Point::new(self.x + dx, self.y + dy)
    }
}

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Size {
    pub w: i32,
    pub h: i32,
}

impl Size {
    pub const ZERO: Size = Size { w: 0, h: 0 };

    pub const fn new(w: i32, h: i32) -> Self {
        Size { w, h }
    }

    pub const fn is_empty(self) -> bool {
        self.w <= 0 || self.h <= 0
    }

    pub const fn area(self) -> i64 {
        if self.is_empty() {
            0
        } else {
            self.w as i64 * self.h as i64
        }
    }
}

/// A size in device pixels (terminal cell-pixel geometry from
/// TIOCGWINSZ / `CSI 14 t` / `CSI 16 t`). Distinct from `Size`, which is
/// always cells — mixing the two units is a classic gfx-scaling bug, so
/// the type system keeps them apart (KERNEL/GFX3D request, cycle 1).
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct PixelSize {
    pub w: u16,
    pub h: u16,
}

impl PixelSize {
    pub const fn new(w: u16, h: u16) -> Self {
        PixelSize { w, h }
    }

    pub const fn is_empty(self) -> bool {
        self.w == 0 || self.h == 0
    }
}

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl Rect {
    pub const ZERO: Rect = Rect {
        x: 0,
        y: 0,
        w: 0,
        h: 0,
    };

    pub const fn new(x: i32, y: i32, w: i32, h: i32) -> Self {
        Rect { x, y, w, h }
    }

    pub const fn from_size(size: Size) -> Self {
        Rect::new(0, 0, size.w, size.h)
    }

    pub const fn origin(self) -> Point {
        Point::new(self.x, self.y)
    }

    pub const fn size(self) -> Size {
        Size::new(self.w, self.h)
    }

    pub const fn right(self) -> i32 {
        self.x + self.w
    }

    pub const fn bottom(self) -> i32 {
        self.y + self.h
    }

    pub const fn is_empty(self) -> bool {
        self.w <= 0 || self.h <= 0
    }

    pub const fn area(self) -> i64 {
        self.size().area()
    }

    pub const fn contains(self, p: Point) -> bool {
        p.x >= self.x && p.y >= self.y && p.x < self.right() && p.y < self.bottom()
    }

    pub fn intersect(self, other: Rect) -> Rect {
        let x = self.x.max(other.x);
        let y = self.y.max(other.y);
        let r = self.right().min(other.right());
        let b = self.bottom().min(other.bottom());
        if r <= x || b <= y {
            Rect::ZERO
        } else {
            Rect::new(x, y, r - x, b - y)
        }
    }

    pub fn intersects(self, other: Rect) -> bool {
        !self.intersect(other).is_empty()
    }

    /// Smallest rect covering both. Empty rects are identity elements.
    pub fn union(self, other: Rect) -> Rect {
        if self.is_empty() {
            return other;
        }
        if other.is_empty() {
            return self;
        }
        let x = self.x.min(other.x);
        let y = self.y.min(other.y);
        let r = self.right().max(other.right());
        let b = self.bottom().max(other.bottom());
        Rect::new(x, y, r - x, b - y)
    }

    pub const fn translate(self, dx: i32, dy: i32) -> Rect {
        Rect::new(self.x + dx, self.y + dy, self.w, self.h)
    }

    /// Shrink by `n` on every side (clamping at empty).
    pub fn inset(self, n: i32) -> Rect {
        let w = (self.w - 2 * n).max(0);
        let h = (self.h - 2 * n).max(0);
        if w == 0 || h == 0 {
            Rect::new(self.x + n, self.y + n, 0, 0)
        } else {
            Rect::new(self.x + n, self.y + n, w, h)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn intersect_and_union() {
        let a = Rect::new(0, 0, 10, 10);
        let b = Rect::new(5, 5, 10, 10);
        assert_eq!(a.intersect(b), Rect::new(5, 5, 5, 5));
        assert_eq!(a.union(b), Rect::new(0, 0, 15, 15));
        assert!(a.intersect(Rect::new(20, 20, 5, 5)).is_empty());
        assert_eq!(Rect::ZERO.union(a), a);
    }

    #[test]
    fn contains_edges() {
        let r = Rect::new(1, 1, 2, 2);
        assert!(r.contains(Point::new(1, 1)));
        assert!(r.contains(Point::new(2, 2)));
        assert!(!r.contains(Point::new(3, 3)));
    }
}
