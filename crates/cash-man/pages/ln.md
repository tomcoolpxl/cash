---
name: ln
summary: Make links between files.
kind: Bundled coreutils, from uutils
see: cp mv readlink paths
---

## Syntax

    ln [OPTION]... TARGET LINK
    ln [OPTION]... TARGET... DIRECTORY

    ln -s TARGET LINK      a symbolic link, LINK naming TARGET
    ln TARGET LINK         a hard link: a second name for the same file
    ln -sf TARGET LINK     replace LINK if it is there
    ln -s TARGET... DIR    links in DIR, each named as its target

## Examples

A second name for a file, a hard link:

    ln notes.txt same.txt
    echo more >> same.txt
    tail -n 1 notes.txt

A symbolic link to a directory:

    mkdir -p real/inner
    ln -s real link
    ls link

Links in a directory, each named as its target:

    mkdir -p shortcuts
    ln notes.txt pairs.txt shortcuts/
    ls shortcuts

Point a link at another directory, replacing it:

<!-- parse only: Git Bash copies a directory for ln -s without the privilege -->

    mkdir -p v1 v2 && touch v2/new
    ln -s v1 current
    ln -sfn v2 current && ls current

## How it works

A hard link (`ln TARGET LINK`) is a second name for the same file: both names are the
file, a change through one shows through the other, and the file goes only when the
last name is removed. Hard links are made on the same drive, to files, not
directories.

A symbolic link (`ln -s TARGET LINK`) is a small file that names TARGET; opening it
opens TARGET. TARGET is written into the link as it is given: a relative one is found
from the link's directory, not from where `ln` ran, so `ln -s ../file dir/link` names
`file` beside `dir`. A symbolic link may name a directory, or something not there yet.

With more than two names and the last a directory, each TARGET gets a link in it.

## Options

    -s, --symbolic         Make symbolic links, not hard ones.
    -f, --force            Remove a LINK that is there first.
    -n, --no-dereference   A LINK that is a link to a directory is
                           replaced, not entered.
    -i, --interactive      Ask before replacing a LINK.
    -r, --relative         Make the symbolic link's target relative to it.
    -v, --verbose          Say each link made: 'link' -> 'target'.
    -t, --target-directory=DIR  Make the links in DIR.
    -T, --no-target-directory   LINK is a name, never a directory to put
                           links in.
    -b, --backup           Keep a LINK that is replaced as LINK~.

Exit status: 0 when every link was made; 1 otherwise.

## Different in cash

- It is cash's own copy of uutils' `ln`, carried inside cash, so it is the same on
  every machine, with or without Git or MSYS2.
- A symbolic link needs Developer Mode or an elevated shell on Windows. Without, `ln
  -s` to a directory makes a junction, which needs neither, names its target by its
  full path, and is otherwise the same to cash and to Windows programs; to a file it
  fails, `Operation not permitted`, saying how to get the privilege.
