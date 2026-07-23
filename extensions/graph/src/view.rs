//! [`GraphView`]: read-only rendering of a [`Layout`] (the view half
//! of backlog 0440).
//!
//! Node CARDS (themed box, title on the border, optional kind-tinted
//! accent, badge slot), edges as `abstracttui::canvas` strokes
//! (smoothed beziers through the layout waypoints, arrowheads,
//! dotted/thick styles, cycle-broken edges visibly distinct), the
//! layout's `fallback` label as an honest notice line, and pan via
//! `Scroll` (the layout bounds are the advertised content size).
//!
//! ## Interaction vocabulary (one tab stop)
//!
//! The view is ONE focus stop (the scroll viewport). While focused:
//! arrows PAN until a node is selected; **Enter** selects the first
//! node, then **arrows move the selection spatially** (nearest card
//! in that direction, deterministic tiebreaks), **Enter presses** the
//! selected node ([`GraphView::on_node_press`]), **Escape deselects**
//! (arrows pan again). Clicking a card selects it and presses.
//! Selection restyles the card border (focus ink + bold title).
//! Hovering a card shows a passive [`Tooltip`] with the node's
//! label/kind/id (opt-in, needs an `Overlays` handle — inside an
//! `App` it resolves from context).
//!
//! ## Reactivity + relayout (the honest rule)
//!
//! Layout is an ACT at view-build time: `view(cx)` runs the selected
//! pass once and renders from the cached `Layout`. Data changes
//! relayout by REBUILDING the view — wrap it in a `dyn_view` over
//! your data signal, exactly like the chart widgets. A rebuilt force
//! layout re-runs under its fixed seed (same graph = same picture;
//! there is no warm-start surface in v1 — cached-position reheat is
//! the 0430 editor's lane). A parked `GraphView` costs zero idle
//! (test-pinned).
//!
//! Colors are caller-resolved [`GraphStyle`] per the engine's widget
//! token rule; `view(cx)` derives one from the ACTIVE theme when no
//! explicit style is given.
//!
//! OWNER: CANVAS (view half of 0440; layout half is cycle 1).

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::time::Duration;

use abstracttui::app::anchored::Tooltip;
use abstracttui::app::{use_theme, Overlays};
use abstracttui::base::{Point, Rect, Rgba};
use abstracttui::layout::{Dimension, Inset, Position, Style as LayoutStyle};
use abstracttui::reactive::{Scope, Signal};
use abstracttui::text::truncate_ellipsis;
use abstracttui::ui::{
    dyn_view_scoped, Element, EventCtx, Key, MouseButton, MouseKind, Phase, Role, UiEvent, View,
};
use abstracttui::widgets::Scroll;

use crate::desc::GraphDesc;
use crate::layout::{force, grid, layered, Layout};

#[path = "view_cards.rs"]
mod cards;
#[path = "view_edges.rs"]
mod edges;
#[path = "view_style.rs"]
mod style;

use cards::CardPaint;
pub use style::{GraphAlgo, GraphStyle};

/// Node activation callback (boxed: builder-owned, fired by id).
type NodePressFn = Box<dyn FnMut(&str)>;
/// Reactive badge resolver, shared into every card's render scope.
type BadgeFn = Rc<dyn Fn(&str) -> Option<String>>;
type PressFn = Rc<RefCell<Option<NodePressFn>>>;

/// Read-only graph widget: computes (or receives) a [`Layout`] and
/// renders cards + canvas-stroke edges with selection, pan and
/// tooltips. See the module docs for the interaction vocabulary and
/// the relayout rule.
pub struct GraphView {
    desc: GraphDesc,
    algo: GraphAlgo,
    layout_override: Option<Layout>,
    style: Option<GraphStyle>,
    selected: Option<Signal<Option<String>>>,
    on_node_press: Option<NodePressFn>,
    badges: Option<BadgeFn>,
    tooltips: Option<Duration>,
    overlays: Option<Overlays>,
    offset_x: Option<Signal<i32>>,
    offset_y: Option<Signal<i32>>,
    layout_style: Option<LayoutStyle>,
}

impl GraphView {
    /// A view over `desc`, laid out by the default layered pass.
    pub fn new(desc: GraphDesc) -> GraphView {
        GraphView {
            desc,
            algo: GraphAlgo::default(),
            layout_override: None,
            style: None,
            selected: None,
            on_node_press: None,
            badges: None,
            tooltips: None,
            overlays: None,
            offset_x: None,
            offset_y: None,
            layout_style: None,
        }
    }

