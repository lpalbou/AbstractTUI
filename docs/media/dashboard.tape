# VHS tape — the flagship dashboard (curves, progress bars, log, table, toast, 3D mark).
# Render: vhs docs/media/dashboard.tape   (run from the repo root)
Output docs/media/dashboard.gif

Set Shell "bash"
Set FontSize 20
Set Width 1480
Set Height 840
Set Padding 18
Set Framerate 24
Set Theme "TokyoNight"

Hide
Type "unset NO_COLOR; export COLORTERM=truecolor TERM=xterm-256color; clear"
Enter
Type "./target/release/examples/dashboard"
Enter
Sleep 1.2s
Show

# Let the rx/tx line chart, sparklines and progress cluster animate.
Sleep 2.5s
# Raise a toast notification.
Type "n"
Sleep 1.5s
# Sort the sessions table.
Type "s"
Sleep 1.2s
Type "s"
Sleep 1.2s
# Bring up the spinning 3D mark panel (mosaic-rendered helmet-style mark).
Type "b"
Sleep 3s
# Open the focus-trapped help modal, then close it.
Type "?"
Sleep 1.8s
Escape
Sleep 1s

Hide
Type "q"
Sleep 500ms
