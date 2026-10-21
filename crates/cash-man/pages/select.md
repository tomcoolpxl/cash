---
name: select
summary: Ask for one of a list of words by its number.
kind: Bash keywords
see: for read case
---

## Syntax

    select NAME [in WORDS...]; do COMMANDS; done

    PS3='Choose: '          the question it asks

## Examples

A choice read from standard input, here the number 2:

    select fruit in apple banana cherry; do
        echo "you chose $fruit ($REPLY)"
        break
    done <<< 2 2>/dev/null

## How it works

`select` writes the WORDS (the positional parameters with no `in`) on standard error, each
with a number, then asks with `PS3` (`#? ` by default) and reads a line from standard
input. The line read is in `REPLY`; when it is the number of a word, NAME is set to that
word, and otherwise to nothing. The commands run, and it asks again, until `break`, or
until the input ends. An empty line writes the list again.

The commands most often look at NAME with `case`, and end with `break` once the answer
is one they take.