    /// Select the layout pass (default: layered with default options).
    pub fn algo(mut self, algo: GraphAlgo) -> GraphView {
        self.algo = algo;
        self
    }

    /// Render a PRECOMPUTED layout instead of running a pass (the
    /// 0430 hand-positioned seam; `desc` still supplies metadata —
    /// labels, kinds, edge styles — via `desc_index`/id joins).
    pub fn with_layout(mut self, layout: Layout) -> GraphView {
        self.layout_override = Some(layout);
        self
    }

    /// Explicit resolved ink set (default: derived from the active
    /// theme at build).
    pub fn style(mut self, style: GraphStyle) -> GraphView {
        self.style = Some(style);
        self
    }

    /// Controlled selection: bind the selected node id to an external
    /// signal (survives rebuilds; an internal signal is used
    /// otherwise and resets with the view).
    pub fn selected(mut self, sig: Signal<Option<String>>) -> GraphView {
        self.selected = Some(sig);
        self
    }

    /// Node activation callback: fires on card click and on Enter
    /// over the selected node. Disposal-safe — the callback may
    /// dispose the view's scope.
    pub fn on_node_press(mut self, f: impl FnMut(&str) + 'static) -> GraphView {
        self.on_node_press = Some(Box::new(f));
        self
    }

    /// Reactive badge slot: evaluated per node id inside the card's
    /// render scope, so signal reads make badges live.
    pub fn badges(mut self, f: impl Fn(&str) -> Option<String> + 'static) -> GraphView {
        self.badges = Some(Rc::new(f));
        self
    }

    /// Enable hover tooltips (node label/kind/id) with the given
    /// hover delay. Needs an overlay store: inside an `App` it
    /// resolves from context, otherwise pass [`GraphView::overlays`];
    /// without either, tooltips are skipped (documented degradation).
    pub fn tooltips(mut self, delay: Duration) -> GraphView {
        self.tooltips = Some(delay);
        self
    }

    /// Explicit overlay store for tooltips (tests, bare trees).
    pub fn overlays(mut self, overlays: &Overlays) -> GraphView {
        self.overlays = Some(overlays.clone());
        self
    }

    /// Bind the horizontal pan offset (overflow-honesty affordances:
    /// the app can derive "N cells off-screen" from offset + bounds).
    pub fn offset_x(mut self, sig: Signal<i32>) -> GraphView {
        self.offset_x = Some(sig);
        self
    }

    /// Bind the vertical pan offset.
    pub fn offset_y(mut self, sig: Signal<i32>) -> GraphView {
        self.offset_y = Some(sig);
        self
    }

    /// Outer layout style (default: a growing column).
    pub fn layout(mut self, layout: LayoutStyle) -> GraphView {
        self.layout_style = Some(layout);
        self
    }

