---
names: dos2unix unix2dos
see: sed awk crlf
spec: D49 D20
---
## Description

`dos2unix FILE...` converts CRLF line endings to LF, in place; `unix2dos` does the
reverse. `-n IN OUT` writes a new file instead, and with no files they convert standard
input to standard output. `-i` reports what a file holds without changing it.

Git for Windows has the real tools only when its `usr/bin` is on `PATH`, so cash carries
both: the one-word fix for a CRLF file works everywhere.

## Notes

- A file is replaced by writing a temporary file beside it and renaming it over the
  original, so a failure never leaves it half written.
- What scripts use is there: `-n`, `-O`, `-k`, `-q`, `-f`, `-e`, `-l`, `-7`, the byte
  order mark options, `-i`. Code-page, UTF-16 and Mac (CR-only) conversions are refused
  by name rather than done wrongly; `enable -n dos2unix` reaches the `PATH` copy.

## Examples

```
dos2unix *.sh
git show HEAD:build.cmd | unix2dos > build.cmd
dos2unix -i notes.txt        # count the line endings
```
