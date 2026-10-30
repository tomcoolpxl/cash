# Paths: C:\, C:/ and /c/ all work, pwd prints one spelling, Windows programs get theirs.
Output paths.gif
Require cash
Source "_settings.tape"

Type "pwd"
Enter
Sleep 1200ms
Type "cd C:\Users\Public\cash-demo\project\src"
Enter
Sleep 300ms
Type "pwd"
Enter
Sleep 1500ms
Type "cd /c/Users/Public/cash-demo/project/logs"
Enter
Sleep 300ms
Type "pwd"
Enter
Sleep 1500ms
Type "ls /c/Users/Public/cash-demo/project/src/*.rs"
Enter
Sleep 1800ms
Type "cmd /c cd"
Enter
Sleep 1500ms
Type "shopt -s unixpaths; pwd"
Enter
Sleep 2500ms
