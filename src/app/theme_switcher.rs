//! [`ThemeSwitcher`] — the drop-in theme control (backlog
//! app-kits/0595): a one-cell icon button any app mounts in its chrome.
//!
//! Two faces, one chrome:
//!
//! - [`ThemeSwitcher::new`] — the MENU face: activation opens an owned
//!   anchored popup listing every visible theme GROUPED by mode (Dark
//!   first, then Light; within a group the curated registry order,
//!   runtime registrations trailing — the `themes_by_mode` contract).
//!   Group headers are disabled rows, so the select-family movement
//!   machinery skips them for free. Highlight movement previews the
//!   theme LIVE (the `Select::commit_on_move` semantic, which was
//!   designed for theme pickers); Enter or a click commits, Escape
//!   restores the pre-open theme, an outside press keeps the preview
//!   (the documented select-family contract: opting into live preview
//!   means moves ARE commits). Type-ahead jumps by label prefix; a
//!   repeated letter cycles its group of matches.
//! - [`ThemeSwitcher::toggle`] — the ONE-CLICK face: activation calls
//!   [`toggle_mode`], flipping dark ↔ light while keeping the user's
//!   last theme CHOICE per mode (cold start: the house palette of the
//!   target mode). One type with two constructors rather than a second
//!   widget or a boolean flag: the chrome, glyph vocabulary and a11y
//!   wiring are one implementation, and the call site reads as intent.
//!
//! ## The glyph (design note)
//!
//! The button shows the CURRENT mode: `☾` (U+263E) in dark themes,
//! `☼` (U+263C) in light ones. Reasoning, recorded:
//!
//! - A static `◐` (the classic theme mnemonic) would spend the cell on
//!   decoration; a mode-reflecting glyph makes the button double as the
//!   app's polarity indicator. The hover/focus affordances (accent
//!   ink, selection pair) keep it reading as a BUTTON, and the a11y
//!   label carries the action, so glyph-shows-state never collides
//!   with glyph-shows-action.
//! - `☾`/`☼` over `☀` (U+2600): both chosen glyphs are East-Asian-
//!   NEUTRAL (narrow in every width convention — safer than `◐`,
//!   which is Ambiguous) and neither is in Unicode emoji-data, while
//!   `☀` carries an emoji flag some terminal stacks promote to a
//!   double-width color glyph. Same mnemonic, none of the risk.
//!
//! ## Zero idle
//!
//! Closed, the switcher is one dormant element + one theme-tracking
//! `dyn_view`: no layers, no timers, no per-frame work — it re-renders
//! only when the theme signal (or its own hover/focus state) is
//! written. The popup and its subscriptions live on a per-open child
//! scope and die at dismissal.
//!
//! OWNER: DESIGN (composing SELECT's 0500 substrate).

use std::cell::RefCell;
use std::rc::Rc;

use crate::base::{Rect, Size};
use crate::layout::{Dimension, Style as LayoutStyle};
use crate::reactive::{Scope, Signal};
use crate::render::Style;
use crate::theme::{themes_by_mode, Theme, ThemeMode};
use crate::ui::{
    dyn_view, Element, EventCtx, Key, Mods, MouseButton, MouseKind, Phase, Role, UiEvent, View,
};

use super::anchored::{DismissReason, PanelAnchor, PanelWidth, Popup};
use super::overlays::Overlays;
use super::select::core::{
    first_enabled, option_rows_view, page_highlight, resolve_overlays, step_highlight,
    type_ahead_target, OptionRows, TypeAhead,
};
use super::select::SelectOption;
use super::theme::{current_theme, set_theme, toggle_mode, use_theme};
use super::viewport::use_viewport;

/// Popup rows shown at once before the list windows around the
/// highlight (28 entries at 26 built-in themes + 2 headers: a taller
/// default than Select's 8 reads better for a browse-y list).
const DEFAULT_MAX_VISIBLE: usize = 12;

/// The mode glyph — see the module docs for the design reasoning.
pub(crate) fn mode_glyph(mode: ThemeMode) -> &'static str {
    match mode {
        ThemeMode::Dark => "☾",
        ThemeMode::Light => "☼",
    }
}

