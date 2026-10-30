# Directories: back and forward through where you have been (Alt-Left and Alt-Right at
# a real keyboard; here Ctrl-O and Ctrl-], bound to the same builtins in demo.bashrc,
# since VHS cannot press Alt with an arrow), the z jump, and Alt-E's picker, twice: to go
# to a directory, and to insert a path after a command. The comment lines are typed so
# that the viewer can read which key is pressed.
Output directories.gif
Require cash
Set Width 1000
Set Height 560
Source "_settings.tape"

Type "cd src"
Enter
Sleep 500ms
Type "cd ../logs"
Enter
Sleep 500ms
Type "cd ../../notes"
Enter
Sleep 500ms
Type "cd ../photos"
Enter
Sleep 700ms
Type "# Alt-Left, three times, then Alt-Right"
Enter
Sleep 700ms
Ctrl+O
Sleep 800ms
Ctrl+O
Sleep 800ms
Ctrl+O
Sleep 800ms
Ctrl+]
Sleep 900ms
Type "pwd"
Enter
Sleep 1200ms
Type "z photos"
Enter
Sleep 400ms
Type "pwd"
Enter
Sleep 1500ms
Type "cd .."
Enter
Sleep 400ms
Type "# Alt-E: pick a directory, Enter goes there"
Enter
Sleep 600ms
Alt+e
Sleep 2000ms
Type@150ms "src"
Sleep 1200ms
Enter
Sleep 700ms
Type "pwd"
Enter
Sleep 1500ms
Type "cd ../.."
Enter
Sleep 400ms
Type "# Alt-E after a command: Enter inserts a path, Esc closes"
Enter
Sleep 600ms
Type "ls -l "
Alt+e
Sleep 1800ms
Type@150ms "logs"
Sleep 1000ms
Enter
Sleep 900ms
Escape
Sleep 400ms
Escape
Sleep 700ms
Enter
Sleep 2500ms
