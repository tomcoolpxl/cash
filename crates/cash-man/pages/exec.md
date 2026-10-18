---
name: exec
summary: Replace the shell with a command, or change its file descriptors.
kind: Bash builtins
see: differences
draft: yes
---

## How it works

`exec COMMAND` replaces the shell with a command; `exec` with only redirections
(`exec >log 2>&1`, `exec 3<file`) changes the shell's own file descriptors.

## Different in cash

Windows cannot replace a running program with another, so `exec COMMAND` runs the
command and then exits the shell with its status: nothing after it runs, and `$?`
reaches whoever started cash. The program has a pid of its own, and cash stays its
parent while it runs.

`exec bash` and `exec sh` start cash, not Git's or WSL's bash. `exec -a NAME` and
`exec -l` are refused for any program but cash: Windows lets no one choose another
program's `argv[0]`.
