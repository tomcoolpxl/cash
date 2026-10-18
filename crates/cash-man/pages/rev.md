---
name: rev
summary: Reverse the characters of each line.
kind: Text and file tools
see: tac crlf
draft: yes
---

## Examples

    echo hello | rev                       # olleh
    rev names.txt | sort | rev             # sort by the ends of the lines

## How it works

`rev` reverses the characters of each line: util-linux's, with `-0` for NUL-separated
lines. Invalid UTF-8 is kept byte by byte.

## Different in cash

### Difference from util-linux

A CRLF line keeps its CR at the end; util-linux's moves it to the front, where it
returns the cursor and hides the line.
