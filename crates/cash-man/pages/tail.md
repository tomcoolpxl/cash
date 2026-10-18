---
name: tail
summary: Print the last lines of files, or follow them as they grow.
kind: Bundled coreutils, from uutils
see: head less cat
---

## Syntax

    tail [OPTION]... [FILE]...

    tail FILE            the last 10 lines
    tail -n 5 FILE       the last 5 lines
    tail -n +5 FILE      from line 5 to the end
    tail -f FILE         the end, and what is added, as it is added
    tail -c 100 FILE     the last 100 bytes

## Examples

The last lines of a file, ten by default:

    tail app.log

A number of your own:

    tail -n 3 app.log
    tail -3 app.log

From a line to the end, here skipping a header line:

    tail -n +2 pairs.txt

The last bytes:

    tail -c 9 notes.txt

The end of what a command prints:

    seq 100 | tail -n 2

Follow a log as it grows, until Ctrl-C:

<!-- parse only: follows the file until Ctrl-C -->
    tail -f app.log

## How it works

`tail` writes the last part of each FILE, or of its standard input without one: ten
lines unless `-n` or `-c` says otherwise. With a `+` before the number, `-n +K` starts
at line K and writes the rest, and `-c +K` starts at byte K.

`-f` goes on after the end, writing what is added to the file as it is added, until
Ctrl-C; `-F` also follows a file that is renamed and made again, as a rotated log is.
`--pid=PID` stops following when the process PID ends.

With more than one FILE, each is written under a line `==> FILE <==`; `-q` leaves the
names out.

## Options

    -n, --lines=[+]K     The last K lines; with a +, from line K on.
    -c, --bytes=[+]K     The last K bytes; with a +, from byte K on.
    -f, --follow[=name|descriptor]
                         Write what is added, as it is added.
    -F                   The same as --follow=name --retry: follow the
                         name, through renames and removals.
    -s, --sleep-interval=N
                         Look for more every N seconds (default 1).
    --pid=PID            Stop following when process PID ends.
    --retry              Keep trying to open a file that is not there.
    -q, --quiet          Never write the names of the files.
    -v, --verbose        Always write them.
    -z, --zero-terminated  Lines end in a NUL byte, not a newline.

Exit status: 0 when every file was read; 1 when one could not be.

## Advanced

### Following a log

`tail -f app.log | grep --line-buffered ERROR` shows the errors as they are written;
`--line-buffered` has `grep` pass each on at once. `tail -F` survives a log that is
rotated (renamed, and a new one started), where `-f` keeps following the renamed one.

### A command's output, kept short

`make 2>&1 | tail -n 20` shows only the end of a long output.

## Different in cash

- It is cash's own copy of uutils' `tail`, carried inside cash, so it is the same on
  every machine, with or without Git or MSYS2.
- `-f` watches the file through Windows' change notifications; `--use-polling` looks
  at it every `-s` seconds instead, for a share whose notifications do not come.
