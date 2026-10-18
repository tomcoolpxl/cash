---
name: nl
summary: Number the lines of files.
kind: Bundled coreutils, from uutils
see: cat grep
---

## Syntax

    nl [OPTION]... [FILE]...

    nl FILE            number the lines that are not empty
    nl -ba FILE        number every line, as cat -n does
    nl -w2 -s'. ' F    two columns wide, ". " after the number

## Examples

The lines numbered, the empty ones left alone:

    nl list.txt

Every line:

    nl -ba list.txt

Narrow, with a separator of your own:

    nl -w2 -s'. ' pairs.txt

From 10, by 10, and with zeros:

    nl -v10 -i10 pairs.txt
    nl -nrz -w3 pairs.txt

Only the lines that match:

    nl -bp'cat' notes.txt

## How it works

`nl` copies its files, or standard input, putting a number before each line it numbers:
by default the lines that are not empty, right-aligned in six columns with a tab after.
`-b` says which lines: `a` all, `t` those not empty, `n` none, `pBRE` those matching
the regular expression BRE. `-n` sets the format: `rn` right-aligned, `ln` left, `rz`
right with zeros. `-w` sets the width, `-s` what follows the number, `-v` the first
number and `-i` the step.

`nl` also knows logical pages: a line `\:\:\:` starts a header, `\:\:` a body and `\:`
a footer, each numbered as `-h`, `-b` and `-f` say, and the count starts again at each
page unless `-p`.

## Options

    -b, --body-numbering=STYLE       Which lines: a, t (default), n, pBRE.
    -n, --number-format=FORMAT       ln, rn (the default) or rz.
    -w, --number-width=N             The numbers' width: 6 without.
    -s, --number-separator=STRING    After each number: a tab without.
    -v, --starting-line-number=N     The first number: 1 without.
    -i, --line-increment=N           The step: 1 without.
    -l, --join-blank-lines=N         N empty lines in a row count as one.
    -h, --header-numbering=STYLE     Which header lines: n without.
    -f, --footer-numbering=STYLE     Which footer lines: n without.
    -d, --section-delimiter=CC       The page delimiter: \: without.
    -p, --no-renumber                Go on counting across pages.

Exit status: 0, or 1 for a file that cannot be read.
