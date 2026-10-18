---
names: base64 base32 basenc
summary: Encode or decode base64 data.
summary base32: Encode or decode base32 data.
summary basenc: Encode or decode data in one of several bases.
kind: Bundled coreutils, from uutils
see: xxd od
---

## Syntax

    base64 [OPTION]... [FILE]
    base32 [OPTION]... [FILE]
    basenc --BASE [OPTION]... [FILE]

    base64 FILE        FILE encoded, in lines of 76
    base64 -d FILE     decoded
    base64 -w0 FILE    encoded on one line
    base32 FILE        in base32: letters and digits 2 to 7
    basenc --base16 F  in hex, or another base basenc names

## Examples

Encoding and decoding:

    echo hello | base64
    echo aGVsbG8K | base64 -d

A file there and back:

    base64 pairs.txt > pairs.b64
    base64 -d pairs.b64 | cmp - pairs.txt && echo "the same"

On one line, for a header or a variable:

    printf 'user:secret' | base64 -w0; echo

Narrower lines:

    seq 30 | base64 -w 20 | head -2

In base32, and in hex:

    echo hello | base32
    echo NBSWY3DPBI====== | base32 -d
    echo hello | basenc --base16

## How it works

`base64` turns bytes into text that survives mail and JSON: letters, digits, `+` and
`/`, four characters for each three bytes, `=` padding the end, in lines of 76. `-d`
turns it back; newlines are skipped, and with `-i` anything else that is not base64.
`-w` sets the line length, and 0 writes one line. Without FILE, or with `-`, it reads
standard input.

`base32` works alike with 32 letters and digits, `A` to `Z` and `2` to `7`, eight
characters for each five bytes. `basenc` takes the base as an option: `--base64`,
`--base64url` (`-` and `_` for `+` and `/`, safe in a URL or a file name),
`--base32`, `--base32hex`, `--base16` (hex), `--base2msbf` and `--base2lsbf` (bits),
`--z85` and `--base58`.

## Options

    -d, --decode            Decode.
    -i, --ignore-garbage    When decoding, skip what is not base64.
    -w, --wrap=COLS         Lines of COLS: 76 without, 0 for one line.

Exit status: 0, or 1 for input that is not base64.
