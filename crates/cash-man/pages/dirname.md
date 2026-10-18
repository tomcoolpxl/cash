---
name: dirname
summary: Strip the last part from a path.
kind: Bundled coreutils, from uutils
see: basename realpath paths
---

## Syntax

    dirname PATH...

    dirname /a/b/file.txt     /a/b
    dirname file.txt          .

## Examples

The directory a path is in:

    dirname /home/me/notes.txt
    dirname src/app/main.rs

A name alone is in the current directory:

    dirname notes.txt

The directory a script is in, a common opening line:

<!-- parse only: depends on where the script is -->
    here=$(dirname "$0")

The shell's own way, without a program:

    f=/a/b/file.txt
    echo "${f%/*}"

## How it works

`dirname` writes PATH without its last part and the `/` before it: what is left is
the directory the last part is in, or `.` when there is none. With several PATHs it
writes a line for each.

It works on the text alone and looks at no file. `${f%/*}` does the same in the shell,
though for a name without a `/` it gives the name back, where `dirname` gives `.`.

## Options

    -z, --zero    End each with a NUL byte, not a newline.

Exit status: 0, or 1 without a PATH.

## Different in cash

- A backslash separates directories too: `dirname 'C:\a\b.txt'` gives `C:\a`.
