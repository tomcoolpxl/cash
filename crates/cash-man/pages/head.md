---
name: head
summary: Print the first lines of files.
kind: Bundled coreutils, from uutils
see: tail less cat sed
---

## Syntax

    head [OPTION]... [FILE]...

    head FILE            the first 10 lines
    head -n 5 FILE       the first 5 lines
    head -n -5 FILE      all but the last 5 lines
    head -c 100 FILE     the first 100 bytes
    COMMAND | head -3    the first 3 lines of a command's output

## Examples

The first lines of a file, ten by default:

    head app.log

A number of your own:

    head -n 3 app.log
    head -3 app.log

Everything but the last lines:

    head -n -22 app.log

The first bytes:

    head -c 10 notes.txt; echo

The first lines of what a command prints:

    seq 100 | head -n 4

Several files, each under its name:

    head -n 1 notes.txt pairs.txt

## How it works

`head` writes the first part of each FILE, or of its standard input without one: ten
lines unless `-n` or `-c` says otherwise. With a `-` before the number, `-n -K` writes
all but the last K lines, and `-c -K` all but the last K bytes.

With more than one FILE, each is written under a line `==> FILE <==`, with an empty
line between them; `-q` leaves the names out and `-v` writes one even for a single
file.

In a pipeline, `head` stops reading once it has written what it was asked for, and the
command before it ends with nothing more to write to: `yes | head -n 2` ends.

## Options

    -n, --lines=[-]K     The first K lines; with a -, all but the last K.
    -c, --bytes=[-]K     The first K bytes; with a -, all but the last K.
                         K may end in b (512), K (1024), M, G, ... or kB
                         (1000), MB, ...
    -q, --quiet          Never write the names of the files.
    -v, --verbose        Always write them.
    -z, --zero-terminated  Lines end in a NUL byte, not a newline.
    -NUM                 The same as -n NUM.

Exit status: 0 when every file was read; 1 when one could not be.

## Advanced

### A file shared with what comes after

`{ head -n 1; cat; } < file` writes the first line with `head` and the rest with
`cat`: `head` leaves a file under its standard input just after what it wrote, as GNU's
does.

### The middle of a file

`head -n 20 file | tail -n 5` writes lines 16 to 20; `sed -n '16,20p' file` does the
same in one command.

## Different in cash

- It is cash's own copy of uutils' `head`, carried inside cash, so it is the same on
  every machine, with or without Git or MSYS2.
- `head` counts lines by `\n` alone: a CRLF file's lines keep their `\r`, as on Linux.
