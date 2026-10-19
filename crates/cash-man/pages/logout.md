---
name: logout
summary: Leave a login shell.
kind: Bash builtins
see: exit
---

## Syntax

    logout [N]

    logout      leave a login shell, as exit does
    logout 3    with status 3

## Examples

In a script, which is no login shell:

    logout 2>/dev/null || echo "not a login shell: use exit"

## How it works

`logout` ends a login shell, the shell a terminal starts with `-l` or as `-bash`, with
status N or the last command's, after running `~/.bash_logout`. In any other shell it
is an error, and the shell goes on: `exit` ends a shell of any kind.

## Options

    N    The status: the last command's without.

Exit status: N, for a login shell; else 1.
