---
name: seq
summary: Print a sequence of numbers.
kind: Bundled coreutils, from uutils
see: shuf printf
---

## Syntax

    seq [OPTION]... [FIRST [STEP]] LAST

    seq 5            1 to 5
    seq 2 5          2 to 5
    seq 0 2 10       0 to 10, by 2
    seq 5 -1 1       5 down to 1
    seq -w 8 10      08 09 10, the same width

## Examples

From 1, from another start, and by a step:

    seq 3
    seq 4 6
    seq 0 5 20

Counting down:

    seq 3 -1 1

The same width, with zeros, and on one line:

    seq -w 8 11
    seq -s, 5

Decimals, and a format of your own:

    seq 0 0.5 2
    seq -f 'file%03g.txt' 3

A loop a number of times:

    for i in $(seq 3); do echo "round $i"; done

## How it works

`seq` writes the numbers from FIRST (1 if not given) to LAST, a step of STEP (1)
apart, a line each. STEP may be negative to count down, and any of them may have
decimals. `-w` pads with zeros to one width, `-s` puts its string between the numbers
instead of a newline, and `-f` gives a printf format for each.

`{1..5}` in the shell does the same for whole numbers without starting anything, and
`for ((i = 1; i <= 5; i++))` counts in arithmetic.

## Options

    -w, --equal-width       Pad with zeros to one width.
    -s, --separator=STRING  Put STRING between the numbers.
    -f, --format=FORMAT     A printf format for each, with %e, %f or %g.

Exit status: 0, or 1 for an argument that is not a number.
