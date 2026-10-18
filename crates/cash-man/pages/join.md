---
name: join
summary: Join the lines of two files on a common field.
kind: Bundled coreutils, from uutils
see: paste comm sort cut
---

## Syntax

    join [OPTION]... FILE1 FILE2

    join a b          the lines whose first field is in both, joined
    join -t, a b      fields apart by commas
    join -a1 a b      and the lines of a with no match
    join -v1 a b      only the lines of a with no match

## Examples

Two files on their first field:

    printf '1 Ann\n2 Bob\n3 Cy\n' > names.txt
    printf '1 31\n3 27\n4 50\n' > ages.txt
    join names.txt ages.txt

The unmatched ones too, with a filler, and the ones with no match:

    printf '1 Ann\n2 Bob\n3 Cy\n' > names.txt
    printf '1 31\n3 27\n4 50\n' > ages.txt
    join -a1 -e NA -o 0,1.2,2.2 names.txt ages.txt
    join -v2 names.txt ages.txt

CSV, on another field of the first file:

    printf 'Ann,1\nBob,2\n' > who.csv
    printf '1,admin\n2,user\n' > roles.csv
    join -t, -1 2 -2 1 who.csv roles.csv

## How it works

`join` reads two files sorted on the join field, and for each pair of lines with the
same join field writes the field, then the rest of the first line, then the rest of
the second. Fields are apart by blanks, or by the character `-t` gives.

`-1 N` and `-2 N` choose each file's join field, 1 without, and `-j N` both. `-a 1`
prints the first file's unpaired lines too, `-v 1` only those, and 2 the second's.
`-o` lists the fields to print: `0` the join field, `1.2` the first file's second;
`-e` fills a field that is missing. `--header` joins the first lines without
comparing them. Files not sorted are sorted on the way: `join <(sort a) <(sort b)`.

## Options

    -1 FIELD                 The join field of FILE1.
    -2 FIELD                 The join field of FILE2.
    -j FIELD                 The join field of both.
    -t CHAR                  Fields apart by CHAR, not blanks.
    -a FILENUM               Also the unpaired lines of file 1 or 2.
    -v FILENUM               Only the unpaired lines of file 1 or 2.
    -o FORMAT                The fields to print: 0, 1.2, 2.1, or auto.
    -e STRING                STRING for a field that is missing.
    -i, --ignore-case        Compare the fields case aside.
    --header                 The first lines are headers: not compared.
    --check-order            Stop at the first line out of order.
    --nocheck-order          Do not check the order.
    -z, --zero-terminated    Lines end with a NUL, not a newline.

Exit status: 0, or 1 when a file cannot be read or is not sorted.
