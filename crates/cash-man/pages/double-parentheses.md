---
name: ((
summary: Evaluate an arithmetic expression; true when it is not 0.
kind: Bash keywords
see: let for [[ declare
---

## Syntax

    (( EXPRESSION ))

    $(( EXPRESSION ))       the value, as a word

    (( n++ ))               add 1 to n
    (( a = b * 2 + 1 ))     assign
    (( n > 5 )) && ...      a test: status 0 when true

## Examples

Counting and testing:

    n=5
    (( n += 3 ))
    (( n > 6 )) && echo "big: $n"

The value as a word, in any base:

    echo $(( 7 / 2 )) $(( 7 % 2 )) $(( 2 ** 10 )) $(( 16#ff ))

## How it works

The expression is read as in C, on 64-bit integers: `+ - * / % **`, the comparisons, `&&
|| !`, the bitwise `& | ^ ~ << >>`, `?:`, assignment with `= += -=` and the rest, `++` and
`--`, and `,`. Variables are named without `$`, and one that is unset or empty is 0. A
number with a leading 0 is octal, `0x` hexadecimal, and `BASE#DIGITS` any base from 2 to
64.

`(( ))` is a command: its status is 0 when the value is not 0, and 1 when it is 0, so it
serves as a test. `$(( ))` is an expansion: the value replaces it in the word. Dividing by
0 is an error.
