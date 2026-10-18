---
name: od
summary: Dump files in octal and other formats.
kind: Bundled coreutils, from uutils
see: xxd hexdump
---

## Syntax

    od [OPTION]... [FILE]...

    od -c FILE          the bytes as characters, escapes for the rest
    od -An -tx1 FILE    the bytes in hex, no offsets
    od -N 16 FILE       only the first 16 bytes

## Examples

The characters of a line, a newline shown as `\n`:

    printf 'hello world\n' | od -c

Bytes in hex and in decimal, without the offsets:

    printf 'abc' | od -An -tx1
    printf 'abc' | od -An -tu1

A part of a file, offsets in hex:

    printf 'hello world\n' | od -Ax -c -j6 -N5

## How it works

`od` writes its input as rows: an offset, then the bytes in the formats `-t` names.
Without `-t` it is two-byte words in octal, as `-o` says. `-t` takes `c` characters
(`\n`, `\t`, `\0` for the rest), `a` named characters (`nl`, `sp`), and `x` hex, `o`
octal, `d` signed and `u` unsigned decimal, each followed by the bytes per number: `x1`,
`x2`, `d4`; `f` is floating point. `-c` is `-tc`, `-x` is `-tx2`. `-A` picks the
offsets' base, `n` for none; `-j` skips bytes at the start, `-N` stops after so many.
A row the same as the one before is shown as `*`, unless `-v`.

## Options

    -t, --format=TYPE           The formats: c, a, x1, o2, d4, u1, f8, ...
    -A, --address-radix=RADIX   Offsets in d, o, x, or n for none.
    -j, --skip-bytes=N          Skip N bytes first.
    -N, --read-bytes=N          Read only N bytes.
    -w, --width=N               N bytes a row: 16 without.
    -v, --output-duplicates     Show every row, none as *.
    -c                          Characters: -tc.
    -x                          Hex words: -tx2.
    -o                          Octal words: -to2.
    -d                          Unsigned decimal words: -tu2.

Exit status: 0, or 1 for a file that cannot be read.
