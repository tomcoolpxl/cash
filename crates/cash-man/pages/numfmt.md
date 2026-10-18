---
name: numfmt
summary: Convert numbers to and from human-readable sizes.
kind: Bundled coreutils, from uutils
see: du df printf
---

## Syntax

    numfmt [OPTION]... [NUMBER]...

    numfmt --to=iec 1048576      1.0M: powers of 1024
    numfmt --to=si 1500          1.5k: powers of 1000
    numfmt --from=iec 2K         2048
    ... | numfmt --field=2 --to=iec    the second column of each line

## Examples

To sizes and back:

    numfmt --to=iec 1048576
    numfmt --to=iec-i 2048
    numfmt --from=iec 1K
    numfmt --from=si 1.5M

A column of a table, with a suffix:

    printf 'a.log 2048\nb.log 5000000\n' |
        numfmt --field=2 --to=iec --suffix=B

Padding and a format:

    numfmt --padding=8 42
    numfmt --format=%.2f 3.14159

## How it works

`numfmt` reads numbers from its arguments, or from each line of standard input, and
writes them converted. `--to=si` writes them in powers of 1000 (`1.5k`, `2.0M`),
`--to=iec` in powers of 1024 (`1.0M`), `--to=iec-i` with `Ki`, `Mi`; `--from` reads such
numbers back. `--field` picks the column of a line to convert, the rest kept as it is;
`--header` skips header lines. `--round` says how to round, `--padding` the width,
`--format` a printf format, and `--suffix` a unit added.

## Options

    --to=UNIT             Write as si, iec, iec-i, or auto.
    --from=UNIT           Read as si, iec, iec-i, or auto.
    --from-unit=N         Each input number counts N.
    --to-unit=N           Each output number counts N.
    --field=FIELDS        Which columns: 1 without.
    -d, --delimiter=X     Columns apart by X, not blanks.
    --header[=N]          Leave the first N lines as they are.
    --round=METHOD        up, down, from-zero, towards-zero, nearest.
    --padding=N           Pad to N columns; a negative N pads on the right.
    --format=FORMAT       A printf format for the number.
    --suffix=SUFFIX       Add SUFFIX to each number.
    --grouping            Group the digits, as the locale does.
    --invalid=MODE        abort (the default), fail, warn or ignore.
    -z, --zero-terminated Lines end with a NUL.

Exit status: 0; 2 for a number that cannot be converted.
