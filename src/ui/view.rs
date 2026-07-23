//! Declarative view descriptions. A `View` is a lightweight blueprint —
//! building one performs no reactive work; `UiTree::mount` turns it into
//! live instances. Components are plain functions:
//! `fn(Scope, Props) -> View`. There is no diffing: dynamic regions are
//! explicit `Dyn` nodes whose closure re-runs (and remounts ONLY its own
//! subtree) when the signals it reads change — that is the fine-grained
//! re-render unit this engine bets on.

use crate::base::Rect;
use crate::layout::Style;
use crate::reactive::Signal;

use super::canvas::StyledCanvas;
use super::event::{EventCtx, KeyChord, Phase, UiEvent};

/// Draw callback for an element: paints INSIDE `rect` (absolute coords,
/// already solved by layout). Runs on every frame the region is damaged.
/// Receives the styled canvas so widgets can emphasize (bold, reverse,
/// links); plain `Canvas` methods remain available through the supertrait.
pub type DrawFn = Box<dyn FnMut(&mut dyn StyledCanvas, Rect)>;

/// Event handler attached to an element for a given phase.
pub type HandlerFn = Box<dyn FnMut(&mut EventCtx, &UiEvent)>;

/// Shortcut action. Boxed FnMut: fires on chord resolution, outside the
/// normal phase routing.
pub type ShortcutFn = Box<dyn FnMut(&mut EventCtx)>;

pub(crate) struct Handler {
    pub phase: Phase,
    pub run: HandlerFn,
}

pub(crate) struct Shortcut {
    pub chord: KeyChord,
    /// Human description for keymap-help surfaces; unlabeled shortcuts
    /// stay functional but render as bare chords.
    pub label: Option<String>,
    pub run: ShortcutFn,
}

/// The view blueprint tree.
pub enum ViewNode {
    Element(Element),
    /// Text leaf. Static content; reactive text = `Dyn` around a text
    /// node (keeps ONE re-render mechanism instead of two).
    Text(TextView),
    /// Reactive region: `build` re-runs when its tracked signals change;
    /// the previous subtree is disposed (scopes, instances, layout) and
    /// the new one mounted in place, its region marked damaged.
    Dyn(DynView),
}

pub struct View(pub(crate) ViewNode);

impl View {
    /// Visit every layout style in this blueprint tree, mutably —
    /// element, text-leaf and dyn-region styles alike (the dyn's OWN
    /// style; styles produced by its build closure at runtime are out of
    /// reach by construction, as are `style_signal` closures). In-crate
    /// policy hook: `app::popups::Modal` floors declared fixed sizes
    /// with it (backlog 0240). Blueprint-time only — mounted instances
    /// are not affected.
    pub(crate) fn for_each_style_mut(&mut self, f: &mut impl FnMut(&mut Style)) {
        match &mut self.0 {
            ViewNode::Element(el) => {
                f(&mut el.style);
                for child in &mut el.children {
                    child.for_each_style_mut(f);
                }
            }
            ViewNode::Text(t) => f(&mut t.style),
            ViewNode::Dyn(d) => f(&mut d.style),
        }
    }
}

pub struct TextView {
    pub(crate) content: String,
    pub(crate) style: Style,
}

pub struct DynView {
    pub(crate) style: Style,
    /// Receives the GENERATION scope: everything created on it (signals,
    /// effects, nested state) is disposed before the next rebuild.
    pub(crate) build: Box<dyn FnMut(crate::reactive::Scope) -> View>,
}

