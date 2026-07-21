//! Tabs: a tab bar over lazily-mounted panels.
//!
//! Panels are BUILDERS (`FnMut() -> View`), invoked inside a `Dyn` that
//! reads the active-tab signal: only the active panel is mounted;
//! switching disposes it (scopes, signals, focus inside) and mounts the
//! next — lazy both ways, which is the honest default for tab content
//! that holds resources. Keep state that must survive switches in
//! signals OWNED OUTSIDE the panel builder.
//!
//! Keyboard: Left/Right cycle while the bar is focused. Mouse: click a
//! title. `on_change` fires on every switch.
//!
//! OWNER: REACT.

use std::cell::RefCell;
use std::rc::Rc;

use crate::layout::{Dimension, Style as LayoutStyle};
use crate::reactive::{Scope, Signal};
use crate::render::{Attrs, Style};
use crate::theme::TokenSet;
use crate::ui::{dyn_view, Element, EventCtx, Key, MouseButton, MouseKind, Phase, UiEvent, View};

type PanelFn = Box<dyn FnMut() -> View>;

pub struct Tabs {
    titles: Vec<String>,
    panels: Vec<PanelFn>,
    active: Option<Signal<usize>>,
    layout: Option<LayoutStyle>,
    on_change: Option<Box<dyn FnMut(usize)>>,
}

impl Tabs {
    pub fn new() -> Tabs {
        Tabs {
            titles: Vec::new(),
            panels: Vec::new(),
            active: None,
            layout: None,
            on_change: None,
        }
    }

    pub fn tab(mut self, title: impl Into<String>, panel: impl FnMut() -> View + 'static) -> Tabs {
        self.titles.push(title.into());
        self.panels.push(Box::new(panel));
        self
    }

    pub fn active(mut self, active: Signal<usize>) -> Tabs {
        self.active = Some(active);
        self
    }

    pub fn layout(mut self, layout: LayoutStyle) -> Tabs {
        self.layout = Some(layout);
        self
    }

    pub fn on_change(mut self, f: impl FnMut(usize) + 'static) -> Tabs {
        self.on_change = Some(Box::new(f));
        self
    }

    /// Canonical one-call build (cycle 8): tokens resolve from the
    /// app's THEME CONTEXT (a tracked read — building inside a
    /// `dyn_view` re-renders on theme switch) and the finished `View`
    /// comes back ready for `.child(..)`. Use `element(cx, &tokens)`
    /// when you need explicit theming or extra Element customization.
    pub fn view(self, cx: Scope) -> crate::ui::View {
        let t = crate::widgets::theme_tokens(cx);
        self.element(cx, &t).build()
    }

    pub fn element(self, cx: Scope, t: &TokenSet) -> Element {
        // Style guide §3.3: active tab ink = `text`, inactive =
        // `text_muted`, and the active marker is a `border_focus`
        // underline-strip drawn AS CELLS (SGR underline downlevels away;
        // a cell strip survives every terminal).
        let active_fg = t.text;
        let idle_fg = t.text_muted;
        let strip_fg = t.border_focus;
        let ground = t.surface;

        let titles = Rc::new(self.titles);
        let len = titles.len();
        let active = self.active.unwrap_or_else(|| cx.signal(0usize));
        let panels = Rc::new(RefCell::new(self.panels));
        let on_change: crate::widgets::SharedCallback<usize> =
            Rc::new(RefCell::new(self.on_change));
        let layout = self.layout.unwrap_or_else(LayoutStyle::column);

        let switch = {
            let on_change = on_change.clone();
            move |target: usize| {
                if len == 0 {
                    return;
                }
                let target = target.min(len - 1);
                if active.get_untracked() != target {
                    active.set(target);
                    if let Some(f) = on_change.borrow_mut().as_mut() {
                        f(target);
                    }
                }
            }
        };

        // Title spans: " title " per tab — shared by click mapping + draw.
        let spans: Rc<Vec<i32>> =
            Rc::new(titles.iter().map(|s| crate::text::width(s) + 2).collect());

        let bar_handler = {
            let spans = spans.clone();
            let switch = switch.clone();
            move |ctx: &mut EventCtx, ev: &UiEvent| {
                match ev {
                    UiEvent::Key(k) => {
                        let cur = active.get_untracked();
                        match k.key {
                            Key::Left => switch(cur.saturating_sub(1)),
                            Key::Right => switch((cur + 1).min(len.saturating_sub(1))),
                            _ => return,
                        }
                        ctx.stop_propagation();
                    }
                    UiEvent::Mouse(m) => {
                        if let MouseKind::Down(MouseButton::Left) = m.kind {
                            let mut x = ctx.current_rect().x;
                            for (i, w) in spans.iter().enumerate() {
                                if m.pos.x >= x && m.pos.x < x + w {
                                    switch(i);
                                    break;
                                }
                                x += w + 1; // 1-cell gap
                            }
                            ctx.stop_propagation();
                        }
                    }
                    _ => {}
                }
            }
        };

        let bar_titles = titles.clone();
        let bar_spans = spans;
        let access_titles = titles.clone();
        // Two rows: titles, then the cell-drawn underline strip.
        let bar = Element::new()
            .style(LayoutStyle::default().height(Dimension::Cells(2)))
            .role(crate::ui::Role::Tabs)
            .access_value(move || {
                access_titles
                    .get(active.get_untracked())
                    .cloned()
                    .unwrap_or_default()
            })
            .focusable()
            .on(Phase::Bubble, bar_handler)
            .child(dyn_view(
                LayoutStyle::default()
                    .width(Dimension::Percent(1.0))
                    .height(Dimension::Cells(2)),
                move || {
                    let current = active.get();
                    let titles = bar_titles.clone();
                    let spans = bar_spans.clone();
                    Element::new()
                        .style(
                            LayoutStyle::default()
                                .width(Dimension::Percent(1.0))
                                .height(Dimension::Cells(2)),
                        )
                        .draw(move |canvas, rect| {
                            if rect.is_empty() {
                                return;
                            }
                            canvas.fill_styled(rect, ' ', &Style::new().fg(idle_fg).bg(ground));
                            let mut x = rect.x;
                            for (i, title) in titles.iter().enumerate() {
                                let style = if i == current {
                                    Style::new().fg(active_fg).bg(ground).attrs(Attrs::BOLD)
                                } else {
                                    Style::new().fg(idle_fg).bg(ground)
                                };
                                canvas.print_styled(
                                    crate::base::Point::new(x + 1, rect.y),
                                    title,
                                    &style,
                                );
                                if i == current && rect.h > 1 {
                                    // The strip spans the title's " t " pad.
                                    let strip = "▔".repeat(spans[i] as usize);
                                    canvas.print_styled(
                                        crate::base::Point::new(x, rect.y + 1),
                                        &strip,
                                        &Style::new().fg(strip_fg).bg(ground),
                                    );
                                }
                                x += spans[i] + 1;
                            }
                        })
                        .build()
                },
            ));

        // Lazy panel mount: the Dyn calls exactly the active builder.
        let panel = dyn_view(LayoutStyle::default().grow(1.0), move || {
            let idx = active.get().min(len.saturating_sub(1));
            match panels.borrow_mut().get_mut(idx) {
                Some(build) => build(),
                None => crate::ui::text(""),
            }
        });

        Element::new().style(layout).child(bar.build()).child(panel)
    }
}

