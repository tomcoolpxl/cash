---
name: sum
summary: Print the checksum and block count of files.
kind: Bundled coreutils, from uutils
see: cksum sha256sum
---

## Syntax

    sum [OPTION]... [FILE]...

    sum FILE       BSD's 16-bit checksum and the size in 1 KiB blocks
    sum -s FILE    System V's checksum and the size in 512-byte blocks

## Examples

Both checksums of a file:

<!-- parse only: Git Bash's sum leaves the name out for one file -->

    sum pairs.txt
    sum -s pairs.txt

## How it works

`sum` prints an old 16-bit checksum of each file and its size in blocks, followed by
its name: BSD's algorithm with 1024-byte blocks (`-r`, the default), or System V's with
512-byte blocks (`-s`). It is kept for old scripts that compare its output; `cksum` and
`sha256sum` are the tools today.

## Options

    -r             BSD's checksum, 1 KiB blocks: the default.
    -s, --sysv     System V's checksum, 512-byte blocks.

Exit status: 0, or 1 for a file that cannot be read.
