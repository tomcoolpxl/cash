---
name: xxd
summary: Dump bytes in hex, or turn a hex dump back into bytes.
kind: Text and file tools
see: od hexdump base64
---

## Syntax

    xxd [OPTION]... [INFILE [OUTFILE]]
    xxd -r [OPTION]... [INFILE [OUTFILE]]

    xxd FILE              offsets, hex and text, 16 bytes a line
    xxd -p FILE           plain hex, nothing else
    xxd -r -p HEX         plain hex back into bytes
    xxd -i FILE           a C array of FILE's bytes

## Examples

A line, dumped:

    printf 'hello world\n' | xxd

Plain hex, and back:

    printf 'hello' | xxd -p
    printf 'hello' | xxd -p | xxd -r -p; echo

The first bytes, one a group:

    printf 'hello world\n' | xxd -l 4 -g 1

A C array for embedding:

    printf 'abc' > abc.bin
    xxd -i abc.bin

## How it works

`xxd` is vim's hex dump: the position, the bytes in hex, and the printable ones as
text. `-c` sets the bytes per line (16), `-g` the bytes per group (2), `-e` dumps
little-endian words (groups of 4), `-b` bits, `-p` plain hex with nothing else, `-i` a
C array named after the file (`-n` names it, `-C` in capitals), `-u` upper-case hex,
`-d` decimal positions, `-o` adds to the positions shown, `-s` starts further in (`-s
-N` from the end of a file), `-l` stops after so many bytes, and `-a` folds a run of
all-zero lines into one `*`.

`xxd -r` reads a dump back into bytes: each line's position decides where its bytes go,
so an edited dump patches the output file in place and gaps are zero-filled. `-r -p`
reads plain hex, `-r -b` bits, and `-r -s N` moves everything by N. An output file
argument writes there.

`-R always` colours the hex and text by byte class as xxd does; the default colours only
a terminal, and never with `NO_COLOR` set.

## Options

    -c COLS     Bytes a line: 16 without.
    -g BYTES    Bytes a group: 2 without.
    -l LEN      Stop after LEN bytes.
    -s SEEK     Start at SEEK; -s -N, N bytes from the end.
    -p          Plain hex.
    -i          A C array; -n NAME names it, -C in capitals.
    -b          Bits, not hex.
    -e          Little-endian groups of 4.
    -u          Upper-case hex.
    -d          Decimal positions.
    -o OFFSET   Add OFFSET to the positions shown.
    -a          A run of zero lines as one *.
    -r          A dump back into bytes; with -p, plain hex.
    -R WHEN     Colours: always, auto, never.

Exit status: 0, or 1 for a file that cannot be read or written.

## Different in cash

### Difference from vim's xxd

`-E` (EBCDIC) is refused, and standard input can be skipped forward with `-s` but not
seeked backward, as on a pipe.