/// A styled box: layout style, optional draw closure, handlers,
/// shortcuts, focusability, children.
pub struct Element {
    pub(crate) style: Style,
    /// Reactive layout style: re-evaluated (tracked) when its signals
    /// change, applied to the live layout node WITHOUT remounting — the
    /// primitive scroll offsets and animated panes ride on.
    pub(crate) style_fn: Option<Box<dyn FnMut() -> Style>>,
    /// Intrinsic-size callback (the RT8-6 collapse-class fix for draw
    /// widgets): consulted when an `Auto` axis needs a content size,
    /// exactly like a text leaf's measurement. See [`Element::measure`].
    pub(crate) measure: Option<crate::layout::MeasureFn>,
    pub(crate) draw: Option<DrawFn>,
    pub(crate) handlers: Vec<Handler>,
    pub(crate) shortcuts: Vec<Shortcut>,
    pub(crate) focusable: bool,
    /// Tab/Shift-Tab cycle stays inside this subtree while focus is in it
    /// (modal dialogs). Nested traps: the deepest one wins.
    pub(crate) focus_trap: bool,
    /// Tab-entering this subtree restores its last-focused descendant.
    pub(crate) focus_memory: bool,
    /// Focus this node when it mounts (initial-focus policy: autofocus
    /// wins; apps without one call `UiTree::focus_first`).
    pub(crate) autofocus: bool,
    /// Draw even when culled (measurement probes) — see
    /// [`Element::probe_when_culled`].
    pub(crate) probe_when_culled: bool,
    /// PROTECTED minimum padding (per-side max), applied at mount and
    /// on every style_signal update — chrome insets (a Block's border
    /// room) that a later user `.style(..)` must NOT clobber (RT8-7).
    pub(crate) padding_floor: Option<crate::layout::Edges>,
    /// Semantic annotation (role/label/value) — the accessibility model.
    /// `pub(crate)` field of a `pub(super)` type: fine for the crate,
    /// but keep the FIELD crate-private too.
    pub(super) access: super::access::AccessProps,
    pub(crate) children: Vec<View>,
}

impl Element {
    pub fn new() -> Element {
        Element {
            style: Style::default(),
            style_fn: None,
            measure: None,
            draw: None,
            handlers: Vec::new(),
            shortcuts: Vec::new(),
            focusable: false,
            focus_trap: false,
            focus_memory: false,
            autofocus: false,
            probe_when_culled: false,
            padding_floor: None,
            access: super::access::AccessProps::default(),
            children: Vec::new(),
        }
    }

    pub fn style(mut self, style: Style) -> Element {
        self.style = style;
        self
    }

    /// Reactive layout style: `f` runs tracked; when its signals change
    /// the style is re-applied to the mounted layout node and the tree
    /// re-solves — content is NOT remounted, so descendant widget state
    /// (focus, cursors, signals) survives. This is how scroll containers
    /// move their content.
    pub fn style_signal(mut self, f: impl FnMut() -> Style + 'static) -> Element {
        self.style_fn = Some(Box::new(f));
        self
    }

    /// Paint callback over the element's solved rect.
    pub fn draw(mut self, f: impl FnMut(&mut dyn StyledCanvas, Rect) + 'static) -> Element {
        self.draw = Some(Box::new(f));
        self
    }

    /// Run this node's OWN draw closure even when its rect lies fully
    /// outside the paint clip (crate-internal, first-app/0281).
    /// Culling is a paint optimization; a measurement-readback probe
    /// riding a draw closure (Scroll's content-extent probe) is not
    /// paint — starving it freezes the measurement exactly when the
    /// content scrolls fully out (the shrunken-feed void state).
    /// Children still cull individually, so the subtree stays cheap;
    /// the canvas remains damage-clipped, so a rogue paint could not
    /// escape anyway.
    pub(crate) fn probe_when_culled(mut self) -> Element {
        self.probe_when_culled = true;
        self
    }

    /// Intrinsic content size for `Auto`-sized axes: given the available
    /// box, report the desired size — the same contract text leaves
    /// fulfil through `text::measure`. Must be pure (called repeatedly
    /// during solving).
    ///
    /// This is how a DRAW widget (an image, a chart canvas) declares a
    /// real content size instead of the default zero: a draw-only
    /// element with no measure contributes NOTHING to an `Auto` parent,
    /// so an unsized flex row of such widgets collapses (the RT8-6
    /// multi-pane trap — see `LayoutStyle::grow`). When present, the
    /// measure wins over children aggregation (solver contract).
    pub fn measure(
        mut self,
        f: impl Fn(crate::base::Size) -> crate::base::Size + 'static,
    ) -> Element {
        self.measure = Some(Box::new(f));
        self
    }

