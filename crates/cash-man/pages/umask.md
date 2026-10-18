---
name: umask
summary: Show or set the file mode mask.
kind: Bash builtins
see: chmod
---

## Syntax

    umask [-p] [-S] [MODE]

    umask           the mask, in octal: 0022
    umask -S        what it lets through, in letters: u=rwx,g=rx,o=rx
    umask 077       new files for you alone
    umask g=rx,o=   the same in letters: what group and others keep

## Examples

The mask, in octal and in letters:

    umask
    umask -S

Set it, in octal or in letters:

    umask 027; umask
    umask u=rwx,g=rx,o=; umask -S

A subshell has its own:

    ( umask 077; umask )
    umask

Kept and put back with `-p`:

    saved=$(umask -p)
    umask 077
    eval "$saved"; umask

## How it works

The mask names the permission bits new files and directories do not get: with 022, the
default, group and others get no write, so a new file is 644, `rw-r--r--`, and a
directory 755. `umask MODE` sets it: in octal, or in `chmod`'s letters, which say
what is kept, not what is taken. `-S` shows it in letters, and `-p` as a `umask`
command that sets it again. Each subshell and child shell has its own copy.

## Options

    -S    Show it in letters: u=rwx,g=rx,o=rx.
    -p    Show it as the command that sets it: umask 0022.

Exit status: 0, or 1 for a MODE that is not one.

## Different in cash

- Windows files have no Unix permission bits; cash makes a file's mode up from its
  read-only attribute and its kind. So the mask is kept and shown, and scripts that set
  it run, but it changes nothing about the files made: after `umask 077`, `touch f`
  makes an ordinary file, which other accounts read as Windows' permissions let them.
