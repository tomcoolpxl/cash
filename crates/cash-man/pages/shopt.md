---
name: shopt
summary: Turn Bash's optional behaviours on and off.
kind: Bash builtins
see: set config
---

## Syntax

    shopt [-pqsu] [-o] [OPTNAME]...

    shopt -s OPTNAME    turn it on
    shopt -u OPTNAME    turn it off
    shopt OPTNAME       show whether it is on
    shopt -q OPTNAME    only the status: 0 when on

## Examples

Match hidden files with `*`, and let a pattern that matches nothing vanish:

    touch .hidden visible
    shopt -s dotglob nullglob
    echo *
    echo *.nosuch | wc -w

`**` for any depth of directories:

    mkdir -p a/b && touch a/b/deep.txt
    shopt -s globstar
    echo **/*.txt

Extended patterns: `!(...)` for everything but:

    touch x.log y.txt
    shopt -s extglob
    echo !(*.log)

Ask whether an option is on:

    shopt -q extglob && echo on || echo off

## How it works

`shopt` turns Bash's optional behaviours on (`-s`) and off (`-u`); without either it
shows whether each OPTNAME is on, and with none, all of them. `-o` works on the
options of `set -o` instead.

The ones scripts reach for most:

- `nullglob`: a pattern that matches nothing expands to nothing, not to itself.
- `dotglob`: `*` matches names that start with `.`.
- `globstar`: `**` matches any number of directories.
- `extglob`: the patterns `?(...)`, `*(...)`, `+(...)`, `@(...)` and `!(...)`.
- `nocaseglob`: patterns ignore case.
- `failglob`: a pattern that matches nothing is an error.
- `expand_aliases`: aliases in a script.
- `lastpipe`: the last command of a pipeline runs in the shell itself, so
  `... | read v` sets `v`.

## Options

    -s    Turn each OPTNAME on.
    -u    Turn each OPTNAME off.
    -q    Say nothing; the status says whether all are on.
    -p    Write them as shopt commands that set them again.
    -o    The options of set -o.

Exit status: 0 when every OPTNAME is on (or was set); 1 otherwise.

## Different in cash

- `nocaseglob` is on unless turned off: Windows' file names ignore case, so `*.txt`
  matches `NOTES.TXT`, as the file system does. `shopt -u nocaseglob` makes patterns
  match case.
- `unixpaths`, cash's own, off unless turned on: paths spelled as Git Bash spells them,
  `/c/Users/me`, and handed to a Windows program as `C:/Users/me`, with `/` the
  directory above the drives; a cash started from the shell takes it too (`help paths`).
