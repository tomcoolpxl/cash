---
name: tsort
summary: Sort a partial order topologically.
kind: Bundled coreutils, from uutils
see: sort
---

## Syntax

    tsort [FILE]

    tsort PAIRS     an order where, for each pair "A B", A is before B

## Examples

Build steps in an order that respects what needs what:

    printf 'compile link\nlink package\nfetch compile\n' | tsort

A loop has no order, and says so:

    printf 'a b\nb a\n' | tsort 2>/dev/null; echo "status $?"

## How it works

`tsort` reads pairs of words, `A B` meaning that A must come before B, and prints every
word once, in an order where each comes after those that must precede it. Words are
apart by blanks or newlines, two at a time. When the pairs make a loop there is no such
order: `tsort` names the loop on standard error, prints an order anyway, and exits 1.

## Options

None.

Exit status: 0, or 1 when the pairs make a loop or the input is not pairs.
