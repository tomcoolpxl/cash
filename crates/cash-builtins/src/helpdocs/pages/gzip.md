---
names: gzip gunzip zcat
see: cat crlf
---
## Description

`gzip FILE...` compresses each file to `FILE.gz` in its place, keeping its time and its
read-only bit, and storing its name and time in the header; `gunzip FILE.gz` (or
`gzip -d`) puts it back; `zcat FILE.gz` (or `gzip -dc`) prints it uncompressed. With no
file, or `-`, they work from standard input to standard output. The options, messages
and exit status (0; 1 for an error; 2 for a warning) are GNU gzip 1.14's: `-c`, `-k`,
`-f`, `-l`, `-t`, `-v`, `-n`/`-N`, `-r`, `-S`, `-1` to `-9`, `--fast`, `--best`.

Compression is deflate from miniz_oxide, in Rust: nothing of zlib or of GNU's C is
built in. What it writes, every gzip reads; what every gzip writes, it reads, with the
CRC and the length of the trailer checked. The stream is not byte for byte zlib's at
the same level: at `-6` (the default) and `-9` a text file comes out the same size as
GNU gzip makes it, give or take a tenth of a percent; `-1` is the fast mode of
miniz_oxide, about fifteen percent larger than GNU gzip's `-1`.

## Windows notes

- A clean Windows machine has no `gzip`, `gunzip` or `zcat`; `tar.exe` reads archives,
  not a bare `.gz`. Git for Windows has gzip only when its `usr/bin` is on `PATH`.
- The file is written beside its target under a temporary name and renamed over it, so
  an interrupted run leaves no half-written file. A file with other hard links is left
  unchanged without `-f`, as GNU gzip leaves it; cash's own tool links are hard links.
- Windows has no mode bits: a read-only input gives a read-only output, and that is
  all. The header's OS byte says Unix, as Git for Windows' gzip writes it.
- `-r` reads a folder in name order. `-l -v` shows dates in the zone `TZ` names, else
  Windows' own.
- compress (`.Z`), pack, lzh and zip data is refused by name, not uncompressed: cash
  carries deflate only. `tar.exe` reads a `.tar.Z`.
- The `GZIP` environment variable is read as GNU gzip 1.14 reads it: a level in it
  counts, the rest is ignored.

## Examples

```
gzip -k big.log                # big.log.gz beside it
gzip -9 *.csv                  # each replaced by its .gz
zcat access.log.gz | grep 404
gunzip -c site.tar.gz | tar -xf -
gzip -l *.gz                   # sizes and ratios
tar -cf - src | gzip > src.tar.gz
```
