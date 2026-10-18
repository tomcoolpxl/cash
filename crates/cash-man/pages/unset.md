---
name: unset
summary: Remove variables or functions.
kind: Bash builtins
see: declare local export
---

## Syntax

    unset [-fnv] NAME...

    unset VAR          remove a variable
    unset -f FUNC      remove a function
    unset 'ARR[2]'     remove one element of an array
    unset -n REF       remove a name reference itself

## Examples

Remove a variable; it is then unset, not empty:

    v=1
    unset v
    echo "[${v-unset}]"

Remove a function:

    greet() { echo hi; }
    unset -f greet
    type greet >/dev/null 2>&1 || echo 'greet is gone'

One element of an array, quoted so the brackets are not a glob:

    a=(x y z)
    unset 'a[1]'
    echo "${#a[@]} left: ${a[*]}"

Several at once:

    a=1 b=2 c=3
    unset a b
    echo "[${a-}] [${b-}] [$c]"

## How it works

`unset NAME` removes the variable NAME, or, when there is none, the function NAME.
`-v` removes only variables and `-f` only functions. A removed variable is unset, as
if it had never been set: `${v-default}` gives the default, `[ -v v ]` is false, and
an exported one is no longer passed to programs.

`unset 'a[i]'` removes one element of an array, leaving the others where they are:
indexes are not renumbered. A read-only variable cannot be unset.

In a function, `unset` of a variable the function made `local` removes the local one,
and the caller's comes into sight again.

## Options

    -v    Only variables.
    -f    Only functions.
    -n    A name reference itself, not the variable it refers to.

Exit status: 0, or 1 when a NAME is read-only.
