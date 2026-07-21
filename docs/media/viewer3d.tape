# VHS tape — software-rasterized 3D: the helmet GLB turning in the terminal.
# Render: vhs docs/media/viewer3d.tape   (run from the repo root)
Output docs/media/viewer3d.gif

Set Shell "bash"
Set FontSize 18
Set Width 1320
Set Height 860
Set Padding 18
Set Framerate 24
Set Theme "TokyoNight"

Hide
Type "unset NO_COLOR; export COLORTERM=truecolor TERM=xterm-256color; clear"
Enter
Type "./target/release/examples/viewer3d"
Enter
Sleep 1.5s
Show

# The turntable spins by default — let it turn in half-block color.
Sleep 4s
# Cycle mosaic render modes: half-block -> quadrant -> sextant -> braille.
Type "2"
Sleep 2.5s
Type "3"
Sleep 2.5s
Type "4"
Sleep 3s
Type "1"
Sleep 3s

Hide
Type "q"
Sleep 500ms