/// Which face this instance wears (two constructors, one chrome).
#[derive(Copy, Clone, PartialEq, Eq)]
enum Face {
    Menu,
    Toggle,
}

/// Session state behind one open menu popup.
struct Session {
    popup: Option<Popup>,
    type_ahead: TypeAhead,
}

/// The one-line theme control: `ThemeSwitcher::new().view(cx)` in any
/// app's chrome. See the module docs for the full contract.
pub struct ThemeSwitcher {
    face: Face,
    layout: Option<LayoutStyle>,
    overlays: Option<Overlays>,
    max_visible: usize,
    on_change: Option<Box<dyn FnMut(&'static Theme)>>,
}

impl Default for ThemeSwitcher {
    fn default() -> Self {
        ThemeSwitcher::new()
    }
}

impl ThemeSwitcher {
    /// The MENU face: activation opens the grouped theme popup.
    pub fn new() -> ThemeSwitcher {
        ThemeSwitcher {
            face: Face::Menu,
            layout: None,
            overlays: None,
            max_visible: DEFAULT_MAX_VISIBLE,
            on_change: None,
        }
    }

    /// The ONE-CLICK face: activation flips dark ↔ light via
    /// [`toggle_mode`] (remembered theme per mode; house palette on a
    /// cold start). No popup ever opens.
    pub fn toggle() -> ThemeSwitcher {
        ThemeSwitcher {
            face: Face::Toggle,
            ..ThemeSwitcher::new()
        }
    }

    /// Override the 1×1-cell default layout (rarely needed; the
    /// control is designed to read at exactly one cell).
    pub fn layout(mut self, layout: LayoutStyle) -> ThemeSwitcher {
        self.layout = Some(layout);
        self
    }

    /// Popup rows shown at once (menu face; default 12). Longer lists
    /// window around the highlight; short viewports shrink further so
    /// the window always fits the solved popup rect.
    pub fn max_visible(mut self, n: usize) -> ThemeSwitcher {
        self.max_visible = n.max(1);
        self
    }

    /// Explicit overlay store (tests, exotic embeddings). Default: the
    /// app-provided reactive context.
    pub fn overlays(mut self, overlays: &Overlays) -> ThemeSwitcher {
        self.overlays = Some(overlays.clone());
        self
    }

