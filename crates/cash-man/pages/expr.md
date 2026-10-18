---
name: expr
summary: Evaluate an expression and print its value.
kind: Bundled coreutils, from uutils
see: let test bc
---

## Syntax

    expr ARG OPERATOR ARG ...

    expr 2 + 3              5
    expr 7 \* 6             42: * quoted, or the shell globs it
    expr length STRING      how many characters
    expr STRING : REGEX     how many match at the start; \(..\) prints them

## Examples

Arithmetic, each part a word of its own:

    expr 2 + 3
    expr 7 \* 6
    expr 7 / 2
    expr 7 % 2

Strings:

    expr length hello
    expr substr hello 2 3
    expr index hello l
    expr hello : 'h\(.*\)o'

Comparisons print 1 or 0, and 0 is a failure:

    expr 3 \> 2
    expr abc \< abd
    expr 1 = 2 || echo "1 is not 2"

## How it works

`expr` evaluates the expression its arguments make, each operand and operator a word
of its own, and prints the value. Numbers are whole: `+ - * / %`. Comparisons (`< <=
= != >= >`) print 1 or 0, as numbers when both sides are, else as strings. `A | B`
gives A unless it is empty or 0, else B; `A & B` gives A when neither is empty or 0.
`STRING : REGEX` matches a basic regular expression anchored at the start: the number
of characters matched, or what `\(...\)` caught. `length`, `substr STRING POS LEN` and
`index STRING CHARS` work on strings.

Operators the shell reads, `* < > | & ( )`, are quoted or escaped. `$(( ))` does the
arithmetic without starting a command, and `${#var}`, `${var:pos:len}` and `[[ =~ ]]`
the strings.

## Options

None: every argument is part of the expression.

Exit status: 0 when the value is neither empty nor 0; 1 when it is; 2 for an expression
that is not valid; 3 for another error.
