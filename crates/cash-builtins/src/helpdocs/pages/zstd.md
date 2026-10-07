---
names: zstd unzstd zstdcat
see: gzip bzip2 xz
---
## Description

`zstd FILE...` compresses each file to `FILE.zst` beside it and keeps the original, as
zstd does (`--rm` removes it); `zstd -d FILE.zst` (or `unzstd`) puts it back; `zstdcat
FILE.zst` (or `zstd -dc`) prints it uncompressed, and prints a file that is not
compressed as it is. With no file, or `-`, they work from standard input to standard
output. The options, messages and exit status are zstd 1.5.7's: `-#`, `--fast`,
`--ultra`, `-d`, `-t`, `-l`, `-c`, `-o`, `-k`, `--rm`, `-f`, `-q`, `-v`, `-r`,
`--filelist`, `--output-dir-flat`, `--output-dir-mirror`, `--[no-]check`,
`--[no-]pass-through`, `--exclude-compressed`, `--format=zstd|gzip|xz|lzma`.

Compression is ruzstd, in Rust: nothing of libzstd's C is built in. What cash writes,
every zstd reads, with its content size and checksum; what zstd writes, cash reads,
skippable frames and several frames in a file included. `zstd -d` also reads `.gz`,
`.xz` and `.lzma` files, as zstd does. `-l` lists the frames of a file.

## Windows notes

- A clean Windows machine has no `zstd`; `tar.exe` reads a `.tar.zst`, not a bare `.zst`.
- Every level compresses at ruzstd's fast level, about zstd's `-1`: files come out
  larger than zstd's at its default `-3`. The levels are read and checked as zstd checks
  them; `--long`, `--adapt`, `-B` and zstd's other tuning change nothing. `-T` is
  zstd's: `-T4` compresses 4 MiB frames on four cores, `-T1` on one, the same bytes for
  any number. Without `-T` or `ZSTD_NBTHREADS`, every core, where zstd uses one.
- Dictionaries (`-D`, `--train`, `--patch-from`), the benchmark (`-b`) and lz4 are
  refused.
- The file is written beside its target under a temporary name and renamed over it, so
  an interrupted run leaves no half-written file.
- Windows has no mode bits: a read-only input gives a read-only output, and that is all.
- `-vv` says what `-v` says, and no progress counter is drawn at a console.
- `ZSTD_CLEVEL` sets the level, as in zstd.

## Examples

```
zstd big.log                   # big.log.zst beside it, big.log kept
zstd --rm *.csv                # each replaced by its .zst
zstdcat access.log.zst | grep 404
zstd -dc site.tar.zst | tar -xf -
zstd -l backup.tar.zst         # frames, sizes and ratio
```
