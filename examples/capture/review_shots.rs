//! The design-review sweep: every runnable example (root + extension
//! crates) captured in two or three staged states — initial, then after
//! a few meaningful keys — so a human (or a docs wave) can review the
//! ACTUAL renderings without babysitting 24 interactive apps.
//!
//! Written by `cargo run --example capture -- review` into
//! `untracked/review-shots/` (gitignored): these are working artifacts
//! for pixel review, NOT published documentation stills — wall-clock
//! frame pacing and machine-dependent capability lines are acceptable
//! here and deliberately kept out of `docs/captures/`.
//!
//! Script conventions (see `Shot` in main.rs):
//! - the first step's delay covers boot + capability probes;
//! - every script ends with `\x03` (Ctrl+C, the engine's default quit)
//!   because the captured frame is the LAST one painted — a plain `q`
//!   would type into composers and a modal Esc would close the very
//!   state being staged;
//! - `splash` is the one self-exiting example: no quit byte, and it
//!   opts back into splashing with `ABSTRACTTUI_NO_SPLASH=0` (the
//!   harness disables splash for everyone else).

use crate::Shot;

/// Force-enable the splash for the one shot that IS the splash.
const SPLASH_ON: (&str, &str) = ("ABSTRACTTUI_NO_SPLASH", "0");

