//! Rendered-cell tests through the real UiTree: flowchart cards +
//! badges + notices, the sequence golden, and the ATOMIC fallback
//! (code fence + named notice + live link; no diagram chrome).

use abstracttui::base::{Point, Size};
use abstracttui::reactive::create_root;
use abstracttui::ui::{BufferCanvas, UiTree};
use abstracttui_mermaid::MermaidView;

struct Rig {
    _root: abstracttui::reactive::RootScope,
    tree: UiTree,
    size: Size,
}

impl Rig {
    fn mount(size: Size, build: impl FnOnce() -> MermaidView) -> Rig {
        let mut tree = UiTree::new(size);
        let (_root, ()) = create_root(|cx| {
            let view = build().view(cx);
            tree.mount(cx, view);
        });
        Rig { _root, tree, size }
    }

    fn rows(&mut self) -> Vec<String> {
        let mut canvas = BufferCanvas::new(self.size);
        self.tree.draw(&mut canvas);
        (0..self.size.h)
            .map(|y| canvas.row_text(y).trim_end().to_string())
            .collect()
    }

    fn count_char(&mut self, ch: char) -> usize {
        let mut canvas = BufferCanvas::new(self.size);
        self.tree.draw(&mut canvas);
        let mut n = 0;
        for y in 0..self.size.h {
            for x in 0..self.size.w {
                if canvas.cell(Point::new(x, y)).unwrap().0 == ch {
                    n += 1;
                }
            }
        }
        n
    }
}

#[test]
fn flowchart_renders_cards_badges_and_arrowheads() {
    let src = "graph TD\nA[Go] --> B{Ok?}";
    let mut rig = Rig::mount(Size::new(40, 16), || MermaidView::new(src));
    assert_eq!(rig.count_char('╭'), 2, "two cards");
    assert_eq!(rig.count_char('◆'), 1, "decision badge sigil");
    assert_eq!(rig.count_char('▼'), 1, "TD arrowhead");
    let rows = rig.rows();
    assert!(rows.iter().any(|r| r.contains("Go")), "{rows:?}");
    assert!(rows.iter().any(|r| r.contains("Ok?")));

    // Determinism: a second mount renders identical cells.
    let mut rig2 = Rig::mount(Size::new(40, 16), || MermaidView::new(src));
    assert_eq!(rig.rows(), rig2.rows());
}

#[test]
fn dropped_directives_render_a_notice_line() {
    let src = "%%{init: {\"theme\":\"dark\"}}%%\ngraph TD\nA --> B";
    let mut rig = Rig::mount(Size::new(48, 14), || MermaidView::new(src));
    let rows = rig.rows();
    assert!(rows[0].contains("init/theme directive ignored"), "{rows:?}");
    assert_eq!(rig.count_char('╭'), 2, "render proceeds under the notice");
}

/// Golden: the docs' Alice/John greeting, pinned rows (glyphs only —
/// inks are theme business).
#[test]
fn sequence_golden_alice_john() {
    let src = "sequenceDiagram\n    participant Alice\n    participant John\n    Alice->>John: Hello John, how are you?\n    John-->>Alice: Great!";
    let mut rig = Rig::mount(Size::new(46, 14), || MermaidView::new(src));
    let rows = rig.rows();
    let expected = sequence_golden_rows();
    assert!(!expected.is_empty(), "golden must be pinned, not vacuous");
    for (i, want) in expected.iter().enumerate() {
        assert_eq!(rows[i], *want, "row {i}");
    }
}

fn sequence_golden_rows() -> Vec<&'static str> {
    vec![
        "╭───────╮                   ╭──────╮",
        "│ Alice │                   │ John │",
        "╰───────╯                   ╰──────╯",
        "    │                           │",
        "    │ Hello John, how are you?  │",
        "    │──────────────────────────▶│",
        "    │                           │",
        "    │          Great!           │",
        "    │◀╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌│",
        "    │                           │",
        "    │                           │",
        "",
    ]
}

