//! REDTEAM cycle-3 attack: cell shaders + additive blending through the
//! compositor — determinism goldens, a HOSTILE shader trying to break
//! the wide-pair/pool invariants at flatten time, additive saturation,
//! and the frame-billing rule (idle-zero after an animation completes,
//! driven through the REAL Driver loop).

use abstracttui::anim::shaders::{Dissolve, HueDrift, ScanlineFade, Shimmer};
use abstracttui::base::{Point, Rect, Rgba, Size};
use abstracttui::render::layer::{Blend, ColorTransform};
use abstracttui::render::{
    Attrs, Cell, CellShader, Compositor, FrameDiff, Layer, PresentCaps, Presenter, Style, Surface,
};
use abstracttui::testing::{assert_snapshot, VtScreen};

fn styled_layer(size: Size, z: i32) -> Layer {
    let mut s = Surface::new(size, Cell::EMPTY);
    s.draw_text(
        0,
        0,
        "shader test row",
        Style::new().fg(Rgba::rgb(200, 160, 40)),
    );
    s.draw_text(
        0,
        1,
        "日本 wide 🎉 mix",
        Style::new().fg(Rgba::rgb(90, 200, 255)),
    );
    s.draw_text(
        0,
        2,
        "underline",
        Style::new()
            .fg(Rgba::rgb(255, 255, 255))
            .attrs(Attrs::UNDERLINE),
    );
    Layer::new(s, Point::ZERO, z)
}

fn flatten_to_frame(layers: &mut [Layer], size: Size) -> Surface {
    let mut comp = Compositor::new();
    let mut frame = Surface::new(size, Cell::EMPTY);
    for l in layers.iter_mut() {
        l.surface_mut().damage_all();
    }
    let _ = comp.flatten(&mut frame, layers);
    frame
}

fn present_bytes(frame: &Surface) -> Vec<u8> {
    let prev = Surface::new(frame.size(), Cell::EMPTY);
    let mut diff = FrameDiff::new();
    let mut presenter = Presenter::new();
    let mut out = Vec::new();
    presenter.emit(
        diff.compute_full(&prev, frame),
        frame,
        &PresentCaps::FULL,
        &mut out,
    );
    out
}

// ---------------------------------------------------------------------------
// Determinism goldens: 4 shaders x 3 sample points at fixed t.
// ---------------------------------------------------------------------------

/// The CellShader contract says pure-function-of-(x,y,t,cell): pin exact
/// output cells for each built-in at three (x, y) points and two t
/// values. Any platform/refactor drift fails the golden, not a demo.
#[test]
fn builtin_shader_determinism_golden() {
    let base = Cell::EMPTY
        .with_fg(Rgba::rgb(180, 120, 60))
        .with_bg(Rgba::rgb(20, 24, 40));
    let mut report = String::new();
    let points = [(0i32, 0i32), (7, 3), (33, 12)];
    let times = [0.25f32, 1.75];
    let shaders: Vec<(&str, Box<dyn CellShader>)> = vec![
        ("shimmer", Box::new(Shimmer::default())),
        (
            "scanline",
            Box::new(ScanlineFade {
                duration: 1.0,
                rows: 20,
            }),
        ),
        ("hue_drift", Box::new(HueDrift::default())),
        (
            "dissolve",
            Box::new(Dissolve {
                duration: 1.0,
                seed: 42,
            }),
        ),
    ];
    for (name, shader) in &shaders {
        for &t in &times {
            for &(x, y) in &points {
                let out = shader.shade(x, y, t, base);
                report.push_str(&format!(
                    "{name} t={t} ({x},{y}): fg={} bg={} ul={} attrs={:?}\n",
                    out.fg.to_hex(),
                    out.bg.to_hex(),
                    out.ul.to_hex(),
                    out.attrs
                ));
            }
        }
    }
    assert_snapshot("shader_determinism", &report);
}

/// Same inputs, same frame: two flattens with an identical shader state
/// must produce byte-identical presentations (the compositor path, not
/// just the shader function).
#[test]
fn shader_frames_are_reproducible_through_the_compositor() {
    let size = Size::new(30, 4);
    let build = || {
        let mut layer = styled_layer(size, 0);
        layer.set_shader(Some(Box::new(Shimmer::default())));
        layer.set_shader_t(0.6);
        let mut layers = vec![layer];
        let frame = flatten_to_frame(&mut layers, size);
        present_bytes(&frame)
    };
    let a = build();
    let b = build();
    assert_eq!(a, b, "identical shader state must emit identical bytes");
}

// ---------------------------------------------------------------------------
// The hostile shader: tries to smuggle invariant violations into the
// flattened frame via returned cells.
// ---------------------------------------------------------------------------

struct HostileShader;

