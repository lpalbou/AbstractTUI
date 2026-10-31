//! Logical selection without clipboard reads, automatic sends or application actions.
//! OWNER: CONTENT. Applications receive source provenance and choose what to do.
pub use crate::render::content::{ContentContext, ContentKind};
use crate::{
    reactive::{Scope, Signal},
    render::content::SourceCell,
};
use std::{cell::RefCell, ops::Range, rc::Rc};
use unicode_segmentation::UnicodeSegmentation;

/// A logical content hit. `source` and `bytes` refer to the original source,
/// not the terminal's composed glyphs. Owner and block identify the producer.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct ContentHit {
    pub(crate) producer: usize,
    pub context: Option<Rc<ContentContext>>,
    pub owner: String,
    pub block: usize,
    pub source: Rc<str>,
    pub bytes: Range<usize>,
    pub scope: Range<usize>,
    pub kind: ContentKind,
}

/// A retained selection. The source snapshot is immutable, so replacing live
/// content cannot silently change what a previously highlighted range means.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SelectedContent {
    pub(crate) producer: usize,
    pub context: Option<Rc<ContentContext>>,
    pub owner: String,
    pub block: usize,
    pub source: Rc<str>,
    pub bytes: Range<usize>,
    pub kind: ContentKind,
    pub text: String,
    pub(crate) scope: Range<usize>,
    pub(crate) origin_bytes: Range<usize>,
    pub(crate) source_kind: ContentKind,
}
impl SelectedContent {
    /// Bounded logical context from the same paragraph, cell or code block.
    /// The booleans report omitted earlier and later text; label excerpts accordingly.
    pub fn surrounding(&self, max_chars: usize) -> (String, bool, bool) {
        let raw = self.source.get(self.scope.clone()).unwrap_or_default();
        let (text, runs) = logical(raw, self.scope.start, self.source_kind);
        let at = runs
            .iter()
            .find(|(_, source)| source.start <= self.bytes.start && self.bytes.start < source.end)
            .map(|(display, source)| display.start + self.bytes.start - source.start)
            .unwrap_or(0);
        let positions: Vec<usize> = text
            .char_indices()
            .map(|(i, _)| i)
            .chain(std::iter::once(text.len()))
            .collect();
        let center = positions
            .partition_point(|p| *p < at)
            .min(positions.len() - 1);
        let start = center.saturating_sub(max_chars / 2);
        let end = start.saturating_add(max_chars).min(positions.len() - 1);
        (
            text[positions[start]..positions[end]].into(),
            start > 0,
            end < positions.len() - 1,
        )
    }
}

impl ContentHit {
    /// Construct a hit from application-owned source. Invalid or split UTF-8 ranges are refused.
    pub fn from_source(
        owner: impl Into<String>,
        block: usize,
        source: impl Into<Rc<str>>,
        bytes: Range<usize>,
        scope: Range<usize>,
        kind: ContentKind,
    ) -> Option<Self> {
        let source = source.into();
        if bytes.start >= bytes.end
            || bytes.start < scope.start
            || bytes.end > scope.end
            || source.get(bytes.clone()).is_none()
            || source.get(scope.clone()).is_none()
        {
            return None;
        }
        Some(Self {
            producer: 0,
            context: None,
            owner: owner.into(),
            block,
            source,
            bytes,
            scope,
            kind,
        })
    }
    pub(crate) fn from_cell(
        producer: usize,
        owner: &str,
        row: &crate::render::content::RowSource,
        cell: &SourceCell,
    ) -> Self {
        Self {
            producer,
            context: cell.context.clone(),
            owner: owner.into(),
            block: row.block,
            source: row.source.clone(),
            bytes: cell.bytes.clone(),
            scope: cell.scope.clone(),
            kind: cell.kind,
        }
    }
    fn is_valid(&self) -> bool {
        self.bytes.start < self.bytes.end
            && self.bytes.start >= self.scope.start
            && self.bytes.end <= self.scope.end
            && self.source.get(self.bytes.clone()).is_some()
            && self.source.get(self.scope.clone()).is_some()
    }
    pub fn select(&self, kind: ContentKind) -> SelectedContent {
        if !self.is_valid() {
            return SelectedContent {
                producer: self.producer,
                context: None,
                owner: self.owner.clone(),
                block: self.block,
                source: self.source.clone(),
                bytes: 0..0,
                kind,
                text: String::new(),
                scope: 0..0,
                origin_bytes: 0..0,
                source_kind: self.kind,
            };
        }
        let raw = &self.source[self.scope.clone()];
        let (display, runs) = logical(raw, self.scope.start, self.kind);
        let at = runs
            .iter()
            .find(|(_, source)| source.start <= self.bytes.start && self.bytes.start < source.end)
            .map(|(display, source)| display.start + self.bytes.start - source.start)
            .unwrap_or(0);
        let display_range = match kind {
            ContentKind::Word => display
                .unicode_word_indices()
                .find(|(start, word)| *start <= at && at < start + word.len())
                .map(|(start, word)| start..start + word.len())
                .unwrap_or_else(|| {
                    display
                        .grapheme_indices(true)
                        .find(|(start, word)| *start <= at && at < start + word.len())
                        .map(|(start, word)| start..start + word.len())
                        .unwrap_or(0..0)
                }),
            ContentKind::Sentence => display
                .split_sentence_bound_indices()
                .find(|(start, sentence)| *start <= at && at < start + sentence.len())
                .map(|(start, sentence)| start..start + sentence.trim_end().len())
                .unwrap_or(0..display.len()),
            _ => 0..display.len(),
        };
        let overlapping: Vec<_> = runs
            .iter()
            .filter(|(d, _)| d.start < display_range.end && d.end > display_range.start)
            .collect();
        let bytes = if matches!(
            kind,
            ContentKind::Paragraph | ContentKind::TableCell | ContentKind::CodeBlock
        ) {
            self.scope.clone()
        } else {
            overlapping
                .first()
                .zip(overlapping.last())
                .map(|(a, b)| {
                    a.1.start + display_range.start.saturating_sub(a.0.start)
                        ..b.1.end - b.0.end.saturating_sub(display_range.end)
                })
                .unwrap_or(self.bytes.clone())
        };
        SelectedContent {
            producer: self.producer,
            context: self.context.clone(),
            owner: self.owner.clone(),
            block: self.block,
            source: self.source.clone(),
            bytes,
            kind,
            text: display[display_range].into(),
            scope: self.scope.clone(),
            origin_bytes: self.bytes.clone(),
            source_kind: self.kind,
        }
    }
}

