---
name: stdbuf
summary: Run a command with changed buffering; on Windows, as it is.
kind: Coreutils written for Windows
see: grep nice nohup
draft: yes
---

## Examples

    # grep writes each matching line at once
    stdbuf -oL grep pattern big.log | head -1
    stdbuf -oL find . -name '*.log' | tee found  # each name as it is found
    stdbuf -o0 tail -f app.log | sed 's/^/> /'

## How it works

`stdbuf -oL COMMAND` runs a command with its standard output line buffered, `-o0`
unbuffered, `-i` and `-e` the same for input and error; scripts write it so that a
program's lines reach a pipe as they are made. GNU's does it by preloading a library
into the program, and Windows has no way into another program's buffering.

### What cash does

A builtin of cash's own gathers its output into larger writes, as a C program's standard
output does into a pipe or a file, and writes what it has within 50 ms, so a slow one
still shows its lines as it goes. Under `-oL` or `-o0` it writes each piece as it comes
instead, and so does every builtin it runs (`stdbuf -oL eval '...'`); `grep` is also
given `--line-buffered`. The bundled coreutils (`cat` among them) line-buffer even into
a pipe, so there is nothing to change; `awk` and `sed` fill a buffer into a pipe, as
GNU's do, and `stdbuf` cannot change that for them.

An external program runs as it is too, after one line on standard error:
`cash: stdbuf: cannot change the buffering of an external program on Windows; running
it as is`. `stdbuf` then exits with the program's status.

The options and messages are coreutils': a bad mode or a missing command is status 125,
a command that cannot be found 127.