pub const REVIEW_SHOTS: &[Shot] = &[
    // ------------------------------------------------------ 1 · hello
    Shot {
        name: "hello--initial",
        example: "hello",
        cols: 80,
        rows: 24,
        steps: &[(1800, "\x03")],
        env: &[],
    },
    Shot {
        name: "hello--counted",
        example: "hello",
        cols: 80,
        rows: 24,
        steps: &[(1800, "   "), (600, "\x03")],
        env: &[],
    },
    // ---------------------------------------------------- 2 · widgets
    Shot {
        name: "widgets--interactive",
        example: "widgets",
        cols: 110,
        rows: 32,
        steps: &[(1800, "\x03")],
        env: &[],
    },
    Shot {
        name: "widgets--typed",
        example: "widgets",
        cols: 110,
        rows: 32,
        // Tab past the button pair into the text input, then type.
        steps: &[(1800, "\t\t\t"), (400, "Ada"), (600, "\x03")],
        env: &[],
    },
    Shot {
        name: "widgets--visual",
        example: "widgets",
        cols: 110,
        rows: 32,
        // Keyboard route: Tab focuses the strip first, Right moves to
        // the "visual" tab (a blind mouse click proved too brittle for
        // a scripted capture).
        steps: &[(1800, "\t"), (300, "\x1b[C"), (700, "\x03")],
        env: &[],
    },
    // ------------------------------------------------- 2 · components
    Shot {
        name: "components--initial",
        example: "components",
        cols: 100,
        rows: 30,
        steps: &[(1800, "\x03")],
        env: &[],
    },
    Shot {
        name: "components--edited",
        example: "components",
        cols: 100,
        rows: 30,
        steps: &[(1800, "\t"), (300, "Ada"), (600, "\x03")],
        env: &[],
    },
    // ---------------------------------------------------- 2 · gallery
    Shot {
        name: "gallery--initial",
        example: "gallery",
        cols: 112,
        rows: 38,
        steps: &[(2000, "\x03")],
        env: &[],
    },
    Shot {
        name: "gallery--restyled",
        example: "gallery",
        cols: 112,
        rows: 38,
        steps: &[(2000, "t"), (700, "\x03")],
        env: &[],
    },
    // ----------------------------------------------------- 2 · themes
    Shot {
        name: "themes--initial",
        example: "themes",
        cols: 110,
        rows: 30,
        steps: &[(1800, "\x03")],
        env: &[],
    },
    Shot {
        name: "themes--browsing",
        example: "themes",
        cols: 110,
        rows: 30,
        steps: &[(1800, "\x1b[C\x1b[C\x1b[B"), (600, "\x03")],
        env: &[],
    },
    Shot {
        name: "themes--applied",
        example: "themes",
        cols: 110,
        rows: 30,
        steps: &[(1800, "\x1b[C\x1b[C\x1b[B"), (400, "\r"), (700, "\x03")],
        env: &[],
    },
    // ------------------------------------------------------- 3 · grid
    Shot {
        name: "grid--fr",
        example: "grid",
        cols: 110,
        rows: 30,
        steps: &[(1800, "\x03")],
        env: &[],
    },
    Shot {
        name: "grid--fixed-fr",
        example: "grid",
        cols: 110,
        rows: 30,
        steps: &[(1800, "g"), (600, "\x03")],
        env: &[],
    },
    Shot {
        name: "grid--percent",
        example: "grid",
        cols: 110,
        rows: 30,
        steps: &[(1800, "gg"), (600, "\x03")],
        env: &[],
    },
    // --------------------------------------------------- 4 · activate
    Shot {
        name: "activate--initial",
        example: "activate",
        cols: 100,
        rows: 30,
        steps: &[(1800, "\x03")],
        env: &[],
    },
    Shot {
        name: "activate--list-commit",
        example: "activate",
        cols: 100,
        rows: 30,
        steps: &[(1800, "\x1b[B\x1b[B"), (300, "\r"), (600, "\x03")],
        env: &[],
    },
    Shot {
        name: "activate--table",
        example: "activate",
        cols: 100,
        rows: 30,
        steps: &[
            (1800, "\t"),
            (300, "\x1b[B\x1b[B"),
            (300, "\r"),
            (600, "\x03"),
        ],
        env: &[],
    },
    // ----------------------------------------------------- 4 · decide
    Shot {
        name: "decide--initial",
        example: "decide",
        cols: 100,
        rows: 30,
        steps: &[(1800, "\x03")],
        env: &[],
    },
    Shot {
        name: "decide--confirm",
        example: "decide",
        cols: 100,
        rows: 30,
        steps: &[(1800, "1"), (700, "\x03")],
        env: &[],
    },
    Shot {
        name: "decide--multi",
        example: "decide",
        cols: 100,
        rows: 30,
        steps: &[
            (1800, "2"),
            (400, "\x1b[B "),
            (300, "\x1b[B "),
            (600, "\x03"),
        ],
        env: &[],
    },
    // ------------------------------------------------------- 5 · feed
    Shot {
        name: "feed--tailing",
        example: "feed",
        cols: 100,
        rows: 30,
        steps: &[(3000, "\x03")],
        env: &[],
    },
    Shot {
        name: "feed--scrolled",
        example: "feed",
        cols: 100,
        rows: 30,
        steps: &[
            (3000, " "),
            (400, "\x1b[A\x1b[A\x1b[A\x1b[A"),
            (600, "\x03"),
        ],
        env: &[],
    },
    // ------------------------------------------------- 5 · transcript
    Shot {
        name: "transcript--streaming",
        example: "transcript",
        cols: 100,
        rows: 30,
        steps: &[(2500, "\x03")],
        env: &[],
    },
    Shot {
        name: "transcript--completion",
        example: "transcript",
        cols: 100,
        rows: 30,
        steps: &[(3500, "/th"), (700, "\x03")],
        env: &[],
    },
    Shot {
        name: "transcript--table",
        example: "transcript",
        cols: 100,
        rows: 30,
        // Long settle: the fourth scripted answer streams the table.
        steps: &[(12000, "\x03")],
        env: &[],
    },
    // ----------------------------------------------------- 5 · reader
    Shot {
        name: "reader--top",
        example: "reader",
        cols: 100,
        rows: 30,
        steps: &[(2000, "\x03")],
        env: &[],
    },
    Shot {
        name: "reader--search",
        example: "reader",
        cols: 100,
        rows: 30,
        steps: &[(2000, "/table"), (400, "\r"), (600, "\x03")],
        env: &[],
    },
    Shot {
        name: "reader--toc",
        example: "reader",
        cols: 100,
        rows: 30,
        steps: &[(2000, "t"), (600, "\x03")],
        env: &[],
    },
    // ------------------------------------------------- 5 · voice_mock
    Shot {
        name: "voice--idle",
        example: "voice_mock",
        cols: 100,
        rows: 30,
        steps: &[(1800, "\x03")],
        env: &[],
    },
    Shot {
        name: "voice--talking",
        example: "voice_mock",
        cols: 100,
        rows: 30,
        // Legacy pty = latch mode: space toggles talk on; capture mid-synth.
        steps: &[(1800, " "), (1200, "\x03")],
        env: &[],
    },
    // ------------------------------------------------------ 6 · shell
    Shot {
        name: "shell--overview",
        example: "shell",
        cols: 100,
        rows: 30,
        steps: &[(1800, "\x03")],
        env: &[],
    },
    Shot {
        name: "shell--reader-page",
        example: "shell",
        cols: 100,
        rows: 30,
        steps: &[(1800, "2"), (600, "\x03")],
        env: &[],
    },
    Shot {
        name: "shell--inspector",
        example: "shell",
        cols: 100,
        rows: 30,
        steps: &[(1800, "i"), (700, "\x03")],
        env: &[],
    },
    // ---------------------------------------------------- 6 · drawers
    Shot {
        name: "drawers--page",
        example: "drawers",
        cols: 100,
        rows: 30,
        steps: &[(1800, "\x03")],
        env: &[],
    },
    Shot {
        name: "drawers--inspector",
        example: "drawers",
        cols: 100,
        rows: 30,
        steps: &[(1800, "i"), (700, "\x03")],
        env: &[],
    },
    Shot {
        name: "drawers--nav",
        example: "drawers",
        cols: 100,
        rows: 30,
        steps: &[(1800, "g"), (700, "\x03")],
        env: &[],
    },
    // -------------------------------------------------- 6 · dashboard
    Shot {
        name: "dashboard--main",
        example: "dashboard",
        cols: 120,
        rows: 35,
        steps: &[(3500, "\x03")],
        env: &[crate::FIXED_CLOCK],
    },
    Shot {
        name: "dashboard--help",
        example: "dashboard",
        cols: 120,
        rows: 35,
        steps: &[(3500, "?"), (700, "\x03")],
        env: &[crate::FIXED_CLOCK],
    },
    Shot {
        name: "dashboard--sorted-toast",
        example: "dashboard",
        cols: 120,
        rows: 35,
        steps: &[(3500, "s"), (300, "n"), (700, "\x03")],
        env: &[crate::FIXED_CLOCK],
    },
    // ----------------------------------------------------- 7 · images
    Shot {
        name: "images--mosaics",
        example: "images",
        cols: 100,
        rows: 30,
        steps: &[(1800, "\x03")],
        env: &[],
    },
    Shot {
        name: "images--dithered",
        example: "images",
        cols: 100,
        rows: 30,
        steps: &[(1800, "d"), (700, "\x03")],
        env: &[],
    },
    Shot {
        name: "images--protocol",
        example: "images",
        cols: 100,
        rows: 30,
        steps: &[(1800, "p"), (700, "\x03")],
        env: &[],
    },
    // ---------------------------------------------------- 7 · effects
    Shot {
        name: "effects--initial",
        example: "effects",
        cols: 100,
        rows: 30,
        steps: &[(2500, "\x03")],
        env: &[],
    },
    Shot {
        name: "effects--mode",
        example: "effects",
        cols: 100,
        rows: 30,
        steps: &[(2500, "m"), (700, "\x03")],
        env: &[],
    },
    Shot {
        name: "effects--toast",
        example: "effects",
        cols: 100,
        rows: 30,
        steps: &[(2500, "n"), (500, "\x03")],
        env: &[],
    },
    // ----------------------------------------------------- 7 · splash
    Shot {
        name: "splash--final",
        example: "splash",
        cols: 100,
        rows: 30,
        // Self-exiting (~2.5 s hard cutoff): no quit byte, just outlive it.
        steps: &[(3000, "")],
        env: &[SPLASH_ON],
    },
    // --------------------------------------------------- 7 · viewer3d
    Shot {
        name: "viewer3d--initial",
        example: "viewer3d",
        cols: 100,
        rows: 30,
        steps: &[(2500, "\x03")],
        env: &[],
    },
    Shot {
        name: "viewer3d--braille-lit",
        example: "viewer3d",
        cols: 100,
        rows: 30,
        steps: &[(2500, "4"), (400, "ll"), (700, "\x03")],
        env: &[],
    },
    // ------------------------------------------- 8 · graph extensions
    Shot {
        name: "workflow--initial",
        example: "workflow",
        cols: 100,
        rows: 30,
        steps: &[(2000, "\x03")],
        env: &[],
    },
    Shot {
        name: "workflow--walk",
        example: "workflow",
        cols: 100,
        rows: 30,
        steps: &[(2000, "\t"), (300, "\r"), (300, "\x1b[C"), (600, "\x03")],
        env: &[],
    },
    Shot {
        name: "network--initial",
        example: "network",
        cols: 100,
        rows: 30,
        steps: &[(2000, "\x03")],
        env: &[],
    },
    Shot {
        name: "network--selected",
        example: "network",
        cols: 100,
        rows: 30,
        steps: &[(2000, "\t\r"), (300, "\x1b[C"), (600, "\x03")],
        env: &[],
    },
    // ---------------------------------------------------- 8 · mermaid
    Shot {
        name: "mermaid--flow-td",
        example: "mermaid",
        cols: 100,
        rows: 30,
        steps: &[(2000, "\x03")],
        env: &[],
    },
    Shot {
        name: "mermaid--sequence",
        example: "mermaid",
        cols: 100,
        rows: 30,
        steps: &[(2000, "\x1b[C\x1b[C"), (600, "\x03")],
        env: &[],
    },
    Shot {
        name: "mermaid--gantt-fallback",
        example: "mermaid",
        cols: 100,
        rows: 30,
        steps: &[(2000, "\x1b[C\x1b[C\x1b[C"), (600, "\x03")],
        env: &[],
    },
    // ------------------------------------------- 9 · testing + capture
    Shot {
        name: "screenshot--initial",
        example: "screenshot",
        cols: 80,
        rows: 24,
        steps: &[(1500, "\x03")],
        env: &[],
    },
    Shot {
        name: "screenshot--captured",
        example: "screenshot",
        cols: 80,
        rows: 24,
        steps: &[(1500, "s"), (700, "\x03")],
        env: &[],
    },
    Shot {
        name: "caps--report",
        example: "caps",
        cols: 100,
        rows: 30,
        steps: &[(2500, "\x03")],
        env: &[],
    },
];
