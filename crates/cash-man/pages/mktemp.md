---
names: mktemp realpath readlink
summary: Make a temporary file or directory and print its name.
summary realpath: Print the resolved, absolute path.
summary readlink: Print a link's target, or a canonical file name.
kind: Bundled coreutils, from uutils
see: paths ln dirname
---

## Syntax

    mktemp [OPTION]... [TEMPLATE]
    realpath [OPTION]... FILE...
    readlink [OPTION]... FILE...

    mktemp                  a new empty file in the temp directory
    mktemp -d               a new directory there
    mktemp draft.XXXX       a file here, the Xs made random
    realpath FILE           FILE's absolute path
    realpath --relative-to=DIR FILE    FILE's path from DIR
    readlink LINK           where a link points
    readlink -f FILE        the absolute path, every link followed

## Examples

A scratch directory, removed when the script ends:

    dir=$(mktemp -d)
    trap 'rm -rf "$dir"' EXIT
    echo data > "$dir/work.txt"
    cat "$dir/work.txt"

A file named after a template, here:

    f=$(mktemp draft.XXXX)
    echo "first try" > "$f"
    cat draft.*
    rm "$f"

One path seen from another directory, and one that does not exist yet:

    mkdir -p src/lib
    realpath --relative-to=src/lib a.md
    realpath -m --relative-to=. src/lib/../app/main.c

The directory a script is in, and where a link points:

<!-- parse only: names files of the reader's own -->

    here=$(dirname "$(realpath "$0")")
    readlink ~/current
    readlink -f ../build/latest

## How it works

`mktemp` makes a new empty file that no other file has the name of, readable by you
alone, and prints its name. Without a TEMPLATE it is `tmp.` and ten random letters in
the temp directory. A TEMPLATE has three or more `X`s in a row, which become random
letters and digits, and is made in the current directory, unless `-t` or `-p DIR` puts
it in the temp directory or DIR; what comes after the `X`s stays (`draft.XXXX.txt`).
`-d` makes a directory instead.

`realpath FILE` prints FILE's absolute path, with `.` and `..` and every link resolved;
all but the last part must exist, unless `-m` (none need) or `-e` (all must).
`--relative-to=DIR` prints the path from DIR instead.

`readlink LINK` prints what a symbolic link or a junction points to, as it is stored;
for anything that is not a link it prints nothing and fails. `readlink -f` prints the
absolute path with every link followed, as `realpath` does.

## Options

mktemp:

    -d, --directory       Make a directory, not a file.
    -u, --dry-run         Print a name and make nothing.
    -q, --quiet           Say nothing when it fails.
    -p DIR, --tmpdir[=DIR]  Make it in DIR; without DIR, the temp directory.
    -t                    Make TEMPLATE in the temp directory.
    --suffix=SUFFIX       Put SUFFIX after the Xs.

realpath:

    -e, --canonicalize-existing  Every part must exist.
    -m, --canonicalize-missing   No part needs to exist.
    -s, --no-symlinks            Leave links as they are.
    -L, --logical                Take `..` away before following links.
    -P, --physical               Follow links as they come: the default.
    --relative-to=DIR            The path from DIR.
    --relative-base=DIR          From DIR when under it, else absolute.
    -q, --quiet                  Say nothing of a path not resolved.
    -z, --zero                   End each with a NUL, not a newline.

readlink:

    -f, --canonicalize           Follow every link; all but the last exist.
    -e, --canonicalize-existing  The same, and every part exists.
    -m, --canonicalize-missing   The same, and no part needs to.
    -n, --no-newline             No newline after it.
    -q, -s, --quiet, --silent    Say nothing of errors: the default.
    -v, --verbose                Say what is wrong.
    -z, --zero                   End each with a NUL, not a newline.

Exit status: 0; 1 when the file cannot be made, or a path cannot be resolved, or
`readlink` is given something that is not a link.

## Different in cash

These three build paths themselves, and Windows answers in backslashes, so cash writes
their output in its own spelling, `C:/Users/me/AppData/Local/Temp/tmp.AbCdEf`: a
`d=$(mktemp -d)` that held backslashes would lose them to the first `xargs` or unquoted
`echo -e` downstream. `/tmp` is your `TEMP` directory.
