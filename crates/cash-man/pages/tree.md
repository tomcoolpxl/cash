---
name: tree
summary: Show a directory hierarchy as a tree.
kind: Text and file tools
see: ls find
draft: yes
---

## How it works

`tree [DIRECTORY]` draws a directory hierarchy: `-a` with dot files, `-d` directories only, `-L
LEVEL` to a depth, `-f` with full paths, `--dirsfirst`, `--noreport`.

## Different in cash

Windows' own `tree.com` draws directories only, with other options; inside cash, `tree` is
this one. Links and junctions are shown and not followed, so a junction cannot loop the
walk or lead it outside the directory. `ls --tree` draws a tree with `ls`'s colours and
icons.
