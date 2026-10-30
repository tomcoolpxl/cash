# The prompt: a suggestion from history, a command that does not exist turns red,
# highlighting as you type, and the Ctrl-R picker.
Output prompt.gif
Require cash
Set Width 1000
Set Height 560
Source "_settings.tape"

Type@120ms "gre"
Sleep 1200ms
Right
Sleep 600ms
Enter
Sleep 1200ms
Type@100ms "gti status"
Sleep 1500ms
Ctrl+U
Sleep 300ms
Type@80ms "grep ERROR logs/app.log | awk '{ print $2 }' | sort -u"
Sleep 1200ms
Enter
Sleep 1500ms
Ctrl+R
Sleep 1200ms
Type@150ms "uniq"
Sleep 1200ms
Enter
Sleep 1000ms
Enter
Sleep 2500ms
