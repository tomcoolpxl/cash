---
name: pathchk
summary: Check whether file names are valid or portable.
kind: Bundled coreutils, from uutils
see: test paths
---

## Syntax

    pathchk [OPTION]... NAME...

    pathchk NAME       too long a name, or a part of it, fails
    pathchk -p NAME    portable: POSIX's characters and lengths
    pathchk -P NAME    no empty name, no part starting with -

## Examples

Fine, and not portable:

    pathchk notes/today.txt && echo "fine"
    pathchk -p 'a*b' 2>/dev/null || echo "not portable"
    pathchk -P -- -x 2>/dev/null || echo "starts with a hyphen"

## How it works

`pathchk` checks each NAME without touching the disk: by default that it and each of
its parts fit the lengths the system allows. `-p` checks it against POSIX's minimum,
so it is portable to any system: parts of at most 14 bytes, the whole at most 256, and
only letters, digits, `.`, `_` and `-`. `-P` also refuses an empty name and a part that
starts with `-`. `--portability` is both.

## Options

    -p                 POSIX's portable lengths and characters.
    -P                 No empty name, no part starting with -.
    --portability      Both -p and -P.

Exit status: 0 when every NAME passes; 1 when one does not.
