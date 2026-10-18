---
names: sha256sum md5sum sha1sum sha224sum sha384sum sha512sum b2sum
summary: Print or check SHA-256 checksums.
summary md5sum: Print or check MD5 checksums.
summary sha1sum: Print or check SHA-1 checksums.
summary sha224sum: Print or check SHA-224 checksums.
summary sha384sum: Print or check SHA-384 checksums.
summary sha512sum: Print or check SHA-512 checksums.
summary b2sum: Print or check BLAKE2b checksums.
kind: Bundled coreutils, from uutils
see: cksum cmp
---

## Syntax

    sha256sum [OPTION]... [FILE]...
    md5sum, sha1sum, sha224sum, sha384sum, sha512sum, b2sum: the same

    sha256sum FILE            FILE's checksum, then its name
    sha256sum FILE > F.sha    kept, to check later
    sha256sum -c F.sha        check the files a list names

## Examples

The checksum alone:

    sha256sum pairs.txt | cut -d' ' -f1

Kept, and checked later:

    sha256sum a.md b.md > sums.txt
    sha256sum -c sums.txt
    rm sums.txt

After a change, only what failed:

    sha256sum a.md b.md > sums.txt
    echo more >> b.md
    sha256sum -c --quiet sums.txt || echo "something changed"
    rm sums.txt

A file checked against a published checksum:

    sum=$(sha256sum pairs.txt | cut -d' ' -f1)
    echo "$sum  pairs.txt" | sha256sum -c

The other algorithms:

    md5sum pairs.txt | cut -d' ' -f1
    sha1sum pairs.txt | cut -d' ' -f1

## How it works

`sha256sum` reads each file, or standard input, and prints its SHA-256 checksum, 64
hex digits, then two spaces and the file's name. Two files with the same checksum have
the same bytes, for all practical purposes.

`-c` reads such lines from a file and checks each file they name, printing `NAME: OK`
or `NAME: FAILED`, and fails if any did; `--quiet` prints only the failures, and
`--status` nothing at all.

The others work alike, each with its algorithm: `md5sum` (32 digits), `sha1sum` (40),
`sha224sum`, `sha384sum`, `sha512sum`, and `b2sum` for BLAKE2b. MD5 and SHA-1 can be
forged, so a download's checksum is SHA-256 or longer; both still catch a damaged copy.

## Options

    -c, --check          Check the files a list names.
    --quiet              With -c, print only the failures.
    --status             With -c, print nothing: the status says.
    --strict             With -c, fail for a line that is not a checksum.
    -w, --warn           With -c, warn of a line that is not a checksum.
    --ignore-missing     With -c, skip the files that are not there.
    -b, --binary         Mark each name with a *, read as binary.
    -t, --text           Mark each with a space: the default.
    --tag                BSD's form: SHA256 (FILE) = checksum.
    -z, --zero           End each line with a NUL, not a newline.
    -l, --length=BITS    b2sum: a checksum of BITS, 512 without.

Exit status: 0; 1 when a file cannot be read, or with `-c` when one fails.

## Different in cash

- Git Bash's tools mark every name with a `*` (`hash *file`), as MSYS2 reads files in
  binary mode; cash's write two spaces, as they do on Linux. The checksum is the same,
  and `-c` reads lines of both kinds.
