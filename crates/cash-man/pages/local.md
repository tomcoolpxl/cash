---
name: local
summary: Make variables local to a function.
kind: Bash builtins
see: declare unset
---

## Syntax

    local [OPTION] NAME[=VALUE]...

    local v=1        a variable of this function alone
    local -a list    a local array
    local -          keep set's options as they are, restoring them after

## Examples

A local variable leaves the caller's alone:

    v=outer
    f() { local v=inner; echo "in f: $v"; }
    f
    echo "after f: $v"

A function's own variables are seen by the functions it calls:

    outer() { local where=outer; inner; }
    inner() { echo "inner sees: $where"; }
    outer

Options set inside, undone on return, with `local -`:

    f() { local -; set -f; echo *; }
    touch a.txt
    f
    echo a.*

## How it works

`local` makes variables that exist while the function runs and go when it returns; a
variable of the same name outside is hidden meanwhile, and back after. It takes the
options `declare` takes: `local -a list`, `local -i n`, `local -r c=1`.

Bash's local variables are dynamic: a function the current one calls sees its locals,
not the global ones of the same name.

`local -` saves the shell's options, the ones `set` changes, and puts them back when
the function returns.

`local` is for functions: outside one it is an error.

## Options

The options of `declare` (`-a`, `-A`, `-i`, `-r`, `-x`, `-n`, `-u`, `-l`), and:

    -     Save set's options, restored when the function returns.

Exit status: 0, or 1 outside a function, for a bad option or a read-only NAME.
