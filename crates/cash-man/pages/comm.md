---
name: comm
summary: Compare two sorted files line by line.
kind: Bundled coreutils, from uutils
see: sort diff uniq join
---

## Syntax

    comm [OPTION]... FILE1 FILE2

    comm a b        three columns: only in a, only in b, in both
    comm -12 a b    the lines in both
    comm -23 a b    the lines only in a
    comm -13 a b    the lines only in b

## Examples

The three columns:

    printf 'apple\nbanana\ncherry\n' > mine.txt
    printf 'banana\ncherry\ndate\n' > yours.txt
    comm mine.txt yours.txt

The lines in both, and the ones only in the first:

    printf 'apple\nbanana\ncherry\n' > mine.txt
    printf 'banana\ncherry\ndate\n' > yours.txt
    comm -12 mine.txt yours.txt
    comm -23 mine.txt yours.txt

Files that are not sorted, sorted on the way in:

    printf 'pear\napple\n' > x.txt
    printf 'fig\napple\n' > y.txt
    comm -12 <(sort x.txt) <(sort y.txt)

How many in each column:

    printf 'apple\nbanana\ncherry\n' > mine.txt
    printf 'banana\ncherry\ndate\n' > yours.txt
    comm --total mine.txt yours.txt | tail -1

## How it works

`comm` reads two sorted files and writes three columns: the lines only in FILE1, those
only in FILE2, one tab in, and those in both, two tabs in. `-1`, `-2` and `-3` leave
out a column, and they combine: `-12` leaves only the lines in both. `-` reads
standard input.

The files must be sorted as `sort` sorts them; when one is not, `comm` says so, goes
on, and exits 1. `sort -u` first, or `<(sort FILE)`, makes them so.

## Options

    -1                        Leave out the lines only in FILE1.
    -2                        Leave out the lines only in FILE2.
    -3                        Leave out the lines in both.
    --check-order             Stop at the first line out of order.
    --nocheck-order           Do not check the order.
    --output-delimiter=STR    STR between columns, not a tab.
    --total                   End with how many lines each column has.
    -z, --zero-terminated     Lines end with a NUL, not a newline.

Exit status: 0, or 1 when a file cannot be read or is not sorted.
