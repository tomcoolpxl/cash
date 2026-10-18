---
name: install
summary: Copy files and set their attributes.
kind: Coreutils written for Windows
see: cp mkdir chmod
---

## Syntax

    install [OPTION]... SOURCE DEST
    install [OPTION]... SOURCE... DIRECTORY
    install -d [OPTION]... DIRECTORY...

    install prog /usr/local/bin      copy prog into a directory
    install -D prog out/bin/prog     the directories on the way too
    install -d out/bin out/lib       make directories
    install -b conf /etc/app.conf    the old file kept as app.conf~

## Examples

Directories, then a file into one:

    install -d out/bin out/lib
    printf 'echo hi\n' > prog
    install -m 755 prog out/bin
    ls out/bin

A file and the directories it needs, at once:

    printf 'x\n' > conf
    install -D conf deep/er/app.conf
    cat deep/er/app.conf

The file that was there kept, as `~` and numbered:

    printf 'v1\n' > conf
    install conf app.conf
    install -b conf app.conf
    install --backup=numbered conf app.conf
    ls app.conf*

## How it works

`install SOURCE DEST` copies a file, as `cp` does, for Makefiles and build scripts:
into DEST, or into the directory DEST names under its own name, several SOURCEs into a
DIRECTORY. `-D` makes the directories leading to DEST, `-t DIRECTORY` names the
directory first, `-T` makes DEST a file, and `-d` makes the directories named instead
of copying. The file gets the time it is copied, or with `-p` its source's times.

`-b` keeps a file that is there as `FILE~` before replacing it; `--backup=numbered`
as `FILE.~1~`, `FILE.~2~`, ...; `--backup=existing`, the default of `-b`, numbered when
numbered backups are there already. `-S SUFFIX` gives a simple backup another suffix,
and makes one by itself. `VERSION_CONTROL` and `SIMPLE_BACKUP_SUFFIX` set the defaults.
`-C` copies only when the file differs.

## Options

    -d, --directory              Make the directories named.
    -D                           Make the directories leading to DEST.
    -t, --target-directory=DIR   Copy into DIR.
    -T, --no-target-directory    DEST is a file, never a directory.
    -m, --mode=MODE              The mode: 444 and 555 make it read-only.
    -p, --preserve-timestamps    The source's times, not now.
    -b                           Back up a file that is there.
    --backup[=CONTROL]           The same: none, numbered, existing, simple.
    -S, --suffix=SUFFIX          The backup suffix: ~ without.
    -C, --compare                Copy only when the file differs.
    -v, --verbose                Say each file and directory.
    -c                           Accepted, as GNU's is; changes nothing.
    -s, --strip                  Accepted; changes nothing on Windows.
    -o, --owner=OWNER            Accepted, and said once: see below.
    -g, --group=GROUP            Accepted, and said once: see below.

Exit status: 0, or 1 when a file cannot be copied or a directory made.

## Different in cash

- Windows does not let an owner be set this way, and has no POSIX groups, so `-o` and
  `-g` say so once, as `chown` and `chgrp` do, and the files are installed as yours.
- Windows files have no mode bits: `-m` makes a file read-only when the mode gives no
  one write (444, 555), and is otherwise accepted.
- `-s` strips nothing: Windows programs keep their symbols in a `.pdb` beside them.
