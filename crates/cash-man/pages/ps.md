---
name: ps
summary: Show a snapshot of the running processes.
kind: Processes and network
see: pstree top pgrep kill job-control
---

## Syntax

    ps [-efj]
    ps aux

    ps          the processes this shell started, and theirs
    ps -e       every process
    ps -ef      every process, in full: owner, parent, start, command
    ps aux      every process, as BSD's ps lays it out, with memory

## Examples

What runs, and finding one:

<!-- parse only: lists the reader's own processes -->

    ps
    ps -ef | grep node
    ps aux | sort -k4 -rn | head -5

## How it works

`ps` lists Windows processes with their native ids, the ids `kill`, `$$`, `$!` and
`$PPID` use. A bare `ps` lists the shell's own descendants: Windows has no controlling
terminal to filter by. `ps -e` lists every process, `ps -ef` in full format (`UID PID
PPID C STIME TTY TIME CMD`), `ps -efj` with the parent and Windows' base priority, and
`ps aux` as BSD's does, with each process's share of the processor and of memory.

`pgrep` finds processes by name, `pstree` draws them as a tree, and `top` keeps the
list up to date.

## Options

    -e      Every process.
    -f      Full format: UID PID PPID C STIME TTY TIME CMD.
    -j      Jobs format: the parent and Windows' base priority too.
    aux     BSD's layout: every process, with %CPU, %MEM, VSZ and RSS.
    ax      Every process, BSD's short layout.

Exit status: 0, or 1 for an option cash's ps does not have.

## Different in cash

The `ps` in Git for Windows lists only MSYS processes, and its ids are MSYS pids that
`kill` cannot use; this one lists every native process. Not there: `ps -o` formats,
`-p` and the other selectors, and another process's full command line, which lives in
that process's memory.
