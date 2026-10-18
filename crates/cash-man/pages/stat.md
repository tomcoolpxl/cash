---
name: stat
summary: Show a file's status: size, times, links, attributes.
kind: Coreutils written for Windows
see: ls chmod test find
---

## Syntax

    stat [OPTION]... FILE...

    stat FILE            everything about FILE
    stat -c %s FILE      its size alone
    stat -c %y FILE      when it last changed
    stat -f DIR          the file system DIR is on

## Examples

One fact about a file, by a format:

    stat -c '%s bytes' notes.txt
    stat -c %F notes.txt
    stat -c %F .

A line for each of several files:

    stat -c '%n is %s bytes' a.md b.md

`--printf` reads escapes and adds no newline:

    stat --printf '%n\t%s\n' a.md b.md

Times, and the newest log:

<!-- parse only: names files of the reader's own -->

    stat build.log
    stat -c %y build.log
    stat -c '%Y %n' *.log | sort -n | tail -1

## How it works

`stat FILE...` shows each file's name, size, blocks, kind, device, inode, links,
permissions, owner and group, and its access, modify, change and birth times, laid out
as GNU stat lays them out. `-c FORMAT` prints only what FORMAT asks for, and a newline,
for each file; `--printf` does the same with backslash escapes such as `\n` and `\t`,
and no newline of its own. `-f` shows the file system instead, its type and sizes.

The format's codes:

    %n  name           %s  size in bytes      %F  kind of file
    %a  mode, octal    %A  mode as ls shows it
    %U  owner          %G  group              %h  number of links
    %i  inode          %d  device
    %x  access time    %y  modify time        %z  change time    %w  birth
    %X %Y %Z %W        the same four, in seconds since 1970

## Options

    -c, --format=FORMAT    Print FORMAT for each file, then a newline.
    --printf=FORMAT        The same, with backslash escapes and no newline.
    -f, --file-system      Show the file system, not the file.
    -L, --dereference      Follow a link to what it points to.
    -t, --terse            Every field on one line, bare.

Exit status: 0, or 1 when a file is not there.

## Different in cash

uutils' `stat` is Unix-only, so cash carries its own, which shows a Windows file as Git
Bash's does: the mode is made up from the read-only attribute and the kind of file, the
inode is the file's index on its volume, and the device is the volume's serial number.
