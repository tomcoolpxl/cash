---
name: mkfifo
summary: Make a named pipe; refused, with the way to a pipe in cash.
kind: Coreutils written for Windows
see: exec job-control
---

## Syntax

    mkfifo [-m MODE] NAME...

    mkfifo NAME         refused on Windows: see below for what to write

## Examples

Instead of a FIFO, a process substitution, which hands a command the other's output as
a path:

    printf 'b\na\n' > one.txt; printf 'a\nc\n' > two.txt
    diff <(sort one.txt) <(sort two.txt)

A descriptor that reads a producer, and a writer handed a reader's path:

<!-- parse only: names programs of the reader's own -->

    exec 3< <(producer)
    tee >(gzip > out.gz) < input

## How it works

`mkfifo NAME` makes a named pipe with a file name: on Linux a FIFO is a file that one
process writes and another reads. Windows has named pipes, but they live under
`\\.\pipe\` and are not files a script can make by a path of its own and open later, so
every `NAME` is refused with status 1, with a message that says named pipes are not
files on Windows and to use a process substitution, `<(…)` or `>(…)`.

Where a script would make a FIFO, write to it in one command and read it in another,
a process substitution hands the reader the writer's output as a path, or the other
way round.

## Options

    -m, --mode=MODE    Accepted; changes nothing.
    -Z, --context      Accepted; changes nothing.

Exit status: 1: every NAME is refused.

## Different in cash

- Windows' named pipes are not files, so `mkfifo` makes none; `<(...)` and `>(...)`
  do what scripts make FIFOs for.
