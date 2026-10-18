---
name: unlink
summary: Remove one file.
kind: Bundled coreutils, from uutils
see: rm link
---

## Syntax

    unlink FILE

    unlink a.txt    remove the name a.txt

## Examples

One name of two gone, the file still there by the other:

    printf 'hello\n' > a.txt
    link a.txt b.txt
    unlink a.txt
    cat b.txt

A file that is not there:

    unlink nosuch.txt 2>/dev/null || echo "nothing to remove"

## How it works

`unlink FILE` removes one name of a file: the file itself goes when its last name
does. It takes exactly one operand and no options, and does not remove directories;
`rm` takes several files and options.

## Options

None.

Exit status: 0, or 1 when FILE cannot be removed.
