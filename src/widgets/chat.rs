//! Backend-independent chat surface: measured message bubbles and a bound composer.
//! Hosts own messages, requests, history, permissions and persistence.
use super::{
    markdown::{
        self,
        doc::{self, DocLayout},
    },
    CustomBlock, Feed, FeedBlock, FeedItem, FeedState, TextArea, TextAreaState,
};
use crate::{
    prelude::*,
    render::{Attrs, Style},
    theme::TokenSet,
};
use std::{cell::RefCell, rc::Rc};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChatRole {
    User,
    Assistant,
    Notice,
    Error,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChatMessage {
    pub role: ChatRole,
    pub text: String,
    pub author: String,
    pub meta: String,
}
impl ChatMessage {
    pub fn new(role: ChatRole, text: impl Into<String>) -> Self {
        Self {
            role,
            text: text.into(),
            author: String::new(),
            meta: String::new(),
        }
    }
    pub fn user(text: impl Into<String>) -> Self {
        Self::new(ChatRole::User, text)
    }
    pub fn assistant(text: impl Into<String>) -> Self {
        Self::new(ChatRole::Assistant, text)
    }
    pub fn author(mut self, label: impl Into<String>) -> Self {
        self.author = label.into();
        self
    }
    pub fn meta(mut self, label: impl Into<String>) -> Self {
        self.meta = label.into();
        self
    }
}

fn bubble_width(role: ChatRole, preferred: i32, width: i32) -> i32 {
    let cap = (width * if role == ChatRole::User { 82 } else { 95 } / 100)
        .max(8)
        .min(width)
        .max(1);
    cap.min(preferred.max(8))
}

fn layout(message: &ChatMessage, t: &TokenSet, width: i32) -> DocLayout {
    let mut doc = doc::layout_doc_all(
        &message.text,
        t,
        (width - 2).max(1),
        None,
        Default::default(),
    );
    // Chat headings keep their styled title without document-depth syntax.
    for ix in &doc.heading_rows {
        if let Some(row) = doc.rows.get_mut(*ix) {
            if row.line.spans.first().is_some_and(|s| {
                s.text.trim().chars().all(|c| c == '#') && !s.text.trim().is_empty()
            }) {
                row.line.spans.remove(0);
            }
        }
    }
    doc
}
fn message_item(mut message: ChatMessage, t: TokenSet, assistant: &str, user: &str) -> FeedItem {
    if message.author.is_empty() {
        message.author = match message.role {
            ChatRole::User => user,
            ChatRole::Assistant => assistant,
            ChatRole::Notice => "",
            ChatRole::Error => "Could not answer",
        }
        .into();
    }
    let preferred = message
        .text
        .lines()
        .map(crate::text::width)
        .max()
        .unwrap_or(0)
        .max(crate::text::width(&message.author))
        + 2;
    let heading = if message.meta.is_empty() {
        message.author.clone()
    } else {
        format!("{} · {}", message.author, message.meta)
    };
    let message = Rc::new(message);
    let cache = Rc::new(RefCell::new(markdown::WidthCache::default()));
    let measured = cache.clone();
    let for_measure = message.clone();
    FeedItem::new().block(FeedBlock::Custom(CustomBlock::new(
        move |width| {
            let width = bubble_width(for_measure.role, preferred, width);
            let mut cache = measured.borrow_mut();
            cache
                .rows_for(width, || layout(&for_measure, &t, width).rows)
                .len()
                .max(1) as i32
                + 2
        },
        move |canvas, rect| {
            let width = bubble_width(message.role, preferred, rect.w);
            let mut cache = cache.borrow_mut();
            let rows = cache.rows_for(width, || layout(&message, &t, width).rows);
            let x = if message.role == ChatRole::User {
                rect.right() - width
            } else {
                rect.x
            };
            let bubble = Rect::new(x, rect.y, width, rect.h);
            let bg = t.surface_raised;
            canvas.fill(bubble, ' ', t.text, bg);
            let ink = match message.role {
                ChatRole::Error => t.error,
                ChatRole::Notice => t.text_muted,
                ChatRole::User => t.accent,
                ChatRole::Assistant => t.text,
            };
            canvas.print_styled(
                Point::new(x + 1, rect.y),
                &crate::text::truncate_ellipsis(&heading, (width - 2).max(1)),
                &Style::new().fg(ink).bg(bg).attrs(Attrs::BOLD),
            );
            markdown::draw_rows(
                canvas,
                Rect::new(x + 1, rect.y + 1, (width - 2).max(1), (rect.h - 2).max(1)),
                &t,
                rows,
            );
        },
    )))
}

/// Message-only form for hosts that provide their own composer and history controls.
pub struct ChatTranscript {
    messages: Vec<ChatMessage>,
    assistant: String,
    user: String,
    follow: Option<Signal<bool>>,
    offset: Option<Signal<i32>>,
}
impl ChatTranscript {
    pub fn new(messages: Vec<ChatMessage>) -> Self {
        Self {
            messages,
            assistant: "Assistant".into(),
            user: "You".into(),
            follow: None,
            offset: None,
        }
    }
    pub fn assistant_label(mut self, label: impl Into<String>) -> Self {
        self.assistant = label.into();
        self
    }
    pub fn user_label(mut self, label: impl Into<String>) -> Self {
        self.user = label.into();
        self
    }
    pub fn follow_tail(mut self, signal: Signal<bool>) -> Self {
        self.follow = Some(signal);
        self
    }
    pub fn offset_y(mut self, signal: Signal<i32>) -> Self {
        self.offset = Some(signal);
        self
    }
    pub fn view(self, cx: Scope) -> View {
        let t = use_theme(cx).get().tokens;
        let feed = FeedState::new(cx);
        for (ix, message) in self.messages.into_iter().enumerate() {
            feed.push(
                ix.to_string(),
                message_item(message, t, &self.assistant, &self.user),
            );
        }
        Scroll::new(Feed::new(&feed).gap(1).view(cx))
            .follow_tail(self.follow.unwrap_or_else(|| cx.signal(true)))
            .offset_y(self.offset.unwrap_or_else(|| cx.signal(0i32)))
            .scrollbar_auto_hide(true)
            .layout(LayoutStyle::default().grow(1.0).basis(Dimension::Cells(0)))
            .view(cx)
    }
}

type ReadMessages = Rc<dyn Fn() -> Vec<ChatMessage>>;
type ReadBusy = Rc<dyn Fn() -> bool>;
type Submit = Rc<RefCell<dyn FnMut(&str)>>;
/// A composable chat page. Supply tracked readers and a draft signal; the
/// component neither sends requests nor clears the draft before host acceptance.
/// Mount inside a Drawer, PageHost or any allocated pane.
pub struct ChatPanel {
    messages: ReadMessages,
    draft: Signal<String>,
    busy: ReadBusy,
    submit: Submit,
    assistant: String,
    user: String,
    placeholder: String,
    empty: String,
    pending: String,
    send_label: String,
    actions: Option<Rc<dyn Fn(Scope) -> View>>,
}
impl ChatPanel {
    pub fn new(
        draft: Signal<String>,
        messages: impl Fn() -> Vec<ChatMessage> + 'static,
        on_submit: impl FnMut(&str) + 'static,
    ) -> Self {
        Self {
            messages: Rc::new(messages),
            draft,
            busy: Rc::new(|| false),
            submit: Rc::new(RefCell::new(on_submit)),
            assistant: "Assistant".into(),
            user: "You".into(),
            placeholder: "Write a message…".into(),
            empty: "Start a conversation.".into(),
            pending: "Answering…".into(),
            send_label: "Send".into(),
            actions: None,
        }
    }
    pub fn busy(mut self, read: impl Fn() -> bool + 'static) -> Self {
        self.busy = Rc::new(read);
        self
    }
    pub fn assistant_label(mut self, label: impl Into<String>) -> Self {
        self.assistant = label.into();
        self
    }
    pub fn user_label(mut self, label: impl Into<String>) -> Self {
        self.user = label.into();
        self
    }
    pub fn placeholder(mut self, label: impl Into<String>) -> Self {
        self.placeholder = label.into();
        self
    }
    pub fn empty_message(mut self, label: impl Into<String>) -> Self {
        self.empty = label.into();
        self
    }
    pub fn pending_message(mut self, label: impl Into<String>) -> Self {
        self.pending = label.into();
        self
    }
    /// Customize the visible send glyph/text; accessible name remains Send message.
    pub fn send_label(mut self, label: impl Into<String>) -> Self {
        self.send_label = label.into();
        self
    }
    /// Host actions share the send row and are mounted in its reactive scope.
    pub fn actions(mut self, build: impl Fn(Scope) -> View + 'static) -> Self {
        self.actions = Some(Rc::new(build));
        self
    }
    pub fn view(self, cx: Scope) -> View {
        let theme = use_theme(cx);
        let follow = cx.signal(true);
        let offset = cx.signal(0i32);
        let editor = TextAreaState::new(cx);
        let sync = editor.clone();
        let draft = self.draft;
        cx.effect(move || {
            let value = draft.get();
            if sync.text() != value {
                sync.set_text(value);
            }
        });
        let busy = self.busy;
        let submit = self.submit;
        let enter_busy = busy.clone();
        let enter_submit = submit.clone();
        let button_busy = busy.clone();
        let button_submit = submit.clone();
        let read_busy = busy.clone();
        let pending = self.pending;
        let messages = self.messages;
        let assistant = self.assistant;
        let user = self.user;
        let empty = self.empty;
        let send_label = self.send_label;
        let actions = self.actions;
        Element::new()
            .style(
                LayoutStyle::column()
                    .grow(1.0)
                    .basis(Dimension::Cells(0))
                    .gap(1),
            )
            .child(dyn_view_scoped(
                LayoutStyle::default().grow(1.0).basis(Dimension::Cells(0)),
                move |scx| {
                    let _ = theme.get();
                    let mut items = messages();
                    if read_busy() {
                        items.push(ChatMessage::new(ChatRole::Notice, pending.clone()));
                    }
                    if items.is_empty() {
                        items.push(ChatMessage::new(ChatRole::Notice, empty.clone()));
                    }
                    ChatTranscript::new(items)
                        .assistant_label(&assistant)
                        .user_label(&user)
                        .follow_tail(follow)
                        .offset_y(offset)
                        .view(scx)
                },
            ))
            .child(
                TextArea::new()
                    .state(&editor)
                    .rows(1, 4)
                    .inset(1, 1)
                    .placeholder(self.placeholder)
                    .placeholder_while_focused(true)
                    .on_change(move |s| draft.set(s.into()))
                    .on_submit(move |s| {
                        if !enter_busy() && !s.trim().is_empty() {
                            enter_submit.borrow_mut()(s);
                        }
                    })
                    .element(cx, &theme.get_untracked().tokens)
                    .autofocus()
                    .build(),
            )
            .child(dyn_view_scoped(
                LayoutStyle::line(1).shrink(0.0),
                move |scx| {
                    let submit = button_submit.clone();
                    let check = button_busy.clone();
                    let button = Button::new(&send_label)
                        .disabled(button_busy())
                        .on_click(move || {
                            let q = draft.get_untracked();
                            if !check() && !q.trim().is_empty() {
                                submit.borrow_mut()(&q);
                            }
                        })
                        .element(scx, &theme.get_untracked().tokens)
                        .access_label("Send message")
                        .build();
                    let button = if let Some(overlays) = scx.use_context::<crate::app::Overlays>() {
                        Tooltip::attach(
                            scx,
                            &overlays,
                            "Send message · Enter",
                            std::time::Duration::from_millis(400),
                            button,
                        )
                    } else {
                        button
                    };
                    let mut row = Element::new()
                        .style(LayoutStyle::row().gap(1))
                        .child(button);
                    if let Some(actions) = &actions {
                        row = row.child(actions(scx));
                    }
                    row.build()
                },
            ))
            .build()
    }
}

#[cfg(test)]
#[path = "chat_tests.rs"]
mod tests;
