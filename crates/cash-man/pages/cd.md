---
name: cd
summary: Change the shell's working directory.
kind: Bash builtin
see: pwd pushd popd dirs prevd nextd cdh z croot winpath paths
---

## Syntax

    cd [-L | -P [-e]] [DIRECTORY]

    cd                    go to your home directory ($HOME)
    cd DIRECTORY          go to DIRECTORY: absolute, relative, or by CDPATH
    cd -                  go back to the previous directory ($OLDPWD), printed
    cd -P DIRECTORY       go to DIRECTORY, with links and junctions resolved
    cd -P -e DIRECTORY    the same, failing if the real directory has no name

## Examples

Go to a directory, by any spelling:

    cd C:/src/cash
    cd /c/src/cash
    cd 'C:\src\cash'

Go up one directory, or two:

    cd ..
    cd ../..

Go home, or to a directory in it:

    cd
    cd ~/Documents

Switch between two directories:

    cd C:/src/cash
    cd C:/work/notes
    cd -              # back in C:/src/cash
    cd -              # back in C:/work/notes

A directory with spaces in its name needs quotes:

    cd "C:/Program Files/Git"

A network share:

    cd //fileserver/projects

In a script, stop if the directory is not there:

    cd "$build_dir" || exit 1

Work in a directory without leaving this one:

    (cd tools && make)

See where a junction really leads:

    cd -P "C:/Documents and Settings"
    pwd               # C:/Users

## How it works

Every process has a working directory: where a relative path such as `notes.txt` or
`src/main.rs` is looked up. The shell's working directory is also where every program
it starts begins. `cd` changes it, and sets two variables:

- `PWD`, the new directory, as `pwd` prints it;
- `OLDPWD`, the directory you left, which `cd -` returns to.

How the directory is found:

1. No DIRECTORY: `$HOME`. If `HOME` is not set, `cd` fails.
2. `-`: `$OLDPWD`, and the directory is printed.
3. A DIRECTORY starting with `/`, a drive (`C:`), `~` or `//`: that directory.
4. A DIRECTORY starting with `.` or `..`: from the current directory.
5. Any other relative DIRECTORY: from each entry of `CDPATH` in turn, then from the
   current directory. One found through a `CDPATH` entry is printed.

`cd` changes only this shell. In a subshell `( … )`, in a pipeline, or in a script you
run (rather than `source`), it changes that process's directory, and the shell that
started it stays where it was. That is why a script cannot `cd` its caller somewhere;
a function or a sourced file can.

### Logical and physical

A link or junction makes one directory appear under two names. By default (`-L`) `cd`
keeps the name you used: after `cd C:/link/sub`, `$PWD` is `C:/link/sub`, and `cd ..`
returns to `C:/link` by dropping the last part of `$PWD`. With `-P`, links are
resolved: `$PWD` is the real directory, and `..` is the real parent.

## Options

    -L           Logical paths, the default. `$PWD` keeps links as written,
                 and `..` drops the last part of `$PWD`.
    -P           Physical paths. Links and junctions are resolved before
                 `$PWD` is set.
    -e           With -P: when the directory was entered but its real name
                 cannot be found, return 1 rather than 0.
    DIRECTORY    Where to go. `-` means `$OLDPWD`; none means `$HOME`.

When both `-L` and `-P` are given, the last one counts.

Exit status: 0 when the directory was changed; 1 when it is missing, is not a
directory, may not be entered, or `HOME` or `OLDPWD` is needed and not set; 2 for a bad
option.

## Advanced

### CDPATH

`CDPATH` lists directories `cd` searches for a relative name:

    CDPATH="C:/src:C:/work"
    cd cash           # C:/src/cash, from wherever you are; printed

An empty entry (a leading, trailing or doubled separator) means the current
directory. Put it first to prefer directories here: `CDPATH=":C:/src"`. Exporting
`CDPATH` affects scripts too, which can surprise them; set it in `.bashrc` without
`export`.

### Directory stack and history

- `pushd DIRECTORY` goes to a directory and remembers the one you left; `popd`
  returns; `dirs -v` lists the stack.
- `prevd` and `nextd` step back and forward through every directory `cd` has visited,
  as a browser's Back and Forward; `cdh` lists them to choose from.
- `z PART` jumps to the most used directory whose name contains PART; `z -i` lists
  them.

### Shell options

`shopt -s cdable_vars` lets `cd NAME` go to the directory the variable NAME holds,
when there is no directory NAME.

### Hooks

`PROMPT_COMMAND` runs before each prompt, so it can react to the directory having
changed:

    PROMPT_COMMAND='[[ $PWD != "$last" ]] && last=$PWD && ls'

## Different in cash

- Windows paths in every spelling: `C:/src`, `/c/src`, `"C:\src"`, `//server/share`
  and `'\\server\share'`. Whatever you typed, `pwd` and `$PWD` print `C:/src` or
  `//server/share`: the spelling Windows programs accept as an argument.
- A backslash is Bash's escape, so `cd C:\Users\me` unquoted reaches `cd` as
  `C:Usersme` in a script. Quote it, or use `/`. At the prompt `shopt winpaths` (on by
  default) keeps the backslashes, and a failed `cd` to such a mangled path says so.
- Junctions, the links Windows uses for directories such as
  `C:/Documents and Settings`, are resolved by `-P` as symbolic links are.
- `CDPATH` entries may be separated by `:` or `;`, and spelled `C:/src` or `/c/src`,
  as `PATH`'s are.
- Every `cd` is recorded in the directory history behind `prevd`, `nextd`, `cdh` and
  Alt-Left / Alt-Right on an empty line, and in `z`'s list. Alt-E opens `croot`, a
  directory picker, below the command line.
- `-@` (extended attributes) is not supported, as in Git Bash.
