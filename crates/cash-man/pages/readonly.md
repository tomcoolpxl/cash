---
name: readonly
summary: Make variables read-only, or list them.
kind: Bash builtins
see: declare unset local
---

## Syntax

    readonly [-aAf] [-p] [NAME[=VALUE]...]

    readonly NAME=VALUE    a variable that cannot change
    readonly NAME          make a variable that is there read-only
    readonly -a NAME=(..)  a read-only array
    readonly -f NAME       a function that cannot be defined again
    readonly               list the read-only variables

## Examples

A constant: changing it fails, and it keeps its value:

    readonly max=3
    max=4
    echo "max is $max"

A read-only array:

    readonly -a sizes=(S M L)
    echo "${sizes[@]}"

A function no one can replace:

    greet() { echo hello; }
    readonly -f greet
    greet() { echo bye; }
    greet

The read-only variables, as `declare` lines:

    readonly version=1.2
    readonly -p | grep version

## How it works

`readonly NAME=VALUE` sets a variable and makes it read-only; `readonly NAME` makes one
that is there read-only, as it is. From then on, for the rest of the shell's life, an
assignment to it fails, and so does `unset`; nothing makes it writable again. A
subshell inherits it.

With `-f`, NAME is a function: a definition of that name fails, and so does `unset -f`.

`readonly` alone, or `readonly -p`, lists the read-only variables as `declare -r`
lines, which `eval` reads back; `readonly -f` lists the read-only functions.
`readonly NAME=VALUE` is `declare -r NAME=VALUE` outside a function, and `declare -r`
inside one makes a local.

## Options

    -a    NAME is an indexed array.
    -A    NAME is an associative array.
    -f    NAME is a function.
    -p    List the read-only variables; with -f, the read-only functions.

Exit status: 0; 1 for a NAME that is not a valid name, or with `-f` for one that is
not a function; 2 for a bad option.
