//! Compact session presentation. OWNER: DESIGN.
//! The host owns read receipts, human-wait detection, navigation and archive actions.
use crate::base::Point;
use crate::layout::{Dimension, Style};
use crate::theme::TokenSet;
use crate::ui::Element;

#[derive(Default, Debug, Clone)]
pub struct SessionCard {
    pub title: String,
    pub turns: Option<u64>,
    pub tools: Option<u64>,
    pub time_label: String,
    pub counts_partial: bool,
    pub selected: bool,
    pub unread: bool,
    pub needs_attention: bool,
    pub working: bool,
}

impl SessionCard {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            ..Self::default()
        }
    }

    pub fn metadata(&self) -> String {
        let mut parts = Vec::new();
        let count = |n: u64, word: &str| {
            format!(
                "{}{n} {word}{}",
                if self.counts_partial { "≥" } else { "" },
                if n == 1 { "" } else { "s" }
            )
        };
        if let Some(n) = self.turns {
            parts.push(count(n, "turn"));
        }
        if let Some(n) = self.tools.filter(|n| *n > 0) {
            parts.push(count(n, "tool"));
        }
        if self.needs_attention {
            parts.push("Needs you".into());
        } else if self.working {
            parts.push("Working".into());
        } else if !self.time_label.is_empty() {
            parts.push(self.time_label.clone());
        }
        parts.join(" · ")
    }

    pub fn view(self, cx: crate::reactive::Scope) -> crate::ui::View {
        let t = crate::widgets::theme_tokens(cx);
        self.element(&t).build()
    }

    /// Pure controlled presenter. Wrap in host interaction/navigation; never resolves a wait.
    pub fn element(self, t: &TokenSet) -> Element {
        let border = if self.needs_attention {
            t.warn
        } else if self.unread {
            t.info
        } else if self.selected {
            t.border_focus
        } else {
            t.border
        };
        let bg = if self.selected {
            t.selection_bg
        } else {
            t.surface_raised
        };
        let fg = if self.selected {
            t.selection_fg
        } else {
            t.text
        };
        let muted = if self.selected {
            t.selection_fg
        } else {
            t.text_muted
        };
        let marker = if self.needs_attention {
            "! "
        } else if self.working {
            "◉ "
        } else if self.unread {
            "• "
        } else {
            ""
        };
        let meta = self.metadata();
        let label = format!(
            "{}. {}{}",
            self.title,
            if self.unread { "New messages. " } else { "" },
            meta
        );
        Element::new()
            .style(Style::default().height(Dimension::Cells(5)).shrink(0.0))
            .access_label(label)
            .draw(move |canvas, r| {
                if r.w < 3 || r.h < 3 {
                    return;
                }
                canvas.fill(r, ' ', fg, bg);
                let line = "─".repeat((r.w - 2) as usize);
                canvas.print(Point::new(r.x, r.y), &format!("╭{line}╮"), border, bg);
                canvas.print(
                    Point::new(r.x, r.y + r.h - 1),
                    &format!("╰{line}╯"),
                    border,
                    bg,
                );
                for y in r.y + 1..r.y + r.h - 1 {
                    canvas.print(Point::new(r.x, y), "│", border, bg);
                    canvas.print(Point::new(r.x + r.w - 1, y), "│", border, bg);
                }
                let width = (r.w - 4).max(0);
                let lines = crate::text::wrap(&format!("{marker}{}", self.title), width);
                for (i, line) in lines.iter().take((r.h - 3).min(2) as usize).enumerate() {
                    let shown = if i == 1 && lines.len() > 2 {
                        crate::text::truncate_ellipsis(&lines[1..].join(" "), width)
                    } else {
                        crate::text::truncate_ellipsis(line, width)
                    };
                    canvas.print(Point::new(r.x + 2, r.y + 1 + i as i32), &shown, fg, bg);
                }
                canvas.print(
                    Point::new(r.x + 2, r.y + r.h - 2),
                    &crate::text::truncate_ellipsis(&meta, width),
                    muted,
                    bg,
                );
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::base::Size;
    use crate::widgets::test_util::{draw_into, row};
    #[test]
    fn human_attention_overrides_unread_without_erasing_selection() {
        let t = TokenSet::default();
        let card = SessionCard {
            unread: true,
            needs_attention: true,
            selected: true,
            turns: Some(2),
            tools: Some(7),
            ..SessionCard::new("A conversation")
        };
        assert!(card.metadata().contains("Needs you"));
        let c = draw_into(card.element(&t), Size::new(40, 5));
        let (_, fg, bg) = c.cell(Point::new(0, 0)).unwrap();
        assert_eq!(fg, t.warn);
        assert_eq!(bg, t.selection_bg);
        assert!(row(&c, 3).contains("2 turns · 7 tools · Needs you"));
    }
    #[test]
    fn partial_counts_and_tiny_or_wide_text_remain_bounded() {
        let t = TokenSet::default();
        for width in [0, 1, 2, 3, 8, 30] {
            let card = SessionCard {
                counts_partial: true,
                turns: Some(3),
                tools: None,
                ..SessionCard::new("Long 日本語 conversation with no guessed tools")
            };
            assert_eq!(card.metadata(), "≥3 turns");
            let _ = draw_into(card.element(&t), Size::new(width, 5));
        }
    }
}
