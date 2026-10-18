---
name: basename
summary: Strip the directory, and a suffix, from a path.
kind: Bundled coreutils, from uutils
see: dirname realpath paths
---

## Syntax

    basename PATH [SUFFIX]
    basename -s SUFFIX -a PATH...

    basename /a/b/file.txt          file.txt
    basename /a/b/file.txt .txt     file
    basename -a /x/one /y/two       one and two, a line each

## Examples

The last part of a path:

    basename /home/me/notes.txt
    basename C:/Users/me/src/

Without a suffix:

    basename report.tar.gz .tar.gz
    basename -s .txt notes.txt pairs.txt

Each file's name without its extension, in a loop:

    for f in notes.txt pairs.txt; do echo "$(basename "$f" .txt)"; done

The shell can do the same without a program:

    f=/a/b/file.txt
    echo "${f##*/}"
    echo "${f%.txt}"

## How it works

`basename` writes PATH without its directories and without a trailing `/`; with
SUFFIX, without that ending too, unless it is all that is left. `-a` takes several
PATHs, `-s` gives the suffix for all of them.

It works on the text alone and looks at no file: the path need not exist.
`${f##*/}` and `${f%.ext}` do the same in the shell, without starting anything.

## Options

    -a, --multiple       Every argument is a PATH.
    -s, --suffix=SUFFIX  Remove SUFFIX from each (implies -a).
    -z, --zero           End each with a NUL byte, not a newline.

Exit status: 0, or 1 without a PATH.

## Different in cash

- A backslash separates directories too, so `basename 'C:\a\b.txt'` gives `b.txt`,
  as Windows paths need.
