---
name: type
summary: Say what a name is: alias, keyword, function, builtin or file.
kind: Bash builtins
see: command which hash alias
---

## Syntax

    type [-afptP] NAME...

    type NAME       what NAME is, and what running it runs
    type -t NAME    one word: alias, keyword, function, builtin or file
    type -a NAME    every NAME there is, in the order they are looked up
    type -P NAME    the program file a search of PATH finds

## Examples

What a name is:

    type cd
    type if

One word for it, as a script wants:

    type -t cd
    type -t if
    f() { :; }; type -t f

A function, shown as its definition:

    greet() { echo hi; }
    type greet

An alias, in an interactive shell or with `expand_aliases`:

    shopt -s expand_aliases
    alias ll='ls -l'
    type ll

Whether a command is there at all:

    type nosuchcommand >/dev/null 2>&1 || echo missing

## How it works

`type` says, for each NAME, what running it would run, looked up in the order the
shell looks: an alias, a keyword (`if`, `for`, `[[`), a function, a builtin, and a
program on `PATH`. `-t` writes one word for it, or nothing for a name that is none of
them; `-a` writes every one there is, not only the first; `-p` and `-P` write the
program file's path.

`type` returns 0 when every NAME was found and 1 otherwise, so `type -t x >/dev/null`
asks whether a command exists. `command -v NAME` asks the same and writes a path or a
name, which POSIX defines too.

## Options

    -t       One word: alias, keyword, function, builtin, file, or nothing.
    -a       Every definition, not only the first.
    -p       The file's path, if NAME runs a file.
    -P       The file a search of PATH finds, whatever NAME is first.
    -f       Leave functions out.

Exit status: 0 when every NAME was found; 1 otherwise.

## Different in cash

- A command cash carries (`ls`, `grep`, `tar`, ...) is a builtin to `type`; `type -a`
  also lists the programs of the same name on `PATH`, Git's or MSYS2's.
- A program's path is written `C:/...`, as cash writes paths.
