---
names: bzip2 bunzip2 bzcat
summary: Compress files to `.bz2` in place, or uncompress them, with bzip2's flags.
summary bunzip2: Uncompress `.bz2` files in place: `bzip2 -d`.
summary bzcat: Print `.bz2` files uncompressed to standard output: `bzip2 -dc`.
kind: Text and file tools
see: gzip xz zstd tar
---

## Syntax

    bzip2 [FLAG]... [FILE]...
    bunzip2 [FLAG]... [FILE]...
    bzcat [FILE]...

    bzip2 FILE          FILE.bz2 in its place
    bzip2 -k FILE       FILE.bz2 beside it, FILE kept
    bunzip2 FILE.bz2    FILE back, in its place
    bzcat FILE.bz2      FILE's text, printed

## Examples

Compressed beside the file, tested, and read back:

    printf 'hello world\n' > h.txt
    bzip2 -k h.txt
    ls h.txt*
    bzip2 -t h.txt.bz2 && echo "sound"
    bzcat h.txt.bz2

In place and back:

    printf 'hello world\n' > h.txt
    bzip2 h.txt
    bunzip2 h.txt.bz2
    cat h.txt

Through a pipe:

    echo "piped through" | bzip2 | bzip2 -d

## How it works

`bzip2 FILE...` compresses each file to `FILE.bz2` in its place, keeping its time and
its read-only bit; `bunzip2 FILE.bz2` (or `bzip2 -d`) puts it back; `bzcat FILE.bz2`
(or `bzip2 -dc`) prints it uncompressed. With no file they work from standard input to
standard output. The flags, messages and exit status are bzip2 1.0.8's, and the flags
may come anywhere among the files.

Compression is libbz2-rs-sys, libbzip2's own code ported to Rust: what cash writes is
the same bytes bzip2 writes at the same block size, and every bzip2 reads it. Files of
several streams are read through; trailing bytes after the last stream are reported and
ignored, as bzip2 does.

## Options

    -c, --stdout        Write to standard output; leave the files.
    -d, --decompress    Uncompress, as bunzip2 does.
    -z, --compress      Compress, whatever the name.
    -k, --keep          Keep the input files.
    -f, --force         Overwrite, and compress what is linked.
    -t, --test          Check each .bz2.
    -q, --quiet         No warnings.
    -v, --verbose       Say each file's ratio.
    -s, --small         Use less memory.
    -1 ... -9           Blocks of 100 to 900 kB: 9 without.
    --fast, --best      -1 and -9.

Exit status: 0; 1 for an error; 2 for a damaged or foreign file.

## Different in cash

- A clean Windows machine has no `bzip2`; `tar.exe` reads a `.tar.bz2`, not a bare
  `.bz2`. Git for Windows has bzip2 only when its `usr/bin` is on `PATH`.
- The file is written beside its target under a temporary name and renamed over it, so
  an interrupted run leaves no half-written file. A file with other hard links is left
  unchanged without `-f`, as bzip2 leaves it; cash's own tool links are hard links.
- Windows has no mode bits: a read-only input gives a read-only output, and that is all.
- `-vv` says what `-v` says: bzip2's block-by-block trace is printed by libbzip2 itself.
- The words of the `BZIP2` and `BZIP` environment variables are read before the command
  line's, as bzip2 reads them.
- Input larger than one block is compressed on every core, as pbzip2 lays it out: a
  stream for each block's input (`-1` to `-9`, 100 to 900 kB), one after another, which
  any bzip2 reads as one file. Input of one block is the stream bzip2 writes, byte for
  byte.
