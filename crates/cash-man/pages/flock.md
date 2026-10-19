---
name: flock
summary: Run a command, or hold a descriptor, under a lock on a file, so scripts take turns.
kind: Processes and network
see: exec mktemp
---

## Syntax

    flock [OPTION]... FILE COMMAND [ARG]...
    flock [OPTION]... FILE -c STRING
    flock [OPTION]... DESCRIPTOR

    flock build.lock make         one make at a time
    flock -n q.lock ./drain.sh    fail at once if another holds it
    flock 9                       lock what descriptor 9 has open

## Examples

A command under a lock, and a string run through the shell:

    flock build.lock echo "building"
    flock build.lock -c 'echo "one at a time"'

A lock held on a descriptor for part of a script:

<!-- parse only: Git Bash's flock cannot lock a descriptor -->

    exec 9> run.lock
    flock -n 9 && echo "got the lock"
    flock -u 9 && echo "let it go"
    exec 9>&-

## How it works

`flock FILE COMMAND...` holds a lock on `FILE` while `COMMAND` runs, so two scripts that
lock the same file take turns; `flock FILE -c 'STRING'` runs the string through the
shell. `flock N` locks the shell's descriptor `N` for as long as it stays open, and
`flock -u N` releases it. util-linux's options: `-s` for a shared lock (`-x`, the
default, is exclusive), `-n` to fail at once rather than wait, `-w SECS` to wait that
long, `-E CODE` for the status when the lock is not got (1 by default), and `--verbose`.
The command's own status is returned otherwise.

## Options

    -x, --exclusive        An exclusive lock: the default.
    -s, --shared           A shared lock.
    -n, --nonblock         Fail at once rather than wait.
    -w, --timeout=SECS     Wait at most SECS.
    -E, --conflict-exit-code=N    The status without the lock: 1.
    -u, --unlock           Release a descriptor's lock.
    -c, --command=STRING   Run STRING through the shell.
    -o, --close            Accepted; see below.
    -F, --no-fork          Accepted; there is no fork.
    --verbose              Say how long it took.

Exit status: the command's; with `-n` or `-w`, 1 (or `-E`'s) when the lock was not got.

## Different in cash

Windows has no `flock`. The lock is a byte-range lock on one byte far past any content
the file could have, so it stops nothing but another `flock`: `cat` and writers of the
lock file go on. A lock belongs to the handle it was taken on, and goes when that handle
is closed: in the command form, when the command returns; in the descriptor form, when
the descriptor is closed, by `exec N>&-` or the end of the subshell that opened it. So a
command that starts something in the background and exits (`flock x.lock start app`, a
service launcher) releases the lock while what it started still runs.

A directory cannot be locked; lock a file inside it. `-o` closes nothing, since there is
no fork and the command never sees the handle. `-c` runs its string in a copy of this
shell rather than in `$SHELL`. `--start` and `--length` lock the bytes named instead,
and a byte one handle has locked cannot be read or written through another.

A wait tries the lock every 25 ms, and Ctrl-C ends it as it ends a program.
