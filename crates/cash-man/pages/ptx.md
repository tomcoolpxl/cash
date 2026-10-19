---
name: ptx
summary: Produce a permuted index of file contents.
kind: Bundled coreutils, from uutils
see: sort grep
---

## Syntax

    ptx [OPTION]... [FILE]...

    ptx FILE           every word of FILE, in context, sorted by the word
    ptx -i STOP FILE   leave out the words listed in STOP
    ptx -o ONLY FILE   only the words listed in ONLY

## Examples

Each word of each line, sorted, in its context:

<!-- parse only: GNU ptx's contexts run on past a line's end -->

    printf 'the cat sat\non the mat\n' > words.txt
    ptx words.txt
    ptx -A words.txt
    ptx -w 30 words.txt

## How it works

`ptx` makes a permuted index, the kind at the back of a book: every word of the input,
sorted, each shown in the middle of its line with the words before it on the left and
the words after it on the right. `-i FILE` leaves out the words FILE lists, `-o FILE`
keeps only those; `-f` sorts case aside. `-A` puts each line's file and number first,
`-r` reads a reference at the start of each line, `-w` sets the width, `-G` keeps to
System V's options, and `-T` and `-O` write TeX or roff instead of text.

## Options

    -i, --ignore-file=FILE      Leave out the words FILE lists.
    -o, --only-file=FILE        Only the words FILE lists.
    -f, --ignore-case           Sort case aside.
    -A, --auto-reference        Each line's file and number first.
    -r, --references            Each line's first word is its reference.
    -w, --width=N               The width of the output: 72 without.
    -R, --right-side-refs       References on the right, past the width.
    -g, --gap-size=N            N columns between the fields.
    -t, --typeset-mode          A width of 100.
    -G, --traditional           System V's ptx.
    -T                          TeX.
    -O                          roff; -M NAME for its macro.
    -F, --flag-truncation=STR   STR where a context is cut.
    -S, --sentence-regexp=RE    What ends a context.
    -W, --word-regexp=RE        What a word is.
    -b, --break-file=FILE       The characters that break words.

Exit status: 0, or 1 for a file that cannot be read.

## Different in cash

- A line ends each context, where GNU's ptx lets a context run on to the end of a
  sentence, over the line's end; GNU's `ptx -S '\n'` gives what cash gives.