    /// Attach a handler for `phase`. Most listeners want `Phase::Bubble`
    /// (fires for self and descendants); `Capture` intercepts on the way
    /// down (modals, drag overlays).
    pub fn on(mut self, phase: Phase, f: impl FnMut(&mut EventCtx, &UiEvent) + 'static) -> Element {
        self.handlers.push(Handler {
            phase,
            run: Box::new(f),
        });
        self
    }

    /// Bubble-phase sugar.
    pub fn on_event(self, f: impl FnMut(&mut EventCtx, &UiEvent) + 'static) -> Element {
        self.on(Phase::Bubble, f)
    }

    /// Register a shortcut on this subtree. Resolution walks root -> focus
    /// path; the DEEPEST matching binding wins (local overrides global).
    pub fn shortcut(mut self, chord: KeyChord, f: impl FnMut(&mut EventCtx) + 'static) -> Element {
        self.shortcuts.push(Shortcut {
            chord,
            label: None,
            run: Box::new(f),
        });
        self
    }

    /// [`Element::shortcut`] with a human description — keymap-help
    /// overlays render it next to the chord.
    pub fn shortcut_labeled(
        mut self,
        chord: KeyChord,
        label: impl Into<String>,
        f: impl FnMut(&mut EventCtx) + 'static,
    ) -> Element {
        self.shortcuts.push(Shortcut {
            chord,
            label: Some(label.into()),
            run: Box::new(f),
        });
        self
    }

    /// Participate in Tab traversal; receives FocusIn/FocusOut and key
    /// events while focused.
    pub fn focusable(mut self) -> Element {
        self.focusable = true;
        self
    }

    /// Trap Tab/Shift-Tab cycling inside this subtree while focus is in
    /// it (modal dialogs). Programmatic `set_focus` can still move focus
    /// out — the trap constrains TRAVERSAL, not authority.
    pub fn focus_trap(mut self) -> Element {
        self.focus_trap = true;
        self
    }

    /// Remember the last focused descendant: Tab re-entering this
    /// subtree from outside restores it instead of the first focusable.
    pub fn focus_memory(mut self) -> Element {
        self.focus_memory = true;
        self
    }

    /// Focus this node when it mounts (dialog default fields, the
    /// palette input). The LAST autofocus mounted wins.
    pub fn autofocus(mut self) -> Element {
        self.autofocus = true;
        self.focusable = true;
        self
    }

    /// Protected MINIMUM padding: applied per-side (`max`) over whatever
    /// layout style is in effect — at mount AND after every
    /// `style_signal` update. Chrome widgets (a `Block`'s border inset)
    /// use it so a caller's later `.style(grow)` sizes the panel WITHOUT
    /// dropping content onto the frame (RT8-7). User padding beyond the
    /// floor wins; below it, the floor holds.
    pub fn padding_floor(mut self, floor: crate::layout::Edges) -> Element {
        self.padding_floor = Some(floor);
        self
    }

    /// Semantic role (accessibility model). Annotated nodes appear in
    /// `UiTree::accessibility_tree()`.
    pub fn role(mut self, role: super::access::Role) -> Element {
        self.access.role = Some(role);
        self
    }

    /// Accessible label ("what is this control"): a button's text, an
    /// input's field name. Also admits the node into the snapshot.
    pub fn access_label(mut self, label: impl Into<String>) -> Element {
        self.access.label = Some(label.into());
        self
    }

    /// Accessible VALUE ("what does it currently hold/state"): sampled
    /// untracked at snapshot time — read your signals with
    /// `get_untracked` inside.
    pub fn access_value(mut self, value: impl Fn() -> String + 'static) -> Element {
        self.access.value = Some(std::rc::Rc::new(value));
        self
    }

