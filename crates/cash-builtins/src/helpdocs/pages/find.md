---
see: xargs where ls tree paths
spec: D48 D3 D16 D6
---
## Description

`find [PATH...] [EXPRESSION]` walks folder trees and acts on the files the expression
selects: `-name`, `-iname`, `-path`, `-type f|d|l`, `-size`, `-mtime`, `-newer`,
`-empty`, `-maxdepth`, `-mindepth`, `-prune`, with `-a`, `-o`, `!` and parentheses; and
`-print`, `-print0`, `-delete`, `-exec`, `-execdir`, `-ok`, `-quit` as actions.

## Windows notes

- On a clean Windows machine, `find` is `C:\Windows\System32\find.exe`, a different
  command that searches for text inside files; a script's `find . -name '*.log'` meets it
  with `FIND: Parameter format not correct`. Inside cash, `find` is this one.
- Paths print as `C:/src/a.txt` or as you spelled the start (`./a.txt`), never with
  backslashes, so `find | xargs` keeps them whole (D3).
- `-name` and `-path` ignore case, as the file system does (D16).
- What `-exec` runs is in the job, as every command is (D6).
- Anything not implemented (`-regex`, `-printf`, ...) is refused by name rather than
  ignored: a `find` that silently drops part of the expression returns the wrong files.

## Examples

```
find . -name '*.log' -mtime +7 -delete
find src -type f -name '*.rs' -exec grep -l TODO {} +
find . -name node_modules -prune -o -name package.json -print
```
