---
name: command
summary: Run a command, skipping functions; or say what a name is.
kind: Bash builtins
see: type which hash
---

## Syntax

    command [-pVv] COMMAND [ARG...]

    command ls -l      run ls, even with a function called ls
    command -v NAME    how NAME would run: its path, or its name
    command -V NAME    the same, said in words
    command -p NAME    run NAME found on a standard PATH

## Examples

A function that wraps the command it is named after:

    ls() { echo "listing:"; command ls "$@"; }
    ls a.md b.md

Is a command there?

    command -v sort >/dev/null && echo "sort is there"
    command -v nosuch >/dev/null || echo "nosuch is not installed"

What a name is:

    command -V cd
    greet() { echo hi; }
    command -V greet

## How it works

`command NAME ARGS` runs NAME as a builtin or a program, skipping any function of that
name: the way a function named after a command reaches the command itself. It skips
aliases too.

With `-v`, it runs nothing and prints how NAME would run: a program's path, or the name
itself for a builtin, function or keyword, and an alias as its `alias` line; for a name
that is none of them it prints nothing and fails, which makes `command -v NAME` the
usual test for a command. `-V` says the same in words, as `type` does, and prints a
function's definition. `-p` looks NAME up on a standard PATH instead of `$PATH`.

## Options

    -v    Print how NAME would run, a path or a name; run nothing.
    -V    Say what NAME is, in words.
    -p    Look NAME up on a standard PATH, not $PATH.

Exit status: the command's; with `-v` or `-V`, 0 when NAME was found and 1 when not;
127 when COMMAND is not found.

## Different in cash

- The tools cash carries are builtins to it, so `command -v ls` prints `ls`, where Git
  Bash prints `/usr/bin/ls`. `type -a ls` shows the programs on PATH too.
