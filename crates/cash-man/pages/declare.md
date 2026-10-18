---
names: declare typeset
summary: Declare variables and give them attributes.
summary typeset: Set variables and their attributes (the same as `declare`).
kind: Bash builtins
see: local export readonly unset
---

## Syntax

    declare [-aAfFgilnrtux] [-p] [NAME[=VALUE]]...
    typeset ...                the same: ksh's name for it

    declare -a ARR=(x y)     an indexed array
    declare -A MAP           an associative array, keys of text
    declare -i N=5           an integer: assignments are arithmetic
    declare -r C=1           read-only
    declare -x V=1           exported, as export does
    declare -p NAME          show NAME as a declare line

## Examples

An associative array, its keys words of your own:

    declare -A color=([apple]=red [banana]=yellow)
    echo "${color[banana]}"
    echo "${!color[@]}" | tr ' ' '\n' | sort

An integer: what is assigned is worked out as arithmetic:

    declare -i n=2
    n+=3*4
    echo "$n"

Upper or lower case on every assignment:

    declare -u up=hello
    declare -l down=WORLD
    echo "$up $down"

Read-only, and how it refuses:

    declare -r fixed=1
    fixed=2 2>/dev/null || echo 'refused'
    echo "$fixed"

Show a variable as a line that makes it again:

    declare -a list=(one two)
    declare -p list

A name reference: one name for another variable:

    target=value
    declare -n ref=target
    echo "$ref"

## How it works

`declare` sets variables, as an assignment does, and gives them attributes: `-a` and
`-A` make arrays, `-i` makes assignments arithmetic, `-u` and `-l` change their case,
`-r` makes them read-only, `-x` exports them, `-n` makes a name reference. `+` instead
of `-` takes an attribute away (`declare +x V`).

Inside a function, `declare` makes the variable local, as `local` does, unless `-g`
says global. `declare -p NAME` writes NAME as a `declare` line that makes it again;
`declare -f` writes the functions, `-F` only their names.

## Options

    -a    An indexed array.
    -A    An associative array.
    -i    An integer: assignments are arithmetic.
    -u    Upper case on assignment.   -l    Lower case on assignment.
    -r    Read-only.
    -x    Exported.
    -n    A name reference to the variable its value names.
    -t    Traced: DEBUG and RETURN traps reach into it (for functions).
    -g    Global, even inside a function.
    -p    Show the NAMEs, or every variable, as declare lines.
    -f    Functions: show them, or give them attributes.
    -F    Function names only.
    -I    A new local takes the value and attributes of the one it hides.

Exit status: 0, or 1 for a bad option, a read-only NAME or an invalid assignment.
