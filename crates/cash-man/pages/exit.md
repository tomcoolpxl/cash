---
name: exit
summary: Exit the shell.
kind: Bash builtins
see: return trap set
---

## Syntax

    exit [N]

    exit        exit with the last command's status
    exit 0      exit, saying it went well
    exit 1      exit, saying it failed

## Examples

A script's status, as its caller sees it:

    bash -c 'exit 3'; echo "status $?"

Exit when something is missing, with a message on standard error:

    check() { [ -e "$1" ] || { echo "no $1" >&2; exit 2; }; }
    ( check notes.txt; echo 'notes.txt is there' )
    ( check nosuch 2>/dev/null ); echo "status $?"

The `EXIT` trap runs on the way out:

    ( trap 'echo cleaning up' EXIT; exit 0 )

In a subshell, `exit` ends the subshell only:

    ( exit 4 ); echo "the subshell gave $?, the shell goes on"

## How it works

`exit N` ends the shell with status N, a number from 0 to 255: 0 says it went well,
anything else that it did not. Without N, the status is the last command's. A script
that runs to its end exits with its last command's status.

The `EXIT` trap runs first. In a subshell (`( ... )`, a pipeline's command, `$(...)`)
`exit` ends the subshell, not the shell around it. In a function, `return` leaves the
function, where `exit` would end the whole script.

A number above 255 is taken modulo 256: `exit 256` is 0.

Exit status: N.
