---
name: alias
summary: Define or show aliases.
kind: Bash builtins
see: unalias type command config
---

## Syntax

    alias [-p] [NAME[=VALUE]]...

    alias NAME='TEXT'        typing NAME runs TEXT
    alias NAME               show one alias
    alias                    show them all
    unalias NAME             remove one

## Examples

Shorter names for commands typed often, as in `~/.bashrc`:

    alias ll='ls -l'
    alias gs='git status'
    alias ll gs

The words after an alias are kept, after its text:

    shopt -s expand_aliases
    alias greet='echo hello,'
    greet world

An alias with several commands:

    shopt -s expand_aliases
    alias both='echo one; echo two'
    both

List every alias, as lines the shell can read back:

    alias ll='ls -l' la='ls -A'
    alias

Run the command itself, not the alias of the same name, with a backslash:

    shopt -s expand_aliases
    alias ls='ls -A'
    \ls -d .

## How it works

When the shell reads a command, it looks at the first word: when it is an alias, the
word is replaced by the alias's text, and the line is read on from there. It happens
as the line is read, before it runs, so an alias defined on a line is used from the
next line on, not later on the same line.

If an alias's text ends in a space, the next word is looked up as an alias too:
`alias sudo='sudo '` lets `sudo ll` use the `ll` alias.

Aliases are for the keyboard: a script does not use them unless it says `shopt -s
expand_aliases`. Put them in `~/.bashrc`, which cash reads when it starts interactive.
For anything with arguments in the middle, or with more than one line, a function is
better: `mkcd() { mkdir -p "$1" && cd "$1"; }`.

## Options

    -p        List every alias, as alias NAME='VALUE' lines (the same with
              no NAME at all).

With `NAME=VALUE`, the alias is set; with a NAME alone, it is shown. In POSIX mode
(`set -o posix`) a listing without `-p` leaves out `alias `, as POSIX asks. A NAME may not
hold blanks, quotes, `/`, `$` or any of `()<>;&|`.

Exit status: 0; 1 when a NAME has no alias or is not a valid name.

## Advanced

### Seeing what runs

`type ll` says what `ll` is: ``ll is aliased to `ls -l'``. `command -v ll` prints the
alias's definition. `\ll`, `command ll` and `'ll'` skip the alias.

### Expanded when read

The text is read again at each use, so `alias now='date +%T'` gives the time it is
run, and an alias may name another: `alias l=ll` runs `ls -l`. An alias is not looked
up inside its own text, so `alias ls='ls -A'` does not loop.

The text joins the line it is read into: with `alias q="echo it's"`, the line
`q quoted'` reads as `echo it's quoted'`, its quote closed by the line.
