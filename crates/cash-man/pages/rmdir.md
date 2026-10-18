---
name: rmdir
summary: Remove empty directories.
kind: Bundled coreutils, from uutils
see: rm mkdir
---

## Syntax

    rmdir [OPTION]... DIRECTORY...

    rmdir DIR          remove it, if it is empty
    rmdir -p A/B/C     remove C, then B, then A, each if empty

## Examples

Remove an empty directory:

    mkdir empty
    rmdir empty
    [ -d empty ] || echo gone

A directory with something in it is left, with an error:

    mkdir full && touch full/x
    rmdir full 2>/dev/null || echo 'not empty: left'

A path of empty directories, all of them:

    mkdir -p a/b/c
    rmdir -p a/b/c
    [ -d a ] || echo 'all gone'

## How it works

`rmdir` removes each DIRECTORY, which must be empty: it never removes files, so it is
the safe way to tidy up. `-p` removes each directory in the path given, from the last
up, as long as each is empty. `--ignore-fail-on-non-empty` leaves a full directory
without an error.

`rm -r` removes a directory with everything in it.

## Options

    -p, --parents                   Remove the directories above too.
    --ignore-fail-on-non-empty      Leave a full directory quietly.
    -v, --verbose                   Say each directory removed.

Exit status: 0 when every DIRECTORY was removed; 1 otherwise.

## Different in cash

- It is cash's own copy of uutils' `rmdir`, carried inside cash, so it is the same on
  every machine, with or without Git or MSYS2.
- Windows will not remove a directory a program is in, the shell's own included: `cd`
  out of it first.
