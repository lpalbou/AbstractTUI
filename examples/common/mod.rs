//! Shared helpers for the example apps (not an example itself — cargo
//! only auto-targets .rs files directly under examples/).
//!
//! Each example compiles its own copy and uses a subset; unused-item
//! warnings would differ per example, hence the module-level allow.
//!
//! OWNER: DESIGN.
#![allow(dead_code)]

use abstracttui::prelude::*;
use abstracttui::ui::Canvas;

/// Minimum comfortable size for the demo apps.
pub const MIN_SIZE: Size = Size { w: 40, h: 10 };

/// Draw-time small-terminal guard: when `rect` is under `min`, paints a
/// centered, theme-correct notice and returns `true` (the caller skips its
/// real content). Draw-time — so it follows live resizes with zero extra
/// plumbing, and examples degrade gracefully instead of clipping into
/// garbage or panicking.
pub fn too_small(canvas: &mut dyn Canvas, rect: Rect, min: Size, t: &TokenSet) -> bool {
    if rect.w >= min.w && rect.h >= min.h {
        return false;
    }
    canvas.fill(rect, ' ', t.text, t.bg);
    let msg = format!("terminal too small — need {}x{}", min.w, min.h);
    let hint = format!("(currently {}x{})", rect.w, rect.h);
    let cy = rect.y + (rect.h / 2 - 1).max(0);
    print_centered(canvas, rect, cy, &msg, t.warn);
    print_centered(canvas, rect, cy + 1, &hint, t.text_muted);
    true
}

/// Print a line centered in `rect` at row `y`, clipped by the canvas.
pub fn print_centered(canvas: &mut dyn Canvas, rect: Rect, y: i32, s: &str, fg: Rgba) {
    let w = s.chars().count() as i32;
    let x = rect.x + (rect.w - w).max(0) / 2;
    canvas.print(Point::new(x, y), s, fg, Rgba::TRANSPARENT);
}

/// Full-viewport absolute layout — the standard slot for the too-small
/// overlay (painted last, no-op above the minimum size).
pub fn overlay_layout() -> LayoutStyle {
    LayoutStyle::default().absolute(abstracttui::layout::Inset {
        left: Some(0),
        right: Some(0),
        top: Some(0),
        bottom: Some(0),
    })
}

/// A one-line key legend rendered in the footer style every example
/// shares: muted text, accent-colored key names, bottom row of `rect`.
pub fn key_legend(canvas: &mut dyn Canvas, rect: Rect, t: &TokenSet, entries: &[(&str, &str)]) {
    let y = rect.bottom() - 1;
    let mut x = rect.x + 1;
    for (key, what) in entries {
        if x >= rect.right() {
            break;
        }
        x += canvas.print(Point::new(x, y), key, t.accent, Rgba::TRANSPARENT);
        x += canvas.print(Point::new(x, y), " ", t.text_muted, Rgba::TRANSPARENT);
        x += canvas.print(Point::new(x, y), what, t.text_muted, Rgba::TRANSPARENT);
        x += canvas.print(Point::new(x, y), "  ", t.text_muted, Rgba::TRANSPARENT);
    }
}

