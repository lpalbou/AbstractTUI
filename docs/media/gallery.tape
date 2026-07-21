# VHS tape — the design system on one screen, restyled live across themes.
# Render: vhs docs/media/gallery.tape   (run from the repo root)
Output docs/media/gallery.gif

Set Shell "bash"
Set FontSize 18
Set Width 1600
Set Height 840
Set Padding 18
Set Framerate 24
Set Theme "TokyoNight"

Hide
Type "unset NO_COLOR; export COLORTERM=truecolor TERM=xterm-256color; clear"
Enter
Type "./target/release/examples/gallery"
Enter
Sleep 1.2s
Show

Sleep 1.8s
# Cycle themes — the whole board repaints through one signal.
Type "t"
Sleep 1.3s
Type "t"
Sleep 1.3s
Type "t"
Sleep 1.3s
Type "t"
Sleep 1.3s
Type "t"
Sleep 1.5s
# Move focus across the interactive widgets.
Tab
Sleep 700ms
Tab
Sleep 700ms
Tab
Sleep 1.2s

Hide
Type "q"
Sleep 500ms
