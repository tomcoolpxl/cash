---
names: xz unxz xzcat lzma unlzma lzcat
summary: Compress files to `.xz` in place, or uncompress or list them, with XZ Utils' options.
summary unxz: Uncompress `.xz` files in place: `xz -d`.
summary xzcat: Print `.xz` files uncompressed to standard output: `xz -dc`.
summary lzma: Compress files to `.lzma` in place, or uncompress them: `xz --format=lzma`.
summary unlzma: Uncompress `.lzma` files in place: `xz --format=lzma -d`.
summary lzcat: Print `.lzma` files uncompressed to standard output: `xz --format=lzma -dc`.
kind: Text and file tools
see: gzip bzip2 zstd tar
---

## Syntax

    xz [OPTION]... [FILE]...
    unxz, xzcat, lzma, unlzma, lzcat: the same, another default

    xz FILE            FILE.xz in its place
    xz -k FILE         FILE.xz beside it, FILE kept
    xz -9e FILE        as small as xz makes it, slowly
    unxz FILE.xz       FILE back, in its place
    xzcat FILE.xz      FILE's text, printed
    xz -l FILE.xz      its streams, blocks and ratio

## Examples

Compressed beside the file, tested, and read back:

    printf 'hello world\n' > h.txt
    xz -k h.txt
    ls h.txt*
    xz -t h.txt.xz && echo "sound"
    xzcat h.txt.xz

In place and back, and the older .lzma format:

    printf 'hello world\n' > h.txt
    xz h.txt
    unxz h.txt.xz
    xz --format=lzma -c h.txt | xz -dc

Through a pipe:

    echo "piped through" | xz | xz -d

## How it works

`xz FILE...` compresses each file to `FILE.xz` in its place, keeping its time and its
read-only bit; `unxz FILE.xz` (or `xz -d`) puts it back; `xzcat FILE.xz` (or `xz -dc`)
prints it uncompressed. `lzma`, `unlzma` and `lzcat` do the same with the older `.lzma`
format. With no file, or `-`, they work from standard input to standard output. The
options, messages and exit status are XZ Utils 5.8's.

Compression is lzma-rust2, in Rust: nothing of liblzma's C is built in. What cash writes,
every xz reads; what XZ Utils writes, cash reads: .xz with any of its checks, several
streams and their padding, .lzma, .lz and raw LZMA2. The compressed bytes are not byte
for byte liblzma's at the same preset. `-l` reads a .xz file's index and prints XZ
Utils' tables.

## Options

    -z, --compress          Compress, whatever the name.
    -d, --decompress        Uncompress.
    -t, --test              Check each file.
    -l, --list              The streams, blocks and ratio of each .xz.
    -k, --keep              Keep the input files.
    -f, --force             Overwrite, and compress what is linked.
    -c, --stdout            Write to standard output; leave the files.
    -0 ... -9               The preset: 6 without.
    -e, --extreme           Smaller still, more slowly.
    -T, --threads=N         N threads: 0, the default, is every core.
    -F, --format=FORMAT     xz, lzma, raw, or auto to read any.
    -C, --check=CHECK       crc32, crc64 (the default), sha256 or none.
    -S, --suffix=SUFFIX     SUFFIX instead of .xz.
    --single-stream         Read only the first stream.
    --block-size=SIZE       Start a new block every SIZE bytes.
    --files[=FILE]          Read the names to work on from FILE.
    --robot                 Machine-readable messages and lists.
    -q, --quiet             No warnings; twice, no errors.
    -Q, --no-warn           Exit 0, not 2, after a warning.
    -v, --verbose           Say each file's ratio.

Exit status: 0; 1 for an error; 2 for a warning.

## Different in cash

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
