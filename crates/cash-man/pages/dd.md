---
name: dd
summary: Copy a file, converting and formatting it.
kind: Bundled coreutils, from uutils
see: head tail truncate split
---

## Syntax

    dd [OPERAND]...

    dd if=IN of=OUT bs=1M        copy IN to OUT, a mebibyte at a time
    dd if=IN bs=1 skip=6 count=5 five bytes, from the seventh
    dd if=/dev/zero of=F bs=1K count=2    a file of 2048 zero bytes
    dd if=IN of=OUT conv=notrunc OUT patched in place, not cut short

## Examples

Part of a file, by bytes:

    printf 'hello world\n' > h.txt
    dd if=h.txt bs=1 count=5 status=none; echo
    dd if=h.txt bs=1 skip=6 status=none

Zeros, and a conversion:

    dd if=/dev/zero bs=1K count=2 status=none | wc -c
    printf 'abc' | dd conv=ucase status=none; echo

A patch in place, the rest of the file kept:

    printf 'hello world\n' > h.txt
    printf 'J' | dd of=h.txt conv=notrunc status=none
    cat h.txt

## How it works

`dd` copies from `if=` (standard input without) to `of=` (standard output without) in
blocks of `bs=` bytes, 512 without. `count=N` stops after N blocks, `skip=N` skips N
blocks of the input first, and `seek=N` N blocks of the output. Sizes take `K`, `M`,
`G` (powers of 1024) and `KB`, `MB` (of 1000). `conv=` converts: `notrunc` writes into
the output without cutting it short, `ucase` and `lcase` change case, `sync` pads each
block. When it ends, `dd` prints the records copied and the speed on standard error,
unless `status=none`; `status=progress` prints as it goes.

## Options

    if=FILE          Read FILE: standard input without.
    of=FILE          Write FILE: standard output without.
    bs=BYTES         Read and write BYTES at a time: 512 without.
    ibs=, obs=       The read and write block sizes apart.
    count=N          Copy N blocks.
    skip=N           Skip N blocks of the input.
    seek=N           Skip N blocks of the output.
    conv=CONVS       notrunc, ucase, lcase, sync, noerror, fsync, ...
    iflag=FLAGS      count_bytes, skip_bytes, fullblock, ...
    oflag=FLAGS      seek_bytes, append, ...
    status=LEVEL     none, noxfer or progress.

Exit status: 0, or 1 when a file cannot be read or written.
