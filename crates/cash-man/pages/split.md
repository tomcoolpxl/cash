---
name: split
summary: Split a file into pieces.
kind: Bundled coreutils, from uutils
see: csplit cat head
---

## Syntax

    split [OPTION]... [FILE [PREFIX]]

    split -l 1000 big.log        pieces of 1000 lines: xaa, xab, ...
    split -b 10M big.iso part.   pieces of 10 MiB: part.aa, part.ab, ...
    split -n 4 big.bin           four pieces of the same size
    cat x* > whole               the pieces back together
    split -l 1000 --filter='gzip > $FILE.gz' big.log
                                 each piece through gzip: xaa.gz, ...

## Examples

Pieces of four lines:

    seq 10 > n.txt
    split -l 4 n.txt
    ls x*
    cat xac

Numbered pieces, with a suffix of your own:

    seq 10 > n.txt
    split -d -l 5 --additional-suffix=.txt n.txt part-
    ls part-*

Pieces of so many bytes, and back together:

    printf 'hello world\n' > h.txt
    split -b 5 h.txt p_
    ls p_*
    cat p_* | cmp - h.txt && echo "the same"

Each piece through a command, which finds the piece's name in `$FILE`:

    seq 10 > n.txt
    split -l 4 --filter='echo "$FILE: $(wc -l) lines"' n.txt
    split -l 5 --filter='gzip > $FILE.gz' n.txt part
    ls part*

## How it works

`split` copies FILE, or standard input, into pieces named PREFIX (`x` without) and a
suffix: `aa`, `ab`, ... or with `-d` `00`, `01`, .... `-l N` makes pieces of N lines,
`-b SIZE` of SIZE bytes (`K`, `M`, `G` for powers of 1024), `-C SIZE` of at most SIZE
bytes of whole lines, and `-n N` N pieces of about the same size, `-n l/N` without
breaking lines. `-a` sets the suffix length, `--additional-suffix` adds an extension.
`cat PREFIX*` puts the pieces back together, in order.

With `--filter=COMMAND` no piece is written: each is piped into COMMAND, run by
cash with `$FILE` the name the piece would have had, so the command can name its
own output after it.

## Options

    -l, --lines=N               N lines a piece.
    -b, --bytes=SIZE            SIZE bytes a piece.
    -C, --line-bytes=SIZE       At most SIZE bytes of whole lines a piece.
    -n, --number=N              N pieces: N, l/N, or K/N for piece K alone.
    -d                          Numbers as suffixes, not letters.
    --numeric-suffixes=FROM     Numbers, starting at FROM.
    -a, --suffix-length=N       Suffixes N long: 2 without.
    --additional-suffix=SUF     Add SUF to every name.
    -e, --elide-empty-files     Make no empty pieces, with -n.
    --filter=COMMAND            Each piece into COMMAND, $FILE its name.
    --verbose                   Say each piece as it is made.

Exit status: 0, or 1 when a file cannot be read or written.

## Different in cash

- `--filter`'s COMMAND runs in cash, not in `$SHELL` or `/bin/sh`.