    /// Build the widget. Layout runs HERE (an act, cached in the
    /// view); see the module docs for the relayout rule.
    pub fn view(self, cx: Scope) -> View {
        let style = Rc::new(match self.style {
            Some(s) => s,
            // Tracked theme read: rebuilt-inside-dyn_view callers
            // retint on theme switch, like core widgets.
            None => GraphStyle::from_tokens(&use_theme(cx).get().tokens),
        });
        let layout = match self.layout_override {
            Some(l) => l,
            None => match &self.algo {
                GraphAlgo::Layered(opts) => layered(&self.desc, opts),
                GraphAlgo::Force(opts) => force(&self.desc, opts),
                GraphAlgo::Grid => grid(&self.desc),
            },
        };
        let plan = Rc::new(edges::plan_edges(&self.desc, &layout));

        // Node metadata joins by id (first occurrence wins, matching
        // the layout's duplicate policy). Lookup-only map.
        let mut meta: HashMap<&str, (&str, Option<&str>)> = HashMap::new();
        for n in &self.desc.nodes {
            meta.entry(n.id.as_str())
                .or_insert((n.label.as_deref().unwrap_or(&n.id), n.kind.as_deref()));
        }

        let sel: Signal<Option<String>> = self.selected.unwrap_or_else(|| cx.signal(None));
        let ox = self.offset_x.unwrap_or_else(|| cx.signal(0i32));
        let oy = self.offset_y.unwrap_or_else(|| cx.signal(0i32));
        let press: PressFn = Rc::new(RefCell::new(self.on_node_press));
        let overlays = self
            .overlays
            .or_else(|| cx.use_context::<Overlays>())
            .filter(|_| self.tooltips.is_some());
        let tooltip_delay = self.tooltips.unwrap_or(Duration::ZERO);
        let badges = self.badges;

        let bounds = layout.bounds;
        let (bw, bh) = (bounds.w.max(1), bounds.h.max(1));

        // ---- edge layer (under the cards) --------------------------
        let edge_ink = style.edge;
        let broken_ink = style.edge_broken;
        let label_ink = style.edge_label;
        let plan_draw = plan.clone();
        let edge_layer = Element::new()
            .style(LayoutStyle {
                position: Position::Absolute,
                inset: Inset {
                    left: Some(0),
                    top: Some(0),
                    right: None,
                    bottom: None,
                },
                width: Dimension::Cells(bw),
                height: Dimension::Cells(bh),
                ..LayoutStyle::default()
            })
            .draw(move |canvas, rect| {
                edges::draw_edges(
                    canvas,
                    Point::new(rect.x, rect.y),
                    (bw, bh),
                    &plan_draw,
                    edge_ink,
                    broken_ink,
                    label_ink,
                );
            });

        // ---- node cards (dyn per card: a selection change damages
        //      exactly the two affected card regions) ----------------
        let mut content = Element::new()
            .style(
                LayoutStyle::default()
                    .width(Dimension::Cells(bw))
                    .height(Dimension::Cells(bh)),
            )
            .child(edge_layer.build());
        // Navigation facts for the key handler: (id, rect) per node.
        let nav: Rc<Vec<(String, Rect)>> = Rc::new(
            layout
                .nodes
                .iter()
                .map(|n| (n.id.clone(), n.rect))
                .collect(),
        );
        for n in &layout.nodes {
            let rect = n.rect;
            let id: Rc<str> = Rc::from(n.id.as_str());
            let (title, kind) = meta
                .get(n.id.as_str())
                .map(|(t, k)| ((*t).to_string(), k.map(str::to_string)))
                .unwrap_or_else(|| (n.id.clone(), None));
            let accent = style.accent_of(kind.as_deref());
            let tip = tooltip_text(&title, kind.as_deref(), &id);
            let abs = LayoutStyle {
                position: Position::Absolute,
                inset: Inset {
                    left: Some(rect.x),
                    top: Some(rect.y),
                    right: None,
                    bottom: None,
                },
                width: Dimension::Cells(rect.w),
                height: Dimension::Cells(rect.h),
                ..LayoutStyle::default()
            };
            let style = style.clone();
            let badges = badges.clone();
            let press = press.clone();
            let overlays = overlays.clone();
            let card = dyn_view_scoped(abs, move |gcx| {
                let selected = sel.get().as_deref() == Some(&*id);
                let paint = CardPaint {
                    title: title.clone(),
                    badge: badges.as_ref().and_then(|f| f(&id)),
                    accent,
                };
                let style = style.clone();
                let click_id = id.clone();
                let click_press = press.clone();
                let el = Element::new()
                    .style(
                        LayoutStyle::default()
                            .width(Dimension::Percent(1.0))
                            .height(Dimension::Percent(1.0)),
                    )
                    .role(Role::Button)
                    .access_label(title.clone())
                    .on(Phase::Bubble, move |ctx: &mut EventCtx, ev: &UiEvent| {
                        if let UiEvent::Mouse(m) = ev {
                            // RELEASE-INSIDE fires — the engine's
                            // Button convention. Firing on DOWN left
                            // the tree's pointer capture STUCK when
                            // `on_node_press` opened a MODAL (drawer,
                            // dialog): the release routed to the
                            // overlay, the capture never dropped, and
                            // every later click anywhere pressed this
                            // card again (found by the wave-9
                            // acceptance battery's tab click).
                            if matches!(m.kind, MouseKind::Up(MouseButton::Left))
                                && ctx.current_rect().contains(m.pos)
                            {
                                sel.set(Some(click_id.to_string()));
                                ctx.stop_propagation();
                                fire_press(&click_press, &click_id);
                            }
                        }
                    })
                    .draw(move |canvas, rect| {
                        cards::draw_card(canvas, rect, &style, &paint, selected);
                    });
                let view = el.build();
                match &overlays {
                    Some(ov) => Tooltip::attach(gcx, ov, tip.clone(), tooltip_delay, view),
                    None => view,
                }
            });
            content = content.child(card);
        }

        let scroll = Scroll::new(content.build())
            .content_size(bw, bh)
            .axes(true, true)
            .offset_x(ox)
            .offset_y(oy)
            // A fitting graph shows no bar (the column is still
            // reserved, painted as ground — engine contract).
            .scrollbar_auto_hide(true)
            .view(cx);
        // Viewport probe (cycle-3 ensure_visible fix): record the
        // scroll host's SOLVED rect at paint time into a plain cell
        // (no signal writes in draw — the RT1-2 law; key handlers
        // read the last-painted value). This excludes root padding
        // and the notice row by construction, where the old
        // widget-rect approximation drifted under padded layouts.
        let viewport_probe: Rc<Cell<(i32, i32)>> = Rc::new(Cell::new((0, 0)));
        let scroll = {
            let probe = viewport_probe.clone();
            Element::new()
                .style(LayoutStyle::default().grow(1.0).basis(Dimension::Cells(0)))
                .draw(move |_canvas, rect| probe.set((rect.w, rect.h)))
                .child(scroll)
                .build()
        };

        // ---- notice line (honesty: never scrolls away) -------------
        let notice_rows = i32::from(layout.fallback.is_some());
        let notice = layout.fallback.clone().map(|label| {
            let ink = style.notice;
            let text = format!("⚠ {label}");
            Element::new()
                .style(
                    LayoutStyle::default()
                        .height(Dimension::Cells(1))
                        .shrink(0.0),
                )
                .draw(move |canvas, rect| {
                    if rect.w <= 0 {
                        return;
                    }
                    let t = truncate_ellipsis(&text, rect.w);
                    canvas.print(rect.origin(), &t, ink, Rgba::TRANSPARENT);
                })
                .build()
        });

        // ---- root: one capture-phase key vocabulary ----------------
        let node_count = layout.nodes.len();
        let edge_count = layout.edges.len();
        let key_handler = {
            let nav = nav.clone();
            let press = press.clone();
            let viewport = viewport_probe.clone();
            move |ctx: &mut EventCtx, ev: &UiEvent| {
                let UiEvent::Key(k) = ev else { return };
                // Plain keys only: modified arrows/Enter stay available
                // to container chords above (the engine's PageHost
                // lesson — never consume modifier combinations you do
                // not implement).
                if k.mods != abstracttui::ui::Mods::NONE {
                    return;
                }
                match k.key {
                    Key::Escape => {
                        if sel.get_untracked().is_some() {
                            sel.set(None);
                            ctx.stop_propagation();
                        }
                    }
                    Key::Enter => {
                        if nav.is_empty() {
                            return;
                        }
                        ctx.stop_propagation();
                        match sel.get_untracked() {
                            Some(id) => fire_press(&press, &id),
                            None => sel.set(Some(nav[0].0.clone())),
                        }
                    }
                    Key::Up | Key::Down | Key::Left | Key::Right => {
                        let Some(cur) = sel.get_untracked() else {
                            return; // no selection: arrows PAN (Scroll)
                        };
                        // Consume even at a boundary — mid-navigation
                        // arrows must never surprise-pan.
                        ctx.stop_propagation();
                        let dir = match k.key {
                            Key::Up => (0, -1),
                            Key::Down => (0, 1),
                            Key::Left => (-1, 0),
                            _ => (1, 0),
                        };
                        if let Some(next) = spatial_next(&nav, &cur, dir) {
                            let rect = nav[next].1;
                            sel.set(Some(nav[next].0.clone()));
                            ensure_visible(
                                viewport.get(),
                                ctx.current_rect(),
                                notice_rows,
                                rect,
                                ox,
                                oy,
                            );
                        }
                    }
                    _ => {}
                }
            }
        };

        let mut root = Element::new()
            .style(
                self.layout_style
                    .unwrap_or_else(|| LayoutStyle::column().grow(1.0)),
            )
            .role(Role::Region)
            .access_label("graph")
            .access_value(move || {
                let selected = sel
                    .get_untracked()
                    .map(|id| format!(", selected {id}"))
                    .unwrap_or_default();
                format!("{node_count} nodes, {edge_count} edges{selected}")
            })
            .on(Phase::Capture, key_handler);
        if let Some(notice) = notice {
            root = root.child(notice);
        }
        root.child(scroll).build()
    }
}

