//! Local procedural avatar preview. No framework, audio, assets or network.
//! 1–7 pose · s style · space animation · +/- level · h visibility · q quit.
use abstracttui::prelude::*;
fn main() -> abstracttui::base::Result<()> {
    if !abstracttui::term::have_tty() {
        println!("avatar: run in an interactive terminal");
        return Ok(());
    }
    let mut app = App::new(Size::new(80, 28));
    let quit = app.quitter();
    app.mount(move|cx| {
        let pose=cx.signal(AvatarPose::Ready);let style=cx.signal(AvatarStyle::Halo);
        let level=cx.signal(0.5f32);let animated=cx.signal(true);let active=cx.signal(true);
        let mut root=Element::new().style(LayoutStyle::column().gap(1).padding(Edges::all(1)))
            .shortcut(KeyChord::plain(Key::Char('q')),move |_|quit.quit())
            .shortcut(KeyChord::plain(Key::Char('s')),move |_|style.set(if style.get_untracked()==AvatarStyle::Halo{AvatarStyle::Hover}else{AvatarStyle::Halo}))
            .shortcut(KeyChord::plain(Key::Char(' ')),move |_|animated.update(|x|*x = !*x))
            .shortcut(KeyChord::plain(Key::Char('h')),move |_|active.update(|x|*x = !*x))
            .shortcut(KeyChord::plain(Key::Char('+')),move |_|level.update(|x|*x=(*x+0.1).min(1.0)))
            .shortcut(KeyChord::plain(Key::Char('-')),move |_|level.update(|x|*x=(*x-0.1).max(0.0)));
        for (key,value) in [('1',AvatarPose::Ready),('2',AvatarPose::Thinking),('3',AvatarPose::Working),('4',AvatarPose::Listening),('5',AvatarPose::Speaking),('6',AvatarPose::Waiting),('7',AvatarPose::Error)]{
            root=root.shortcut(KeyChord::plain(Key::Char(key)),move |_|pose.set(value));
        }
        root.child(text("Procedural avatar · 1–7 pose · s style · space animation · +/- level · h hide · q quit"))
            .child(Avatar::new(pose).style_signal(style).level(level).active(active).animated(animated).layout(LayoutStyle::default().width(Dimension::Cells(32)).height(Dimension::Cells(17))).view(cx))
            .child(dyn_view(LayoutStyle::line(1),move ||text(format!("{:?} · {:?} · level {:.1} · animation {}",style.get(),pose.get(),level.get(),animated.get())))).build()
    })?;
    app.run()
}
