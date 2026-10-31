//! Logical Feed provenance and selection binding. OWNER: CONTENT.
use super::{item, typeset, EntryKind, FeedState};
use crate::{
    reactive::Scope,
    ui::Element,
    widgets::{ContentSelection, SelectedContent},
};
use std::rc::Rc;
impl FeedState {
    /// Resolve a content-relative column and row to exact source provenance.
    /// Mapping is built lazily on first interaction, never by reading screen glyphs.
    pub fn content_at(&self, column: i32, row: i32) -> Option<crate::widgets::ContentHit> {
        if column < 0 || column >= self.inner.borrow().width {
            return None;
        }
        let (key, within) = self.item_at_row(row)?;
        let mut inner = self.inner.borrow_mut();
        let index = *inner.index.get(&key)?;
        inner.map_content(index);
        let entry = &inner.entries[index];
        let mut offset = within;
        for segment in &entry.segments {
            if offset < segment.height() {
                if let typeset::Segment::Rows(rows) = segment {
                    let row = rows.get(offset as usize)?;
                    // Hyperlinks retain their normal click behavior.
                    let mut x = row.indent;
                    for span in &row.line.spans {
                        if span.link.is_some() && column >= x && column < x + span.width() {
                            return None;
                        }
                        x += span.width();
                    }
                    let source = row.source.as_ref()?;
                    let cell = source
                        .cells
                        .iter()
                        .find(|cell| cell.columns.contains(&column))?;
                    return Some(crate::widgets::ContentHit::from_cell(
                        Rc::as_ptr(&self.inner) as usize,
                        &key,
                        source,
                        cell,
                    ));
                }
                return None;
            }
            offset -= segment.height();
        }
        None
    }

    pub(crate) fn source_is_current(&self, selected: &SelectedContent) -> bool {
        use item::ItemBlock;
        let inner = self.inner.borrow();
        let Some(index) = inner.index.get(&selected.owner) else {
            return false;
        };
        match &inner.entries[*index].kind {
            EntryKind::Stream(stream) => stream.raw == selected.source.as_ref(),
            EntryKind::Static(blocks) => match blocks.get(selected.block) {
                Some(ItemBlock::Markdown { src, .. }) => src == selected.source.as_ref(),
                Some(ItemBlock::Code { source, .. }) => source == selected.source.as_ref(),
                Some(ItemBlock::Text { text, .. }) => text == selected.source.as_ref(),
                Some(ItemBlock::Rich { text, .. }) => {
                    text.lines
                        .iter()
                        .map(|l| l.plain())
                        .collect::<Vec<_>>()
                        .join("\n")
                        == selected.source.as_ref()
                }
                _ => false,
            },
        }
    }
}
pub(super) fn bind(
    el: Element,
    cx: Scope,
    state: FeedState,
    selection: ContentSelection,
) -> Element {
    let value = selection.value();
    let watch = selection.clone();
    let valid = state.clone();
    let version = state.version;
    cx.effect(move || {
        version.get();
        if let Some(selected) = value.get_untracked() {
            if selected.producer == Rc::as_ptr(&valid.inner) as usize
                && !valid.source_is_current(&selected)
            {
                watch.clear();
            }
        }
    });
    let cleanup = selection.clone();
    let owner = Rc::as_ptr(&state.inner) as usize;
    cx.on_cleanup(move || {
        if let Some(Some(selected)) = cleanup.value().try_get_untracked() {
            if selected.producer == owner {
                cleanup.clear();
            }
        }
    });
    selection.bind(el, move |point| state.content_at(point.x, point.y))
}
