---
name: base64
summary: Encode or decode base64 data.
kind: Bundled coreutils, from uutils
see: xxd od
---

## Syntax

    base64 [OPTION]... [FILE]

    base64 FILE        FILE encoded, in lines of 76
    base64 -d FILE     decoded
    base64 -w0 FILE    encoded on one line

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

## How it works

`base64` turns bytes into text that survives mail and JSON: letters, digits, `+` and
`/`, four characters for each three bytes, `=` padding the end, in lines of 76. `-d`
turns it back; newlines are skipped, and with `-i` anything else that is not base64.
`-w` sets the line length, and 0 writes one line. Without FILE, or with `-`, it reads
standard input.

## Options

    -d, --decode            Decode.
    -i, --ignore-garbage    When decoding, skip what is not base64.
    -w, --wrap=COLS         Lines of COLS: 76 without, 0 for one line.

Exit status: 0, or 1 for input that is not base64.
