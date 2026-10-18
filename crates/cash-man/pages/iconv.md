---
name: iconv
summary: Convert text between character sets, through Windows' code pages.
kind: Text and file tools
see: dos2unix crlf
---

## Syntax

    iconv [-c] -f FROM -t TO [FILE]...
    iconv -l

    iconv -f CP1252 -t UTF-8 F      an old Windows file to UTF-8
    iconv -f UTF-8 -t UTF-16LE F    for a program that wants UTF-16
    iconv -c -f UTF-8 -t ASCII F    drop what ASCII lacks
    iconv -l                        the names it knows

## Examples

UTF-8 to Latin-1 and back:

    printf 'caf\xc3\xa9\n' | iconv -f UTF-8 -t LATIN1 | od -An -c
    printf 'caf\xe9\n' | iconv -f LATIN1 -t UTF-8

UTF-16, two bytes a character:

    printf 'hi\n' | iconv -f UTF-8 -t UTF-16LE | od -An -tx1

Dropping what cannot be converted, and stopping at it:

    printf 'a\xffb\n' | iconv -c -f UTF-8 -t UTF-8
    printf '\xff\n' | iconv -f UTF-8 -t ASCII 2>/dev/null || echo "stopped"

## How it works

`iconv -f FROM -t TO [FILE...]` converts text from one character set to another, with
glibc's options: `-c` drops what cannot be converted (and exits 1), `-o FILE` writes
there, `-l` lists the names, and `//TRANSLIT` or `//IGNORE` on the target name
approximates or drops instead of stopping. Both sets default to UTF-8. Standard input is
read when there is no file, or for `-`. A byte that is not a character stops the
conversion with its position in the file, and the output up to it is still written.

## Options

    -f, --from-code=SET    The character set of the input: UTF-8 without.
    -t, --to-code=SET      The character set to write: UTF-8 without.
    -c                     Drop what cannot be converted.
    -o, --output=FILE      Write to FILE.
    -s, --silent           Say nothing of what was dropped.
    -l, --list             List the character sets by name.

Exit status: 0; 1 when something could not be converted, or with `-c` was dropped.

## Different in cash

- The conversions are Windows' own code pages, by their iconv names: `CP1252` is code
  page 1252, `LATIN1` 28591, `SHIFT_JIS` 932, `GBK` 936, `KOI8-R` 20866, and any
  `CPnnn`, `IBMnnn` or `WINDOWS-nnn` the machine has. UTF-8, UTF-16, UTF-32, UCS-2,
  UCS-4, ASCII and ISO-8859-10, -14 and -16 are converted by cash itself.
- `UTF-16` and `UTF-32` read a byte-order mark and write one (little-endian, as glibc
  does here); `UTF-16LE`, `UTF-16BE` and the like never write one, and keep one read as
  the character U+FEFF. A UTF-8 mark is a character too.
- `//TRANSLIT` uses Windows' best-fit mappings and then `?`, so `€` to ASCII becomes `?`
  where glibc writes `EUR`.
- A code page's table is Windows': CP1252's `0x81` is U+0081 rather than an error, and
  `SHIFT_JIS` is Microsoft's variant.
- The shift-state encodings (ISO-2022-JP, ISO-2022-KR, HZ, UTF-7) are converted whole,
  so an error's position in them is approximate.
- Line endings pass through unchanged; `dos2unix` is for those.