#[test]
fn fallback_is_atomic_fence_notice_and_link() {
    // Valid until line 4 — the WHOLE diagram must fence. (`subgraph`
    // used to be the example here; it renders now, so the specimen is
    // a statement that is still not a node reference.)
    let src = "graph TD\nA --> B\nB --> C\n!! nope\nC --> D";
    let mut rig = Rig::mount(Size::new(72, 14), || MermaidView::new(src));
    let rows = rig.rows();
    assert!(
        rows[0].contains("unsupported mermaid at line 4"),
        "{}",
        rows[0]
    );
    assert!(rows[0].contains("node reference"), "{}", rows[0]);
    // The escape hatch is offered. Its ROWS are not fixed: without
    // OSC 8 the URL wraps so it stays copyable, so the fence's start
    // is found, not assumed.
    assert!(
        rows.iter().any(|r| r.contains("view online")),
        "the live link is offered: {rows:?}"
    );
    assert!(
        rows.iter().any(|r| r.contains("mermaid.live/edit#base64:")),
        "the URL is on screen: {rows:?}"
    );
    // The fence: every source line verbatim, in order.
    let first = src.lines().next().unwrap();
    let start = rows
        .iter()
        .position(|r| r.starts_with(first))
        .expect("the fence starts somewhere below the notice");
    for (i, line) in src.lines().enumerate() {
        assert!(
            rows[start + i].starts_with(line),
            "fence row {i}: {:?} vs {line:?}",
            rows[start + i]
        );
    }
    // No diagram chrome leaked: no cards, no arrowheads.
    assert_eq!(rig.count_char('╭'), 0);
    assert_eq!(rig.count_char('▼'), 0);
    assert_eq!(rig.count_char('▶'), 0);
}

#[test]
fn live_link_opt_out_removes_the_link_row() {
    let src = "gantt\ntitle X";
    let mut rig = Rig::mount(Size::new(60, 8), || MermaidView::new(src).live_link(false));
    let rows = rig.rows();
    assert!(rows[0].contains("gantt"));
    assert!(!rows.iter().any(|r| r.contains("mermaid.live")));
    assert!(rows[1].starts_with("gantt"), "fence directly under notice");
}

/// The label-loss defect: an `LR` flowchart used to draw ONE of the
/// three labels in the crate's own accept fixture. The other two were
/// suppressed to avoid overprinting a card, silently — a diagram that
/// renders a DIFFERENT graph than the source describes.
#[test]
fn horizontal_flowcharts_keep_every_edge_label() {
    let src = include_str!("../fixtures/accept_flowchart_lr_shapes_labels.mmd");
    let mut rig = Rig::mount(Size::new(120, 24), || MermaidView::new(src));
    let rows = rig.rows();
    let screen = rows.join("\n");
    let labels: Vec<&str> = src
        .lines()
        .filter_map(|l| {
            l.split_once('|')
                .and_then(|(_, r)| r.split_once('|'))
                .map(|(t, _)| t)
        })
        .collect();
    assert!(!labels.is_empty(), "the fixture carries |labels|");
    for label in labels {
        assert!(
            screen.contains(label),
            "label {label:?} missing from the render:\n{screen}"
        );
    }
}

/// Same guarantee in the direction that always worked, so the fix
/// cannot regress it.
#[test]
fn vertical_flowcharts_keep_every_edge_label() {
    let src = "graph TD\n  A[Start] -->|yes, always| B{Check}\n  B -->|no| C(Stop)";
    let mut rig = Rig::mount(Size::new(80, 24), || MermaidView::new(src));
    let screen = rig.rows().join("\n");
    for label in ["yes, always", "no"] {
        assert!(screen.contains(label), "{label:?} missing:\n{screen}");
    }
}

