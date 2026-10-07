---
names: bzip2 bunzip2 bzcat
see: gzip
---
## Description

`bzip2 FILE...` compresses each file to `FILE.bz2` in its place, keeping its time and
its read-only bit; `bunzip2 FILE.bz2` (or `bzip2 -d`) puts it back; `bzcat FILE.bz2`
(or `bzip2 -dc`) prints it uncompressed. With no file they work from standard input to
standard output. The flags, messages and exit status (0; 1 for an error; 2 for a damaged
or foreign file) are bzip2 1.0.8's: `-c`, `-d`, `-z`, `-k`, `-f`, `-t`, `-q`, `-v`, `-s`,
`-1` to `-9`, `--fast`, `--best`, and the flags may come anywhere among the files.

Compression is libbz2-rs-sys, libbzip2's own code ported to Rust: what cash writes is
the same bytes bzip2 writes at the same block size, and every bzip2 reads it. Files of
several streams are read through; trailing bytes after the last stream are reported and
ignored, as bzip2 does.

## Windows notes

- A clean Windows machine has no `bzip2`; `tar.exe` reads a `.tar.bz2`, not a bare
  `.bz2`. Git for Windows has bzip2 only when its `usr/bin` is on `PATH`.
- The file is written beside its target under a temporary name and renamed over it, so
  an interrupted run leaves no half-written file. A file with other hard links is left
  unchanged without `-f`, as bzip2 leaves it; cash's own tool links are hard links.
- Windows has no mode bits: a read-only input gives a read-only output, and that is all.
- `-vv` says what `-v` says: bzip2's block-by-block trace is printed by libbzip2 itself.
- The words of the `BZIP2` and `BZIP` environment variables are read before the command
  line's, as bzip2 reads them.

## Examples

```
bzip2 -k big.log               # big.log.bz2 beside it
bzip2 -9 *.csv                 # each replaced by its .bz2
bzcat access.log.bz2 | grep 404
bunzip2 -c site.tar.bz2 | tar -xf -
bzip2 -tv *.bz2                # test each one
```
