---
title: Line endings: CRLF and LF
summary: Why `sed` and `awk` keep CRLF files CRLF, and how `$(...)` and `read` treat CR.
see: sed awk dos2unix unix2dos read mapfile
spec: D20 D49 D41
---
## The problem

Windows text files and many Windows programs end lines with CRLF (`\r\n`). A tool that
treats the CR as data fails in ways you cannot see: `version=$(python v.py)` holds
`1.2.0\r`, which prints as `1.2.0` but is not equal to it.

## Where cash decides where a line ends

Wherever cash itself splits text into lines, `\r\n` ends a line just as `\n` does (D20):
`$(...)` and backticks, `read` and `while read`, `mapfile` and `readarray`,
here-documents and here-strings. Bytes in a pipe between two programs are never
touched: `a.exe | b.exe` stays byte for byte. cash's own builtins write LF.

## The bundled sed and awk

By default the bundled `sed` and `awk` read a CRLF line without its CR and write it back
with it (D49): `s/foo$/bar/` and `$NF == "x"` work on Windows files, an edited CRLF file
stays CRLF, and an LF file stays LF.

The CR becomes ordinary data, as on Linux, when:

- the program names a carriage return: `sed 's/\r$//'` and `awk '{sub(/\r$/, "")} 1'`
  do what they say;
- `CASH_EOL=lf` is exported, for every run of sed and awk;
- `sed -b` (`--binary`), for one run of sed.

## Converting files

`dos2unix FILE` and `unix2dos FILE` convert a file in place. For files in a git
repository, `.gitattributes` (`* text eol=lf`) fixes the source instead.

## Other encodings

cash strips a UTF-8 byte order mark when it reads a script, and sets the console to
UTF-8 (D41).
