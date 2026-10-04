---
see: type where command hash
spec: D8 D58
---
## Description

`which NAME` says what cash would run for the name: an alias, a function, a builtin or
a program on `PATH`, in the order cash looks for them (D8). `-a` lists every match, and
`-p` searches `PATH` only.

## Windows notes

Many of cash's builtins are programs elsewhere (`ls`, `sed`, `ps`). For those, `which`
prints a path that cash can run, cash's own executable with the name appended:
`C:/.../cash.exe/ls`. No file is there, but a script that does `LS=$(which ls); "$LS"
-la` works, as do `exec`, `xargs` and `find -exec` with it (D58). A program outside cash
cannot run that path. Bash's own builtins, which are no program anywhere, still say
`cd: shell builtin`.

## Examples

```
which -a sort         # the builtin, then each sort.exe on PATH
which -p python
```
