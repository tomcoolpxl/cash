---
name: unalias
summary: Remove aliases.
kind: Bash builtins
see: alias
---

## Syntax

    unalias [-a] NAME...

    unalias ll     remove the alias ll
    unalias -a     remove every alias

## Examples

An alias, used and removed:

    shopt -s expand_aliases
    alias greet='echo hello'
    greet
    unalias greet
    greet 2>/dev/null || echo "greet is gone"

All of them at once:

    alias a='echo a' b='echo b'
    unalias -a
    alias | wc -l

## How it works

`unalias NAME` removes the alias NAME, and `unalias -a` every alias. A line already
read keeps the aliases it was read with; the next line is read without them. A NAME
that is not an alias is an error, and the others are still removed.

## Options

    -a    Remove every alias.

Exit status: 0, or 1 when a NAME is not an alias.
