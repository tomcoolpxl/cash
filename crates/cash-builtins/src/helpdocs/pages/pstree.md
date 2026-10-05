---
see: ps top kill
---
## Description

`pstree` draws Windows processes as a tree of parents and children, from every root or
from one pid. `-p` adds each process's id.

## Windows notes

Windows records a process's parent only as an id at its start, and reuses ids: a process
whose parent has ended appears as a root of its own, and an old process can appear under
a newer one that took its parent's id.

## Examples

```
pstree -p $$          # this shell and what it started
```
