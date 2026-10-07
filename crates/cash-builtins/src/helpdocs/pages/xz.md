---
names: xz unxz xzcat lzma unlzma lzcat
see: gzip bzip2 zstd
---
## Description

`xz FILE...` compresses each file to `FILE.xz` in its place, keeping its time and its
read-only bit; `unxz FILE.xz` (or `xz -d`) puts it back; `xzcat FILE.xz` (or `xz -dc`)
prints it uncompressed. `lzma`, `unlzma` and `lzcat` do the same with the older `.lzma`
format. With no file, or `-`, they work from standard input to standard output. The
options, messages and exit status (0; 1 for an error; 2 for a warning) are XZ Utils
5.8's: `-z`, `-d`, `-t`, `-l`, `-k`, `-f`, `-c`, `-S`, `-F`, `-C`, `-0` to `-9`, `-e`,
`-T`, `-q`, `-Q`, `-v`, `--single-stream`, `--block-size`, `--files`, `--robot`.

Compression is lzma-rust2, in Rust: nothing of liblzma's C is built in. What cash writes,
every xz reads; what XZ Utils writes, cash reads: .xz with any of its checks, several
streams and their padding, .lzma, .lz and raw LZMA2. The compressed bytes are not byte
for byte liblzma's at the same preset. `-l` reads a .xz file's index and prints XZ
Utils' tables.

## Windows notes

- A clean Windows machine has no `xz`; `tar.exe` reads a `.tar.xz`, not a bare `.xz`.
- The file is written beside its target under a temporary name and renamed over it, so
  an interrupted run leaves no half-written file. A file with other hard links is left
  unchanged without `-f` or `-k`, as xz leaves it; cash's own tool links are hard links.
- Windows has no mode bits: a read-only input gives a read-only output, and that is all.
- Custom filter chains (`--filters`, `--lzma2=...`, the BCJ filters, `--delta`) are
  refused; cash compresses with the presets. As in xz, every core compresses by
  default, each a block of three dictionaries (fewer cores when a quarter of the memory
  would not hold them all), and a file of several blocks decompresses on every core;
  `-T1` is one thread and one block. The memory limits, `--block-list` and
  `--flush-timeout` are checked and change nothing.
- `-vv` says what `-v` says, and `-lvv` what `-lv` says. At a console, `-v` shows each
  file's final line.
- The words of `XZ_DEFAULTS` and `XZ_OPT` are read before the command line's, as xz reads
  them.

## Examples

```
xz -k big.log                  # big.log.xz beside it
xz -9e *.csv                   # each replaced by its .xz
xzcat access.log.xz | grep 404
unxz -c site.tar.xz | tar -xf -
xz -lv backup.tar.xz           # streams, blocks and ratios
```
