//! [`MermaidFence`] — the ```mermaid fence, rendered inside a
//! document.
//!
//! `MarkdownView` typesets every block in one draw closure, so a
//! diagram cannot be a child widget without splitting the document
//! (and losing its scroll surface, its outline, and its search index).
//! The core's [`FenceBlock`] seam solves that the way in-flow images
//! already do: the block reserves rows in the document and paints
//! cells into them.
//!
//! Rendering a diagram needs a reactive scope and a tree, so this
//! renders ONCE per `(source, width)` into a cell buffer and serves
//! rows out of it. A reader scrolling through a document therefore
//! pays for each diagram once, not once per frame.

use std::cell::RefCell;
use std::rc::Rc;

use abstracttui::app::current_theme;
use abstracttui::base::{Point, Rgba, Size};
use abstracttui::reactive::{create_root, flush_effects, untrack};
use abstracttui::ui::{BufferCanvas, StyledCanvas, UiTree};
use abstracttui::widgets::FenceBlock;

use crate::view::MermaidView;

/// How many rows a diagram may take in a document before it is
/// clipped. A runaway diagram must not push the prose off the page.
const MAX_ROWS: i32 = 60;

/// Claims ```mermaid fences in a [`MarkdownView`](abstracttui::widgets::MarkdownView).
///
/// ```no_run
/// use abstracttui::widgets::MarkdownView;
/// use abstracttui_mermaid::MermaidFence;
/// use std::rc::Rc;
///
/// let doc = "# Design\n\n```mermaid\ngraph LR\n  A --> B\n```\n";
/// let view = MarkdownView::new(doc).fence_block(Rc::new(MermaidFence::new()));
/// # let _ = view;
/// ```
#[derive(Default)]
pub struct MermaidFence {
    /// The info strings claimed (default: `mermaid`).
    langs: Vec<String>,
    /// One rendered diagram, keyed by the source and width that made
    /// it. Documents show the same diagram on every frame.
    cache: RefCell<Option<Rendered>>,
}

/// One painted row: the cells a document row will carry.
type CellRow = Vec<(char, Rgba, Rgba)>;

struct Rendered {
    source: String,
    width: i32,
    /// The theme the cells were painted under: a theme switch must
    /// re-render, and the cache key is what makes that automatic.
    theme: &'static str,
    rows: Vec<CellRow>,
}

impl MermaidFence {
    /// Claim ```mermaid.
    pub fn new() -> MermaidFence {
        MermaidFence {
            langs: vec!["mermaid".to_string()],
            cache: RefCell::new(None),
        }
    }

    /// Claim a different info string as well (```diagram, say).
    pub fn also_claiming(mut self, lang: impl Into<String>) -> MermaidFence {
        self.langs.push(lang.into());
        self
    }

    fn claims(&self, lang: &str) -> bool {
        let lang = lang.trim().to_ascii_lowercase();
        // ```mermaid title="x" — the info string's FIRST word decides.
        let first = lang.split_whitespace().next().unwrap_or("");
        self.langs.iter().any(|l| l == first)
    }

    /// Render (or reuse) the diagram for this source, width and theme.
    fn render(&self, source: &str, width: i32) -> usize {
        let theme = current_theme().id;
        let mut slot = self.cache.borrow_mut();
        if let Some(r) = slot.as_ref() {
            if r.source == source && r.width == width && r.theme == theme {
                return r.rows.len();
            }
        }
        let rows = paint(source, width);
        let len = rows.len();
        *slot = Some(Rendered {
            source: source.to_string(),
            width,
            theme,
            rows,
        });
        len
    }
}

/// Draw the diagram into a buffer and keep the cells that carry ink.
///
/// UNTRACKED on purpose: this runs inside the document's draw closure,
/// and the engine rightly refuses tracked reads there (a draw closure
/// cannot subscribe to anything). The diagram's own dependency on the
/// theme is handled by the cache KEY instead — a theme switch changes
/// the key and re-renders — so nothing goes stale and nothing
/// subscribes from a draw.
fn paint(source: &str, width: i32) -> Vec<CellRow> {
    untrack(|| paint_inner(source, width))
}

fn paint_inner(source: &str, width: i32) -> Vec<CellRow> {
    let width = width.max(4);
    let size = Size::new(width, MAX_ROWS);
    let src = source.to_string();
    let out: Rc<RefCell<Vec<CellRow>>> = Rc::new(RefCell::new(Vec::new()));
    let sink = Rc::clone(&out);

    // The build + draw runs inside an EFFECT so it has an observer of
    // its own. That is what separates a self-contained computation
    // (this one, whose subscriptions die with the root disposed two
    // lines later) from a widget subscribing to a signal it can never
    // be woken for — the case the draw-phase guard exists to catch.
    let root = create_root(|cx| {
        cx.effect(move || {
            let mut tree = UiTree::new(size);
            let view = MermaidView::new(src.clone()).view(cx);
            tree.mount(cx, view);
            let mut canvas = BufferCanvas::new(size);
            tree.draw(&mut canvas);
            let mut rows: Vec<CellRow> = Vec::new();
            for y in 0..size.h {
                rows.push(
                    (0..size.w)
                        .map(|x| {
                            canvas.cell(Point::new(x, y)).unwrap_or((
                                ' ',
                                Rgba::TRANSPARENT,
                                Rgba::TRANSPARENT,
                            ))
                        })
                        .collect(),
                );
            }
            *sink.borrow_mut() = rows;
        });
    })
    .0;
    flush_effects();
    drop(root);

    let mut rows = out.borrow().clone();
    // Trim the empty tail: a diagram takes the rows it needs, not the
    // buffer's height.
    while rows
        .last()
        .is_some_and(|r| r.iter().all(|(c, _, _)| *c == ' ' || *c == '\0'))
    {
        rows.pop();
    }
    rows
}

impl FenceBlock for MermaidFence {
    fn measure(&self, lang: &str, source: &str, width: i32) -> Option<i32> {
        if !self.claims(lang) {
            return None;
        }
        Some((self.render(source, width) as i32).clamp(1, MAX_ROWS))
    }

    fn draw_row(
        &self,
        lang: &str,
        source: &str,
        row: i32,
        at: Point,
        width: i32,
        _tokens: &abstracttui::theme::TokenSet,
        canvas: &mut dyn StyledCanvas,
    ) {
        // The diagram carries its own token-derived styles from the
        // live theme (it renders through `MermaidView`), so the
        // document's tokens are already reflected in the cells.
        if !self.claims(lang) {
            return;
        }
        self.render(source, width);
        let slot = self.cache.borrow();
        let Some(rendered) = slot.as_ref() else {
            return;
        };
        let Some(cells) = rendered.rows.get(row.max(0) as usize) else {
            return;
        };
        for (i, (ch, fg, bg)) in cells.iter().enumerate().take(width.max(0) as usize) {
            if *ch == ' ' && bg.a == 0 {
                continue;
            }
            canvas.put(Point::new(at.x + i as i32, at.y), *ch, *fg, *bg);
        }
    }
}
