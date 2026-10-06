---
see: dos2unix crlf
---
## Description

`iconv -f FROM -t TO [FILE...]` converts text from one character set to another, with
glibc's options: `-c` drops what cannot be converted (and exits 1), `-o FILE` writes
there, `-l` lists the names, and `//TRANSLIT` or `//IGNORE` on the target name
approximates or drops instead of stopping. Both sets default to UTF-8. Standard input is
read when there is no file, or for `-`. A byte that is not a character stops the
conversion with its position in the file, and the output up to it is still written.

## Windows notes

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

## Examples

```
iconv -f CP1252 -t UTF-8 old.txt > new.txt
iconv -f UTF-16 -t UTF-8 export.csv | head         # a BOM picks the byte order
iconv -c -f UTF-8 -t ASCII names.txt               # drop what ASCII lacks
iconv -f UTF-8 -t ASCII//TRANSLIT names.txt        # approximate it instead
iconv -l | sed -n '/^CP/p'                         # the code pages by name
```
