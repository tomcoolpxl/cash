---
name: ulimit
summary: Show resource limits, as far as Windows has them.
kind: Bash builtins
see: differences
---

## Syntax

    ulimit [-HS] [-a | -fmnstuv [LIMIT]]

    ulimit -a          every limit
    ulimit -n          how many files may be open
    ulimit -n 4096     ask for that many

## Examples

What scripts ask, and the line many have:

<!-- parse only: Git Bash reports MSYS2's own limits -->

    ulimit -n
    ulimit -n 4096 || echo "could not raise it"
    ulimit -a

## How it works

`ulimit` shows or sets limits on what the shell and the commands it starts may use:
`-n` open files, `-u` processes, `-f` the size of a file written, `-s` the stack, `-t`
processor seconds, `-v` virtual memory, `-m` memory in use. `-a` shows them all. A
LIMIT is a number or `unlimited`. `-H` is the hard limit, the ceiling, and `-S` the
soft one, in force; without either, a setting changes both.

## Options

    -a    Show every limit.
    -H    The hard limit.
    -S    The soft limit.
    -f    The size of a file written, in blocks of 1024 bytes.
    -m    Memory in use, in kilobytes.
    -n    Open files.
    -s    The stack, in kilobytes.
    -t    Processor time, in seconds.
    -u    Processes.
    -v    Virtual memory, in kilobytes.

Exit status: 0, or not 0 for a LIMIT that is not a number.

## Different in cash

Windows has no per-process limits of this kind; its job objects limit other things, per
job. So `ulimit` reports `unlimited`, which is true of what scripts ask about (there is
no cap on open files), and accepts a setting without applying it, so the common `ulimit
-n 4096` line does not stop a script under `set -e`. Background jobs and subshells are
limited to 256 at once, or `CASH_MAX_SUBSHELLS`.