/// A header with no statements parses (it is valid mermaid) and used
/// to draw an empty rectangle — indistinguishable from a broken
/// renderer. It says what it is now.
#[test]
fn an_empty_diagram_says_so() {
    let mut rig = Rig::mount(Size::new(60, 4), || MermaidView::new("flowchart TD"));
    let rows = rig.rows();
    assert!(rows.iter().any(|r| r.contains("empty diagram")), "{rows:?}");
}

/// The escape hatch has to be usable. A mermaid.live URL carries the
/// whole diagram in its fragment, so on one row it was truncated to
/// dead text — unreadable AND uncopyable. Without OSC 8 the URL now
/// wraps across rows so a selection can take all of it.
#[test]
fn the_live_link_is_never_clipped_to_dead_text() {
    let src = "gantt\n  title Not in the subset\n  section S\n  A task :a1, 2014-01-01, 30d";
    let url = abstracttui_mermaid::live_link_url(src);
    assert!(url.len() > 120, "the fixture's URL is genuinely long");

    let mut rig = Rig::mount(Size::new(72, 20), || MermaidView::new(src));
    let rows = rig.rows();
    let screen = rows.join("");
    // Every character of the URL reaches the screen (wrapped, not cut).
    let on_screen: String = screen.chars().filter(|c| !c.is_whitespace()).collect();
    let wanted: String = url.chars().filter(|c| !c.is_whitespace()).collect();
    assert!(
        on_screen.contains(&wanted),
        "the URL is truncated:\n{}",
        rows.join("\n")
    );
    assert!(
        !screen.contains('…')
            || rows
                .iter()
                .all(|r| !r.contains("base64") || !r.contains('…')),
        "no ellipsis in the link rows: {rows:?}"
    );
}

// ---------------------------------------------------------------------------
// Control-flow frames on screen: the tab names the construct, the
// divider separates the branches, and the bar shows who is busy.
// ---------------------------------------------------------------------------

#[test]
fn an_alt_block_draws_a_named_frame_with_a_divider() {
    let src = "sequenceDiagram\n  participant Alice\n  participant Bob\n  Alice->>Bob: Hungry?\n  alt is lunchtime\n    Bob-->>Alice: Yes!\n  else not yet\n    Bob-->>Alice: Later\n  end";
    let mut rig = Rig::mount(Size::new(60, 24), || MermaidView::new(src));
    let rows = rig.rows();
    let screen = rows.join("\n");

    // The tab names WHICH construct this is — without it a frame is
    // just a box, and `alt` and `loop` become indistinguishable.
    assert!(
        screen.contains("alt: is lunchtime"),
        "the frame is labeled:\n{screen}"
    );
    assert!(
        screen.contains("not yet"),
        "the else branch is labeled:\n{screen}"
    );
    // Both branches' messages still render inside it.
    assert!(
        screen.contains("Yes!") && screen.contains("Later"),
        "{screen}"
    );
    // Frame chrome: corners top and bottom.
    assert!(rig.count_char('╭') >= 3, "two participant boxes + a frame");
    assert!(screen.contains('╌'), "the divider is dashed: {screen}");
}

#[test]
fn every_block_kind_names_itself() {
    for (src, tab) in [
        (
            "sequenceDiagram\n  opt if slow\n    A->>B: warn\n  end",
            "opt: if slow",
        ),
        (
            "sequenceDiagram\n  loop every minute\n    A->>B: poll\n  end",
            "loop: every minute",
        ),
        (
            "sequenceDiagram\n  par to B\n    A->>B: hi\n  and to C\n    A->>C: hi\n  end",
            "par: to B",
        ),
    ] {
        let mut rig = Rig::mount(Size::new(70, 22), || MermaidView::new(src));
        let screen = rig.rows().join("\n");
        assert!(screen.contains(tab), "expected {tab:?} in:\n{screen}");
    }
}

