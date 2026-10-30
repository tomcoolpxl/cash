# A bash script with find, xargs, grep, sed and awk runs on a bare machine: every tool
# is inside cash.exe.
Output script.gif
Require cash
Set Width 1000
Set Height 560
Source "_settings.tape"

Type "cat scripts/report.sh"
Enter
Sleep 3500ms
Type "clear"
Enter
Type "./scripts/report.sh"
Enter
Sleep 3000ms
Type "type find xargs grep sed awk sort"
Enter
Sleep 2000ms
Type "echo $BASH_VERSION"
Enter
Sleep 2000ms
