---
name: tree
summary: Show a directory hierarchy as a tree.
kind: Text and file tools
see: ls find
---

## Syntax

    tree [OPTION]... [DIRECTORY]...

    tree           the current directory and everything under it
    tree -L 2 DIR  two levels deep
    tree -d DIR    directories only
    tree -a DIR    with the names that start with a dot

## Examples

A small tree, whole, then one level of it:

<!-- parse only: Git Bash has no tree to compare with -->

    mkdir -p proj/src/lib proj/docs
    touch proj/README proj/src/main.c proj/src/lib/util.c
    tree proj
    tree -L 1 proj

Directories only, and full paths:

<!-- parse only: Git Bash has no tree to compare with -->

    mkdir -p proj/src/lib proj/docs
    touch proj/src/main.c
    tree -d proj
    tree -f --noreport proj

## How it works

`tree DIRECTORY` draws the directory and what is under it, a line each, with lines that
show what is inside what, and ends with a count of the directories and files. Names are
in order, and names that start with a dot are left out unless `-a`. `-L N` stops N
levels down, `-d` shows directories only, `-f` each entry's path from the top,
`--dirsfirst` directories before files, and `--noreport` leaves the count out. The
first example draws

    proj
    ├── README
    ├── docs
    └── src
        ├── lib
        │   └── util.c
        └── main.c

    3 directories, 3 files

## Options

    -a            Names that start with a dot too.
    -d            Directories only.
    -L LEVEL      At most LEVEL levels down.
    -f            Each entry's path, not its name.
    --dirsfirst   Directories before files.
    --noreport    No count at the end.

Exit status: 0, or 1 when a DIRECTORY is not there.

## Different in cash

Windows' own `tree.com` draws directories only, with other options; inside cash, `tree`
is this one. Links and junctions are shown and not followed, so a junction cannot loop
the walk or lead it outside the directory. `ls --tree` draws a tree with `ls`'s colours
and icons.