/// A label longer than the columns it spans must not truncate to
/// `alt: is lunc…`: the frame widens instead, because the tab is the
/// only thing naming the construct.
#[test]
fn a_frame_widens_to_hold_its_own_label() {
    let src =
        "sequenceDiagram\n  alt a very long condition indeed, spelled out\n    A->>B: x\n  end";
    let mut rig = Rig::mount(Size::new(90, 16), || MermaidView::new(src));
    let screen = rig.rows().join("\n");
    assert!(
        screen.contains("alt: a very long condition indeed, spelled out"),
        "the tab is whole:\n{screen}"
    );
    assert!(!screen.contains('…'), "nothing truncated:\n{screen}");
}

/// Activation bars ride the lifeline and must not erase the frame tab
/// they pass through — the z-order that took two tries to get right.
#[test]
fn activation_bars_ride_the_lifeline_under_the_frames() {
    let src = "sequenceDiagram\n  A->>+B: work\n  loop every minute\n    B->>B: poll\n  end\n  B-->>-A: done";
    let mut rig = Rig::mount(Size::new(60, 26), || MermaidView::new(src));
    let screen = rig.rows().join("\n");
    assert!(rig.count_char('┃') >= 3, "the bar spans rows:\n{screen}");
    assert!(
        screen.contains("loop: every minute"),
        "the bar did not erase the tab:\n{screen}"
    );
}

/// Nested frames must not share a border with the frame that holds
/// them, or the two read as one box.
#[test]
fn nested_frames_inset() {
    let src = "sequenceDiagram\n  alt outer\n    loop inner\n      A->>B: tick\n    end\n  end";
    let mut rig = Rig::mount(Size::new(60, 24), || MermaidView::new(src));
    let rows = rig.rows();
    let screen = rows.join("\n");
    assert!(
        screen.contains("alt: outer") && screen.contains("loop: inner"),
        "{screen}"
    );
    // The inner frame's top-left corner sits strictly inside the outer.
    let corner_cols: Vec<usize> = rows.iter().filter_map(|r| r.find('╭')).collect();
    assert!(
        corner_cols.iter().any(|c| *c > corner_cols[0]),
        "the inner frame is inset: {corner_cols:?}\n{screen}"
    );
}

/// The bar must start on the arrow that opened it. `->>+` expands to
/// a message followed by an activation, and reading the cursor after
/// the message put the bar three rows below its own arrow — reading as
/// if it belonged to the NEXT message.
#[test]
fn an_activation_bar_starts_on_the_arrow_that_opened_it() {
    let src = "sequenceDiagram\n  A->>+B: request\n  B-->>-A: response";
    let mut rig = Rig::mount(Size::new(50, 16), || MermaidView::new(src));
    let rows = rig.rows();
    let arrow_row = rows
        .iter()
        .position(|r| r.contains('▶'))
        .expect("the request arrow is drawn");
    let bar_row = rows
        .iter()
        .position(|r| r.contains('┃'))
        .expect("the activation bar is drawn");
    assert_eq!(
        bar_row,
        arrow_row,
        "the bar opens on its own arrow:\n{}",
        rows.join("\n")
    );
}

/// A frame's border must survive everything inside it. Notes are
/// FILLED boxes, so a note poking out does not overlap the border — it
/// erases it; a self-message writes its label to the right of the
/// lifeline, past where the columns end.
#[test]
fn nothing_inside_a_frame_is_drawn_outside_it() {
    for (name, src) in [
        (
            "note",
            "sequenceDiagram\n  participant A\n  participant B\n  opt maybe\n    A->>B: hi\n    Note over A,B: a really quite long note here\n  end",
        ),
        (
            "self-message",
            "sequenceDiagram\n  participant A\n  participant B\n  loop retry\n    A->>A: think about it for a while\n  end",
        ),
    ] {
        let mut rig = Rig::mount(Size::new(90, 24), || MermaidView::new(src));
        let rows = rig.rows();
        let screen = rows.join("\n");
        let top = rows
            .iter()
            .position(|r| r.contains('╭') && (r.contains("opt:") || r.contains("loop:")))
            .unwrap_or_else(|| panic!("{name}: no frame drawn:\n{screen}"));
        let bottom = rows
            .iter()
            .rposition(|r| r.contains('╰'))
            .expect("frame bottom");
        // Every row of the frame carries both of its borders.
        for (i, row) in rows.iter().enumerate().take(bottom).skip(top + 1) {
            let bars = row.matches('│').count();
            assert!(
                bars >= 2,
                "{name}: row {i} lost a frame border ({bars} bars):\n{screen}"
            );
        }
    }
}

