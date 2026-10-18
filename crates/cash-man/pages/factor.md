---
name: factor
summary: Print the prime factors of numbers.
kind: Bundled coreutils, from uutils
see: expr seq
---

## Syntax

    factor [NUMBER]...

    factor 60        60: 2 2 3 5
    factor -h 72     72: 2^3 3^2, powers written once

## Examples

Numbers as arguments, and from standard input:

    factor 12 97 100
    echo 60 | factor

A prime has itself alone:

    factor 7919

## How it works

`factor` prints each number, a colon, and its prime factors from the smallest, each as
often as it divides the number. Without arguments it reads numbers from standard input,
blanks between them. Numbers may be as large as 128 bits. `-h` writes a repeated factor
once, with its power.

## Options

    -h, --exponents    A repeated factor once, as p^e.

Exit status: 0, or 1 for an argument that is not a number.
