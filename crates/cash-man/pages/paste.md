---
name: paste
summary: Merge the lines of files side by side.
kind: Bundled coreutils, from uutils
see: cut join column
---

## Syntax

    paste [OPTION]... [FILE]...

    paste a b         a line of a, a tab, the line of b
    paste -d, a b     a comma between them
    paste -s a        all of a's lines on one line
    paste - - < f     f's lines two by two

## Examples

Two files side by side:

    printf 'Ann\nBob\n' > names.txt
    printf '31\n27\n' > ages.txt
    paste names.txt ages.txt

With a comma, as CSV:

    paste -d, names.txt ages.txt

A file's lines on one line, and summed:

    seq 5 | paste -sd+
    echo $(( $(seq 5 | paste -sd+) ))

Standard input three lines at a time:

    seq 6 | paste - - -

## How it works

`paste` reads its files side by side and writes their first lines on one line, a tab
between, then their second lines, and so on; a file that ends early gives empty
fields. `-` is standard input, and naming it twice or more takes its lines in turn.

`-d LIST` puts the characters of LIST between the fields instead of a tab, one after
the other: `-d',;'` a comma, then a semicolon. `-s` pastes one file at a time, all its
lines on one line.

## Options

    -d, --delimiters=LIST    Between fields, in turn: \n, \t, \\, \0 none.
    -s, --serial             One file at a time, all its lines on one line.
    -z, --zero-terminated    Lines end with a NUL, not a newline.

Exit status: 0, or 1 for a file that cannot be read.
