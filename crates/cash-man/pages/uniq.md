---
name: uniq
summary: Report or omit repeated lines.
kind: Bundled coreutils, from uutils
see: sort comm wc
---

## Syntax

    uniq [OPTION]... [INPUT [OUTPUT]]

    uniq FILE            each run of equal lines once
    uniq -c FILE         and how many there were
    uniq -d FILE         only lines that repeat
    uniq -u FILE         only lines that do not
    sort FILE | uniq     each line once, wherever it was

## Examples

Each run of equal lines once:

    printf '%s\n' a a b a | uniq

Only lines next to each other count: sort first for each line once in all:

    printf '%s\n' a a b a | sort | uniq

How many of each, the most first:

    printf '%s\n' b a b c b | sort | uniq -c | sort -rn

The lines that repeat, and those that do not:

    printf '%s\n' a a b c c | uniq -d
    printf '%s\n' a a b c c | uniq -u

Upper and lower case alike:

    printf '%s\n' Apple apple APPLE | uniq -i

## How it works

`uniq` reads its INPUT (a file, or standard input) and writes each line unless it is
the same as the line before it: a run of equal lines comes out once. Only lines next
to each other are compared, so `sort | uniq`, or `sort -u`, gives each line once
wherever it was.

`-c` puts the count before each line; `-d` writes only lines that were repeated, `-u`
only those that were not. `-f N` skips the first N fields and `-s N` the first N
characters before comparing; `-w N` compares no more than N characters.

With an OUTPUT file named, the result goes there.

## Options

    -c, --count           The number of times each line came, before it.
    -d, --repeated        Only lines that came more than once, once each.
    -D, --all-repeated    Every line of a repeated run.
    -u, --unique          Only lines that came once.
    -i, --ignore-case     Upper and lower case alike.
    -f, --skip-fields=N   Skip N fields (blanks and what is not) first.
    -s, --skip-chars=N    Skip N characters first.
    -w, --check-chars=N   Compare at most N characters.
    -z, --zero-terminated Lines end in a NUL byte, not a newline.

Exit status: 0 on success; 1 when a file cannot be read or written.

## Different in cash

- It is cash's own copy of uutils' `uniq`, carried inside cash, so it is the same on
  every machine, with or without Git or MSYS2.
