---
name: tar
summary: Pack files into an archive, or list or unpack one, with GNU tar's options.
kind: Text and file tools
see: gzip bzip2 xz zstd zip 7z
---

## Syntax

    tar -c|-t|-x [-f ARCHIVE] [OPTION]... [FILE]...

    tar -czf A.tar.gz DIR       pack DIR into a gzip-compressed archive
    tar -tf A.tar.gz            list what is in it
    tar -xf A.tar.gz            unpack it here
    tar -xf A.tar.gz -C DIR     unpack it into DIR
    tar -caf A.tar.zst DIR      compress as the name's suffix says

## Examples

Pack a directory into a compressed archive, and list it:

    mkdir -p site && cp notes.txt pairs.txt site/
    tar -czf site.tar.gz site
    tar -tf site.tar.gz

Unpack it into another directory:

    mkdir -p site && cp notes.txt site/
    tar -czf site.tar.gz site
    mkdir restore && tar -xf site.tar.gz -C restore
    ls restore/site

Unpack without the directory at the top of every name:

    mkdir -p site && cp notes.txt pairs.txt site/
    tar -czf site.tar.gz site
    mkdir flat && tar -xf site.tar.gz -C flat --strip-components=1
    ls flat

Leave some files out:

    tar -cf texts.tar --exclude='p*' *.txt
    tar -tf texts.tar

Unpack one file from an archive:

    tar -cf all.tar notes.txt pairs.txt list.txt
    mkdir one && tar -xf all.tar -C one pairs.txt
    ls one

## How it works

`tar` packs files and directories into one archive file (`-c`), lists one (`-t`) or
unpacks one (`-x`); `-f ARCHIVE` names the archive, and `-f -` is standard input or
output. Directories are packed with everything in them. The names in the archive are
the names as given, so `tar -cf a.tar site` stores `site/...`, and unpacking makes
`site` again where it runs, or in the directory `-C` names.

`-z` compresses with gzip, `-j` bzip2, `-J` xz, `--zstd` zstd; `-a` takes the
compression from the archive's suffix (`.tar.gz`, `.tgz`, `.tar.xz`, `.tar.zst`,
...). Reading needs none of these: `tar -xf` finds the compression by itself.

The options can be written together after one dash, `-czf`, or GNU's long way,
`--create --gzip --file=a.tar.gz`; the old form without a dash, `tar czf`, works too.
`f` takes the next word, so it comes last in a group.

## Options

### What to do

    -c, --create          Make an archive.
    -t, --list            List an archive.
    -x, --extract         Unpack an archive.
    -r, --append          Add files at the end of an archive.
    -u, --update          Add files newer than their copies in it.
    -A, --concatenate     Add the members of other archives to it.
    --delete              Remove members from an archive.
    -d, --diff            Compare an archive with the files.

### Which archive and files

    -f, --file=ARCHIVE    The archive; - for standard input or output.
    -C, --directory=DIR   Change to DIR first: unpack there, or pack
                          names relative to it.
    -T, --files-from=F    Read the names to pack from F, one a line.
    --exclude=PATTERN     Leave out files whose names match PATTERN.
    -X, --exclude-from=F  Read patterns to leave out from F.
    --wildcards           Match the names asked for as patterns.

### Compression

    -z, --gzip            gzip (.gz).
    -j, --bzip2           bzip2 (.bz2).
    -J, --xz              xz (.xz).
    --zstd                zstd (.zst).
    --lzma, --lzip        lzma (.lzma), lzip (.lz).
    -a, --auto-compress   As the archive's suffix says.

### Names and contents

    --strip-components=N  Drop the first N directories of each name.
    --one-top-level[=DIR] Unpack everything under one directory.
    --transform=EXPR      Rename members with a sed s/// expression.
    -k, --keep-old-files  Do not overwrite files that are there.
    --owner, --group, --mode, --mtime
                          What to store as each member's owner, mode
                          and time.
    --sort=name           Pack in name order.
    --format=FORMAT       gnu (the default), oldgnu, ustar, posix, v7.

### Output

    -v, --verbose         Name each file; with -t, list as ls -l does.
    --totals              Say how many bytes were written.

Exit status: 0 on success; 1 when some files differ (`-d`) or changed while being
packed; 2 on a fatal error.

## Advanced

### Long listings

`tar -tvf ARCHIVE` lists like `ls -l`: mode, owner, size, time and name.

### Reading from a pipe

`curl -sL URL | tar -xzf -` unpacks a download without saving it; `tar -cf - DIR |
ssh host 'tar -xf -'` copies a directory through a pipe.

### The same bytes as GNU tar

The options, messages, listings and exit status are GNU tar 1.35's, and so are the
archive's bytes for the same options and files. `TAR_OPTIONS` and `TAPE` are read.

## Different in cash

- It is cash's own, in Rust: no `tar.exe`, `gzip` or other program is started, and
  compression runs on every core. What cash writes, Windows' `tar.exe` reads, and
  what `tar.exe` writes, cash reads.
- Windows has its own `tar.exe` (bsdtar), with other options and messages. `tar` in
  cash is GNU's; `"$SYSTEMROOT/System32/tar.exe"` or `enable -n tar` reach Windows'.
- The owner stored is the Windows account: its name as owner and group, its RID as
  the number (`id -u`); Git Bash stores Cygwin's number, `197609` and up.
- Windows has no mode bits, so a member's mode is made from the file: `rw-rw-r--`,
  `r--r--r--` when it is read-only, with `x` for a directory or a program (an
  extension in `PATHEXT`, or a `#!` line).