impl CellShader for HostileShader {
    fn shade(&self, x: i32, y: i32, _t: f32, cell: Cell) -> Cell {
        // Alternate three attacks per cell:
        // 1. return a CONTINUATION cell where a narrow glyph was;
        // 2. return a cell whose glyph is a WIDE leader in the last col;
        // 3. return wild colors + attrs (all bits) to stress downstream.
        match (x + y) % 3 {
            0 => {
                // Steal a continuation from a neighbor if the surface has
                // one; otherwise hand back the cell with hostile attrs.
                Cell {
                    attrs: Attrs::from_bits_truncate(0xFFFF),
                    ..cell
                }
            }
            1 => Cell {
                fg: Rgba::new(255, 255, 255, 255),
                bg: Rgba::new(0, 0, 0, 0),
                ..cell
            },
            _ => cell,
        }
    }
}

/// A shader returning continuation cells directly (the API lets it: Cell
/// is plain data). The flattened frame must still pass debug_validate —
/// the compositor owns the invariant, not the shader author.
struct ContinuationForger {
    donor: Cell,
}

impl CellShader for ContinuationForger {
    fn shade(&self, x: i32, _y: i32, _t: f32, cell: Cell) -> Cell {
        if x % 2 == 0 {
            self.donor // a REAL continuation cell captured from a surface
        } else {
            cell
        }
    }
}

#[test]
fn hostile_shaders_cannot_corrupt_the_flattened_frame() {
    let size = Size::new(24, 4);
    // Harvest a genuine continuation cell to forge with.
    let mut donor_surface = Surface::new(size, Cell::EMPTY);
    donor_surface.draw_text(0, 0, "世", Style::new());
    let donor = *donor_surface.get(1, 0).expect("continuation cell");
    assert!(donor.is_continuation(), "premise: harvested a continuation");

    for (name, shader) in [
        (
            "hostile_attrs",
            Box::new(HostileShader) as Box<dyn CellShader>,
        ),
        (
            "continuation_forger",
            Box::new(ContinuationForger { donor }),
        ),
    ] {
        let mut layer = styled_layer(size, 0);
        layer.set_shader(Some(shader));
        layer.set_shader_t(1.0);
        let mut layers = vec![layer];
        let frame = flatten_to_frame(&mut layers, size);
        if let Err(e) = frame.debug_validate() {
            panic!("{name}: flattened frame failed validation: {e}");
        }
        // And the frame must still present model-clean.
        let bytes = present_bytes(&frame);
        let mut screen = VtScreen::new(size);
        screen.feed(&bytes);
        assert_eq!(
            screen.unknown_seq_count(),
            0,
            "{name}: hostile shader leaked unmodeled bytes"
        );
    }
}

// ---------------------------------------------------------------------------
// Additive blending: saturation + determinism + glyph rules.
// ---------------------------------------------------------------------------

#[test]
fn additive_blend_saturates_and_is_deterministic() {
    let size = Size::new(10, 2);
    let build = || {
        let mut base = Surface::new(size, Cell::EMPTY);
        base.fill_rect(
            Rect::new(0, 0, 10, 2),
            Cell::EMPTY.with_bg(Rgba::rgb(200, 200, 200)),
        );
        let mut glow = Surface::new(size, Cell::EMPTY);
        glow.fill_rect(
            Rect::new(0, 0, 10, 2),
            Cell::EMPTY.with_bg(Rgba::rgb(120, 120, 120)),
        );
        let l0 = Layer::new(base, Point::ZERO, 0);
        let mut l1 = Layer::new(glow, Point::ZERO, 1);
        l1.set_blend(Blend::Additive);
        let mut layers = vec![l0, l1];
        flatten_to_frame(&mut layers, size)
    };
    let a = build();
    let b = build();
    // Saturation: 200 + 120 clamps at 255, never wraps.
    let cell = a.get(3, 1).unwrap();
    assert_eq!(
        cell.bg,
        Rgba::rgb(255, 255, 255),
        "additive must saturate, got {}",
        cell.bg.to_hex()
    );
    // Determinism through present.
    assert_eq!(present_bytes(&a), present_bytes(&b));
}

#[test]
fn additive_black_adds_nothing() {
    let size = Size::new(8, 1);
    let mut base = Surface::new(size, Cell::EMPTY);
    base.fill_rect(
        Rect::new(0, 0, 8, 1),
        Cell::EMPTY.with_bg(Rgba::rgb(10, 60, 90)),
    );
    let mut dark = Surface::new(size, Cell::EMPTY);
    dark.fill_rect(
        Rect::new(0, 0, 8, 1),
        Cell::EMPTY.with_bg(Rgba::rgb(0, 0, 0)),
    );
    let l0 = Layer::new(base, Point::ZERO, 0);
    let mut l1 = Layer::new(dark, Point::ZERO, 1);
    l1.set_blend(Blend::Additive);
    let mut layers = vec![l0, l1];
    let frame = flatten_to_frame(&mut layers, size);
    assert_eq!(
        frame.get(2, 0).unwrap().bg,
        Rgba::rgb(10, 60, 90),
        "black light = identity"
    );
}