type LogicalRange = (Range<usize>, Range<usize>);

fn logical(raw: &str, base: usize, kind: ContentKind) -> (String, Vec<LogicalRange>) {
    if !matches!(kind, ContentKind::Paragraph | ContentKind::TableCell) {
        return (raw.into(), vec![(0..raw.len(), base..base + raw.len())]);
    }
    let styles = crate::render::md::MdStyles::default();
    let (line, part) = crate::render::md::inline_source(raw, &styles, styles.base, base, true);
    (
        line.plain(),
        part.runs
            .into_iter()
            .map(|r| (r.display, r.source))
            .collect(),
    )
}

#[derive(Clone)]
pub struct ContentSelection {
    selected: Signal<Option<SelectedContent>>,
    pub(crate) anchor: Rc<RefCell<Option<ContentHit>>>,
    capture: Rc<RefCell<Option<crate::ui::PointerRelease>>>,
}
impl ContentSelection {
    pub fn new(cx: Scope) -> Self {
        let selection = Self {
            selected: cx.signal(None),
            anchor: Rc::new(RefCell::new(None)),
            capture: Rc::new(RefCell::new(None)),
        };
        let held = Rc::new(std::cell::Cell::new(false));
        let live_hold = held.clone();
        let value = selection.selected;
        cx.effect(move || {
            let on = value.get().is_some();
            if live_hold.replace(on) != on {
                super::scroll::adjust_content_hold(on);
            }
        });
        let cleanup = selection.clone();
        cx.on_cleanup(move || {
            cleanup.clear();
            if held.replace(false) {
                super::scroll::adjust_content_hold(false);
            }
        });
        selection
    }
    /// Add logical click/drag selection to a stable element. The resolver receives
    /// coordinates relative to that element's current content rect, including
    /// scrolling. It must return None for chrome, links and absent content.
    /// Only actual content hits own drags; ordinary click delivery is preserved.
    pub fn bind(
        &self,
        element: crate::ui::Element,
        resolve: impl FnMut(crate::base::Point) -> Option<ContentHit> + 'static,
    ) -> crate::ui::Element {
        use crate::{
            base::{Point, Rect},
            ui::{Key, MouseButton, MouseKind, Phase, UiEvent},
        };
        let selection = self.clone();
        let value = self.value();
        let mut element = element;
        let mut previous_draw = element.draw.take();
        let previous_drag = element.drag_zone.take();
        let resolve = Rc::new(RefCell::new(resolve));
        let probe = resolve.clone();
        let visible = Rc::new(std::cell::Cell::new(Rect::ZERO));
        let bounds = visible.clone();
        element
            .focusable()
            .drag_hit_test(move |rect, point| {
                previous_drag
                    .as_ref()
                    .is_some_and(|prior| prior.contains(rect, point))
                    || probe.borrow_mut()(Point::new(point.x - rect.x, point.y - rect.y))
                        .is_some_and(|hit| hit.is_valid())
            })
            .draw(move |canvas, rect| {
                bounds.set(rect.intersect(canvas.clip_rect()));
                if let Some(draw) = previous_draw.as_mut() {
                    draw(canvas, rect);
                }
            })
            .on(Phase::Bubble, move |ctx, event| {
                if let UiEvent::Key(key) = event {
                    if key.key == Key::Escape && value.get_untracked().is_some() {
                        selection.clear();
                        ctx.release_pointer();
                        ctx.stop_propagation();
                        ctx.request_repaint();
                    }
                    return;
                }
                let UiEvent::Mouse(mouse) = event else {
                    return;
                };
                let rect = ctx.current_rect();
                let bounds = visible.get();
                match mouse.kind {
                    MouseKind::Down(MouseButton::Left) if bounds.contains(mouse.pos) => {
                        if let Some(hit) = resolve.borrow_mut()(Point::new(
                            mouse.pos.x - rect.x,
                            mouse.pos.y - rect.y,
                        )) {
                            let kind = match hit.kind {
                                ContentKind::TableCell | ContentKind::CodeBlock => hit.kind,
                                _ => match ctx.click_count() {
                                    2 => ContentKind::Sentence,
                                    3.. => ContentKind::Paragraph,
                                    _ => ContentKind::Word,
                                },
                            };
                            *selection.anchor.borrow_mut() = Some(hit.clone());
                            selection.choose(&hit, kind);
                            if selection.anchor.borrow().is_some() {
                                if let Some(current) = ctx.current() {
                                    *selection.capture.borrow_mut() =
                                        Some(ctx.pointer_release_handle(current));
                                    ctx.capture_pointer(current);
                                }
                            }
                            ctx.request_repaint();
                        } else {
                            selection.clear();
                        }
                    }
                    MouseKind::Drag(MouseButton::Left)
                        if selection.has_anchor() && !bounds.is_empty() =>
                    {
                        let point = Point::new(
                            mouse.pos.x.clamp(bounds.x, bounds.right() - 1),
                            mouse.pos.y.clamp(bounds.y, bounds.bottom() - 1),
                        );
                        if let Some(hit) =
                            resolve.borrow_mut()(Point::new(point.x - rect.x, point.y - rect.y))
                        {
                            selection.drag(&hit);
                            if !selection.has_anchor() {
                                ctx.release_pointer();
                            }
                            ctx.request_repaint();
                        }
                        ctx.stop_propagation();
                    }
                    MouseKind::Up(MouseButton::Left) => {
                        selection.anchor.borrow_mut().take();
                        selection.capture.borrow_mut().take();
                        ctx.release_pointer();
                    }
                    _ => {}
                }
            })
    }

