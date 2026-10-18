---
name: mv
summary: Move or rename files.
kind: Bundled coreutils, from uutils
see: cp rm ln mkdir paths
---

## Syntax

    mv [OPTION]... SOURCE DEST
    mv [OPTION]... SOURCE... DIRECTORY
    mv [OPTION]... -t DIRECTORY SOURCE...

    mv OLD NEW               rename OLD to NEW
    mv FILE... DIRECTORY     move files into a directory
    mv -i OLD NEW            ask before replacing NEW
    mv -n OLD NEW            never replace NEW

## Examples

Rename a file:

    mv notes.txt notes.old

Move several files into a directory:

    mkdir docs
    mv *.md docs/

Rename a directory:

    mkdir docs
    mv docs documents

Say what is moved, as it is moved:

    mv -v config.ini settings.ini

Never replace a file that is already there:

    mv -n pairs.txt list.txt

Keep the file it replaces, as `list.txt~`:

    mv -b pairs.txt list.txt

Name the directory first, as `xargs` and `find` like it:

    mkdir docs
    mv -t docs paths.txt report.txt

## How it works

With two names, `mv` renames the first to the second, a file or a directory; with
more, or with a last name that is a directory, it moves each SOURCE into that
DIRECTORY under its own name. `-t DIRECTORY` names the directory first, and `-T` says
the last name is never a directory to move into.

On one drive a move is a rename, as quick for a directory of a million files as for
one file. Across drives (`C:` to `D:`) it is a copy and then a delete, and takes as
long as the copy.

A file already there is replaced without asking. `-i` asks first, `-n` never
replaces, `-u` replaces only with a newer file, and `-b` keeps the one replaced, as
`NAME~` (or `NAME.~1~` with `--backup=numbered`). The last of `-i`, `-f` and `-n`
given counts.

## Options

    -f, --force                 Replace without asking (the default).
    -i, --interactive           Ask before replacing a file.
    -n, --no-clobber            Never replace a file that is already there.
    -u, --update[=WHEN]         Replace only with a newer file (older), or
                                always (all), never (none, none-fail).
    -b, --backup[=CONTROL]      Keep each file replaced: simple (NAME~),
                                numbered (NAME.~1~), existing (numbered
                                when there are numbered ones), none.
    -S, --suffix SUFFIX         The suffix of a simple backup, ~ by default.
    -t, --target-directory DIRECTORY
                                Move every SOURCE into DIRECTORY.
    -T, --no-target-directory   DEST is never a directory to move into.
    -v, --verbose               Say what is moved: renamed 'a' -> 'b'.
    --strip-trailing-slashes    Take the / off the end of each SOURCE.
    -g, --progress              Show a progress bar (uutils' own).
    --debug                     Say how each file is moved; implies -v.
    --exchange                  Swap SOURCE and DEST: not on Windows.
    -Z, --context[=CTX]         SELinux contexts: nothing on Windows.

Exit status: 0 when every file was moved; 1 when one could not be.

## Advanced

### Backups

`--backup=numbered` keeps every version: `notes.txt.~1~`, `notes.txt.~2~`. With
`--backup=existing`, files that already have numbered backups get another, others a
simple `~` one. `-S .orig` or `SIMPLE_BACKUP_SUFFIX` changes the simple suffix;
`VERSION_CONTROL` gives the kind when `--backup` names none.

### Moving a directory into itself

`mv dir dir/inside` is refused: `cannot move 'dir' to a subdirectory of itself`.

## Different in cash

- It is uutils' `mv`, carried inside cash, so it is the same on every machine, with or
  without Git or MSYS2.
- A file that another program has open cannot be moved on Windows, as it can on
  Linux: `mv` says `Access is denied` or that the file is in use. `fuser` or `lsof`
  name the program.
- When `-i` asks and the answer is not yes, `mv` leaves the file and exits 1, where
  GNU's `mv` exits 0.
- `--exchange` is refused: it is Linux's atomic swap. `-Z` and `--context` set SELinux
  contexts, which Windows does not have.
