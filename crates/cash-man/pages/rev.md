---
name: rev
summary: Reverse the characters of each line.
kind: Text and file tools
see: tac crlf
---

## Syntax

    rev [OPTION] [FILE]...

    rev FILE                each line of FILE, back to front
    rev | sort | rev        sort lines by their ends

## Examples

A line back to front:

    echo hello | rev

The last field, however many there are:

    echo 'C:/Users/me/report.txt' | rev | cut -d/ -f1 | rev

Sorted by their ends, so by extension:

    printf 'report.pdf\nnotes.txt\nphoto.jpg\ndata.txt\n' | rev | sort | rev

## How it works

`rev` writes each line of its files, or standard input, with its characters in reverse
order: characters, not bytes, so `é` stays whole. Invalid UTF-8 is kept byte by byte.
`-0` reads lines that end with a NUL.

## Options

    -0, --zero    Lines end with a NUL, not a newline.

Exit status: 0, or 1 for a file that cannot be read.

## Different in cash

### Difference from util-linux

A CRLF line keeps its CR at the end; util-linux's moves it to the front, where it
returns the cursor and hides the line.