/// A divider's label is a tab like any other: the frame is sized for
/// the widest of them, not just the opener's.
#[test]
fn a_frame_holds_its_widest_branch_label() {
    let src = "sequenceDiagram\n  alt ok\n    A->>B: x\n  else a much much longer alternative branch label\n    A->>B: y\n  end";
    let mut rig = Rig::mount(Size::new(90, 20), || MermaidView::new(src));
    let screen = rig.rows().join("\n");
    assert!(
        screen.contains("a much much longer alternative branch label"),
        "the else label is whole:\n{screen}"
    );
    assert!(!screen.contains('…'), "nothing truncated:\n{screen}");
}

#[test]
fn an_empty_sequence_diagram_says_so() {
    let mut rig = Rig::mount(Size::new(60, 4), || MermaidView::new("sequenceDiagram"));
    let rows = rig.rows();
    assert!(rows.iter().any(|r| r.contains("empty diagram")), "{rows:?}");
}

/// Four deep: every frame keeps all four of its corners, and each sits
/// strictly inside its parent. Insets that saturate with depth make
/// two frames share a border and read as one broken box.
#[test]
fn frames_stay_nested_four_deep() {
    let src = "sequenceDiagram\n  opt a\n    opt b\n      opt c\n        opt d\n          A->>B: m\n        end\n      end\n    end\n  end";
    let mut rig = Rig::mount(Size::new(80, 30), || MermaidView::new(src));
    let rows = rig.rows();
    let screen = rows.join("\n");
    // Two participant boxes + four frames, corner for corner.
    for (corner, what) in [
        ('╭', "top-left"),
        ('╮', "top-right"),
        ('╰', "bottom-left"),
        ('╯', "bottom-right"),
    ] {
        assert_eq!(
            rig.count_char(corner),
            6,
            "{what} corners missing (2 boxes + 4 frames):\n{screen}"
        );
    }
    // Each frame opens strictly right of the one outside it.
    let opens: Vec<usize> = rows
        .iter()
        .filter(|r| r.contains("opt:"))
        .map(|r| r.find('╭').expect("a frame row opens with a corner"))
        .collect();
    assert_eq!(opens.len(), 4, "four frames: {opens:?}");
    for pair in opens.windows(2) {
        assert!(pair[1] > pair[0], "frames must inset: {opens:?}\n{screen}");
    }
}

/// Layout is linear in nesting, not quadratic: deriving each frame's
/// extent by re-walking its subtree cost 51 ms at 1600 blocks — a
/// dropped frame — where a bottom-up pass costs microseconds.
#[test]
fn deep_nesting_plans_quickly() {
    let depth = 60;
    let src = format!(
        "sequenceDiagram\n  participant A\n  participant B\n{}\n  A->>B: x\n{}",
        "  opt x\n".repeat(depth),
        "  end\n".repeat(depth)
    );
    let start = std::time::Instant::now();
    let mut rig = Rig::mount(Size::new(200, 250), || MermaidView::new(&src));
    let _ = rig.rows();
    let elapsed = start.elapsed();
    assert!(
        elapsed < std::time::Duration::from_millis(250),
        "{depth} nested blocks took {elapsed:?} to plan and draw"
    );
}
