---
name: pstree
summary: Show processes as a tree of parents and children.
kind: Processes and network
see: ps top kill
draft: yes
---

## Examples

    pstree -p $$          # this shell and what it started

## How it works

`pstree` draws Windows processes as a tree of parents and children, from every root or
from one pid. `-p` adds each process's id.

## Different in cash

Windows records a process's parent only as an id at its start, and reuses ids: a process
whose parent has ended appears as a root of its own, and an old process can appear under
a newer one that took its parent's id.
