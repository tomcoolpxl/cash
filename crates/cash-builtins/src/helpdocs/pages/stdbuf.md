---
see: grep nice nohup
---
## Description

`stdbuf -oL COMMAND` runs a command with its standard output line buffered, `-o0`
unbuffered, `-i` and `-e` the same for input and error; scripts write it so that a
program's lines reach a pipe as they are made. GNU's does it by preloading a library
into the program, and Windows has no way into another program's buffering.

## What cash does

A builtin or bundled tool of cash's own runs as it is: cash's builtins write each line
as they make it, and the bundled coreutils (`cat` among them) line-buffer even into a
pipe, so there is nothing to change. `grep` is given `--line-buffered`, which is what
`-oL` and `-o0` ask of it. `awk` and `sed` fill a buffer into a pipe, as GNU's do, and
`stdbuf` cannot change that for them either.

An external program runs as it is too, after one line on standard error:
`cash: stdbuf: cannot change the buffering of an external program on Windows; running
it as is`. `stdbuf` then exits with the program's status.

The options and messages are coreutils': a bad mode or a missing command is status 125,
a command that cannot be found 127.

## Examples

```
stdbuf -oL grep pattern big.log | head -1   # grep flushes each matching line
stdbuf -o0 tail -f app.log | sed 's/^/> /'
```
