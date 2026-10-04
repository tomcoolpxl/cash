---
see: history keys
spec: D44
---
## Description

`fc` lists, edits and reruns commands from the history: `fc -l` lists the last ones,
`fc` opens the last command in an editor and runs what you save, `fc -s old=new` reruns
the last command with a substitution. The editor is `-e EDITOR`, else `$FCEDIT`, else
`$EDITOR`, else `vi`.

## Windows notes

Windows' `fc.exe` compares files; inside cash, `fc` is this builtin. To compare files,
use `diff` or `cmp` from Git for Windows, or `"$SYSTEMROOT/System32/fc.exe"`.

Ctrl-X Ctrl-E at the prompt edits the current line the same way, with `$VISUAL` first.
