---
names: gzip gunzip zcat
summary: Compress files to `.gz` in place, or uncompress them, with GNU gzip's options.
summary gunzip: Uncompress `.gz` files in place: `gzip -d`.
summary zcat: Print `.gz` files uncompressed to standard output: `gzip -dc`.
kind: Text and file tools
see: cat crlf bzip2 xz zstd tar
---

## Syntax

    gzip [OPTION]... [FILE]...
    gunzip [OPTION]... [FILE]...
    zcat [FILE]...

    gzip FILE        FILE.gz in its place
    gzip -k FILE     FILE.gz beside it, FILE kept
    gunzip FILE.gz   FILE back, in its place
    zcat FILE.gz     FILE's text, printed
    gzip -l FILE.gz  sizes and ratio

## Examples

Compressed beside the file, checked, and read back:

    printf 'hello world\n' > h.txt
    gzip -k h.txt
    ls h.txt*
    gzip -t h.txt.gz && echo "the archive is sound"
    zcat h.txt.gz

In place and back:

    printf 'hello world\n' > h.txt
    gzip h.txt
    ls h.txt*
    gunzip h.txt.gz
    cat h.txt

Through a pipe:

    echo "piped through" | gzip | gzip -d

A directory's files, each in its place:

    mkdir -p logs; printf 'a\n' > logs/1.log; printf 'b\n' > logs/2.log
    gzip -r logs
    ls logs

## How it works

`gzip FILE...` compresses each file to `FILE.gz` in its place, keeping its time and its
read-only bit, and storing its name and time in the header; `gunzip FILE.gz` (or
`gzip -d`) puts it back; `zcat FILE.gz` (or `gzip -dc`) prints it uncompressed. With no
file, or `-`, they work from standard input to standard output. The options, messages
and exit status (0; 1 for an error; 2 for a warning) are GNU gzip 1.14's.

Compression is deflate from miniz_oxide, in Rust: nothing of zlib or of GNU's C is
built in. What it writes, every gzip reads; what every gzip writes, it reads, with the
CRC and the length of the trailer checked. The stream is not byte for byte zlib's at
the same level: at `-6` (the default) and `-9` a text file comes out the same size as
GNU gzip makes it, give or take a tenth of a percent; `-1` is the fast mode of
miniz_oxide, about fifteen percent larger than GNU gzip's `-1`.

## Options

    -c, --stdout       Write to standard output; leave the files.
    -d, --decompress   Uncompress, as gunzip does.
    -k, --keep         Keep the input files.
    -f, --force        Overwrite, and compress what is linked or compressed.
    -l, --list         The sizes and ratio of each .gz.
    -t, --test         Check each .gz.
    -r, --recursive    The files in directories too.
    -v, --verbose      Say each file's ratio.
    -q, --quiet        No warnings.
    -n, --no-name      Store no name or time; do not restore them.
    -N, --name         Store, and restore, the name and time.
    -S, --suffix=SUF   SUF instead of .gz.
    -1 ... -9          Faster to smaller: 6 without.
    --fast, --best     -1 and -9.

Exit status: 0; 1 for an error; 2 for a warning.

## Different in cash

- A clean Windows machine has no `gzip`, `gunzip` or `zcat`; `tar.exe` reads archives,
  not a bare `.gz`. Git for Windows has gzip only when its `usr/bin` is on `PATH`.
- The file is written beside its target under a temporary name and renamed over it, so
  an interrupted run leaves no half-written file. A file with other hard links is left
  unchanged without `-f`, as GNU gzip leaves it; cash's own tool links are hard links.
- Windows has no mode bits: a read-only input gives a read-only output, and that is
  all. The header's OS byte says Unix, as Git for Windows' gzip writes it.
- `-r` reads a directory in name order. `-l -v` shows dates in the zone `TZ` names, else
  Windows' own.
- compress (`.Z`), pack, lzh and zip data is refused by name, not uncompressed: cash
  carries deflate only. `tar.exe` reads a `.tar.Z`.
- The `GZIP` environment variable is read as GNU gzip 1.14 reads it: a level in it
  counts, the rest is ignored.
- Input over 1 MiB is compressed on every core, as pigz lays it out: 1 MiB chunks
  deflated apart, in one member any gzip reads. The bytes are the same on any machine;
  input of one chunk is what one thread writes.
