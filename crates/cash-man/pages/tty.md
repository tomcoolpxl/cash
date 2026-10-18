---
name: tty
summary: Print the name of the terminal on standard input.
kind: Coreutils written for Windows
see: test stty
---

## Syntax

    tty [-s]

    tty       the terminal standard input is, or "not a tty"
    tty -s    nothing: the status says whether it is a terminal

## Examples

Behave one way at a terminal, another in a pipe:

    if tty -s; then echo "at a terminal"; else echo "not a terminal"; fi
    echo text | tty

## How it works

`tty` prints the name of the terminal standard input is connected to, and exits 0; when
it is a file or a pipe, it prints `not a tty` and exits 1. `-s` prints nothing, for a
test. `[ -t 0 ]` asks the same of descriptor 0, and `[ -t 1 ]` of standard output.

## Options

    -s, --silent, --quiet    Print nothing; only the status says.

Exit status: 0 for a terminal; 1 for anything else; 2 for a bad option.

## Different in cash

- Windows has no `/dev/pts/N` for each terminal: at a console, `tty` prints `/dev/tty`,
  which cash opens as the console in a redirection, where Git Bash prints the MSYS2
  name, `/dev/pty0`.
