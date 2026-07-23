//! Parser robustness: no input panics (the decode_image marker-soup
//! house style). Byte soup, token soup, and truncation sweeps over
//! real sources — parse never panics; views build and draw for
//! truncated real sources.

use abstracttui::base::Size;
use abstracttui::reactive::create_root;
use abstracttui::ui::{BufferCanvas, UiTree};
use abstracttui_mermaid::{parse, MermaidView};

/// splitmix64 (std-only, deterministic soup).
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
}

#[test]
fn byte_soup_never_panics() {
    let mut rng = Rng(0xF00D);
    for _ in 0..3000 {
        let len = (rng.next() % 200) as usize;
        let bytes: Vec<u8> = (0..len).map(|_| (rng.next() % 256) as u8).collect();
        let s = String::from_utf8_lossy(&bytes);
        let _ = parse(&s);
    }
}

#[test]
fn token_soup_never_panics() {
    const VOCAB: [&str; 24] = [
        "graph",
        "flowchart",
        "sequenceDiagram",
        "stateDiagram-v2",
        "TD",
        "LR",
        "-->",
        "---",
        "-.->",
        "==>",
        "->>",
        "-->>",
        "[",
        "]",
        "(",
        ")",
        "{",
        "}",
        "|",
        ";",
        ":",
        "%%",
        "\"",
        "\n",
    ];
    let mut rng = Rng(0xBEEF);
    for _ in 0..2000 {
        let n = (rng.next() % 40) as usize;
        let mut s = String::new();
        for _ in 0..n {
            s.push_str(VOCAB[(rng.next() % VOCAB.len() as u64) as usize]);
            if rng.next().is_multiple_of(3) {
                s.push(' ');
            }
        }
        let _ = parse(&s);
    }
}

const REAL: [&str; 3] = [
    "flowchart LR\n    A[Christmas] -->|Get money| B(Go shopping)\n    B --> C{Let me think}\n    C -->|One| D[Laptop]",
    "sequenceDiagram\n    participant a as Alice\n    a->>b: hi\n    Note over a,b: both",
    "stateDiagram-v2\n    [*] --> Still\n    Still --> [*]",
];

#[test]
fn truncation_sweep_never_panics() {
    for src in REAL {
        for (i, _) in src.char_indices() {
            let _ = parse(&src[..i]);
        }
        let _ = parse(src);
    }
}

#[test]
fn views_build_and_draw_for_truncated_sources() {
    for src in REAL {
        // Every 7th boundary keeps the sweep cheap while covering
        // header/mid-statement/mid-quote cuts.
        for (i, _) in src.char_indices().step_by(7) {
            let piece = src[..i].to_string();
            let mut tree = UiTree::new(Size::new(60, 18));
            let (_root, ()) = create_root(|cx| {
                let view = MermaidView::new(piece.clone()).view(cx);
                tree.mount(cx, view);
            });
            let mut canvas = BufferCanvas::new(Size::new(60, 18));
            tree.draw(&mut canvas);
        }
    }
}
