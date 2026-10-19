---
names: zstd unzstd zstdcat
summary: Compress files to `.zst`, or uncompress or list them, with zstd's options.
summary unzstd: Uncompress `.zst` files: `zstd -d`.
summary zstdcat: Print `.zst` files uncompressed to standard output: `zstd -dcf`.
kind: Text and file tools
see: gzip bzip2 xz tar
---

## Syntax

    zstd [OPTION]... [FILE]...
    unzstd, zstdcat: the same, another default

    zstd FILE           FILE.zst beside it, FILE kept
    zstd --rm FILE      FILE.zst in its place
    zstd -d FILE.zst    FILE back, FILE.zst kept
    zstdcat FILE.zst    FILE's text, printed
    zstd -l FILE.zst    its frames, sizes and ratio

## Examples

Compressed beside the file, tested, and read back:

<!-- parse only: Git Bash has no zstd to compare with -->

    printf 'hello world\n' > h.txt
    zstd -q h.txt
    ls h.txt*
    zstd -tq h.txt.zst && echo "sound"
    zstdcat h.txt.zst
    zstd -dc site.tar.zst | tar -xf -

## How it works

`zstd FILE...` compresses each file to `FILE.zst` beside it and keeps the original, as
zstd does (`--rm` removes it); `zstd -d FILE.zst` (or `unzstd`) puts it back; `zstdcat
FILE.zst` (or `zstd -dc`) prints it uncompressed, and prints a file that is not
compressed as it is. With no file, or `-`, they work from standard input to standard
output. The options, messages and exit status are zstd 1.5.7's.

Compression is ruzstd, in Rust: nothing of libzstd's C is built in. What cash writes,
every zstd reads, with its content size and checksum; what zstd writes, cash reads,
skippable frames and several frames in a file included. `zstd -d` also reads `.gz`,
`.xz` and `.lzma` files, as zstd does. `-l` lists the frames of a file.

## Options

    -#                         The level, 1 to 19: 3 without.
    --ultra                    Levels up to 22.
    --fast[=N]                 The fast levels.
    -d, --decompress           Uncompress.
    -t, --test                 Check each file.
    -l, --list                 The frames, sizes and ratio of each .zst.
    -c, --stdout               Write to standard output.
    -o FILE                    Write to FILE.
    -k, --keep                 Keep the input files: the default.
    --rm                       Remove them once done.
    -f, --force                Overwrite; pass through what is not zstd.
    -r                         The files in directories too.
    --filelist=LIST            Read the names to work on from LIST.
    --output-dir-flat=DIR      Write into DIR.
    --output-dir-mirror=DIR    Write into DIR, the directories mirrored.
    --[no-]check               A checksum in each frame: on without.
    --exclude-compressed       Leave files that are compressed already.
    --format=FORMAT            zstd (the default), gzip, xz or lzma.
    -T#                        # threads: 0, the default, is every core.
    -q, --quiet                No warnings; twice, no errors.
    -v, --verbose              Say more of each file.

Exit status: 0, or 1 for an error.

## Different in cash

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
