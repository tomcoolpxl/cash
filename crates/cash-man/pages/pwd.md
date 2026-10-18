---
name: pwd
summary: Print the working directory.
kind: Bash builtins
see: cd dirs paths winpath
---

## Syntax

    pwd [-LPW]

    pwd           the directory the shell is in
    pwd -P        the same, with symbolic links and junctions resolved
    "$PWD"        the same, as a variable

## Examples

The directory the shell is in, here its last part:

    mkdir -p src/app && cd src/app
    basename "$(pwd)"

`$PWD` holds the same, and `$OLDPWD` the directory before:

    mkdir -p build/out && cd build && cd out
    [ "$PWD" = "$(pwd)" ] && echo same
    basename "$OLDPWD"

Remember where you are, go elsewhere, and come back:

    here=$PWD
    mkdir -p tmp && cd tmp
    cd "$here" && [ "$PWD" = "$here" ] && echo back

## How it works

`pwd` writes the full path of the shell's working directory: the directory relative
paths start from, and the one programs it starts begin in. `cd` changes it; `pwd`
only reads it.

The shell keeps the path it got there by, links included, in `$PWD`, and `pwd`
writes that (`-L`, the default). `-P` writes the physical path instead, each
symbolic link and junction followed to the directory it names: a script that
compares paths, or moves out with `..`, sees what the file system sees.

`$PWD` is quicker than `$(pwd)`, which starts a subshell, and holds the same.

## Options

    -L        The path the shell got there by, links kept (the default).
    -P        The physical path, links resolved.
    -W        The Windows form; cash prints it always, so -W changes nothing
              (Git Bash's pwd prints /c/... without it).

Other words after `pwd` are ignored, as Bash ignores them.

Exit status: 0; 2 for a bad option.

## Different in cash

- The path is written `C:/Users/me/src`, with forward slashes, where Git Bash writes
  `/c/Users/me/src`; `cd` takes either, and `winpath -u` gives the `/c/` form. See
  `help paths`.
- A path over 260 characters prints whole; programs started there begin in its 8.3
  short name.
