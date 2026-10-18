---
names: mapfile readarray
summary: Read lines into an array (also called readarray).
summary readarray: Read lines into an indexed array (the same as `mapfile`).
kind: Bash builtins
see: read declare
---

## Syntax

    mapfile [-t] [-n COUNT] [-s SKIP] [-d DELIM] [ARRAY]
    readarray ...

    mapfile -t lines < FILE      each line of FILE an element
    mapfile -t -n 5 top < FILE   the first 5 lines
    mapfile -t < <(COMMAND)      a command's lines

## Examples

Each line of a file into an array:

    mapfile -t lines < pairs.txt
    echo "${#lines[@]} lines; the second: ${lines[1]}"

A command's output, through a process substitution:

    mapfile -t nums < <(seq 3)
    echo "${nums[*]}"

Skip lines, and take a number of them:

    mapfile -t -s 1 -n 2 part < app.log
    printf '[%s]\n' "${part[@]}"

Without `-t`, each element keeps its newline:

    mapfile kept < pairs.txt
    printf '%s' "${kept[0]}"

## How it works

`mapfile` reads lines from its standard input into an indexed array, one element a
line, from index 0; without an ARRAY name it uses `MAPFILE`. `-t` drops the newline
at each line's end, which is what is almost always wanted.

In a pipeline `mapfile` runs in a subshell and its array is gone after: write `mapfile
-t a < file` or `mapfile -t a < <(command)`, not `command | mapfile -t a`.

`readarray` is the same command under another name.

## Options

    -t          Drop the newline at each line's end.
    -n COUNT    Read at most COUNT lines.
    -s SKIP     Skip the first SKIP lines.
    -O ORIGIN   Start at index ORIGIN, not 0.
    -d DELIM    End lines at DELIM, not a newline.
    -u FD       Read from descriptor FD.
    -c N, -C CMD  Run CMD every N lines.

Exit status: 0, or 1 for a bad option or a read-only ARRAY.
