---
name: df
summary: Show free disk space on file systems.
kind: Bundled coreutils, from uutils
see: du free
---

## Syntax

    df [OPTION]... [FILE]...

    df -h          every drive, its size, used and free, in K, M, G
    df -h .        the drive the current directory is on
    df -T          with each file system's type

## Examples

How much is free where you are:

<!-- parse only: sizes of the reader's own drives -->

    df -h .
    df -h --output=avail . | tail -1
    df -T

The columns a drive has:

    df -P . | head -1 | tr -s ' '

## How it works

`df` lists the file systems, or the ones FILE are on, with their size, the space used
and free, and the share used, in blocks of 1024 bytes. `-h` picks a unit for each
number, `K`, `M`, `G` in powers of 1024, `-H` in powers of 1000. `-T` adds each file
system's type, `-P` the POSIX layout, one line each, and `--output` the columns
named: `source`, `fstype`, `size`, `used`, `avail`, `pcent`, `target`. `--total` adds a
line of totals.

## Options

    -h, --human-readable    Sizes in K, M, G, powers of 1024.
    -H, --si                The same, powers of 1000.
    -T, --print-type        Each file system's type.
    -P, --portability       POSIX's layout.
    -t, --type=TYPE         Only file systems of TYPE.
    -x, --exclude-type=TYPE Not file systems of TYPE.
    --output[=FIELDS]       The columns named.
    --total                 A line of totals.
    -a, --all               File systems of size 0 too.

Exit status: 0, or 1 when a FILE is not there.

## Different in cash

- Each drive is a file system, `C:\` and the rest, mounted at its letter.
- `-i` is refused: Windows' file systems count no inodes.
