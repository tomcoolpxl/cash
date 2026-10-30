# sextant: the shooter, playing itself. Drawn with the font's own sextant glyphs
# (--unicode) rather than the soft font cash sends a terminal, which xterm.js does not
# take; the clips' font has them (_settings.tape). Twelve frames a second: the whole
# screen changes every frame, and the GIF grows with each one.
Output sextant.gif
Require cash
Set Width 1000
Set Height 560
Set Framerate 12
Source "_settings.tape"

Type "sextant --demo --unicode --fps 12"
Enter
Sleep 12000ms
Escape
Sleep 1500ms
