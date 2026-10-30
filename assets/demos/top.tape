# top: the procps overview with meters, a meter for each core (1, two to a row), sorted
# by memory, then as a tree.
Output top.gif
Require cash
Set Width 1000
Set Height 560
Source "_settings.tape"

Type "top"
Enter
Sleep 3000ms
Type "1"
Sleep 4000ms
Type "1"
Sleep 1500ms
Type "M"
Sleep 2800ms
Type "V"
Sleep 2800ms
Type "q"
Sleep 1000ms
