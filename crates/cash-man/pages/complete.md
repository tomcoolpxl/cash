---
name: complete
summary: Set how a command's arguments are completed.
kind: Bash builtins
see: compgen compopt
---

## Syntax

    complete [OPTION]... NAME...
    complete -p [NAME]...
    complete -r [NAME]...

    complete -W "start stop" svc    svc's arguments from a list
    complete -F _svc svc            from a function's COMPREPLY
    complete -d mycd                directories only
    complete -p svc                 how svc completes, as a command

## Examples

A command's completion set, shown and taken away:

    complete -W "start stop status" svc
    complete -p svc
    complete -r svc
    complete -p svc 2>/dev/null || echo "svc completes as a file again"

A function that completes from the words before the cursor:

<!-- parse only: completes at the reader's prompt -->

    _svc() {
      COMPREPLY=($(compgen -W "start stop" -- "${COMP_WORDS[COMP_CWORD]}"))
    }
    complete -F _svc svc

## How it works

`complete` says how Tab completes the arguments of the commands NAME: from a list
(`-W`), commands (`-c`), files (`-f`), directories (`-d`), or what a function puts in
`COMPREPLY` (`-F`), called with `COMP_WORDS`, `COMP_CWORD`, `COMP_LINE` and
`COMP_POINT` set. `-o` adds options such as `nospace`, `filenames` and `default`.
`-p` prints the completions set, as commands that set them again; `-r` removes them.
`-D` sets the default for commands that have none, `-E` for an empty line, `-I` for
the first word.

## Options

    -W WORDLIST    From the words of WORDLIST.
    -F FUNCTION    From FUNCTION's COMPREPLY.
    -C COMMAND     From COMMAND's lines.
    -A ACTION, -a, -b, -c, -d, -f, ...    As compgen's.
    -G GLOB, -X PATTERN, -P PREFIX, -S SUFFIX    As compgen's.
    -o OPTION      nospace, filenames, default, dirnames, plusdirs, ...
    -p             Print the completions set.
    -r             Remove them.
    -D, -E, -I     The default, the empty line, the first word.

Exit status: 0, or 1 for a NAME with no completion set.
