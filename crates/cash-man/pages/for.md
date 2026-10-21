---
name: for
summary: Run commands for each word of a list, or as arithmetic counts.
kind: Bash keywords
see: while select break continue ((
---

## Syntax

    for NAME [in WORDS...]; do COMMANDS; done

    for (( INIT; TEST; STEP )); do COMMANDS; done

    for f in *.txt          each file that matches
    for arg                 each positional parameter, as for arg in "$@"
    for (( i = 0; i < 3; i++ ))
                            0, 1 and 2

## Examples

Each word in turn:

    for colour in red green blue; do
        echo "colour: $colour"
    done

Counting, as in C:

    for (( i = 1; i <= 3; i++ )); do
        echo "step $i"
    done

The elements of an array, each whole:

    files=('a b.txt' c.txt)
    for f in "${files[@]}"; do echo "<$f>"; done

## How it works

The first form expands the WORDS, as the words of a command are expanded (with field
splitting and wildcards), and sets NAME to each in turn before running the commands.
With no `in`, it takes the positional parameters. With an empty list the commands do not
run at all.

The second form evaluates INIT once, then TEST before each turn, as `(( ))` evaluates
them: the loop runs while TEST is not 0, and STEP is evaluated after each turn. An empty
TEST is 1, so `for (( ;; ))` runs until `break`.

`break` leaves the loop and `continue` goes on to the next turn. The status of the loop
is that of the last command it ran, or 0 when it ran none.
