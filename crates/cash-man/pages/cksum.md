---
name: cksum
summary: Print the CRC checksum and size of files.
kind: Bundled coreutils, from uutils
see: sha256sum sum
---

## Syntax

    cksum [OPTION]... [FILE]...

    cksum FILE               the CRC, the size in bytes, the name
    cksum -a sha256 FILE     another algorithm: md5, sha1, sha256, ...
    cksum -a sha256 -c LIST  check the files a list names

## Examples

The CRC and the size, of a file and of standard input:

    cksum pairs.txt
    echo hi | cksum

## How it works

`cksum` prints, for each file, a CRC-32 checksum of its bytes as POSIX defines it, its
size in bytes, and its name. It tells whether a copy is whole; for security,
`sha256sum` is the tool. With `-a`, `cksum` computes another algorithm instead (`md5`,
`sha1`, `sha224`, `sha256`, `sha384`, `sha512`, `blake2b`, `sm3`, `sysv`, `bsd`), in the
`SHA256 (FILE) = ...` form, and `-c` checks a list of them as `sha256sum -c` does.

## Options

    -a, --algorithm=TYPE    crc (the default), md5, sha1, sha256, ...
    -c, --check             Check the checksums a list holds.
    --untagged              With -a, sha256sum's form.
    -l, --length=BITS       blake2b: a checksum of BITS.
    --base64                The checksum in base64, not hex.
    -z, --zero              End each line with a NUL.

Exit status: 0, or 1 when a file cannot be read or a check fails.