/// Fire the press callback last, holding NO borrow across the call
/// (take-call-restore): the callback may dispose the view's scope
/// (the engine's 0297 disposal-safety law) — our `Rc` clone keeps the
/// slot alive — and a reentrant press during the callback sees an
/// empty slot instead of a RefCell panic.
fn fire_press(press: &PressFn, id: &str) {
    let press = press.clone(); // keep the slot alive through disposal
    let taken = press.borrow_mut().take();
    if let Some(mut f) = taken {
        f(id);
        *press.borrow_mut() = Some(f);
    }
}

fn tooltip_text(title: &str, kind: Option<&str>, id: &str) -> String {
    let mut out = title.to_string();
    if let Some(kind) = kind {
        out.push_str(&format!(" [{kind}]"));
    }
    if title != id {
        out.push_str(&format!(" ({id})"));
    }
    out
}

/// Nearest node strictly in direction `dir` from the selected one:
/// candidates must lie forward along the axis (doubled-center integer
/// math, no floats); score = forward distance + 2x perpendicular
/// offset, ties to the earliest node (input order) — deterministic.
///
/// The vocabulary is ALIGNED-FIRST: doubling the perpendicular cost
/// means Down from a diamond's apex lands on the aligned sink below,
/// not a diagonal flank — flanks are one more arrow away (test-pinned
/// in view_interact.rs). Predictable beats rank-stepping here: force
/// layouts have no ranks, and one rule must serve every pass.
fn spatial_next(nav: &[(String, Rect)], cur_id: &str, dir: (i32, i32)) -> Option<usize> {
    let cur = nav.iter().position(|(id, _)| id == cur_id)?;
    let c0 = doubled_center(nav[cur].1);
    let mut best: Option<(i64, usize)> = None;
    for (i, (_, r)) in nav.iter().enumerate() {
        if i == cur {
            continue;
        }
        let c = doubled_center(*r);
        let (vx, vy) = (i64::from(c.0 - c0.0), i64::from(c.1 - c0.1));
        let forward = vx * i64::from(dir.0) + vy * i64::from(dir.1);
        if forward <= 0 {
            continue;
        }
        let perp = if dir.0 != 0 { vy.abs() } else { vx.abs() };
        let score = forward + 2 * perp;
        if best.is_none_or(|(s, _)| score < s) {
            best = Some((score, i));
        }
    }
    best.map(|(_, i)| i)
}

