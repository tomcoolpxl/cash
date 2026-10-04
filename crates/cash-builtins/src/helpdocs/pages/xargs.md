---
see: find
spec: D48 D32 D52
---
## Description

`xargs [COMMAND...]` reads items from standard input and runs the command with them as
arguments, `echo` by default: `-n COUNT` per run, `-I TOKEN` one item per run in place
of the token, `-0` for NUL-separated items (`find -print0`), `-r` to run nothing on empty
input, `-t` to show each command.

## Windows notes

Windows has no `xargs`; the one in Git for Windows' MSYS tree is the only one most
machines have.

- The command is looked up as cash looks it up: builtins (`echo`, `rm`, `sed`) first,
  each run in its own copy of the shell, then `PATH`. A path runs that program.
- A Windows program gets its arguments quoted as it will split them, and an MSYS2
  program as Cygwin splits them (D32, D52).
- As in GNU xargs, a backslash in the input escapes the next character, so feed it
  paths spelled `C:/src`, as cash's tools print them, not `C:\src`.
- `-P` is accepted and the commands run one at a time.

## Examples

```
find . -name '*.tmp' -print0 | xargs -0 rm -f
git ls-files '*.sh' | xargs dos2unix
```
