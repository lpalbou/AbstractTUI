//! reader — the mdpad-class markdown reader (app-widgets wave 3 proof:
//! 0142 tables · 0144 in-flow images · 0146 anchors/TOC · 0148 search).
//!
//! `cargo run --example reader -- path/to/doc.md` (falls back to an
//! embedded sample that exercises every feature, including a generated
//! PNG decoded lazily on first view and an HONESTLY-missing image).
//!
//! Keys: `/` search (type, Enter keeps the query, Esc clears) · `n`/`N`
//! next/previous match with a live count · `t` TOC panel (Enter jumps
//! via anchor rows) · arrows/PgUp/PgDn/Home/End + wheel scroll ·
//! Ctrl+T theme · `q` quit.
//!
//! OWNER: READER.

use abstracttui::prelude::*;
use abstracttui::theme::themes;
use abstracttui::ui::{MouseKind, Phase, UiEvent};
use abstracttui::widgets::{MarkdownView, MdSearchMatch};

/// Fixed TOC panel width (the content width must be derivable OUTSIDE
/// the draw pass — search/outline fold at the same width the view
/// draws at).
const TOC_W: i32 = 28;

fn main() -> abstracttui::base::Result<()> {
    if !abstracttui::term::have_tty() {
        println!("reader: needs an interactive terminal — skipping cleanly");
        return Ok(());
    }
    if let Ok(id) = std::env::var("ABSTRACTTUI_THEME") {
        set_theme_by_id(&id);
    }
    let doc = match std::env::args().nth(1) {
        Some(path) => std::fs::read_to_string(&path)
            .unwrap_or_else(|e| format!("# reader\n\nCould not read `{path}`: {e}\n")),
        None => sample_doc(),
    };
    let doc: &'static str = Box::leak(doc.into_boxed_str());

    let mut app = App::new(Size::new(100, 30));
    let quitter = app.quitter();
    app.mount(move |cx| {
        let theme = use_theme(cx);
        let viewport = use_viewport(cx);
        let offset = cx.signal(0i32);
        let search_open = cx.signal(false);
        let query_live = cx.signal(String::new()); // input box contents
        let query = cx.signal(String::new()); // committed search
        let current = cx.signal(0usize);
        let toc_open = cx.signal(false);
        let theme_ix = cx.signal(0usize);

        // ONE width truth: content column = viewport minus the TOC
        // panel; the find/outline folds and the drawn view all use it.
        let content_w = cx.memo(move || {
            let w = viewport.get().w;
            (w - if toc_open.get() { TOC_W } else { 0 }).max(10)
        });
        let matches = cx.memo(move || {
            let q = query.get();
            let t = theme.get().tokens;
            if q.is_empty() {
                Vec::new()
            } else {
                MarkdownView::find(doc, &t, content_w.get(), &q, true)
            }
        });
        let total_rows = cx.memo(move || {
            MarkdownView::rows(doc, &theme.get().tokens, content_w.get()) as i32
        });

        let clamp_offset = move |o: i32| {
            let body_h = (viewport.get_untracked().h - 2).max(1);
            o.clamp(0, (total_rows.get_untracked() - body_h).max(0))
        };
        let scroll_by = move |d: i32| offset.update(|o| *o = clamp_offset(*o + d));
        // Center a row in the viewport (TOC jumps, match navigation).
        let scroll_to_row = move |row: usize| {
            let h = viewport.get_untracked().h - 2;
            offset.set(clamp_offset(row as i32 - (h / 3).max(0)));
        };
        let goto_match = move |ix: usize| {
            let m = matches.get_untracked();
            if let Some(hit) = m.get(ix) {
                current.set(ix);
                scroll_to_row(hit.row);
            }
        };

        Element::new()
            .style(LayoutStyle::column())
            .shortcut(KeyChord::plain(Key::Char('q')), move |_| quitter.quit())
            .shortcut(KeyChord::new(Mods::CTRL, Key::Char('t')), move |_| {
                theme_ix.update(|i| *i = (*i + 1) % themes().len());
                set_theme_by_id(themes()[theme_ix.get_untracked()].id);
            })
            .shortcut(KeyChord::plain(Key::Char('/')), move |_| {
                query_live.set(query.get_untracked());
                search_open.set(true);
            })
            .shortcut(KeyChord::plain(Key::Char('t')), move |_| {
                toc_open.update(|v| *v = !*v);
            })
            .shortcut(KeyChord::plain(Key::Escape), move |_| {
                if search_open.get_untracked() {
                    search_open.set(false);
                    query.set(String::new());
                    current.set(0);
                }
            })
            .shortcut(KeyChord::plain(Key::Char('n')), move |_| {
                let len = matches.get_untracked().len();
                if len > 0 {
                    goto_match((current.get_untracked() + 1) % len);
                }
            })
            .shortcut(KeyChord::plain(Key::Char('N')), move |_| {
                let len = matches.get_untracked().len();
                if len > 0 {
                    goto_match((current.get_untracked() + len - 1) % len);
                }
            })
            .shortcut(KeyChord::plain(Key::Up), move |_| scroll_by(-1))
            .shortcut(KeyChord::plain(Key::Down), move |_| scroll_by(1))
            .shortcut(KeyChord::plain(Key::PageUp), move |_| scroll_by(-20))
            .shortcut(KeyChord::plain(Key::PageDown), move |_| scroll_by(20))
            .shortcut(KeyChord::plain(Key::Home), move |_| offset.set(0))
            .shortcut(KeyChord::plain(Key::End), move |_| {
                offset.set(clamp_offset(i32::MAX - 1))
            })
            .child(dyn_view(LayoutStyle::default().grow(1.0), move || {
                let t = theme.get().tokens;
                let label = theme.get().label;
                let w = content_w.get();
                let hits: Vec<MdSearchMatch> = matches.get();
                let cur = current.get().min(hits.len().saturating_sub(1));
                let off = offset.get();

                // Header: title + theme badge.
                let header = Element::new()
                    .style(LayoutStyle::row().gap(2).h(1))
                    .child(text_styled("reader", t.accent))
                    .child(text_styled(label, t.text_muted))
                    .build();

                // Content row: optional TOC list + the document.
                let mut content = Element::new().style(LayoutStyle::row().grow(1.0));
                if toc_open.get() {
                    let entries = MarkdownView::outline_rows(doc, &t, w);
                    let items: Vec<String> = entries
                        .iter()
                        .map(|e| {
                            format!(
                                "{}{}",
                                "  ".repeat((e.heading.level as usize).saturating_sub(1)),
                                e.heading.text
                            )
                        })
                        .collect();
                    let rows: Vec<usize> = entries.iter().map(|e| e.row).collect();
                    let cx2 = cx;
                    content = content.child(
                        Element::new()
                            .style(LayoutStyle::column().w(TOC_W))
                            .child(
                                List::new(if items.is_empty() {
                                    vec!["(no headings)".to_string()]
                                } else {
                                    items
                                })
                                .on_activate(move |ix| {
                                    if let Some(row) = rows.get(ix) {
                                        scroll_to_row(*row);
                                        toc_open.set(false);
                                    }
                                })
                                .element(cx2, &t)
                                .build(),
                            )
                            .build(),
                    );
                }
                let wheel = move |ctx: &mut abstracttui::ui::EventCtx, ev: &UiEvent| {
                    if let UiEvent::Mouse(m) = ev {
                        match m.kind {
                            MouseKind::ScrollUp => {
                                scroll_by(-3);
                                ctx.stop_propagation();
                            }
                            MouseKind::ScrollDown => {
                                scroll_by(3);
                                ctx.stop_propagation();
                            }
                            _ => {}
                        }
                    }
                };
                content = content.child(
                    MarkdownView::new(doc)
                        .scroll_offset(off)
                        .highlights(hits.clone(), Some(cur))
                        .layout(LayoutStyle::default().grow(1.0))
                        .element(&t)
                        .on(Phase::Bubble, wheel)
                        .build(),
                );

                // Footer: search bar when open, else the key legend +
                // live match count.
                let footer = if search_open.get() {
                    Element::new()
                        .style(LayoutStyle::row().gap(1).h(1))
                        .child(text_styled("/", t.accent))
                        .child(
                            TextInput::new()
                                .value(query_live)
                                .placeholder("find in document…")
                                .on_submit(move |q| {
                                    query.set(q.to_string());
                                    search_open.set(false);
                                    current.set(0);
                                    // Jump to the first match, if any.
                                    let t = current_theme().tokens;
                                    let m = MarkdownView::find(
                                        doc,
                                        &t,
                                        content_w.get_untracked(),
                                        q,
                                        true,
                                    );
                                    if let Some(first) = m.first() {
                                        scroll_to_row(first.row);
                                    }
                                })
                                .layout(LayoutStyle::default().grow(1.0))
                                .element(cx, &t)
                                .autofocus()
                                .build(),
                        )
                        .build()
                } else {
                    let count = if hits.is_empty() {
                        if query.get().is_empty() {
                            String::new()
                        } else {
                            format!("{:?}: no matches · ", query.get())
                        }
                    } else {
                        format!("match {}/{} · ", cur + 1, hits.len())
                    };
                    Element::new()
                        .style(LayoutStyle::row().h(1))
                        .child(text_styled(
                            format!(
                                "{count}/ search · n/N next/prev · t toc · ↑↓ scroll · ^T theme · q quit"
                            ),
                            t.text_muted,
                        ))
                        .build()
                };

                Element::new()
                    .style(LayoutStyle::column().grow(1.0))
                    .child(header)
                    .child(content.build())
                    .child(footer)
                    .build()
            }))
            .build()
    })?;
    app.run()
}

