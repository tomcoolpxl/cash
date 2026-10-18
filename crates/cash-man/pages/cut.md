---
name: cut
summary: Print selected parts of lines.
kind: Bundled coreutils, from uutils
see: awk paste tr sort
---

## Syntax

    cut OPTION... [FILE]...

    cut -d: -f1 FILE       the first field, fields split at :
    cut -d, -f2,4 FILE     the second and fourth, split at ,
    cut -f3- FILE          from the third tab-separated field on
    cut -c1-8 FILE         the first 8 characters of each line

## Examples

A field, with `=` between fields:

    cut -d= -f1 config.ini

Several fields, and a range:

    printf '%s\n' 'a,b,c,d' | cut -d, -f2,4
    printf '%s\n' 'a,b,c,d' | cut -d, -f2-

Characters by position:

    cut -c1-4 notes.txt

Fields split by tabs, the default:

    printf 'one\ttwo\tthree\n' | cut -f2

Everything but a field:

    printf '%s\n' 'a,b,c' | cut -d, -f2 --complement

## How it works

`cut` writes part of each line of its FILEs (or standard input). With `-f`, a line is
split into fields at a delimiter, a tab unless `-d` names another single character,
and the fields LIST names are written, joined by the same delimiter. With `-c` or
`-b`, the characters or bytes at the LIST's positions are written.

A LIST is numbers and ranges, separated by commas: `1,3`, `2-4`, `3-` (from 3 to the
end), `-2` (up to 2). Counting starts at 1. A line without the delimiter is written
whole, unless `-s` leaves it out.

`cut` splits at every delimiter: two spaces in a row make an empty field between them.
For fields separated by runs of blanks, `awk '{ print $2 }'` is the tool.

## Options

    -f, --fields=LIST        The fields LIST names.
    -d, --delimiter=C        Fields are split at C, not at tabs.
    -c, --characters=LIST    The characters at LIST's positions.
    -b, --bytes=LIST         The bytes at LIST's positions.
    -s, --only-delimited     Leave out lines without the delimiter.
    --complement             Everything but what LIST names.
    --output-delimiter=STR   Join the fields with STR.
    -z, --zero-terminated    Lines end in a NUL byte, not a newline.

Exit status: 0 on success; 1 when a file cannot be read or the LIST is wrong.

## Different in cash

- It is uutils' `cut`, carried inside cash, so it is the same on every machine, with
  or without Git or MSYS2.
