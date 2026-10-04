---
see: tac crlf
spec: D55 D20
---
## Description

`rev` reverses the characters of each line: util-linux's, with `-0` for NUL-separated
lines. Invalid UTF-8 is kept byte by byte.

## Difference from util-linux

A CRLF line keeps its CR at the end (D20); util-linux's moves it to the front, where it
returns the cursor and hides the line.

## Examples

```
echo hello | rev                       # olleh
rev names.txt | sort | rev             # sort by the ends of the lines
```
