---
see: fc keys vars
spec: D44
---
## Description

`history` lists the command history; `history N` the last N, `-c` clears the list, `-d
N` deletes an entry, `-w` writes the file, `-s LINE` adds a line.

## Windows notes

- The file is `~/.cash_history` unless `HISTFILE` says otherwise, so it does not mix
  with Git Bash's `.bash_history`.
- Each command is appended to the file as it runs, not when the shell exits: a cash
  killed from Task Manager loses nothing, and several Windows Terminal tabs append to
  one file safely (D44).
- While `HISTSIZE` and `HISTFILESIZE` are unset, nothing is cut: Bash would cut both to
  500.
- To clear it: `history -c; history -w`.
