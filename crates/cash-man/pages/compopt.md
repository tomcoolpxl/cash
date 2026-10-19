---
name: compopt
summary: Change the completion options of the running completion.
kind: Bash builtins
see: complete compgen
---

## Syntax

    compopt [-o|+o OPTION] [-DEI] [NAME]...

    compopt -o nospace    no space after this completion
    compopt +o nospace    a space after it again
    compopt -o nospace svc    for svc's completions from now on

## Examples

A completion function that leaves no space after a directory:

<!-- parse only: runs inside the reader's completion -->

    _go() {
      COMPREPLY=($(compgen -d -- "${COMP_WORDS[COMP_CWORD]}"))
      compopt -o nospace
    }
    complete -F _go go

## How it works

`compopt` changes the options of a completion: inside a completion function, of the one
running now; with NAMEs, of those commands' completions. `-o OPTION` sets an option and
`+o OPTION` clears it, with `complete -o`'s options: `nospace`, `filenames`, `default`,
`dirnames`, `plusdirs`, `bashdefault`, `noquote`, `nosort`.

## Options

    -o OPTION     Set OPTION.
    +o OPTION     Clear OPTION.
    -D, -E, -I    The default, the empty line, the first word.

Exit status: 0, or 1 for an option it does not know.

## Different in cash

- Outside a completion function and without a NAME, `compopt` changes nothing and
  succeeds, where Bash says it is not completing.