/// Paint a dense, agent-transcript-like page into `rect`: prose with
/// highlighted inline code, fenced code blocks, and cycle summary rows with
/// right-aligned stats. A deliberately BUSY ground for overlay demos (modal
/// backdrops, floating avatars): the kind of page where a see-through panel
/// or an erased rectangle is easy to spot.
pub fn paint_busy_page(canvas: &mut dyn abstracttui::ui::StyledCanvas, rect: Rect, t: &TokenSet) {
    use abstracttui::render::Style;
    use abstracttui::theme::derive::mix;
    canvas.fill(rect, ' ', t.text, t.bg);
    let code_bg = mix(t.bg, t.info, 0.28);
    let block_bg = mix(t.bg, t.info, 0.16);
    let prose: [&[(&str, bool)]; 4] = [
        &[
            ("The last error is mechanical: ", false),
            ("run_shell", true),
            (" returns ", false),
            ("String", true),
            (", but the ", false),
            ("execute_command", true),
            (" arm still wraps it in ", false),
            ("Ok(...)", true),
            (" inside a closure.", false),
        ],
        &[
            ("Permission gate: ", false),
            ("PermissionMode::{Deny, Ask, ApproveAll}", true),
            (" gates ", false),
            ("write_file", true),
            (", ", false),
            ("edit_file", true),
            (", ", false),
            ("execute_command", true),
            ("; read and search tools never ask.", false),
        ],
        &[
            (
                "Collaboration: a durable JSONL bus, one inbox per session; ",
                false,
            ),
            ("delegate", true),
            (" spawns a child agent with its own context and ", false),
            ("message", true),
            (" posts to any peer.", false),
        ],
        &[
            ("Remaining uncertainty: ", false),
            ("ComposerSubmission::text()", true),
            (" may not exist; check ", false),
            ("Element::style_style()", true),
            (" and ", false),
            ("KeyChord::tab()", true),
            (" before relying on them.", false),
        ],
    ];
    let code = [
        "\"execute_command\" => match args.get(\"command\").and_then(|v| v.as_str()) {",
        "    Some(cmd) => Ok(run_shell(cmd, ws, Duration::from_secs(t))),",
        "    None => Err(\"command required\".into()),",
        "},",
    ];
    let cycle = [
        "Let me check the Modal, List and Feed APIs in the api.md doc.",
        "The endpoint fully supports chat, tool calls and streaming.",
        "I have the full API picture. Now let me verify the exact signatures.",
        "Rust 1.100.0 installed. Now let me study the two libraries' APIs.",
    ];
    let mut y = rect.y;
    let mut n = 0usize;
    while y < rect.bottom() {
        match n % 4 {
            0 | 2 => {
                let mut x = rect.x + 1;
                for &(span, is_code) in prose[(n / 2) % prose.len()] {
                    let style = if is_code {
                        Style::new().fg(t.info).bg(code_bg)
                    } else {
                        Style::new().fg(t.text)
                    };
                    x += canvas.print_styled(Point::new(x, y), span, &style);
                }
                y += 2;
            }
            1 => {
                for line in code {
                    if y >= rect.bottom() {
                        break;
                    }
                    canvas.fill(Rect::new(rect.x, y, rect.w, 1), ' ', t.text, block_bg);
                    canvas.print(Point::new(rect.x + 2, y), line, t.warn, block_bg);
                    y += 1;
                }
                y += 1;
            }
            _ => {
                for i in 0..6 {
                    if y >= rect.bottom() {
                        break;
                    }
                    let k = n * 3 + i;
                    let head = format!("▸ cycle {:<3}", k + 1);
                    let x = rect.x + 1;
                    let x = x + canvas.print(Point::new(x, y), &head, t.text, Rgba::TRANSPARENT);
                    let stats = format!(
                        "{} tools ✓{} · {} words · {}s · {}k↑ {}↓ tk · {}% cached",
                        1 + k % 3,
                        1 + k % 3,
                        40 + (k * 37) % 900,
                        4 + (k * 7) % 60,
                        11 + k * 3,
                        200 + (k * 53) % 700,
                        86 + k % 12
                    );
                    let sx = rect.right() - abstracttui::text::width(&stats) - 1;
                    let room = sx - x - 2;
                    if room > 0 {
                        let line =
                            abstracttui::text::truncate_ellipsis(cycle[k % cycle.len()], room);
                        canvas.print(Point::new(x, y), &line, t.text_muted, Rgba::TRANSPARENT);
                    }
                    canvas.print(
                        Point::new(sx.max(x), y),
                        &stats,
                        t.text_muted,
                        Rgba::TRANSPARENT,
                    );
                    y += 1;
                }
                y += 1;
            }
        }
        n += 1;
    }
}
