---
name: csplit
summary: Split a file into sections at context lines.
kind: Bundled coreutils, from uutils
see: split head sed
---

## Syntax

    csplit [OPTION]... FILE PATTERN...

    csplit FILE 10           at line 10: xx00 lines 1-9, xx01 the rest
    csplit FILE /^##/ '{*}'  before every line that starts with ##
    csplit -f part FILE ...  pieces named part00, part01, ...

## Examples

At a line number, the sizes printed:

    seq 10 > n.txt
    csplit n.txt 4
    cat xx01

At every line that matches, quietly, with names of your own:

    printf 'intro\n## one\na\n## two\nb\n' > doc.md
    csplit -s -f sec doc.md '/^##/' '{*}'
    head -1 sec*

## How it works

`csplit` copies FILE into pieces, `xx00`, `xx01`, ..., cutting it where each PATTERN
says, and prints each piece's size. A number N cuts before line N; `/REGEX/` before the
next line that matches, `%REGEX%` the same but drops what comes before it; an offset
after a pattern, `/REGEX/+1`, moves the cut. `{N}` repeats the pattern before it N more
times, and `{*}` as often as it matches. `-f` sets the prefix, `-b` a printf format for
the number, `-n` its digits; `-s` prints no sizes, and `-z` makes no empty pieces.

## Options

    -f, --prefix=PREFIX          Names start with PREFIX: xx without.
    -b, --suffix-format=FORMAT   The number as FORMAT: %02d without.
    -n, --digits=N               N digits: 2 without.
    -s, -q, --quiet, --silent    Print no sizes.
    -z, --elide-empty-files      Make no empty pieces.
    -k, --keep-files             Keep the pieces made when it fails.
    --suppress-matched           Leave out the lines that matched.

Exit status: 0, or 1 when a pattern does not match, or a line number is past the end.
