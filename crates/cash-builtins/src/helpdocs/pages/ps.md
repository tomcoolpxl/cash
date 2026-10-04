---
see: pstree top pgrep kill job-control
spec: D48
---
## Description

`ps` lists Windows processes with their native ids, the ids `kill`, `$$`, `$!` and
`$PPID` use. A bare `ps` lists the shell's own descendants: Windows has no controlling
terminal to filter by. `ps -e` lists every process, `ps -ef` in full format, `ps -efj`
with the parent and Windows' base priority, and `ps aux` as BSD's does.

## Windows notes

The `ps` in Git for Windows lists only MSYS processes, and its ids are MSYS pids that
`kill` cannot use; this one lists every native process. Not there: `ps -o` formats, and
another process's full command line, which lives in that process's memory.

## Examples

```
ps -ef | grep node
top -b -n 1 -o mem       # sorted by memory, once
```
