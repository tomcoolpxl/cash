---
name: hash
summary: Show or change the remembered locations of commands.
kind: Bash builtins
see: type command which
---

## Syntax

    hash [-lr] [-p PATH] [-dt] [NAME...]

    hash              the programs remembered, and how often each ran
    hash NAME         look NAME up on PATH now, and remember it
    hash -r           forget them all: after a program moved
    hash -t NAME      where NAME was found
    hash -d NAME      forget NAME
    hash -p P NAME    remember P as NAME, without looking

## Examples

Remembered, and forgotten:

    git --version > /dev/null
    hash -t git > /dev/null && echo "git is remembered"
    hash -d git
    hash -t git 2>/dev/null || echo "and forgotten"

A name for a program of your choosing:

    hash -p /usr/bin/env myenv
    hash -t myenv

## How it works

The shell looks a program up on `PATH` the first time it runs it, and remembers where
it found it; `hash` lists them with how many times each ran. Setting `PATH` forgets
them all, as `hash -r` does: run it after a program moved, or the shell looks where it
was. `hash NAME` looks NAME up now; `-p PATH NAME` remembers PATH as NAME without
looking; `-t` prints where NAME was found, `-d` forgets it, and `-l` lists them as
`hash -p` commands that would remember them again.

## Options

    -d    Forget each NAME.
    -l    List as hash -p commands.
    -p    Remember PATH as NAME.
    -r    Forget them all.
    -t    Print where each NAME was found.

Exit status: 0, or 1 when a NAME is not found.