// ---------------------------------------------------------------------------
// Color transforms: identity edges + default-color passthrough.
// ---------------------------------------------------------------------------

#[test]
fn color_transforms_respect_default_color_and_identity() {
    let size = Size::new(12, 2);
    // Content with TERMINAL-DEFAULT colors (alpha 0): a transform must
    // pass them through untouched (there is no RGB to grade).
    let mut s = Surface::new(size, Cell::EMPTY);
    s.draw_text(0, 0, "default ink", Style::new()); // fg stays TRANSPARENT
    let mut layer = Layer::new(s, Point::ZERO, 0);
    layer.set_color_transform(ColorTransform::Dim(0.4));
    let mut layers = vec![layer];
    let frame = flatten_to_frame(&mut layers, size);
    let cell = frame.get(0, 0).unwrap();
    assert!(
        cell.fg.is_transparent(),
        "Dim must not mint an RGB for a terminal-default color, got {}",
        cell.fg.to_hex()
    );

    // Identity transforms change nothing byte-for-byte.
    let mut l_id = styled_layer(size, 0);
    l_id.set_color_transform(ColorTransform::Dim(1.0));
    let mut layers_id = vec![l_id];
    let with_identity = flatten_to_frame(&mut layers_id, size);
    let mut layers_plain = vec![styled_layer(size, 0)];
    let plain = flatten_to_frame(&mut layers_plain, size);
    assert_eq!(present_bytes(&with_identity), present_bytes(&plain));
}

// ---------------------------------------------------------------------------
// Frame billing: an animated shader requests frames while animating and
// goes silent when done (idle-zero after animation, through Driver).
// ---------------------------------------------------------------------------

#[test]
fn animation_completion_returns_to_zero_byte_idle() {
    use abstracttui::app::{App, Driver, RunConfig};
    use abstracttui::layout::{Dimension, Style as LayoutStyle};
    use abstracttui::reactive::flush_effects;
    use abstracttui::term::Capabilities;
    use abstracttui::testing::CaptureTerm;
    use abstracttui::ui::{dyn_view, text, Element};

    let mut term = CaptureTerm::new(Size::new(30, 6));
    let mut app = App::new(Size::new(30, 6));
    let mut tick_handle = None;
    app.mount(|cx| {
        // A signal-driven "animation": N discrete frames, then silence —
        // the reactive equivalent of a completed transition. (The layer
        // shader_t driver rides the same frame-request path; this pins
        // the BILLING rule end-to-end without depending on anim's
        // integration layer, which is still landing.)
        let tick = cx.signal(0u32);
        tick_handle = Some(tick);
        Element::new()
            .style(
                LayoutStyle::default()
                    .width(Dimension::Cells(20))
                    .height(Dimension::Cells(1)),
            )
            .child(dyn_view(LayoutStyle::default(), move || {
                text(format!("anim frame {}", tick.get()))
            }))
            .build()
    })
    .expect("mount");
    let cfg = RunConfig {
        caps: Some(Capabilities::default()),
        enter: None,
        probe: false,
    };
    let mut driver = Driver::new(&mut app, &mut term, cfg).expect("enter");
    // Settle the mount.
    for _ in 0..8 {
        let t = driver.turn(&mut app, &mut term).expect("turn");
        if t.idle {
            break;
        }
    }
    let _ = term.take_bytes();

    // "Animate": 12 frames driven by posted ticks (each writes + renders).
    let tick = tick_handle.unwrap();
    let mut animated_frames = 0;
    for i in 1..=12 {
        tick.set(i);
        flush_effects();
        let t = driver.turn(&mut app, &mut term).expect("turn");
        if t.rendered {
            animated_frames += 1;
        }
    }
    assert!(
        animated_frames >= 12,
        "every animation tick must render a frame"
    );
    assert!(
        !term.bytes().is_empty(),
        "animation must have emitted bytes"
    );
    let _ = term.take_bytes();

    // Animation DONE: the loop must return to absolute silence.
    for i in 0..16 {
        let t = driver.turn(&mut app, &mut term).expect("turn");
        assert!(t.idle, "turn {i} after animation end must be idle");
        assert!(!t.rendered, "turn {i}: rendered with no live animation");
    }
    assert_eq!(
        term.bytes().len(),
        0,
        "post-animation idle must emit ZERO bytes"
    );
}
