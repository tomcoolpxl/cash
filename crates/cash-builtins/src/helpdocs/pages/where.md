---
see: which type find paths
---
## Description

`where PATTERN...` finds files as Windows' `where.exe` does: in the current folder and
along `PATH`, trying the `PATHEXT` extensions on a name without one, with `*` and `?`
wildcards, in any case. `-r DIR` searches a folder tree instead, and `$VAR:PATTERN` or
`DIR:PATTERN` searches the folders a variable or a path names.

## Differences from where.exe

- Options take dashes: `-r`, `-q`, `-f`, `-t`, bundled as `-qf`, and `--help`. A `/q`
  is a pattern, so a path is never taken for an option.
- Paths print as `pwd` prints them: `C:/Windows/notepad.exe`.
- Search order, matching, messages and exit statuses (0 found, 1 not found, 2 for a bad
  command line) are `where.exe`'s.

`which` says what the shell would run for a name; `where` lists every file of that
name.

## Examples

```
where git
where -r . '*.toml'
where '$GOPATH:*.exe'
```
