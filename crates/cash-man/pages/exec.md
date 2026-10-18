---
name: exec
summary: Replace the shell with a command, or change its file descriptors.
kind: Bash builtins
see: differences command
---

## Syntax

    exec [-cl] [-a NAME] [COMMAND [ARG...]]
    exec REDIRECTION...

    exec prog ARGS     run prog in the shell's place: the shell ends with it
    exec >log 2>&1     the rest of the script's output into log
    exec 3<FILE        open FILE for reading on descriptor 3
    exec 3<&-          close descriptor 3

## Examples

A command as the last thing a script does:

    echo "starting"
    exec echo "the last command"
    echo "never printed"

The rest of a script's output into a file:

    exec >run.log
    echo "first line"
    echo "second line"

A file read on a descriptor of its own, a line at a time:

    exec 3<pairs.txt
    read -r first <&3
    read -r second <&3
    exec 3<&-
    echo "$first / $second"

A log written on its own descriptor:

    exec 4>>steps.log
    echo "started" >&4
    echo "done" >&4
    exec 4>&-
    cat steps.log

A command with nothing of the shell's environment:

    export GREETING=hi
    (exec -c printenv GREETING) || echo "GREETING was not passed on"

## How it works

`exec COMMAND` runs a command in the shell's place: what follows `exec` in the script
never runs, and the command's status is the script's. In a subshell, `( exec ... )`,
only the subshell ends.

`exec` with only redirections changes the shell's own file descriptors for the rest of
the script: `exec >log 2>&1` sends everything after it to `log`, `exec 3<file` opens a
file to read from with `<&3`, `exec 4>>file` one to append to with `>&4`, and `exec
3<&-` closes one.

## Options

    -c        Run COMMAND with an empty environment.
    -l        Put a dash before COMMAND's name, as a login shell is started.
    -a NAME   Pass NAME to COMMAND as its name, its $0.

Exit status: with a command, the shell ends with its status: 127 when it is not found,
126 when it cannot run. With redirections only, 0, or 1 when one fails.

## Different in cash

Windows cannot replace a running program with another, so `exec COMMAND` runs the
command and then exits the shell with its status: nothing after it runs, and `$?`
reaches whoever started cash. The program has a pid of its own, and cash stays its
parent while it runs.

`exec bash` and `exec sh` start cash, not Git's or WSL's bash. `exec -a NAME` and
`exec -l` are refused for any program but cash: Windows lets no one choose another
program's `argv[0]`.
