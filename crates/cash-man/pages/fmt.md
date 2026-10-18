---
name: fmt
summary: Reformat paragraphs of text.
kind: Bundled coreutils, from uutils
see: fold column
---

## Syntax

    fmt [OPTION]... [FILE]...

    fmt FILE           paragraphs filled to 75 columns
    fmt -w 60 FILE     to 60
    fmt -s FILE        long lines split, short ones left alone
    fmt -p '#' FILE    only the lines that start with #

## Examples

A long line filled to a width:

    s='The quick brown fox jumps over the lazy dog, and then it runs away.'
    echo "$s" | fmt -w 30

Short lines joined, paragraphs kept apart:

    printf 'short\nlines\nare\njoined\n\nnew paragraph\n' | fmt

One space between words, two after a sentence; and a list left as it is:

    printf 'too    many   spaces.  Here.\n' | fmt -u
    printf 'a list:\n- one\n- two\n' | fmt -s

Comments filled, their marks kept:

    c='# a comment that is long enough to be wrapped here'
    echo "$c" | fmt -p '#' -w 30

## How it works

`fmt` fills paragraphs: it joins their lines and breaks them again to fit the width, 75
columns without `-w`, breaking only between words. A blank line ends a paragraph, and so
does a change of indentation. `-s` only splits long lines, so lists and code keep their
lines; `-u` leaves one space between words and two after a sentence; `-p PREFIX` fills
only the lines that start with PREFIX, keeping it on every line; `-c` and `-t` keep the
first two lines' indentation.

`fold` cuts lines at the width without joining them.

## Options

    -w, --width=WIDTH        The width: 75 without.
    -g, --goal=WIDTH         The width to aim for: a little less than -w.
    -s, --split-only         Split long lines; join none.
    -u, --uniform-spacing    One space between words, two after a sentence.
    -p, --prefix=STRING      Only lines that start with STRING.
    -c, --crown-margin       Keep the first two lines' indentation.
    -t, --tagged-paragraph   The first line indented apart from the rest.

Exit status: 0, or 1 for a file that cannot be read.
