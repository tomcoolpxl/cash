---
see: od xxd
---
## Description

`hexdump` is util-linux's. Without options it shows two-byte hex words, as `-x` does;
`-C` is the canonical hex and text layout, `-b` one-byte octal, `-c` one-byte
characters, `-d` two-byte decimal, `-o` two-byte octal and `-X` one-byte hex. `-n`
limits the bytes read, `-s` skips a number of them (with `k`, `M`, `G` and `KiB`/`KB`
suffixes), and `-v` prints every block where identical ones would otherwise fold into a
`*`. Several files are read as one.

`-e` takes a format of your own, `-f` a file of them: units of `[count][/bytes] "text"`
where the text is a printf format. `%x`, `%d`, `%o`, `%u` take 1, 2, 4 or 8 bytes,
`%e`/`%f`/`%g` 4 or 8, `%c` one, `%s` the unit's byte count or its precision; `%_a[dox]`
prints the position, `%_A[dox]` only once at the end, `%_c` a byte as a C escape, `%_p`
as a printable character or `.`, `%_u` by its control name. A last unit without a count
is repeated to fill the block, which is as long as the longest format.

## Difference from util-linux

`-s` on standard input skips the bytes, where util-linux cannot seek a pipe. A file that
cannot be read gives status 1. Colour is not printed: `-L=never` and `-L=auto` are
accepted, `-L=always` refused, and `_L[...]` in a format is read and ignored.

## Examples

```
hexdump -C file.bin | less                        # the classic layout
hexdump -n 16 -s 0x200 -C disk.img                # 16 bytes at offset 512
hexdump -v -e '16/1 "%02x " "\n"' file.bin        # hex, every line
hexdump -e '"%08_ax  " 4/4 "%08x " "\n"' file.bin # 32-bit words, with positions
hexdump -e '/1 "%_c\n"' file.txt                  # one character per line, escaped
```