/// A one-line styled text element (the example's tiny helper).
fn text_styled(s: impl Into<String>, fg: Rgba) -> View {
    let s = s.into();
    Element::new()
        .style(LayoutStyle::default().h(1).w(s.chars().count() as i32 + 1))
        .draw(move |canvas, rect| {
            canvas.print(rect.origin(), &s, fg, Rgba::TRANSPARENT);
        })
        .build()
}

/// The embedded sample: every wave-3 feature on one page. Generates a
/// real PNG into the temp dir (lazy-decoded on first view) and names a
/// missing one (honest failure state).
fn sample_doc() -> String {
    let img = generated_png();
    format!(
        "\
# AbstractTUI reader

A tiny markdown reader proving the wave-3 enablers: **tables**, in-flow
**images**, heading **anchors**, and **search**. Press `/` and type
`table` — matches highlight live; `n` hops to the next one. Jump to
[the data section](#a-small-table) via its anchor, or open the TOC
with `t`.

## Formatting

Inline styles compose: **bold**, *italic*, `code`, ~~struck through~~,
and [links](https://example.org). Blockquotes and fences still work:

> Wrapped prose dims politely inside a quote bar.

```rust
fn hello() -> &'static str {{ \"fences stay verbatim\" }}
```

## A small table

| Feature | Item | State |
|:--------|:----:|------:|
| GFM tables | 0142 | shipped |
| In-flow images | 0144 | shipped |
| Anchors + TOC | 0146 | shipped |
| Search overlay | 0148 | shipped |

Alignment comes from the delimiter row; overwide cells truncate with
an ellipsis instead of wrapping.

## Pictures

A generated PNG, decoded lazily the first time it scrolls into view:

![a generated test card]({img})

And an honestly-missing one (the reader never fakes pixels):

![this image does not exist](/tmp/definitely-not-here.png)

## Tasks

- [x] parse the document vocabulary
- [x] stream it token by token
- [ ] read a whole book in the terminal

## The end

Search for `needle` to test wrap-aware matching: the needle hides in
this final paragraph, wrapped somewhere a plain byte offset would
miss.
"
    )
}

/// A colorful 48x20 test card written to the temp dir.
fn generated_png() -> String {
    use abstracttui::gfx::png_encode;
    let bmp = Bitmap::from_fn(48, 20, |x, y| {
        let r = (x * 5) as u8;
        let g = (y * 12) as u8;
        let b = 255 - (x * 3) as u8;
        if (x / 6 + y / 5) % 2 == 0 {
            Rgba::rgb(r, g, b)
        } else {
            Rgba::rgb(b, r, g)
        }
    });
    let path = std::env::temp_dir().join("abstracttui_reader_sample.png");
    let _ = std::fs::write(&path, png_encode::encode(&bmp));
    path.display().to_string()
}
