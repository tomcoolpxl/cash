---
name: hexdump
summary: Dump bytes in hex, octal, decimal or text, or in a format of your own.
kind: Text and file tools
see: od xxd
---

## Syntax

    hexdump [OPTION]... [FILE]...

    hexdump -C FILE         offsets, hex and text: the classic layout
    hexdump -n 16 -C FILE   the first 16 bytes
    hexdump -e FORMAT FILE  a layout of your own

## Examples

The classic layout, and a part of a file:

<!-- parse only: Git Bash has no hexdump to compare with -->

    printf 'hello world\n' | hexdump -C
    hexdump -n 16 -s 0x200 -C disk.img
    hexdump -v -e '16/1 "%02x " "\n"' file.bin
    hexdump -e '"%08_ax  " 4/4 "%08x " "\n"' file.bin

## How it works

`hexdump` is util-linux's. Without options it shows two-byte hex words, as `-x` does;
`-C` is the canonical hex and text layout, `-b` one-byte octal, `-c` one-byte
characters, `-d` two-byte decimal, `-o` two-byte octal and `-X` one-byte hex. `-n`
limits the bytes read, `-s` skips a number of them (with `k`, `M`, `G` and `KiB`/`KB`
suffixes), and `-v` prints every block where identical ones would otherwise fold into a
`*`. Several files are read as one. `-C` prints a line for each 16 bytes: the offset
in hex, the bytes in two groups of eight, and the printable ones between bars, a `.`
for the rest.

`-e` takes a format of your own, `-f` a file of them: units of `[count][/bytes] "text"`
where the text is a printf format. `%x`, `%d`, `%o`, `%u` take 1, 2, 4 or 8 bytes,
`%e`/`%f`/`%g` 4 or 8, `%c` one, `%s` the unit's byte count or its precision; `%_a[dox]`
prints the position, `%_A[dox]` only once at the end, `%_c` a byte as a C escape, `%_p`
as a printable character or `.`, `%_u` by its control name. A last unit without a count
is repeated to fill the block, which is as long as the longest format.

## Options

    -C, --canonical         Offsets, hex and text.
    -x, --two-bytes-hex     Two-byte hex words: the default.
    -X, --one-byte-hex      One-byte hex.
    -b, --one-byte-octal    One-byte octal.
    -c, --one-byte-char     One-byte characters.
    -d, --two-bytes-decimal Two-byte decimal.
    -o, --two-bytes-octal   Two-byte octal.
    -e, --format=FORMAT     A format of your own.
    -f, --format-file=FILE  Formats from FILE.
    -n, --length=N          Read only N bytes.
    -s, --skip=OFFSET       Skip OFFSET bytes first.
    -v, --no-squeezing      Every block, none folded into *.

Exit status: 0, or 1 for a file that cannot be read.

## Different in cash

### Difference from util-linux

`-s` on standard input skips the bytes, where util-linux cannot seek a pipe. A file that
cannot be read gives status 1. Colour is not printed: `-L=never` and `-L=auto` are
accepted, `-L=always` refused, and `_L[...]` in a format is read and ignored.
