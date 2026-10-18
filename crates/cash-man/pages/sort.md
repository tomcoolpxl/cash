---
name: sort
summary: Sort lines of text.
kind: Bundled coreutils, from uutils
see: uniq shuf cut comm
---

## Syntax

    sort [OPTION]... [FILE]...

    sort FILE              lines in order
    sort -r FILE           in reverse order
    sort -n FILE           by number, 9 before 10
    sort -u FILE           and each line once
    sort -t, -k2 FILE      by the second field, fields split at ,
    sort -h FILE           by size: 2K before 1M

## Examples

Lines in order, and in reverse:

    sort list.txt
    sort -r list.txt

By number, so that 9 comes before 10:

    printf '%s\n' 10 9 100 | sort -n

Each line once, sorted:

    printf '%s\n' b a b c a | sort -u

By a field: here the second, fields split at `:`:

    printf '%s\n' 'x:3' 'y:1' 'z:2' | sort -t: -k2

Count what is there most, a common pipeline:

    printf '%s\n' b a b c b a | sort | uniq -c | sort -rn

By size, as `du -h` and `ls -lh` write them:

    printf '%s\n' 1M 512K 2G 10K | sort -h

## How it works

`sort` reads every line of its FILEs (or standard input), sorts them, and writes them.
By default whole lines are compared character by character; `-n` compares the numbers
at their start, `-h` sizes with K, M and G, `-V` version numbers (`1.9` before
`1.10`), `-M` month names. `-r` turns the order around.

`-k` sorts by a part of each line: `-k2` from the second field to the end of the line,
`-k2,2` the second field alone, `-k2n` the second as a number. Fields are separated by
blanks, or by the character `-t` gives. Several `-k` are tried in turn.

`-u` writes one of each run of lines that compare equal. `-o FILE` writes the result to
FILE, which may be one of the inputs: `sort -o list.txt list.txt` sorts it in place,
where `> list.txt` would empty it first.

## Options

    -n, --numeric-sort        By the number at a line's start.
    -h, --human-numeric-sort  By size: 2K, 1M, 3G.
    -V, --version-sort        By version number: 1.9 before 1.10.
    -g, --general-numeric-sort  By number, exponents too: 1e3.
    -M, --month-sort          By month name: JAN before FEB.
    -r, --reverse             The other way round.
    -k, --key=POS1[,POS2]     By fields POS1 to POS2, or the line's end.
    -t, --field-separator=C   Fields are split at C, not at blanks.
    -u, --unique              One line of each run that compares equal.
    -f, --ignore-case         Upper and lower case alike.
    -b, --ignore-leading-blanks  Blanks at a field's start do not count.
    -d, --dictionary-order    Only letters, digits and blanks count.
    -s, --stable              Equal lines keep the input's order.
    -o, --output=FILE         Write to FILE, which may be an input.
    -c, --check               Say whether the input is sorted; sort nothing.
    -m, --merge               Merge files already sorted.
    -R, --random-sort         Shuffle, equal lines together.
    -z, --zero-terminated     Lines end in a NUL byte, not a newline.
    --parallel=N              Sort with N threads.
    -S, --buffer-size=SIZE    Use SIZE of memory before files on disk.

Exit status: 0 on success; 1 when `-c` finds the input out of order; 2 on an error.

## Advanced

### Sort order and locale

With `LC_ALL=C`, lines compare byte by byte: upper case before lower case, `B` before
`a`. In another locale, as GNU's `sort` does, letters compare as the locale orders
them. Set `LC_ALL=C` for the order scripts expect from machine to machine.

### Large files

A file larger than memory is sorted in parts on disk, in `TMPDIR` (or `-T DIR`), and
the parts merged.

## Different in cash

- It is cash's own copy of uutils' `sort`, carried inside cash, so it is the same on
  every machine, with or without Git or MSYS2.
- `>(...)` as an output is written as the pipe it is.
