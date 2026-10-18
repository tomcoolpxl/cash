---
name: source
summary: Run a file's commands in the current shell.
kind: Bash builtins
see: . config
---

## Syntax

    source FILE [ARG]...
    . FILE [ARG]...

    source FILE         run FILE's commands here, not in a new shell
    . FILE              the same, POSIX's spelling
    source FILE A B     with $1 and $2 set to A and B while it runs

## Examples

Variables and functions a file defines stay in the shell that sources it:

    printf '%s\n' 'GREETING=hello' \
        'greet() { echo "$GREETING, $1"; }' > lib.sh
    source lib.sh
    greet world

Running the file instead leaves nothing behind:

    printf '%s\n' 'SET_BY_FILE=yes' > vars.sh
    bash vars.sh 2>/dev/null; echo "after running: [${SET_BY_FILE-}]"
    . ./vars.sh; echo "after sourcing: [$SET_BY_FILE]"

Arguments, as `$1` and on:

    printf '%s\n' 'echo "args: $*"' > args.sh
    source args.sh one two

A file's `return` ends it, not the shell:

    printf '%s\n' 'echo before' 'return 3' 'echo never' > early.sh
    source early.sh; echo "status $?"

## How it works

`source FILE` reads FILE and runs its commands in the current shell, as if they were
typed: its variables, functions, aliases, `cd` and `set` options stay after it ends.
Running the file (`bash FILE`, `./FILE`) starts a new shell, whose changes go with it.
That is why `~/.bashrc` is sourced, and why a script that sets up an environment
(`activate`, `env.sh`) is run as `source env.sh`.

A FILE without a `/` is looked for in the directories of `PATH`, then in the current
directory; `./FILE` names the one here. With ARGs, `$1`, `$2`, ... are theirs while
the file runs, and the shell's own come back after.

`return` in the file ends it, with a status; the status of `source` is that of the
last command the file ran.

## Options

    -p PATH    Look for FILE in PATH, not in $PATH.

Exit status: the last command's in FILE; 1 when FILE cannot be read.

## Different in cash

- A FILE with CRLF line ends is read as if it had LF ones, so a script saved by a
  Windows editor runs (see `help crlf`).
- A FILE may be spelled `C:/x/env.sh`, `/c/x/env.sh` or `"C:\x\env.sh"`.
