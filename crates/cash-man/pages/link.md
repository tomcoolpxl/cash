---
name: link
summary: Make a hard link to a file.
kind: Bundled coreutils, from uutils
see: ln unlink
---

## Syntax

    link FILE NEWNAME

    link a.txt b.txt    b.txt a second name for a.txt

## Examples

A second name, the same file:

    printf 'hello\n' > a.txt
    link a.txt b.txt
    echo more >> b.txt
    cat a.txt

## How it works

`link FILE NEWNAME` makes NEWNAME a hard link to FILE: a second name for the same file,
whose contents change through either name. It takes exactly two operands and no
options; `ln` does the same with options, and makes symbolic links too.

## Options

None.

Exit status: 0, or 1 when the link cannot be made.

## Different in cash

- A hard link must be on the same drive as its file, and NTFS keeps at most 1024 names
  for one file.
