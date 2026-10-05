---
see: ls find
---
## Description

`tree [FOLDER]` draws a folder hierarchy: `-a` with dot files, `-d` folders only, `-L
LEVEL` to a depth, `-f` with full paths, `--dirsfirst`, `--noreport`.

## Windows notes

Windows' own `tree.com` draws folders only, with other options; inside cash, `tree` is
this one. Links and junctions are shown and not followed, so a junction cannot loop the
walk or lead it outside the folder. `ls --tree` draws a tree with `ls`'s colours and
icons.
