---
name: nproc
summary: Print the number of processors available.
kind: Bundled coreutils, from uutils
see: xargs uname
---

## Syntax

    nproc [OPTION]...

    nproc               the processors this shell may use
    nproc --all         all the machine has
    nproc --ignore=1    one fewer, to leave one free

## Examples

At least one, always:

    [ "$(nproc)" -ge 1 ] && echo "at least one processor"
    [ "$(nproc --ignore=99)" -eq 1 ] && echo "never fewer than one"

As many jobs at once as there are processors:

    printf '%s\n' a.md b.md notes.txt | xargs -P "$(nproc)" -n1 wc -l | sort

## How it works

`nproc` prints how many processors the shell may use: logical ones, so a core with two
threads counts twice. `--all` prints all the machine has, `--ignore=N` N fewer, but
never fewer than 1. Set, `OMP_NUM_THREADS` and `OMP_THREAD_LIMIT` lower the answer, as
OpenMP programs read them.

## Options

    --all           All the processors the machine has.
    --ignore=N      N fewer, but at least 1.

Exit status: 0.
