use super::*;
use crate::{
    base::Size,
    widgets::itest_util::{key, mount_widget, settle, type_str},
};
#[test]
fn short_messages_start_at_the_top_with_distinct_bubble_alignment() {
    let size = Size::new(60, 24);
    let (root, mut tree) = mount_widget(size, move |cx| {
        ChatTranscript::new(vec![
            ChatMessage::user("A short question"),
            ChatMessage::assistant("### A useful heading\n\nA useful answer."),
        ])
        .view(cx)
    });
    let screen = {
        let canvas = settle(&mut tree, size);
        (0..size.h)
            .map(|y| canvas.row_text(y))
            .collect::<Vec<_>>()
            .join("\n")
    };
    let lines = screen.lines().collect::<Vec<_>>();
    let uy = lines.iter().position(|s| s.contains("You")).expect(&screen);
    let ay = lines
        .iter()
        .position(|s| s.contains("Assistant"))
        .expect(&screen);
    assert!(
        uy < 3 && ay < 10,
        "short chats must not float in the middle:\n{screen}"
    );
    assert!(lines[uy].find("You").unwrap() > lines[ay].find("Assistant").unwrap());
    assert!(!screen.contains("###"));
    assert!(screen.contains("A useful heading"));
    assert!(screen.contains("A useful answer."));
    root.dispose();
}
#[test]
fn composer_keeps_the_draft_until_the_host_accepts_and_busy_prevents_duplicate_send() {
    let size = Size::new(60, 20);
    let state = Rc::new(RefCell::new(None));
    let out = state.clone();
    let calls = Rc::new(RefCell::new(Vec::<String>::new()));
    let received = calls.clone();
    let (root, mut tree) = mount_widget(size, move |cx| {
        let draft = cx.signal(String::new());
        let busy = cx.signal(false);
        *out.borrow_mut() = Some((draft, busy));
        ChatPanel::new(draft, Vec::new, move |q| {
            received.borrow_mut().push(q.into())
        })
        .busy(move || busy.get())
        .view(cx)
    });
    type_str(&mut tree, "hello");
    key(&mut tree, Key::Enter);
    let (draft, busy) = state.borrow().unwrap();
    assert_eq!(draft.get_untracked(), "hello");
    assert_eq!(&*calls.borrow(), &["hello"]);
    busy.set(true);
    key(&mut tree, Key::Enter);
    assert_eq!(calls.borrow().len(), 1);
    draft.set(String::new());
    let screen = {
        let canvas = settle(&mut tree, size);
        (0..size.h)
            .map(|y| canvas.row_text(y))
            .collect::<Vec<_>>()
            .join("\n")
    };
    assert!(!screen.contains("hello"));
    assert!(screen.contains("Answering"));
    root.dispose();
}
#[test]
fn long_messages_scroll_to_the_tail_and_narrow_messages_stay_in_bounds() {
    for width in [24, 40, 84] {
        let size = Size::new(width, 16);
        let body = (0..80)
            .map(|i| format!("Line {i} with enough words to wrap.\n\n"))
            .collect::<String>();
        let (root, mut tree) = mount_widget(size, move |cx| {
            ChatTranscript::new(vec![ChatMessage::assistant(body)]).view(cx)
        });
        let screen = {
            let canvas = settle(&mut tree, size);
            (0..size.h)
                .map(|y| canvas.row_text(y))
                .collect::<Vec<_>>()
                .join("\n")
        };
        assert!(screen.contains("Line 79"), "{width}:\n{screen}");
        root.dispose();
    }
}