    pub fn value(&self) -> Signal<Option<SelectedContent>> {
        self.selected
    }
    pub fn clear(&self) {
        self.anchor.borrow_mut().take();
        if let Some(capture) = self.capture.borrow_mut().take() {
            capture.release();
        }
        if self.selected.try_get_untracked().is_some() {
            self.selected.set_if_changed(None);
        }
    }
    pub fn choose(&self, hit: &ContentHit, kind: ContentKind) {
        let selected = hit.select(kind);
        if selected.text.is_empty() {
            self.clear();
        } else {
            self.selected.set_if_changed(Some(selected));
        }
    }
    /// Expand the retained selection using its original logical scope.
    pub fn expand(&self, kind: ContentKind) {
        if let Some(s) = self.selected.get_untracked() {
            self.choose(
                &ContentHit {
                    producer: s.producer,
                    context: s.context,
                    owner: s.owner,
                    block: s.block,
                    source: s.source,
                    bytes: s.origin_bytes,
                    scope: s.scope,
                    kind: s.source_kind,
                },
                kind,
            );
        }
    }
    fn has_anchor(&self) -> bool {
        self.anchor.borrow().is_some()
    }
    pub(crate) fn drag(&self, head: &ContentHit) {
        let Some(anchor) = self.anchor.borrow().clone() else {
            return;
        };
        if !anchor.is_valid() || !head.is_valid() {
            self.clear();
            return;
        }
        if anchor.producer != head.producer
            || anchor.owner != head.owner
            || anchor.block != head.block
            || anchor.source != head.source
        {
            return;
        }
        if anchor.bytes == head.bytes {
            return;
        }
        let bytes = anchor.bytes.start.min(head.bytes.start)..anchor.bytes.end.max(head.bytes.end);
        let Some(text) = anchor.source.get(bytes.clone()) else {
            self.clear();
            return;
        };
        let text = text.to_owned();
        let same_scope = anchor.scope == head.scope && anchor.kind == head.kind;
        let scope = if same_scope {
            anchor.scope.clone()
        } else {
            bytes.clone()
        };
        let source_kind = if same_scope {
            anchor.kind
        } else {
            ContentKind::Span
        };
        let context = if same_scope {
            anchor.context.clone()
        } else {
            None
        };
        self.selected.set_if_changed(Some(SelectedContent {
            producer: anchor.producer,
            context,
            owner: anchor.owner,
            block: anchor.block,
            source: anchor.source,
            bytes,
            scope,
            origin_bytes: anchor.bytes,
            kind: ContentKind::Span,
            text,
            source_kind,
        }));
    }
}

#[cfg(test)]
#[path = "content_selection_tests.rs"]
mod tests;
