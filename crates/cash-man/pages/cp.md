---
name: cp
summary: Copy files and directories.
kind: Bundled coreutils, from uutils
see: mv rm ln install paths
---

## Syntax

    cp [OPTION]... SOURCE DEST
    cp [OPTION]... SOURCE... DIRECTORY
    cp [OPTION]... -t DIRECTORY SOURCE...

    cp FILE COPY             copy FILE to the new name COPY
    cp FILE... DIRECTORY     copy files into a directory
    cp -r DIR NEW            copy a directory and all in it
    cp -i FILE COPY          ask before replacing COPY

## Examples

Copy a file under a new name:

    cp notes.txt notes.bak

Copy several files into a directory:

    mkdir docs
    cp *.md docs/

Say what is copied, as it is copied:

    cp -v config.ini config.copy

Copy a directory and everything in it:

    mkdir docs
    cp *.md docs/
    cp -r docs docs2

Never replace a file that is already there:

    cp -n notes.txt pairs.txt

Keep the file it would replace, as `pairs.txt~`:

    cp -b notes.txt pairs.txt

Name the directory first, as `xargs` and `find` like it:

    mkdir docs
    cp -t docs list.txt pairs.txt

## How it works

With two names, `cp` copies the first to the second: a new file, or the old one
replaced. With more, or with a last name that is a directory, it copies each SOURCE
into that DIRECTORY under its own name. `-t DIRECTORY` names the directory first, and
`-T` says the last name is never a directory.

A directory is copied only with `-r` (or `-R`, `-a`): itself and everything below
it. Without, `cp` leaves it out and says so, and exits 1.

An existing file is replaced without asking. `-i` asks first, `-n` never replaces,
`-u` replaces only with a newer file, and `-b` keeps the one replaced, as `NAME~` (or
`NAME.~1~` with `--backup=numbered`).

## Options

    -r, -R, --recursive       Copy directories and all in them.
    -a, --archive             The same as -dR --preserve=all: a copy as
                              like the original as can be.
    -i, --interactive         Ask before replacing a file.
    -n, --no-clobber          Never replace a file that is already there.
    -u, --update[=WHEN]       Replace only with a newer file (older), or
                              always (all), never (none, none-fail).
    -f, --force               Replace a file that cannot be opened for
                              writing, a read-only one too.
    --remove-destination      Remove each file to be replaced first.
    -b, --backup[=CONTROL]    Keep each file replaced: simple (NAME~),
                              numbered (NAME.~1~), existing (numbered
                              when there are numbered ones), none.
    -S, --suffix SUFFIX       The suffix of a simple backup, ~ by default.
    -v, --verbose             Say what is copied: 'a' -> 'b'.
    -t, --target-directory DIRECTORY
                              Copy every SOURCE into DIRECTORY.
    -T, --no-target-directory
                              DEST is never a directory to copy into.
    --parents                 Copy a/b/c to DIRECTORY/a/b/c, its path kept.
    -p                        The same as --preserve=mode,timestamps.
    --preserve[=LIST]         Keep these of the original: mode,
                              timestamps, links, xattr, context, all.
    --no-preserve=LIST        Do not keep these.
    --attributes-only         Copy the attributes, not the data.
    -l, --link                Make hard links rather than copies.
    -s, --symbolic-link       Make symbolic links rather than copies.
    -P, --no-dereference      Copy a link as a link.
    -L, --dereference         Copy what a link leads to.
    -H                        Follow the links named on the command line.
    -d                        The same as --no-dereference --preserve=links.
    -x, --one-file-system     Stay on the drive SOURCE is on.
    --sparse=WHEN             Make copies sparse: auto, always, never.
    --reflink[=WHEN]          Clone rather than copy: not on Windows.
    --strip-trailing-slashes  Take the / off the end of each SOURCE.
    -g, --progress            Show a progress bar (uutils' own).
    --debug                   Say how each file is copied; implies -v.

Exit status: 0 when every file was copied; 1 when one could not be.

## Advanced

### Backups

`--backup=numbered` keeps every version: `notes.txt.~1~`, `notes.txt.~2~`. With
`--backup=existing` (or `nil`), files that already have numbered backups get another,
others a simple `~` one. `-S .orig` changes the simple suffix; `VERSION_CONTROL` gives
the kind when `--backup` names none.

### Keeping attributes

`-p` keeps the times and the mode, which on Windows is the read-only attribute (a
copy of a read-only file is read-only either way); `-a` keeps everything `cp` can,
and copies a directory whole. Ownership is not kept: a copy belongs to the account
that makes it.

### Links

By default `cp` copies what a link leads to, except under `-r`, where a link is
copied as a link. `-L` and `-P` choose either way; `-l` and `-s` make links instead of
copies, hard ones (on the same drive) or symbolic ones.

## Different in cash

- It is uutils' `cp`, carried inside cash, so it is the same on every machine, with or
  without Git or MSYS2.
- A copy keeps the original's modification time even without `-p`, as Windows'
  CopyFile copies it; GNU's `cp` gives the copy the time it was made.
- `-s` makes a symbolic link only where Windows lets your account make one (Developer
  Mode, or an elevated shell); otherwise it says `A required privilege is not held by
  the client`. `ln -s` makes a junction for a directory without that privilege.
- `-f` replaces a read-only file. `--reflink` is refused: it is Linux's and macOS's.
- `-p` of a read-only file makes the copy, then says `Access is denied` and exits 1.
- `-Z` and `--context` set SELinux contexts, which Windows does not have; they change
  nothing.
- A source that is not there is said in Windows' words, without its name: `cp: The
  system cannot find the file specified.`
