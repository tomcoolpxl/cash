# Jobs: Ctrl-Z stops a program, bg and fg resume it, Ctrl-C ends it with 130, and
# kill %1 ends a background job. (The second scene uses python because it answers the
# Ctrl-Break that kill %1 sends a background job at once; ping and PowerShell ignore it
# and are terminated five seconds later, which is a long wait in a clip.)
Output jobs.gif
Require cash
Set Width 1000
Set Height 560
Source "_settings.tape"

Type "cmd /c 'ping -n 100 127.0.0.1 > nul'"
Enter
Sleep 1800ms
Ctrl+Z
Sleep 1500ms
Type "jobs"
Enter
Sleep 1200ms
Type "bg"
Enter
Sleep 1200ms
Type "jobs"
Enter
Sleep 1200ms
Type "fg"
Enter
Sleep 1500ms
Ctrl+C
Sleep 1200ms
Type "echo $?"
Enter
Sleep 1500ms
Type "python -c 'import time; time.sleep(100)' &"
Enter
Sleep 2500ms
Type "kill %1"
Enter
Sleep 3000ms
Type "jobs"
Enter
Sleep 2500ms
