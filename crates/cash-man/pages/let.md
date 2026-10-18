---
name: let
summary: Evaluate arithmetic expressions.
kind: Bash builtins
see: expr declare test
---

## Syntax

    let EXPRESSION...

    let x=2+3          x is 5
    let x++            add 1 to x
    let "x = y * 2"    spaces, in quotes
    (( x = y * 2 ))    the same, with no quotes needed

## Examples

Counting:

    n=0
    let n++
    let n+=5
    echo "$n"

Several at once, and spaces in quotes:

    let a=6 b=7 "c = a * b"
    echo "$c"

The status says whether the value is not 0:

    let "3 > 5" || echo "3 is not more than 5"
    let "2 + 2 == 4" && echo "2 + 2 is 4"

The same with `(( ))`, and a value with `$(( ))`:

    (( total = 10 * 3 ))
    echo "total $total, half $(( total / 2 ))"

## How it works

Each argument of `let` is an arithmetic expression, as inside `$(( ))`: `+ - * / %`
and `**` for a power, comparisons (`< <= == != >= >`), `&& || !`, the bit operators,
`a ? b : c`, and assignments, `=`, `+=` and the rest, `++` and `--`. A name stands for
its value, with no `$`; a name that is not set is 0. The numbers are whole, 64 bits;
`0x1F` is hexadecimal, `017` octal, `2#101` binary.

The status is 0 when the last expression's value is not 0, and 1 when it is 0: so `let`
and `(( ))` work as conditions. `(( expression ))` is `let "expression"`, and needs no
quotes around spaces or `<`.

## Options

None: every argument is an expression.

Exit status: 0 when the last value is not 0; 1 when it is 0, or an expression is not
valid.
