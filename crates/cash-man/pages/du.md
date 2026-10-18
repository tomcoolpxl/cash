---
name: du
summary: Show how much space files and directories take.
kind: Bundled coreutils, from uutils
see: df ls find
---

## Syntax

    du [OPTION]... [FILE]...

    du -sh DIR          one total, in K, M and G
    du -h -d 1 DIR      each directory one level down
    du -ah DIR          every file too
    du -sh * | sort -h  what is largest, here

## Examples

The total of a directory, sized for reading:

    mkdir -p d && seq 2000 > d/a.txt
    du -sh d | cut -f2

Each part one level down, and the largest:

    mkdir -p big small && seq 20000 > big/f && seq 10 > small/f
    du -s big small | sort -n | tail -1 | cut -f2

Apparent sizes, in bytes:

    printf 'hello' > h.txt
    du -b h.txt

## How it works

`du` adds up the space each FILE takes, a directory with everything in it, and writes
the size and the name, for each directory under it too. By default sizes are in
1024-byte blocks; `-h` writes them as `4.0K`, `12M`, `1.5G`. `-s` writes one total for
each FILE, `-d N` goes N levels down, and `-a` lists every file, not only directories.

Space taken is in whole blocks of the file system, so a small file takes 4K;
`--apparent-size` (or `-b`, in bytes) counts what the files hold instead.

## Options

    -h, --human-readable    Sizes in K, M, G.
    -s, --summarize         One total for each FILE.
    -d, --max-depth=N       Go no more than N levels down.
    -a, --all               Every file, not only directories.
    -c, --total             A grand total at the end.
    -b, --bytes             Apparent sizes, in bytes.
    --apparent-size         What the files hold, not the space taken.
    -k, -m                  In KiB or MiB.
    --exclude=PATTERN       Leave out what matches PATTERN.
    -x, --one-file-system   Stay on one drive.
    --time                  The latest modification time beside each.

Exit status: 0, or 1 when something cannot be read.

## Different in cash

- It is uutils' `du`, carried inside cash, so it is the same on every machine, with or
  without Git or MSYS2.
- The space taken is what Windows reports for each file, compressed and sparse files
  counted at their size on disk.