impl Default for Tabs {
    fn default() -> Self {
        Tabs::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::base::Size;
    use crate::theme::default_theme;
    use crate::ui::{text, Key};
    use crate::widgets::itest_util::{click, key, mount_widget, render};

    fn two_tabs(cx: Scope, counter: Rc<RefCell<u32>>) -> Element {
        let t = &default_theme().tokens;
        Tabs::new()
            .tab("one", || text("panel one"))
            .tab("two", move || {
                *counter.borrow_mut() += 1;
                text("panel two")
            })
            .element(cx, t)
    }

    #[test]
    fn panels_mount_lazily_and_switch_by_keyboard() {
        let size = Size::new(20, 4); // 2 bar rows (titles + strip) + panel
        let builds = Rc::new(RefCell::new(0u32));
        let b2 = builds.clone();
        let (_root, mut tree) = mount_widget(size, move |cx| two_tabs(cx, b2).build());
        let canvas = render(&mut tree, size);
        assert!(canvas.row_text(2).contains("panel one"));
        assert!(
            canvas.row_text(1).contains('▔'),
            "active tab wears the cell strip"
        );
        assert_eq!(
            canvas.cell(crate::base::Point::new(0, 1)).unwrap().1,
            default_theme().tokens.border_focus,
            "strip ink is border_focus (§3.3)"
        );
        assert_eq!(*builds.borrow(), 0, "inactive panel never built");
        key(&mut tree, Key::Tab); // focus the bar
        key(&mut tree, Key::Right);
        let canvas = render(&mut tree, size);
        assert!(canvas.row_text(2).contains("panel two"));
        assert_eq!(*builds.borrow(), 1, "built exactly on activation");
        key(&mut tree, Key::Left);
        let canvas = render(&mut tree, size);
        assert!(
            canvas.row_text(2).contains("panel one"),
            "switch back disposes + remounts"
        );
    }

    #[test]
    fn click_selects_a_tab_and_fires_on_change() {
        let t = &default_theme().tokens;
        let size = Size::new(20, 3);
        let changes: Rc<RefCell<Vec<usize>>> = Rc::new(RefCell::new(Vec::new()));
        let c2 = changes.clone();
        let (_root, mut tree) = mount_widget(size, move |cx| {
            Tabs::new()
                .tab("one", || text("p1"))
                .tab("two", || text("p2"))
                .on_change(move |i| c2.borrow_mut().push(i))
                .element(cx, t)
                .build()
        });
        // Bar: " one " = cols 0..5, gap, " two " starts at col 6.
        click(&mut tree, 7, 0);
        assert_eq!(*changes.borrow(), vec![1]);
        let canvas = render(&mut tree, size);
        assert!(canvas.row_text(2).contains("p2"));
    }
}
