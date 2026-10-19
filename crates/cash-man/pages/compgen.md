---
name: compgen
summary: Print the completions of a word.
kind: Bash builtins
see: complete compopt
---

## Syntax

    compgen [OPTION]... [WORD]

    compgen -W "a b c" -- WORD    the words of a list that start with WORD
    compgen -c WORD               the commands that start with WORD
    compgen -f WORD               the files that start with WORD
    compgen -A function           every function

## Examples

The words of a list that complete a word, with something before and after:

    compgen -W "alpha beta gamma" -- b
    compgen -P '<' -S '>' -W "a b"

Functions, builtins and files:

    greet() { :; }
    compgen -A function
    compgen -b | grep -x cd
    printf 'x\n' > notes.md
    compgen -f note

## How it works

`compgen` prints, a line each, the completions of WORD that its options choose, the
same way `complete` would offer them: `-W` the words of a list, `-c` commands, `-f`
files, `-d` directories, `-b` builtins, `-k` keywords, `-a` aliases, `-v` variables,
`-e` exported ones, `-A ACTION` any of these by name. `-G` takes a glob, `-X` leaves out
what matches a pattern, `-P` and `-S` add a prefix and a suffix. A completion function
uses it to fill `COMPREPLY`: `COMPREPLY=($(compgen -W "$words" -- "$cur"))`.

## Options

    -W WORDLIST    The words of WORDLIST.
    -A ACTION      alias, builtin, command, directory, file, function, ...
    -a, -b, -c, -d, -e, -f, -k, -u, -v    Aliases, builtins, commands, ...
    -G GLOB        The files GLOB matches.
    -X PATTERN     Leave out what matches PATTERN; !PATTERN keeps only it.
    -P PREFIX      Put PREFIX before each.
    -S SUFFIX      Put SUFFIX after each.
    -F FUNCTION    Run FUNCTION and take its COMPREPLY.
    -C COMMAND     Run COMMAND and take its lines.
    -o OPTION      As complete -o.

Exit status: 0, or 1 when nothing completes.