    /// THE hover recipe: bind a signal to this element's hover state.
    /// MouseEnter/MouseLeave are delivered per-node (an ancestor is
    /// hovered whenever the pointer is anywhere in its subtree), so a
    /// button wrapping a text child still reads hovered over the text.
    /// Read the signal in a `Dyn` for hover-reactive visuals.
    pub fn hover_signal(self, hovered: Signal<bool>) -> Element {
        self.on(Phase::Bubble, move |_ctx, ev| match ev {
            UiEvent::MouseEnter => hovered.set(true),
            UiEvent::MouseLeave => hovered.set(false),
            _ => {}
        })
    }

    /// Same recipe for focus: FocusIn/FocusOut keep the signal truthful.
    pub fn focus_signal(self, focused: Signal<bool>) -> Element {
        self.on(Phase::Bubble, move |_ctx, ev| match ev {
            UiEvent::FocusIn => focused.set(true),
            UiEvent::FocusOut => focused.set(false),
            _ => {}
        })
    }

    pub fn child(mut self, view: View) -> Element {
        self.children.push(view);
        self
    }

    pub fn children(mut self, views: impl IntoIterator<Item = View>) -> Element {
        self.children.extend(views);
        self
    }

    pub fn build(self) -> View {
        View(ViewNode::Element(self))
    }
}

impl Default for Element {
    fn default() -> Self {
        Element::new()
    }
}

impl From<Element> for View {
    fn from(e: Element) -> View {
        e.build()
    }
}

/// Static text leaf.
pub fn text(content: impl Into<String>) -> View {
    View(ViewNode::Text(TextView {
        content: content.into(),
        style: Style::default(),
    }))
}

pub fn styled_text(content: impl Into<String>, style: Style) -> View {
    View(ViewNode::Text(TextView {
        content: content.into(),
        style,
    }))
}

/// Reactive region. The closure is TRACKED: signals read inside subscribe
/// the region; a change re-runs it, replacing only this subtree.
///
/// ```
/// use abstracttui::base::Size;
/// use abstracttui::layout::LayoutStyle;
/// use abstracttui::reactive::{create_root, flush_effects};
/// use abstracttui::ui::{dyn_view, text, BufferCanvas, Element, UiTree};
///
/// let mut tree = UiTree::new(Size::new(12, 1));
/// let mut probe = None;
/// let (root, ()) = create_root(|cx| {
///     let count = cx.signal(0);
///     probe = Some(count);
///     let view = Element::new()
///         .child(dyn_view(LayoutStyle::line(1), move || {
///             text(format!("n = {}", count.get()))
///         }))
///         .build();
///     tree.mount(cx, view);
/// });
/// let mut canvas = BufferCanvas::new(Size::new(12, 1));
/// tree.draw(&mut canvas);
/// assert_eq!(canvas.row_text(0).trim_end(), "n = 0");
/// probe.unwrap().set(5); // ONLY the Dyn region re-renders
/// flush_effects();
/// tree.draw(&mut canvas);
/// assert_eq!(canvas.row_text(0).trim_end(), "n = 5");
/// root.dispose();
/// ```
///
/// State discipline: signals CREATED inside this closure land on the
/// MOUNTING scope and accumulate one per rebuild (they die only when the
/// whole Dyn unmounts). Durable state belongs OUTSIDE the closure;
/// per-generation state belongs in [`dyn_view_scoped`].
pub fn dyn_view(style: Style, mut build: impl FnMut() -> View + 'static) -> View {
    View(ViewNode::Dyn(DynView {
        style,
        build: Box::new(move |_| build()),
    }))
}

/// [`dyn_view`] with a per-GENERATION scope: the closure receives a
/// child scope that is disposed right before the next rebuild, so
/// signals/effects created inside (widget internals: hover, pressed,
/// caret) die with their generation instead of accumulating. This is
/// the theme-rebuild recipe's missing half — durable state (selection,
/// value) stays OUTSIDE on the mount scope, throwaway internals ride
/// the generation scope (DESIGN cycle-3 request 1b).
pub fn dyn_view_scoped(
    style: Style,
    build: impl FnMut(crate::reactive::Scope) -> View + 'static,
) -> View {
    View(ViewNode::Dyn(DynView {
        style,
        build: Box::new(build),
    }))
}
