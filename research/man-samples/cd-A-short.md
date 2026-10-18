---
name: cd
summary: Change the shell's working directory.
kind: Bash builtin
see: pwd pushd prevd z croot
---

## Syntax

    cd [directory]

Without a directory, `cd` goes home. `cd -` goes back to the directory you were in before.

## Examples

Go to a directory:

    cd C:/src/cash

Go up one directory:

    cd ..

Go back to where you were:

    cd -

A directory with spaces in its name:

    cd "C:/Program Files"

A network share:

    cd //server/share

## Options

    -P    resolve links and junctions: $PWD shows the real directory
    -L    keep links as you typed them (the default)

## Different in cash

- Every spelling works: `C:/src`, `/c/src`, `"C:\src"`, `//server/share`. `$PWD` is
  always `C:/src` style.
- Quote a path with backslashes (`cd 'C:\Users\me'`): unquoted, a backslash is an escape.
- `prevd`, `nextd` and Alt-Left / Alt-Right walk the directories you have been in.
