---
names: mktemp realpath readlink
summary: Make a temporary file or directory and print its name.
summary realpath: Print the resolved, absolute path.
summary readlink: Print a link's target, or a canonical file name.
kind: Bundled coreutils, from uutils
see: paths
draft: yes
---

## Examples

    dir=$(mktemp -d); trap 'rm -rf "$dir"' EXIT
    here=$(realpath "$(dirname "$0")")

## How it works

- `mktemp` makes a temporary file (or directory, with `-d`) and prints its name.
- `realpath PATH` prints the absolute path with links and `..` resolved.
- `readlink LINK` prints a link's target; `readlink -f` the resolved path.

## Different in cash

These three build paths themselves, and Windows answers in backslashes, so cash writes
their output in its own spelling, `C:/Users/me/AppData/Local/Temp/tmp.AbCdEf`: a
`d=$(mktemp -d)` that held backslashes would lose them to the first `xargs` or unquoted
`echo -e` downstream. `/tmp` is your `TEMP` directory.
