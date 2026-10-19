---
name: sync
summary: Flush cached writes to disk.
kind: Coreutils written for Windows
see: dd
---

## Syntax

    sync [OPTION] [FILE]...

    sync           every drive Windows lets this shell flush
    sync FILE      FILE's data and metadata, written to disk
    sync -d FILE   FILE's data
    sync -f FILE   the drive FILE is on

## Examples

A file written, then flushed, and a file that is not there:

<!-- parse only: Git Bash's sync cannot open a file to flush it -->

    printf 'important\n' > data.txt
    sync data.txt && echo "on disk"
    sync nosuch.txt 2>/dev/null || echo "nosuch.txt is not there"

## How it works

Windows keeps what programs write in memory for a while and writes it to disk later.
`sync FILE` writes FILE's cached data to disk now, before `sync` returns, as before a
machine is switched off or a drive pulled out. `sync` alone asks the same of every
drive, and `-f` of the drive each FILE is on. `-d` writes the data, leaving metadata
such as times.

## Options

    -d, --data           The data of each FILE, not its metadata.
    -f, --file-system    The drive each FILE is on.

Exit status: 0; 1 when a FILE is not there, or `-d` has no FILE.

## Different in cash

- Windows lets only an elevated process flush a whole drive. Elevated, `sync` and
  `sync -f` flush every fixed drive; otherwise they flush none and succeed, and Windows
  writes the cache back by itself within seconds. `sync FILE` flushes the file either
  way.
- A file this shell may only read is left to Windows, as Windows flushes through a
  handle that may write.
