---
see: exec mktemp
---
## Description

`flock FILE COMMAND...` holds a lock on `FILE` while `COMMAND` runs, so two scripts that
lock the same file take turns; `flock FILE -c 'STRING'` runs the string through the
shell. `flock N` locks the shell's descriptor `N` for as long as it stays open, and
`flock -u N` releases it. util-linux's options: `-s` for a shared lock (`-x`, the
default, is exclusive), `-n` to fail at once rather than wait, `-w SECS` to wait that
long, `-E CODE` for the status when the lock is not got (1 by default), and `--verbose`.
The command's own status is returned otherwise.

## Windows notes

Windows has no `flock`. The lock is a byte-range lock on one byte far past any content
the file could have, so it stops nothing but another `flock`: `cat` and writers of the
lock file go on. A lock belongs to the handle it was taken on, and goes when that handle
is closed: in the command form, when the command returns; in the descriptor form, when
the descriptor is closed, by `exec N>&-` or the end of the subshell that opened it.

A directory cannot be locked; lock a file inside it. `-o` closes nothing, since there is
no fork and the command never sees the handle. `-c` runs its string in a copy of this
shell rather than in `$SHELL`. `--start` and `--length` lock the bytes named instead,
and a byte one handle has locked cannot be read or written through another.

A wait tries the lock every 25 ms, and Ctrl-C ends it as it ends a program.

## Examples

```
flock /tmp/build.lock make                 # one build at a time
flock -n -E 75 queue.lock ./drain.sh       # 75 if another drain is running
flock -w 10 -s state.db cat state.db       # wait up to 10 s for a shared lock
(
  flock -n 9 || exit 1                     # the lock lives on descriptor 9
  echo "$$" > pid
) 9> run.lock                              # and is released here
```
