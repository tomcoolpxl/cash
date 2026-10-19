---
name: where
summary: Find files by pattern on `PATH` or in directories, as Windows' `where.exe` does.
kind: Windows commands
see: which type find paths
---

## Syntax

    where [-r DIR] [-q] [-f] [-t] PATTERN...

    where git               every git on PATH, in order
    where -r . '*.toml'     every .toml below here
    where '$GOPATH:*.exe'   the programs in the directories GOPATH names

## Examples

A program on PATH, and files below a directory:

<!-- parse only: Git Bash runs Windows' where.exe, which writes backslashes -->

    where git
    where -r . '*.toml'
    where -q nosuch || echo "nosuch is nowhere"

## How it works

`where PATTERN...` finds files as Windows' `where.exe` does: in the current directory and
along `PATH`, trying the `PATHEXT` extensions on a name without one, with `*` and `?`
wildcards, in any case. `-r DIR` searches a directory tree instead, and `$VAR:PATTERN` or
`DIR:PATTERN` searches the directories a variable or a path names.

### Differences from where.exe

- Options take dashes: `-r`, `-q`, `-f`, `-t`, bundled as `-qf`, and `--help`. A `/q`
  is a pattern, so a path is never taken for an option.
- Paths print as `pwd` prints them: `C:/Windows/notepad.exe`.
- Search order, matching, messages and exit statuses (0 found, 1 not found, 2 for a bad
  command line) are `where.exe`'s.

`which` says what the shell would run for a name; `where` lists every file of that
name.
## Options

    -r, --recursive DIR    Search DIR and every directory below it.
    -q, --quiet            Print nothing: the status says.
    -f, --quote            Each name in double quotes.
    -t, --times            The size and last change of each.

Exit status: 0 when a file was found; 1 when none was; 2 for a bad command line.

