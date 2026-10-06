---
see: od hexdump
---
## Description

`xxd` is vim's hex dump: the position, the bytes in hex, and the printable ones as
text. `-c` sets the bytes per line (16), `-g` the bytes per group (2), `-e` dumps
little-endian words (groups of 4), `-b` bits, `-p` plain hex with nothing else, `-i` a
C array named after the file (`-n` names it, `-C` in capitals, `-t` adds a final zero),
`-u` upper-case hex, `-d` decimal positions, `-o` adds to the positions shown, `-s`
starts further in (`-s -N` from the end of a file), `-l` stops after so many bytes, and
`-a` folds a run of all-zero lines into one `*`.

`xxd -r` reads a dump back into bytes: each line's position decides where its bytes go,
so an edited dump patches the output file in place and gaps are zero-filled. `-r -p`
reads plain hex, `-r -b` bits, and `-r -s N` moves everything by N. An output file
argument writes there.

`-R always` colours the hex and text by byte class as xxd does; the default colours only
a terminal, and never with `NO_COLOR` set.

## Difference from vim's xxd

`-E` (EBCDIC) is refused, and standard input can be skipped forward with `-s` but not
seeked backward, as on a pipe.

## Examples

```
xxd file.bin | less                    # browse a binary
xxd -l 64 -g 1 file.bin                # the first 64 bytes, one per group
printf 'hello' | xxd -p                # 68656c6c6f
xxd -i logo.png > logo.h               # a C array for embedding
xxd file.bin > dump.txt; xxd -r dump.txt file.bin   # edit, then write back
```
