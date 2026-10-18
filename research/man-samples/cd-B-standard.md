---
name: cd
summary: Change the shell's working directory.
kind: Bash builtin
see: pwd pushd popd dirs prevd z croot paths
---

## Syntax

    cd                 go to your home directory
    cd directory       go to directory
    cd -               go back to the previous directory
    cd -P directory    go to directory, with links and junctions resolved

## Examples

Go to a directory, by any spelling:

    cd C:/src/cash
    cd /c/src/cash

Go up one directory, or two:

    cd ..
    cd ../..

Go home, or to a directory in it:

    cd
    cd ~/Documents

Go back to the directory you were in, and back again:

    cd -

A directory with spaces in its name needs quotes:

    cd "C:/Program Files"

In a script, stop if the directory is not there:

    cd "$build_dir" || exit 1

Work in a directory without leaving this one:

    (cd tools && make)

## How it works

The working directory is where the shell, and every program it starts, looks for a
relative path such as `notes.txt` or `src/main.rs`. `cd` changes it and sets two
variables: `PWD`, the new directory, and `OLDPWD`, the directory you left, which `cd -` goes
back to.

A relative directory is found from the current one. If it is not there and does not start
with `.` or `..`, `cd` also looks in the directories listed in `CDPATH`, and prints the
directory when it finds it that way.

`cd` changes only this shell. In a subshell `( … )` or a pipeline it changes the
subshell's directory, and the shell around it stays where it was.

## Options

    -L           Logical (the default). `..` drops the last part of `$PWD`, so
                 after `cd C:/link/sub`, `cd ..` returns to `C:/link`, even when
                 `link` is a junction to somewhere else.
    -P           Physical. Links and junctions are resolved: `$PWD` names the
                 real directory, and `..` is its real parent.
    -e           With `-P`: fail when the new directory's real name cannot be
                 found.
    directory    Where to go. `-` is `$OLDPWD`; no directory is `$HOME`.

`cd` returns 0 when it changed the directory, 1 when the directory is missing, is not a
directory, or may not be entered.

## Advanced

- `CDPATH="C:/src:C:/work"` makes `cd cash` reach `C:/src/cash` from anywhere. An empty
  entry means the current directory.
- `pushd`, `popd` and `dirs` keep a stack of directories to return to.
- `pwd -P` prints the real directory without changing it.

## Different in cash

- Windows paths in every spelling: `C:/src`, `/c/src`, `"C:\src"`, `//server/share` and
  `'\\server\share'`. `pwd` and `$PWD` always print `C:/src` or `//server/share`.
- A backslash is Bash's escape, so `cd C:\Users\me` unquoted reaches `cd` as
  `C:Usersme` in a script. Quote it. At the prompt `shopt winpaths` keeps backslashes.
- Every `cd` is recorded in the directory history: `prevd`, `nextd`, `cdh`, and Alt-Left /
  Alt-Right on an empty line. `z` jumps to a directory by part of its name, and Alt-E opens
  `croot` to pick one.
- `CDPATH` entries may be separated by `:` or `;` and spelled `C:/src` or `/c/src`.
- `-@` (extended attributes) is not supported, as in Git Bash.
