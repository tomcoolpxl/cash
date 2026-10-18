---
name: truncate
summary: Shrink or extend files to a size.
kind: Bundled coreutils, from uutils
see: dd split
---

## Syntax

    truncate -s SIZE FILE...

    truncate -s 0 FILE      empty FILE, keeping it
    truncate -s 1M FILE     make FILE one mebibyte, zeros at the end
    truncate -s +10 FILE    ten bytes longer
    truncate -r REF FILE    the size of REF

## Examples

Shorter, and empty:

    printf 'hello world\n' > h.txt
    truncate -s 5 h.txt; cat h.txt; echo
    truncate -s 0 h.txt; wc -c < h.txt

Longer, and relative to the size it has:

    printf 'abc' > a.bin
    truncate -s 20 a.bin; wc -c < a.bin
    truncate -s +2 a.bin; wc -c < a.bin
    truncate -s -1 a.bin; wc -c < a.bin

The size of another file:

    truncate -r pairs.txt a.bin
    [ "$(wc -c < a.bin)" = "$(wc -c < pairs.txt)" ] && echo "the same size"

## How it works

`truncate -s SIZE FILE` sets the file's size: what is past it is cut off, and a file
that grows gets zeros at the end. A file that is not there is made, unless `-c`. SIZE
is a number of bytes with an optional unit, `K`, `M`, `G` (powers of 1024) or `KB`,
`MB` (of 1000), and may start with `+` or `-` to add or take away, `<` or `>` for at
most or at least, and `/` or `%` to round down or up to a multiple. `-r REF` takes the
size of the file REF.

## Options

    -s, --size=SIZE         The size, or a change to it.
    -r, --reference=FILE    The size of FILE.
    -c, --no-create         Do not make a file that is not there.
    -o, --io-blocks         SIZE counts blocks, not bytes.

Exit status: 0, or 1 when a file cannot be sized.
