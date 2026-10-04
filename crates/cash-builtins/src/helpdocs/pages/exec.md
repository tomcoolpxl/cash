---
see: differences
spec: D7
---
## Description

`exec COMMAND` replaces the shell with a command; `exec` with only redirections
(`exec >log 2>&1`, `exec 3<file`) changes the shell's own file descriptors.

## Windows notes

Windows cannot replace a running program with another, so `exec COMMAND` runs the
command and then exits the shell with its status: nothing after it runs, and `$?`
reaches whoever started cash. The program has a pid of its own, and cash stays its
parent while it runs.

`exec bash` and `exec sh` start cash, not Git's or WSL's bash (D7). `exec -a NAME` and
`exec -l` are refused for any program but cash: Windows lets no one choose another
program's `argv[0]`.
