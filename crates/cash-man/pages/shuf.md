---
name: shuf
summary: Print a random permutation of the input lines.
kind: Bundled coreutils, from uutils
see: sort seq
---

## Syntax

    shuf [OPTION]... [FILE]
    shuf -e [OPTION]... [ARG]...
    shuf -i LO-HI [OPTION]...

    shuf FILE          FILE's lines in a random order
    shuf -n 1 FILE     one line, at random
    shuf -e a b c      the arguments, shuffled
    shuf -i 1-6        the numbers 1 to 6, shuffled

## Examples

A random line, and three:

    shuf -n 1 pairs.txt | wc -l
    seq 10 | shuf -n 3 | wc -l

Every argument once, in some order:

    shuf -e red green blue | sort

Dice: one throw, and ten with repeats:

    shuf -i 1-6 -n 1 | grep -cx '[1-6]'
    shuf -r -i 1-6 -n 10 | wc -l

## How it works

`shuf` writes its input lines in a random order, each once. `-n N` stops after N lines;
`-r` draws with repeats, so a line may come again, and with `-n` makes as many as asked.
`-e` shuffles its arguments instead of lines, `-i LO-HI` the numbers from LO to HI.
`-o FILE` writes to FILE, which may be the input. `--random-source=FILE` takes its
randomness from FILE, so the same file gives the same order every time.

## Options

    -n, --head-count=N        At most N lines.
    -r, --repeat              Lines may repeat.
    -e, --echo                Shuffle the arguments.
    -i, --input-range=LO-HI   Shuffle the numbers LO to HI.
    -o, --output=FILE         Write to FILE.
    --random-source=FILE      Randomness from FILE.
    -z, --zero-terminated     Lines end with a NUL, not a newline.

Exit status: 0, or 1 for a file that cannot be read.
