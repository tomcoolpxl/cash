# Paths: C:\, C:/ and /c/ all work, spaces in either spelling, pwd prints one spelling,
# a Windows program gets $PWD as it is, and arguments are never rewritten.
Output paths.gif
Require cash
Set Width 1000
Set Height 560
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
Sleep 2000ms
Type "clear"
Enter
Type "# spaces, in either spelling"
Enter
Type `cd "C:\Program Files"`
Enter
Sleep 300ms
Type "pwd"
Enter
Sleep 1200ms
Type "ls -d /c/Program\ Files/Windows*"
Enter
Sleep 2000ms
Type "# a Windows program gets $PWD as it is"
Enter
Type `powershell -c "ls '$PWD' -Name -Filter Windows* | select -first 3"`
Enter
Sleep 2500ms
Type "# arguments reach a program as typed"
Enter
Type "cmd /c echo --scope=/subscriptions/1/rg/app"
Enter
Sleep 1800ms
Type "shopt -s unixpaths; pwd"
Enter
Sleep 2500ms