    /// Fires when a switch STICKS: on menu Commit or outside-press
    /// dismissal with a theme different from the pre-open one (never
    /// on preview moves, never on Escape-restore, never on mechanical
    /// endings — resize, opener unmount), and on every toggle flip.
    /// Persist-my-theme hooks belong here; live restyling needs no
    /// callback at all (the theme signal already drives it).
    pub fn on_change(mut self, f: impl FnMut(&'static Theme) + 'static) -> ThemeSwitcher {
        self.on_change = Some(Box::new(f));
        self
    }

    /// Canonical one-call build: tokens resolve from the app's theme
    /// context; state lives on `cx`.
    pub fn view(self, cx: Scope) -> View {
        let face = self.face;
        let max_visible = self.max_visible;
        let overlays = resolve_overlays(cx, self.overlays);
        let viewport = use_viewport(cx);
        let theme = use_theme(cx);
        let on_change: crate::widgets::SharedCallback<&'static Theme> =
            Rc::new(RefCell::new(self.on_change));

        let focused = cx.signal(false);
        let hovered = cx.signal(false);
        // Menu-face state, created once and reused across opens (the
        // Select precedent — per-open signals on a long-lived scope
        // would accumulate).
        let display: Signal<Vec<usize>> = cx.signal(Vec::new());
        let highlight: Signal<usize> = cx.signal(0);
        let session: Rc<RefCell<Session>> = Rc::new(RefCell::new(Session {
            popup: None,
            type_ahead: TypeAhead::default(),
        }));

        let open_menu = Rc::new({
            let session = session.clone();
            let on_change = on_change.clone();
            move |anchor: Rect| {
                if session.borrow().popup.is_some() {
                    return;
                }
                let Some(overlays) = overlays.clone() else {
                    debug_assert!(
                        false,
                        "ThemeSwitcher: no Overlays available — build inside an App \
                         (context) or pass .overlays(..) explicitly"
                    );
                    return;
                };
                open_theme_menu(
                    cx,
                    &overlays,
                    viewport.get_untracked(),
                    anchor,
                    max_visible,
                    display,
                    highlight,
                    &session,
                    &on_change,
                );
            }
        });

        let activate = Rc::new({
            let on_change = on_change.clone();
            move |ctx: &mut EventCtx| match face {
                Face::Menu => open_menu(ctx.current_rect_screen()),
                Face::Toggle => {
                    let now = toggle_mode();
                    if let Some(f) = on_change.borrow_mut().as_mut() {
                        f(now);
                    }
                }
            }
        });

        let mut el = Element::new()
            .style(self.layout.unwrap_or_else(|| {
                LayoutStyle::default()
                    .width(Dimension::Cells(1))
                    .height(Dimension::Cells(1))
                    .shrink(0.0)
            }))
            .role(Role::Button)
            .access_label(match face {
                Face::Menu => "theme",
                Face::Toggle => "toggle theme mode",
            })
            .access_value(|| current_theme().label.to_string())
            .hover_signal(hovered)
            .focus_signal(focused)
            .focusable();
        {
            let activate = activate.clone();
            el = el.on(Phase::Bubble, move |ctx, ev| match ev {
                UiEvent::Key(k)
                    if (k.key == Key::Enter || k.key == Key::Char(' ')) && k.mods == Mods::NONE =>
                {
                    // Keyboard activation requires FOCUS (the Button
                    // rule: unfocused keys can route here through the
                    // root fallback and must not hijack the app).
                    if focused.get_untracked() {
                        activate(ctx);
                        ctx.stop_propagation();
                    }
                }
                UiEvent::Mouse(m) if matches!(m.kind, MouseKind::Down(MouseButton::Left)) => {
                    activate(ctx);
                    ctx.stop_propagation();
                }
                _ => {}
            });
        }
        el.child(dyn_view(
            LayoutStyle::default()
                .width(Dimension::Cells(1))
                .height(Dimension::Cells(1)),
            move || {
                // Tracked reads: the glyph follows the LIVE theme (a
                // toggle or an external switch retints and re-glyphs),
                // hover shifts ink to accent, focus wears the
                // selection pair (the widget state table).
                let t = theme.get();
                let tokens = t.tokens;
                let glyph = mode_glyph(t.mode());
                let (fg, bg) = if focused.get() {
                    (tokens.selection_fg, tokens.selection_bg)
                } else if hovered.get() {
                    (tokens.accent, crate::base::Rgba::TRANSPARENT)
                } else {
                    (tokens.text, crate::base::Rgba::TRANSPARENT)
                };
                Element::new()
                    .style(
                        LayoutStyle::default()
                            .width(Dimension::Cells(1))
                            .height(Dimension::Cells(1)),
                    )
                    .draw(move |canvas, rect| {
                        if rect.is_empty() {
                            return;
                        }
                        canvas.print_styled(rect.origin(), glyph, &Style::new().fg(fg).bg(bg));
                    })
                    .build()
            },
        ))
        .build()
    }
}

/// The grouped rows: for each mode (Dark first), a disabled header row
/// (`☾ Dark` / `☼ Light` — disabled rows render faint and are skipped
/// by movement AND type-ahead, the select-core contract that makes
/// them headers for free), then that mode's themes in the documented
/// `themes_by_mode` order. The theme active at open is marked with a
/// `●` hint — during a live preview it stays on the RESTORE target
/// (what Escape returns to), which is the honest reading.
///
/// Returns the options, the parallel display-index → theme map
/// (`None` for headers), and the highlight seed (the current theme's
/// row, else the first theme row).
fn menu_rows(
    current_id: &str,
) -> (
    Vec<SelectOption>,
    Vec<Option<&'static Theme>>,
    Option<usize>,
) {
    let mut options = Vec::new();
    let mut themes_at: Vec<Option<&'static Theme>> = Vec::new();
    let mut seed = None;
    for mode in [ThemeMode::Dark, ThemeMode::Light] {
        options.push(
            SelectOption::new(format!("{} {}", mode_glyph(mode), mode.label())).disabled(true),
        );
        themes_at.push(None);
        for t in themes_by_mode(mode) {
            let mut opt = SelectOption::keyed(t.id, t.label);
            if t.id == current_id {
                opt = opt.hint("●");
                seed = Some(options.len());
            }
            options.push(opt);
            themes_at.push(Some(t));
        }
    }
    (options, themes_at, seed)
}

/// Open the grouped menu popup: an OWNED anchored popup (modal, above
/// the whole live stack — a switcher inside a Modal/Drawer layers
/// correctly and anchors at SCREEN cells) whose content re-resolves
/// tokens per theme write, so the live preview retints the menu
/// itself, not just the app behind it.
#[allow(clippy::too_many_arguments)] // one private seam below the
                                     // builder (the select open_impl precedent).
fn open_theme_menu(
    cx: Scope,
    overlays: &Overlays,
    viewport: Size,
    anchor: Rect,
    max_visible: usize,
    display: Signal<Vec<usize>>,
    highlight: Signal<usize>,
    session: &Rc<RefCell<Session>>,
    on_change: &crate::widgets::SharedCallback<&'static Theme>,
) {
    let pre_open = current_theme();
    let (options, themes_at, seed) = menu_rows(pre_open.id);
    let options: Rc<Vec<SelectOption>> = Rc::new(options);
    let themes_at: Rc<Vec<Option<&'static Theme>>> = Rc::new(themes_at);
    let disp: Vec<usize> = (0..options.len()).collect();
    let seed = seed.or_else(|| first_enabled(&options, &disp)).unwrap_or(0);
    display.set(disp);
    highlight.set(seed);
    session.borrow_mut().type_ahead.clear();

    // Rows that fit: cap the window by the longer side of the anchor,
    // so the highlight window always lives INSIDE the solved rect even
    // on short viewports (place_owned clamps the rect; a window larger
    // than the rect would scroll the highlight out of the visible
    // rows).
    let below = (viewport.h - anchor.bottom()).max(0);
    let above = anchor.y.max(0);
    let room = below.max(above).max(1) as usize;
    let visible = options.len().min(max_visible).min(room);
    // Width: widest label + left pad (1) + gap to the hint (2) + hint
    // (1) + right pad (1) — room for the ● mark beside the longest
    // name.
    let width = options
        .iter()
        .map(|o| crate::text::width(&o.label))
        .max()
        .unwrap_or(8)
        + 5;

    // Live preview: moving the highlight APPLIES the theme (the
    // Select::commit_on_move semantic, designed for theme pickers).
    let apply_at = Rc::new({
        let themes_at = themes_at.clone();
        move |pos: usize| {
            let disp = display.get_untracked();
            let Some(Some(theme)) = disp.get(pos).map(|&ix| themes_at[ix]) else {
                return;
            };
            if current_theme().id != theme.id {
                set_theme(theme);
            }
        }
    });
    let move_to = Rc::new({
        let apply_at = apply_at.clone();
        move |pos: usize| {
            highlight.set(pos);
            apply_at(pos);
        }
    });
    // Commit = apply + close (the dismissal callback owns on_change).
    let commit_at = Rc::new({
        let session = session.clone();
        let move_to = move_to.clone();
        move |pos: usize| {
            move_to(pos);
            // Borrow released before dismiss: the dismissal callback
            // borrows the session too.
            let popup = session.borrow().popup.clone();
            if let Some(popup) = popup {
                popup.dismiss(DismissReason::Commit);
            }
        }
    });

    let build = {
        let options = options.clone();
        let session = session.clone();
        let commit_at = commit_at.clone();
        let move_to = move_to.clone();
        move |pcx: Scope, _flipped: bool| -> View {
            let key_handler = {
                let options = options.clone();
                let session = session.clone();
                let commit_at = commit_at.clone();
                let move_to = move_to.clone();
                move |ctx: &mut EventCtx, ev: &UiEvent| {
                    let UiEvent::Key(k) = ev else { return };
                    if k.mods != Mods::NONE {
                        return;
                    }
                    let disp = display.get_untracked();
                    let h = highlight.get_untracked().min(disp.len().saturating_sub(1));
                    match k.key {
                        Key::Down => move_to(step_highlight(&options, &disp, h, 1)),
                        Key::Up => move_to(step_highlight(&options, &disp, h, -1)),
                        Key::Home => {
                            if let Some(p) = first_enabled(&options, &disp) {
                                move_to(p);
                            }
                        }
                        Key::End => {
                            if let Some(p) = super::select::core::last_enabled(&options, &disp) {
                                move_to(p);
                            }
                        }
                        Key::PageDown => move_to(page_highlight(&options, &disp, h, 1, visible)),
                        Key::PageUp => move_to(page_highlight(&options, &disp, h, -1, visible)),
                        Key::Enter => commit_at(h),
                        Key::Char(c) if !c.is_control() => {
                            // Type-ahead: prefix jump / same-char
                            // cycle, on the injectable event clock
                            // (the select-core contract).
                            let now =
                                crate::ui::event_time().unwrap_or_else(std::time::Instant::now);
                            let target = {
                                let mut s = session.borrow_mut();
                                let buf = s.type_ahead.push(c, now);
                                type_ahead_target(&options, &disp, buf, h)
                            };
                            if let Some(p) = target {
                                move_to(p);
                            }
                        }
                        _ => return, // Escape and the rest: substrate's turn
                    }
                    ctx.stop_propagation();
                }
            };
            let theme = use_theme(pcx);
            let options = options.clone();
            let on_activate = Rc::new(move |pos: usize| commit_at(pos));
            Element::new()
                .style(
                    LayoutStyle::column()
                        .width(Dimension::Percent(1.0))
                        .height(Dimension::Percent(1.0)),
                )
                .role(Role::Menu)
                .access_label("themes")
                .on(Phase::Bubble, key_handler)
                .child(dyn_view(
                    LayoutStyle::column()
                        .width(Dimension::Percent(1.0))
                        .height(Dimension::Percent(1.0)),
                    move || {
                        // Tracked theme read: the live preview retints
                        // the MENU too — ground and rows re-resolve on
                        // every applied theme, so what you see while
                        // browsing is the theme itself.
                        let t = theme.get().tokens;
                        let ink = t.text;
                        let ground = t.surface_raised;
                        Element::new()
                            .style(
                                LayoutStyle::column()
                                    .width(Dimension::Percent(1.0))
                                    .height(Dimension::Percent(1.0)),
                            )
                            .draw(move |canvas, rect| {
                                canvas.fill_styled(rect, ' ', &Style::new().fg(ink).bg(ground));
                            })
                            .child(option_rows_view(
                                &t,
                                OptionRows {
                                    options: options.clone(),
                                    display,
                                    highlight,
                                    checks: None,
                                    max_visible: visible,
                                    on_activate: on_activate.clone(),
                                },
                            ))
                            .build()
                    },
                ))
                .build()
        }
    };

    let popup = Popup::open(
        overlays,
        cx,
        viewport,
        PanelAnchor { rect: anchor },
        PanelWidth::Content {
            min: width.min(viewport.w.max(1)),
            max: width,
        },
        Size::new(width, visible as i32),
        build,
    );
    let Some(popup) = popup else { return };
    popup.on_dismiss({
        let session = session.clone();
        let on_change = on_change.clone();
        move |reason| {
            session.borrow_mut().popup = None;
            match reason {
                DismissReason::Escape => {
                    // Abandon the preview: restore what was active at
                    // open (write-if-different keeps a no-move Escape
                    // free of a redundant full-tree damage).
                    if current_theme().id != pre_open.id {
                        set_theme(pre_open);
                    }
                }
                DismissReason::Commit | DismissReason::OutsidePress => {
                    // Deliberate endings keep the previewed theme;
                    // report the switch when one actually happened.
                    let now = current_theme();
                    if now.id != pre_open.id {
                        if let Some(f) = on_change.borrow_mut().as_mut() {
                            f(now);
                        }
                    }
                }
                // Mechanical endings (viewport resize, opener
                // unmount): the previewed theme stays — restoring
                // would yank the screen for a non-choice — but no
                // "user chose" event fires.
                DismissReason::AnchorGone | DismissReason::Resize => {}
            }
        }
    });
    session.borrow_mut().popup = Some(popup);
}

#[cfg(test)]
#[path = "theme_switcher_tests.rs"]
mod tests;
