---
name: touch
summary: Change file times, or make empty files.
kind: Bundled coreutils, from uutils
see: stat date ls
---

## Syntax

    touch [OPTION]... FILE...

    touch FILE                 make it empty if it is not there; date it now
    touch -d 2020-01-02 FILE   date it then
    touch -r REF FILE          give it REF's times
    touch -c FILE              never make it

## Examples

Make empty files:

    touch new.txt other.txt
    ls new.txt other.txt

Date a file, and see the date:

    touch -d '2020-01-02 03:04' notes.txt
    date -r notes.txt '+%Y-%m-%d %H:%M'

Give a file another's times:

    touch -d 2021-06-01 notes.txt
    touch -r notes.txt pairs.txt
    date -r pairs.txt +%F

Change the access time alone, or the modification time alone:

    touch -a -d 2019-01-01 notes.txt
    date -r notes.txt +%Y

Leave a file that is not there unmade:

    touch -c nosuch.txt; ls nosuch.txt 2>/dev/null || echo 'not made'

## How it works

`touch` sets each FILE's access and modification times to now, or to the time `-d`,
`-t` or `-r` gives, and makes an empty file for a name that is not there, unless `-c`
says not to. `-a` changes the access time alone and `-m` the modification time alone.

Make and other build tools compare modification times, so `touch file` makes it newer
than what was built from it, and it is built again.

`-d` takes most dates `date -d` takes: `2020-01-02`, `'2020-01-02 03:04:05'`,
`yesterday`, `'2 hours ago'`, `@1700000000`.

## Options

    -a                    Change the access time.
    -m                    Change the modification time.
    -c, --no-create       Make no file.
    -d, --date=STRING     Use STRING's time, not now.
    -t [[CC]YY]MMDDhhmm[.ss]  Use this time.
    -r, --reference=FILE  Use FILE's times.
    -h, --no-dereference  Change a symbolic link, not what it names.

Exit status: 0 on success; 1 when a file cannot be made or changed.

## Different in cash

- It is uutils' `touch`, carried inside cash, so it is the same on every machine, with
  or without Git or MSYS2.
- Windows keeps a third time, the time a file was made, which `touch` leaves alone;
  `stat` shows it as Birth.
