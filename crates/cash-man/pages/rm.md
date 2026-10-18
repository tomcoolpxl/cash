---
name: rm
summary: Remove files or directories.
kind: Bundled coreutils, from uutils
see: rmdir mv cp find shred
---

## Syntax

    rm [OPTION]... FILE...

    rm FILE...        remove files
    rm -r DIRECTORY   remove a directory and everything in it
    rm -i FILE...     ask before each one
    rm -f FILE...     no questions, no complaint about files not there

## Examples

Remove a file, or several:

    rm notes.txt
    rm *.md

Say what is removed, as it is removed:

    rm -v config.ini

Remove a directory and all in it:

    mkdir -p build/out
    rm -r build

Remove an empty directory:

    mkdir empty
    rm -d empty

No complaint about a file that is not there, as a clean-up script wants:

    rm -f nosuch.tmp

A file whose name starts with a dash:

    touch -- -draft.txt
    rm -- -draft.txt

## How it works

`rm` removes each FILE it is given. It does not remove a directory unless `-r` (or
`-R`) is given, which removes the directory and everything below it, or `-d`, which
removes an empty one. Removed files do not go to the Recycle Bin: they are gone.

A file that cannot be written (read-only) is removed after a question, when standard
input is a terminal: `remove write-protected regular file 'a.txt'?`. `-f` never asks
and is quiet about files that are not there; `-i` asks before every removal, `-I`
once, before removing more than three files or removing recursively.

A name that starts with `-` reads as an option: write `rm -- -name` or `rm ./-name`.

## Options

    -f, --force                 Never ask; say nothing of files not there.
    -i                          Ask before every removal.
    -I                          Ask once, before removing more than three
                                files or removing recursively.
    --interactive[=WHEN]        Ask never, once (-I), or always (-i, the
                                default).
    -r, -R, --recursive         Remove directories and all in them.
    -d, --dir                   Remove empty directories.
    -v, --verbose               Say what is removed: removed 'a.txt'.
    --preserve-root[=all]       Refuse to remove the root of a drive (the
                                default).
    --no-preserve-root          Do not treat the root specially.
    --one-file-system           Taken, not carried out: rm does not stop
                                at another drive.
    -g, --progress              Show a progress bar (uutils' own).

Exit status: 0 when every FILE was removed (or, with `-f`, was not there); 1
otherwise.

## Advanced

### Asking

With `-i`, each file and each directory before it is entered is asked about: `rm:
descend into directory 'build'?`, `rm: remove regular file 'build/out.txt'?`. An
answer starting with `y` removes it. With standard input not a terminal, as in a
script, `rm` asks nothing about read-only files, and removes them.

### Removing in a script

`rm -rf "$dir"` removes a directory whether it is there or not, and never stops to
ask; quote the variable, and check that it is set: `${dir:?}` stops the script when
it is not. `find . -name '*.tmp' -delete` removes by pattern through a tree.

## Different in cash

- It is cash's own copy of uutils' `rm`, carried inside cash, so it is the same on
  every machine, with or without Git or MSYS2.
- A directory link (a junction, or a symbolic link to a directory) is removed as one
  under `-r`, with `RemoveDirectoryW`: what it points to is left alone.
- `rm -r` can remove the directory the shell is in: it moves out of it first, as
  Windows will not remove a directory a process sits in, and the shell's directory is
  then gone, as on Linux.
- A file another program has open may not be removable on Windows: `rm` says so.
  `fuser` or `lsof` name the program.
- Read-only files are removed as on Linux: after a question at a terminal, at once
  with `-f` or in a script.
