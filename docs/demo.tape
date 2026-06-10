# vhs tape for the omnitop demo GIF.
# Record with: vhs docs/demo.tape   (run from the repo root, release binary built)

Output docs/demo.gif

Set Shell zsh
Set FontSize 16
Set Width 1280
Set Height 760
Set Padding 14
Set Theme "Catppuccin Mocha"

# Launch (hide the command itself)
Hide
Type "./target/release/omnitop"
Enter
Sleep 2.5s
Show

# Processes view: sort toggling
Sleep 2s
Type "m"
Sleep 1.5s
Type "m"
Sleep 1.5s
Type "c"
Sleep 1.5s

# Live filter
Type "/"
Sleep 400ms
Type "code"
Sleep 2.5s
Escape
Sleep 1s

# Process tree
Type "t"
Sleep 3s
Type "t"
Sleep 800ms

# Containers tab
Type "2"
Sleep 4s

# Back to processes, then quit
Type "1"
Sleep 1.5s
Type "q"
Sleep 500ms
