---
name: mkdir
summary: Make directories.
kind: Bundled coreutils, from uutils
see: rmdir cd cp install
---

## Syntax

    mkdir [OPTION]... DIRECTORY...

    mkdir DIR             make a directory
    mkdir -p A/B/C        make it, and the directories above it if missing
    mkdir -v DIR...       say each one made

## Examples

Make a directory, or several:

    mkdir docs
    mkdir one two three
    ls -d */

A whole path at once, the directories above made as needed; no complaint when they
are there already:

    mkdir -p src/app/tests
    mkdir -p src/app
    find src

Say what is made:

    mkdir -pv build/out

Several directories side by side, with braces:

    mkdir -p project/{src,docs,tests}
    ls project

A name with a space, quoted:

    mkdir 'my notes'
    ls -d my*

## How it works

`mkdir` makes each DIRECTORY, which must not be there yet, in a directory that is.
`mkdir a/b` fails when `a` is missing; `-p` makes `a` first, and with `-p` a
directory that is there already is no error, which suits scripts: `mkdir -p "$dir"`
makes sure it exists.

A file of the same name is in the way, with or without `-p`.

## Options

    -p, --parents        Make missing directories above; no error if
                         the directory is there.
    -v, --verbose        Say each directory made: mkdir: created
                         directory 'a'.
    -m, --mode=MODE      Taken, not carried out: Windows has no mode
                         bits.
    -Z, --context[=CTX]  SELinux; taken, not carried out.

Exit status: 0 when every directory was made (or, with `-p`, was there); 1
otherwise.

## Advanced

### Making and entering

`mkdir -p dir && cd dir` makes a directory and goes in; as a function:
`mkcd() { mkdir -p "$1" && cd "$1"; }`.

### A temporary directory

`mktemp -d` makes a new, empty directory with a name no one else has, for a script's
scratch files.

## Different in cash

- It is uutils' `mkdir`, carried inside cash, the same on every machine.
- Errors are in Windows' words: `mkdir nosuch/x` says `The system cannot find the
  path specified.` where GNU says `No such file or directory`, and a directory that is
  there gives `mkdir: docs: File exists`.
- `-m` is accepted and ignored: Windows has access lists, not mode bits. Git Bash's
  `mkdir -m 700` fails, `cannot change permissions`.
- Names Windows forbids are refused: `CON`, `NUL`, `COM1`, and names with `<>:"|?*`.
  A dot or a space at the end of a name is dropped, as Windows drops it: `mkdir
  'dot.'` makes `dot`. Git Bash makes all of these by tricks of its own, names other
  Windows programs cannot open.