fn doubled_center(r: Rect) -> (i32, i32) {
    (2 * r.x + r.w, 2 * r.y + r.h)
}

/// Clamp the pan offsets so `rect` (content cells) is visible in the
/// viewport. The primary source is the paint-time PROBE of the scroll
/// host's solved rect (padding and the notice row excluded by
/// construction — the cycle-3 padded-root fix); the widget-rect
/// approximation remains the pre-first-paint fallback. Minus one
/// column either way: the scrollbar strip is always reserved.
/// Scroll's own repair effect re-clamps against the true max, so an
/// over-ask is safe.
fn ensure_visible(
    probed: (i32, i32),
    widget: Rect,
    notice_rows: i32,
    rect: Rect,
    ox: Signal<i32>,
    oy: Signal<i32>,
) {
    let (w, h) = if probed.0 > 0 {
        probed
    } else {
        (widget.w, widget.h - notice_rows)
    };
    let vw = (w - 1).max(1);
    let vh = h.max(1);
    let x = ox.get_untracked();
    let y = oy.get_untracked();
    let nx = clamp_into(x, rect.x, rect.right(), vw);
    let ny = clamp_into(y, rect.y, rect.bottom(), vh);
    if nx != x {
        ox.set(nx);
    }
    if ny != y {
        oy.set(ny);
    }
}

/// Smallest offset change bringing [lo, hi) into a window of `span`.
fn clamp_into(offset: i32, lo: i32, hi: i32, span: i32) -> i32 {
    if lo < offset {
        lo
    } else if hi > offset + span {
        (hi - span).max(0)
    } else {
        offset
    }
}
