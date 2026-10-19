---
name: pstree
summary: Show processes as a tree of parents and children.
kind: Processes and network
see: ps top kill
---

## Syntax

    pstree [-p] [PID]

    pstree         every process, as a forest of trees
    pstree -p $$   this shell and what it started, with ids

## Examples

This shell's tree, with the ids:

<!-- parse only: Git Bash has no pstree to compare with -->

    pstree -p $$
    pstree | head -20

## How it works

`pstree` draws Windows processes as a tree of parents and children, from every root or
from one pid. `-p` adds each process's id.

## Options

    -p     Each process's id after its name.
    PID    Start at this process, not at every root.

Exit status: 0, or 1 for a PID that is not running.

## Different in cash

Windows records a process's parent only as an id at its start, and reuses ids: a process
whose parent has ended appears as a root of its own, and an old process can appear under
a newer one that took its parent's id.
