//! Headless visual fixture: cargo run --example avatar_catalog -- /tmp/catalog.png error
//! Uses the real terminal adapter, layout and VT cell renderer, without a Gateway.
//! This static preview supplies an explicit sample envelope of 0.6.
use abstracttui::{app::Driver, prelude::*, testing::CaptureTerm};
fn main() -> abstracttui::base::Result<()> {
    let args: Vec<_> = std::env::args().collect();
    let path = args
        .get(1)
        .map(String::as_str)
        .unwrap_or("avatar-catalog.png");
    let pose = AvatarPose::ALL
        .into_iter()
        .find(|p| {
            p.label()
                .eq_ignore_ascii_case(args.get(2).map(String::as_str).unwrap_or("ready"))
        })
        .unwrap_or(AvatarPose::Ready);
    abstracttui::app::set_theme_by_id("abstract-dark");
    // The designs a user may choose from; experimental designs are not shown.
    let styles = avatar_ready_styles();
    let rows = styles.len().div_ceil(3);
    let size = Size::new(114, (rows * 21) as i32);
    let mut app = App::new(size);
    app.mount(move |cx| {
        let pose = cx.signal(pose);
        let mut root = Element::new().style(LayoutStyle::column().gap(1));
        for choices in styles.chunks(3) {
            let mut row =
                Element::new().style(LayoutStyle::row().height(Dimension::Cells(20)).gap(2));
            for &style in choices {
                row = row.child(
                    Element::new()
                        .style(LayoutStyle::column().width(Dimension::Cells(36)))
                        .child(text(style.label()))
                        .child(
                            Avatar::new(pose)
                                .style(style)
                                .level(cx.signal(0.6))
                                .animated(cx.signal(false))
                                .layout(
                                    LayoutStyle::default()
                                        .width(Dimension::Cells(36))
                                        .height(Dimension::Cells(19)),
                                )
                                .view(cx),
                        )
                        .build(),
                );
            }
            root = root.child(row.build());
        }
        root.build()
    })?;
    let mut term = CaptureTerm::new(size);
    let mut driver = Driver::new(
        &mut app,
        &mut term,
        RunConfig {
            probe: false,
            caps: Some(abstracttui::term::Capabilities::with(|c| {
                c.truecolor = true;
                c.colors_256 = true;
                c.unicode_ok = true;
            })),
            ..Default::default()
        },
    )?;
    driver.turn(&mut app, &mut term)?;
    term.screen()
        .screenshot()
        .write_png(path)
        .expect("write avatar catalog capture");
    println!("Wrote {path}");
    Ok(())
}
