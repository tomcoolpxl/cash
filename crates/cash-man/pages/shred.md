---
name: shred
summary: Overwrite a file to hide its contents, and optionally delete it.
kind: Bundled coreutils, from uutils
see: rm truncate
---

## Syntax

    shred [OPTION]... FILE...

    shred FILE         overwrite FILE three times with random bytes
    shred -u FILE      and remove it
    shred -zu FILE     the last pass zeros, then remove it

## Examples

Overwritten and removed, and overwritten with zeros last:

    printf 'secret\n' > s1
    shred -u s1
    ls s1 2>/dev/null || echo "s1 is gone"
    printf 'secret\n' > s2
    shred -n 1 -z s2
    od -An -c s2 | head -1

## How it works

`shred FILE` overwrites a file's contents in place, three times with random bytes
(`-n` for another count), so that its old bytes cannot be read back from the disk; `-z`
adds a last pass of zeros to hide the shredding. A file grows to a whole number of
blocks unless `-x`. `-u` removes the file after, renaming it first so its name is
gone too; `--remove=unlink` removes it at once. `-s` shreds only so many bytes, and `-v`
says each pass.

## Options

    -n, --iterations=N     N passes of random bytes: 3 without.
    -z, --zero             A last pass of zeros.
    -u                     Remove the file after.
    --remove[=HOW]         unlink, wipe, or wipesync (the default).
    -s, --size=N           Shred N bytes only.
    -x, --exact            Do not round up to whole blocks.
    -f, --force            Make a read-only file writable first.
    -v, --verbose          Say each pass.

Exit status: 0, or 1 when a file cannot be written.

## Different in cash

- On an SSD, or with NTFS compression or a volume shadow copy, the drive may keep the
  old bytes elsewhere: shredding overwrites the file's own clusters, not those copies.
